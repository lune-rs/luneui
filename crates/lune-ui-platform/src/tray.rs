//! The tray item: the notification-area icon on Windows, the menu-bar status item on macOS. It
//! holds an image and a tooltip and reports clicks with its rect; the surfaces it opens are the
//! app's, drawn in a [`crate::popover::PopoverWindow`] and placed by the core's `tray` module.
//!
//! macOS and Windows go through `tray-icon`, with no menu attached and a non-template image (the
//! coloured dot could not survive a template). Other platforms have no tray: [`Tray::new`] is
//! [`TrayError::Unsupported`].
//!
//! A title ([`Tray::set_title`]) is text beside the image. macOS draws it in the status item, in
//! the system's menu-bar font and colour. Windows draws none in the notification area, so there
//! the title becomes the first line of the tooltip ([`Tray::shows_title`] says which); an app that
//! wants text at a glance there draws it into the image.

use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::platform;

/// A straight-alpha RGBA picture, `width * height * 4` bytes, row-major from the top left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The tray item's rect: physical pixels, top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrayRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The mouse button of a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayButton {
    Left,
    Right,
    Other,
}

/// What the tray item reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayEvent {
    /// A button came up over the item, with the item's rect at that moment.
    Click { button: TrayButton, rect: TrayRect },
}

/// Why a tray call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayError {
    /// This platform has no tray.
    Unsupported,
    /// The picture's byte count is not `width * height * 4`, or it has no pixels.
    BadImage,
    /// The OS refused; the text is its reason.
    Os(String),
}

impl std::fmt::Display for TrayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("no tray on this platform"),
            Self::BadImage => f.write_str("the tray image's size does not match its bytes"),
            Self::Os(reason) => write!(f, "the OS refused the tray item: {reason}"),
        }
    }
}

impl std::error::Error for TrayError {}

/// The tray item. It lives on the main thread, and the OS item goes when this drops.
#[derive(Debug)]
pub struct Tray {
    item: platform::TrayItem,
    /// The last title and tooltip, which Windows folds into one tooltip (see [`fold`]). Behind an
    /// `Rc` so the type's auto traits stay what they were: a bare `RefCell` would change them.
    texts: Rc<RefCell<Texts>>,
    /// Keeps the type `!Send` and `!Sync` on every platform, whatever `item` is.
    main_thread: PhantomData<*const ()>,
}

/// The title and the tooltip the app last gave.
#[derive(Debug, Default)]
struct Texts {
    title: String,
    tooltip: String,
}

impl Texts {
    /// What the OS gets as its tooltip: the tooltip itself where the title has a place of its own,
    /// else the title folded into it.
    fn os_tooltip(&self) -> String {
        if platform::SHOWS_TITLE {
            self.tooltip.clone()
        } else {
            fold(&self.title, &self.tooltip)
        }
    }
}

/// The most UTF-16 units of a tooltip the OS is given where the title folds into it: Windows' field
/// holds 128 units with the terminator, and a tip of 128 is stored without one.
const TOOLTIP_UNITS: usize = 127;

/// The one tooltip text for a `title` and a `tooltip`: both empty gives nothing, one empty gives
/// the other, both give the title, a line break and the tooltip. The result is cut to at most
/// [`TOOLTIP_UNITS`] UTF-16 units, always between whole characters (a pair of units is never
/// split).
fn fold(title: &str, tooltip: &str) -> String {
    let joined = match (title.is_empty(), tooltip.is_empty()) {
        (true, _) => tooltip.to_owned(),
        (_, true) => title.to_owned(),
        _ => format!("{title}\n{tooltip}"),
    };
    let (mut units, mut end) = (0, 0);
    for (at, c) in joined.char_indices() {
        units += c.len_utf16();
        if units > TOOLTIP_UNITS {
            break;
        }
        end = at + c.len_utf8();
    }
    joined[..end].to_owned()
}

