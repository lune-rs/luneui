//! Platforms without a tray: the item is refused, the popover window is a plain window (on a
//! phone and in a browser there is none: `wrap` refuses).

use winit::window::Window;

use crate::tray::{TrayError, TrayEvent, TrayImage, TrayRect};

// On a phone and in a browser `wrap` never builds one, so the rest of the type is unused there.
#[derive(Debug)]
#[cfg_attr(
    any(target_os = "ios", target_os = "android", target_family = "wasm"),
    allow(dead_code)
)]
pub(crate) struct Panel;

#[cfg_attr(
    any(target_os = "ios", target_os = "android", target_family = "wasm"),
    allow(dead_code)
)]
impl Panel {
    /// A phone has no popover window: a second window is never attached to a scene (iOS) or shown
    /// beside the app (Android), so it would be invisible while `wrap` answered `Ok` (D359).
    #[cfg(any(target_os = "ios", target_os = "android"))]
    pub(crate) fn wrap(_window: &Window) -> Result<Self, String> {
        Err("popover windows do not exist on a phone".into())
    }

    /// A browser has no floating window either: winit would make another canvas in the page, which
    /// is no popover, so `wrap` answers `Err` and no caller believes it has one (D430).
    #[cfg(target_family = "wasm")]
    pub(crate) fn wrap(_window: &Window) -> Result<Self, String> {
        Err("popover windows do not exist in a browser".into())
    }

    #[cfg(not(any(target_os = "ios", target_os = "android", target_family = "wasm")))]
    pub(crate) fn wrap(_window: &Window) -> Result<Self, String> {
        Ok(Self)
    }

    pub(crate) fn show(&self, window: &Window) {
        window.set_visible(true);
        window.focus_window();
    }

    pub(crate) fn hide(&self, window: &Window) {
        window.set_visible(false);
    }

    pub(crate) fn restore(&self, window: &Window) {
        window.set_visible(false);
    }
}

pub(crate) fn frontmost_app() -> Option<String> {
    None
}

/// No item can exist here, so no title is shown.
pub(crate) const SHOWS_TITLE: bool = false;

/// Never built: [`TrayItem::new`] refuses, so the methods have no value to run on.
#[derive(Debug)]
pub(crate) enum TrayItem {}

impl TrayItem {
    pub(crate) fn new(_image: TrayImage, _tooltip: &str) -> Result<Self, TrayError> {
        Err(TrayError::Unsupported)
    }

    pub(crate) fn set_image(&self, _image: TrayImage) -> Result<(), TrayError> {
        match *self {}
    }

    pub(crate) fn set_tooltip(&self, _tooltip: &str) -> Result<(), TrayError> {
        match *self {}
    }

    pub(crate) fn set_title(&self, _title: &str) -> Result<(), TrayError> {
        match *self {}
    }

    pub(crate) fn rect(&self) -> Option<TrayRect> {
        match *self {}
    }

    pub(crate) fn bar_is_dark(&self) -> Option<bool> {
        match *self {}
    }
}

pub(crate) fn set_event_handler(_f: impl Fn(TrayEvent) + Send + Sync + 'static) {}

#[cfg(test)]
mod tests {
    use crate::tray::{Tray, TrayError, TrayImage};

    #[test]
    fn a_tray_is_unsupported_here() {
        let image = TrayImage {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
        };
        assert!(matches!(
            Tray::new(image, "tip"),
            Err(TrayError::Unsupported)
        ));
    }

    #[test]
    fn a_bad_image_is_refused_first() {
        let image = TrayImage {
            width: 1,
            height: 1,
            rgba: vec![],
        };
        assert!(matches!(Tray::new(image, "tip"), Err(TrayError::BadImage)));
    }

    /// The auto traits of `Tray` where the item is the empty one: the same as before the title was
    /// added (an app may bound a type holding a tray by them). This fails to compile when a field
    /// takes one away.
    #[test]
    fn a_tray_keeps_its_unwind_traits() {
        fn holds<T: std::panic::UnwindSafe + std::panic::RefUnwindSafe>() {}
        holds::<Tray>();
        holds::<Option<Tray>>();
    }
}
