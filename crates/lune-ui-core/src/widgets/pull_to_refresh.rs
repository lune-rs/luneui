//! Pull to refresh of a phone list (`layout.pull-to-refresh`, D375): a [`Ui::scroll_area`] that
//! reports `refresh` when a finger pulls its content down past the top by 64 px and lets go, and
//! holds a busy indicator above the content while the app refreshes.
//!
//! The area is the scroll area's own, so the finger, the rubber band and the way back are the
//! scroll area's. The wrapper reads how far the content showed past its top
//! (`Ui::scroll_overscroll`) on the frame the finger's pan ends: at least [`THRESHOLD`] px and
//! `refresh` is true on that frame, once for the pan. It never fires from a wheel, a key or a
//! mouse (they cannot pull past the top), nor under [`Overscroll::Clamp`], nor while the app already passes `refreshing`.
//!
//! The app owns the refresh: it starts its work on `refresh`, passes `refreshing` while it runs
//! and passes `false` when it is done. While `refreshing` (and on the frame `refresh` is true) the
//! scroll area's rect starts [`BAND`] px lower and is that much shorter; the band moves with the
//! drawer motion ([`Kind::Drawer`], D3) from where it was, so the content's top is continuous
//! across the frame of the release (the scroll area's own spring carries the overscroll back as
//! the band opens). An app that leaves `refreshing` false on the frame after `refresh` closes the
//! band again. Under the Reduced and Off tiers the band cuts.
//!
//! The indicator is drawn in the strip between the rect's top and the content's top, clipped to
//! it: while pulling, a ring that closes as the pull nears the threshold (a fraction of `shown /
//! THRESHOLD`), then the toolkit's spinner glyph, which turns while the refresh runs. A mouse and
//! a keyboard have no pull: the app keeps its own refresh control (a button, a menu entry).

use std::f32::consts::{FRAC_PI_2, TAU};
use std::hash::Hash;

use super::gesture_track::{Bounds, Track};
use super::scroll_area::{Extent, Overscroll, ScrollOptions, ScrollResponse, ScrollView};
use super::spinner::{self, SpinnerOptions};
use crate::color::with_alpha;
use crate::motion::{Kind, Motion};
use crate::paint::{Rect, StrokeStyle};
use crate::ui::Ui;

/// The pull, in px shown past the top, a release at or past which refreshes.
pub const THRESHOLD: f32 = 64.0;
/// The height of the band the area gives up to the indicator while it refreshes.
pub const BAND: f32 = 56.0;
/// The indicator's size.
const SIZE: f32 = 20.0;
/// The pulling ring's radius and stroke width.
const RING_R: f32 = 8.0;
const RING_W: f32 = 2.0;

/// What [`Ui::pull_to_refresh`] returns.
#[derive(Debug)]
#[non_exhaustive]
pub struct RefreshResponse<R> {
    /// The scroll area's response: what the content returned, its offset (the band does not
    /// change it) and its extent.
    pub scroll: ScrollResponse<R>,
    /// True on exactly the frame a finger's pan ended with at least [`THRESHOLD`] px of the
    /// content shown past the top.
    pub refresh: bool,
}

/// What the wrapper keeps between frames.
#[derive(Debug, Clone, Copy)]
struct RefreshState {
    /// The band's height: 0 to [`BAND`].
    band: Track,
    /// Where the band was last sent.
    target: f32,
    init: bool,
}

impl Default for RefreshState {
    fn default() -> Self {
        RefreshState {
            band: Track::new(0.0),
            target: 0.0,
            init: false,
        }
    }
}

impl Ui<'_> {
    /// A scroll area under `key` over `rect` (as [`Ui::scroll_area`], with the same `opts` and
    /// `content`) that refreshes on a pull: see the module docs. `refreshing` is whether the
    /// app's refresh is running; while it is, the area starts [`BAND`] px lower and shows the
    /// spinner above.
    pub fn pull_to_refresh<R>(
        &mut self,
        key: impl Hash,
        rect: Rect,
        refreshing: bool,
        opts: &ScrollOptions,
        content: impl FnOnce(&mut Ui<'_>, &ScrollView) -> (R, Extent),
    ) -> RefreshResponse<R> {
        let area = self.id(&key);
        let own = area.with("refresh");
        let now = self.time();
        let mut st = std::mem::take(self.persistent::<RefreshState>(own));

        // The release: the frame the finger's pan ends, with what the last frame showed past
        // the top. A pull the physics clamps shows nothing, so it never reaches the threshold.
        let (_, over) = self.scroll_overscroll(area);
        let pan = self.pan(area);
        let rubber = self.scroll_physics().overscroll == Overscroll::RubberBand;
        let fired = rubber && !refreshing && pan.ended && -over >= THRESHOLD;

        // The band follows the app's flag, and the trigger frame's own opening.
        let want = if refreshing || fired { BAND } else { 0.0 };
        let b = Bounds::new(0.0, BAND, BAND);
        if !st.init {
            st = RefreshState {
                band: Track::new(want),
                target: want,
                init: true,
            };
        } else if want != st.target {
            let drawer = Motion::of(Kind::Drawer);
            let slide = self
                .theme()
                .motion
                .resolve(&drawer, self.tier())
                .filter(|r| r.transforms);
            st.band.go_to(now, want, slide, b);
            st.target = want;
        }
        let (band, moving) = st.band.at(now, 0.0, BAND, BAND);
        if moving {
            self.mark_moving();
        }
        *self.persistent::<RefreshState>(own) = st;

        let inner = Rect::new(rect.x, rect.y + band, rect.w, (rect.h - band).max(0.0));
        let scroll = self.scroll_area(&key, inner, opts, content);

        // The indicator, in the strip above the content, after the area so that it reads the
        // overscroll the area just drew.
        let (_, over) = self.scroll_overscroll(area);
        let pulled = (-over).max(0.0);
        let strip = Rect::new(rect.x, rect.y, rect.w, (band + pulled).min(rect.h));
        if strip.h > 0.0 {
            let progress = (pulled / THRESHOLD).min(1.0);
            let shown = progress.max(band / BAND).min(1.0);
            let color = with_alpha(self.theme().fg_secondary, shown);
            let center = (rect.x + rect.w / 2.0, rect.y + strip.h / 2.0);
            self.painter().push_clip(strip);
            if refreshing || fired || band > 0.0 {
                let name = self.tr("luneui.refresh.busy");
                let spin = SpinnerOptions {
                    busy: refreshing || fired,
                    size: SIZE,
                    color: Some(color),
                    name,
                };
                self.spinner(own.with("spinner"), center, &spin);
            } else if progress >= 1.0 {
                spinner::paint(self.painter(), center, SIZE, color, 0.0);
            } else {
                self.painter().arc(
                    [center.0, center.1],
                    RING_R,
                    -FRAC_PI_2,
                    progress * TAU,
                    StrokeStyle::solid(color, RING_W),
                );
            }
            self.painter().pop_clip();
        }
        RefreshResponse {
            scroll,
            refresh: fired,
        }
    }
}