impl Tray {
    /// Puts `image` in the tray with `tooltip`. Call [`set_event_handler`] first: without a handler
    /// no click reaches the app.
    pub fn new(image: TrayImage, tooltip: &str) -> Result<Tray, TrayError> {
        check_image(&image)?;
        let texts = Texts {
            title: String::new(),
            tooltip: tooltip.to_owned(),
        };
        Ok(Tray {
            item: platform::TrayItem::new(image, &texts.os_tooltip())?,
            texts: Rc::new(RefCell::new(texts)),
            main_thread: PhantomData,
        })
    }

    /// Replaces the picture.
    pub fn set_image(&self, image: TrayImage) -> Result<(), TrayError> {
        check_image(&image)?;
        self.item.set_image(image)
    }

    /// Replaces the tooltip. Where the title has no place of its own (Windows, see
    /// [`Tray::shows_title`]) the OS gets the title, a line break and this tooltip, cut to 127
    /// UTF-16 units between whole characters; a tooltip with no title, or a title with no tooltip,
    /// is that one alone.
    pub fn set_tooltip(&self, tooltip: &str) -> Result<(), TrayError> {
        let next = Texts {
            title: self.texts.borrow().title.clone(),
            tooltip: tooltip.to_owned(),
        };
        self.item.set_tooltip(&next.os_tooltip())?;
        *self.texts.borrow_mut() = next;
        Ok(())
    }

    /// Shows `title` beside the image; `""` clears it. Where [`Tray::shows_title`] is true (macOS)
    /// it is the status item's own title, to the right of the image, in the system's menu-bar font
    /// and colour; the tooltip is untouched. Where it is false (Windows) the notification area has
    /// no such text, so the title becomes the first line of the tooltip, as [`Tray::set_tooltip`]
    /// says: draw text into the image to show it at a glance.
    pub fn set_title(&self, title: &str) -> Result<(), TrayError> {
        if platform::SHOWS_TITLE {
            self.item.set_title(title)?;
            self.texts.borrow_mut().title = title.to_owned();
            return Ok(());
        }
        let next = Texts {
            title: title.to_owned(),
            tooltip: self.texts.borrow().tooltip.clone(),
        };
        self.item.set_tooltip(&next.os_tooltip())?;
        *self.texts.borrow_mut() = next;
        Ok(())
    }

    /// Whether [`Tray::set_title`] draws text beside the image (macOS) or folds it into the
    /// tooltip (Windows).
    pub fn shows_title(&self) -> bool {
        platform::SHOWS_TITLE
    }

    /// The item's rect now, or `None` while the OS has not placed it. On macOS that lasts until the
    /// event loop has run: straight after [`Tray::new`] the status item has no height.
    pub fn rect(&self) -> Option<TrayRect> {
        self.item.rect().and_then(placed)
    }

    /// Whether the bar behind the item is dark, which picks the glyph's tone: the status button's
    /// effective appearance on macOS, `None` elsewhere. `None` too until the item is placed, as
    /// [`Tray::rect`] is, because an item not yet on the bar answers for no bar.
    pub fn bar_is_dark(&self) -> Option<bool> {
        self.rect()?;
        self.item.bar_is_dark()
    }
}

/// Sets the handler of the tray's events. Only the first call takes effect, and it must come before
/// the first [`Tray::new`]: the OS layer fixes its route at the first event. The handler runs on the
/// thread the OS reports on, so it should only pass the event to the app's event loop.
pub fn set_event_handler(f: impl Fn(TrayEvent) + Send + Sync + 'static) {
    platform::set_event_handler(f);
}

/// `rect`, if it has an area: an item the OS has not laid out yet has none.
fn placed(rect: TrayRect) -> Option<TrayRect> {
    (rect.w > 0.0 && rect.h > 0.0).then_some(rect)
}

/// `Ok` when the bytes are exactly the pixels the size says, and there is at least one.
fn check_image(image: &TrayImage) -> Result<(), TrayError> {
    let bytes = u64::from(image.width) * u64::from(image.height) * 4;
    if bytes == 0 || bytes != image.rgba.len() as u64 {
        return Err(TrayError::BadImage);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECT: TrayRect = TrayRect {
        x: 1863.0,
        y: -1.0,
        w: 34.0,
        h: 33.0,
    };

    #[test]
    fn a_rect_with_no_area_is_not_placed() {
        assert_eq!(placed(RECT), Some(RECT));
        // Measured on macOS straight after the status item is made.
        let unplaced = TrayRect {
            x: 0.0,
            y: 2700.0,
            w: 68.0,
            h: 0.0,
        };
        assert_eq!(placed(unplaced), None);
        assert_eq!(placed(TrayRect { w: 0.0, ..RECT }), None);
    }

    fn units(s: &str) -> usize {
        s.encode_utf16().count()
    }

    #[test]
    fn a_title_and_a_tooltip_fold_into_one_text() {
        assert_eq!(fold("", ""), "");
        assert_eq!(fold("Title", ""), "Title");
        assert_eq!(fold("", "Tooltip"), "Tooltip");
        assert_eq!(fold("Title", "Tooltip"), "Title\nTooltip");
    }

    #[test]
    fn a_long_text_is_cut_to_127_units() {
        let long = "x".repeat(200);
        let folded = fold(&long, "");
        assert_eq!(units(&folded), 127);
        assert_eq!(folded, "x".repeat(127));
        // The cut can fall in the tooltip, after the line break.
        let both = fold(&"a".repeat(100), &"b".repeat(100));
        assert_eq!(units(&both), 127);
        assert!(both.starts_with(&"a".repeat(100)) && both[100..].starts_with('\n'));
        // A text that fits is kept whole.
        assert_eq!(fold(&"y".repeat(127), ""), "y".repeat(127));
    }

    #[test]
    fn a_pair_of_units_is_never_split() {
        // The pair for U+1F600 would take units 127 and 128: it is left out whole.
        let straddles = format!("{}\u{1F600}tail", "a".repeat(126));
        let folded = fold(&straddles, "");
        assert_eq!(folded, "a".repeat(126));
        assert_eq!(units(&folded), 126);
        // The same pair ending exactly at unit 127 stays.
        let fits = format!("{}\u{1F600}tail", "a".repeat(125));
        let folded = fold(&fits, "");
        assert_eq!(folded, format!("{}\u{1F600}", "a".repeat(125)));
        assert_eq!(units(&folded), 127);
        // Every cut of a text of pairs is whole characters at most 127 units long.
        for n in 0..140 {
            let folded = fold(&"\u{1F600}".repeat(n), "");
            assert!(units(&folded) <= 127);
            assert_eq!(units(&folded) % 2, 0);
        }
    }

    #[test]
    fn what_the_os_gets_is_the_tooltip_alone_where_the_title_has_a_place_and_folded_otherwise() {
        let texts = Texts {
            title: "Title".into(),
            tooltip: "Tooltip".into(),
        };
        let want = if platform::SHOWS_TITLE {
            "Tooltip"
        } else {
            "Title\nTooltip"
        };
        assert_eq!(texts.os_tooltip(), want);
    }

    fn image(width: u32, height: u32, bytes: usize) -> TrayImage {
        TrayImage {
            width,
            height,
            rgba: vec![0; bytes],
        }
    }

    #[test]
    fn an_image_of_the_right_size_passes() {
        assert_eq!(check_image(&image(18, 18, 18 * 18 * 4)), Ok(()));
        assert_eq!(check_image(&image(36, 18, 36 * 18 * 4)), Ok(()));
    }

    #[test]
    fn a_size_mismatch_is_a_bad_image() {
        assert_eq!(
            check_image(&image(18, 18, 18 * 18 * 4 - 1)),
            Err(TrayError::BadImage)
        );
        assert_eq!(
            check_image(&image(18, 18, 18 * 18 * 4 + 4)),
            Err(TrayError::BadImage)
        );
        assert_eq!(check_image(&image(0, 18, 0)), Err(TrayError::BadImage));
        assert_eq!(check_image(&image(18, 0, 0)), Err(TrayError::BadImage));
    }

    #[test]
    fn a_size_past_u32_does_not_wrap_to_a_match() {
        // 2^16 * 2^16 * 4 wraps to 0 in 32 bits.
        assert_eq!(
            check_image(&image(1 << 16, 1 << 16, 0)),
            Err(TrayError::BadImage)
        );
    }
}
