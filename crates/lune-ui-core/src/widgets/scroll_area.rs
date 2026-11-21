//! The scroll area (`layout.scroll-area`): a clipped viewport over taller or wider content, with a
//! per-id offset kept by the `Ui` (persistent state, so it survives a hidden tab under
//! `keep_alive`), smooth wheel stepping, trackpad scrolling 1:1, scroll-into-view, follow-tail,
//! offset preservation across replaced content and the overlay thumb of
//! [`scrollbar`](super::scrollbar).
//!
//! Offsets and extents are `f64` (D6): at a million 45 px rows an `f32` offset steps 4 px. An
//! animated offset is kept as an exact `f64` base plus a small `f32` [`Transition`] relative to
//! it, so it lands on the target exactly and an `f32` never carries a large offset.
//!
//! The pure functions ([`clamp_offset`], [`should_restore`], [`restore_offset`],
//! [`centre_target`], [`reveal_offset`]) port the reference app's offset preserver and its centring
//! formula; the frame logic lives in [`Ui::scroll_area`].
//!
//! A finger scrolls it too (D350): a touch pan on the area follows the finger 1:1, lets go as a
//! fling that decays at a rate and rubber-bands at the ends, then springs back. The overscroll is
//! drawn as one extra translate around the content; [`ScrollView::offset`] and
//! [`ScrollResponse::offset`] stay clamped. [`ScrollPhysics`] and [`Ui::with_scroll_physics`] set
//! the decay, the ends and the spring.
//!
//! Frame order: the wheel and key input of the frame moves the offset, the content draws at it,
//! then everything that needs the content's size runs (the measured clamp, follow-tail, reveals,
//! page keys, the thumb). When one of those moves the offset after the content drew, the frame asks
//! for a catch-up frame (`Repaint::Now`), so the stale picture lasts one frame.

use std::hash::Hash;

use crate::access::{Action, Node, Role, State, Value};
use crate::input::{FocusEvent, Key, NamedKey};
use crate::layout::Axis;
use crate::motion::{
    DampedSpring, Decay, Easing, Kind, Motion, Resolved, Time, Transition, rubber_band,
    rubber_band_inverse,
};
use crate::paint::Rect;
use crate::ui::{Response, Sense, Ui, WidgetId};

use super::scrollbar::{ThumbGeometry, thumb_geometry};

/// Logical px per wheel line (D9).
pub const LINE: f64 = 48.0;
/// The wheel step: `scroll` kind, `fast`, `quad-out`, snapped in Reduced.
pub const WHEEL: Motion = Motion::of(Kind::Scroll);
/// Scroll-into-view within two viewports: `fast`, `sine-in-out`, snapped in Reduced.
pub const INTO_VIEW: Motion = Motion {
    easing: Easing::SineInOut,
    ..Motion::of(Kind::Scroll)
};
/// A reveal farther than this many viewports snaps.
pub const SNAP_DISTANCE: f64 = 2.0;

/// Whether an axis shows its bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bars {
    /// The thumb shows (while the pointer is over the area) when the content overflows.
    Auto,
    /// No thumb; the wheel and the keys still scroll.
    Hidden,
    /// No thumb and no scrolling on this axis.
    Disabled,
}

impl Bars {
    /// This bar while `extent` overflows `viewport`, else [`Bars::Disabled`]. A composite that nests
    /// an area and knows its extent before the area runs passes its axes through this: an area with
    /// no live axis registers no scroll sense, so the delta reaches the area around it, which the
    /// single-topmost rule would otherwise end here (D176). The offset still clamps on a disabled
    /// axis, so an area that shrinks to fit returns to 0.
    pub(crate) fn unless_fits(self, extent: f64, viewport: f64) -> Self {
        if extent > viewport {
            self
        } else {
            Self::Disabled
        }
    }
}

/// What a scroll area is given besides its rect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollOptions {
    /// The horizontal and the vertical bar.
    pub bars: (Bars, Bars),
    /// The app's "same document" key. A new key starts at the top; the same key keeps the offset
    /// through frames with a shorter extent (a reload) and restores it. `None` never restores.
    pub identity: Option<u64>,
    /// When the extent grows, go to the end (the vertical offset).
    pub follow_tail: bool,
    /// The content's extent when the app knows it before drawing: the clamp then runs before the
    /// content (D7); otherwise the content returns it and the clamp runs after.
    pub extent: Option<(f64, f64)>,
    /// A vertical wheel or pixel delta scrolls the horizontal axis, so a plain wheel works on a
    /// strip that only scrolls sideways. The larger of a gesture's two components wins.
    pub wheel_x: bool,
    /// The room the horizontal and the vertical bar leave at their far end (right, bottom), in
    /// px, for a control that shares that corner: the track stops that much short, so the thumb
    /// never lies under the control at any offset. 0 by default.
    pub bar_room: (f32, f32),
}

impl Default for ScrollOptions {
    fn default() -> Self {
        Self {
            bars: (Bars::Auto, Bars::Auto),
            identity: None,
            follow_tail: false,
            extent: None,
            wheel_x: false,
            bar_room: (0.0, 0.0),
        }
    }
}

/// What the content closure is told: the viewport (in the area's own space) and the offset the
/// content is drawn at. Content coordinates have their origin at the viewport's top-left corner
/// (offset 0): a box drawn at `viewport.y + 100` is at `y = 100` for [`Ui::scroll_to`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollView {
    pub viewport: Rect,
    pub offset: (f64, f64),
}

/// The size of the content, as the content measured it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Extent {
    pub width: f64,
    pub height: f64,
}

impl Extent {
    /// An extent of `width` by `height`.
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// What [`Ui::scroll_area`] returns.
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollResponse<R> {
    /// What the content closure returned.
    pub value: R,
    /// The area's own region (the wheel target).
    pub response: Response,
    /// The offset after this frame's input, clamped to the extent.
    pub offset: (f64, f64),
    /// The extent this frame used.
    pub extent: (f64, f64),
    pub viewport: Rect,
}

/// How a target is brought into view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reveal {
    /// Centre it: `start - (viewport - len) / 2` (the spec's formula).
    Center,
    /// Move the least that shows it all.
    Nearest,
}

/// A box in content coordinates, per axis a `(start, length)` or nothing (that axis stays put).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Target {
    pub x: Option<(f64, f64)>,
    pub y: Option<(f64, f64)>,
}

impl Target {
    /// A box on both axes.
    pub fn rect(r: Rect) -> Self {
        Self {
            x: Some((f64::from(r.x), f64::from(r.w))),
            y: Some((f64::from(r.y), f64::from(r.h))),
        }
    }

    /// A span on the vertical axis only, such as a list row: `index * row_height`, `row_height`.
    pub fn span_y(start: f64, len: f64) -> Self {
        Self {
            x: None,
            y: Some((start, len)),
        }
    }

    /// A span on the horizontal axis only.
    pub fn span_x(start: f64, len: f64) -> Self {
        Self {
            x: Some((start, len)),
            y: None,
        }
    }
}

/// The offset range's end: `extent - viewport`, at least 0 (0 for anything not finite).
pub fn max_offset(extent: f64, viewport: f64) -> f64 {
    if !extent.is_finite() || !viewport.is_finite() {
        return 0.0;
    }
    (extent - viewport).max(0.0)
}

/// `offset` clamped to `[0, max(0, extent - viewport)]`; 0 for anything not finite or not
/// positive (the reference app's `ClampRestoreOffset`).
pub fn clamp_offset(offset: f64, extent: f64, viewport: f64) -> f64 {
    if !offset.is_finite() || offset <= 0.0 {
        return 0.0;
    }
    offset.min(max_offset(extent, viewport))
}

/// Whether a rebind restores the previous offset: the area has rendered, the new key is `Some` and
/// equals the last one (`None` never restores). The reference app's `ShouldRestore`.
pub fn should_restore(has_rendered: bool, last_key: Option<u64>, new_key: Option<u64>) -> bool {
    has_rendered && new_key.is_some() && new_key == last_key
}

/// The offset a restore writes, or `None` for no write: a saved offset of 0 or less is not
/// restored, and a change under 0.5 px from `current` is not written.
pub fn restore_offset(saved: f64, extent: f64, viewport: f64, current: f64) -> Option<f64> {
    if !saved.is_finite() || saved <= 0.0 {
        return None;
    }
    let target = clamp_offset(saved, extent, viewport);
    ((current - target).abs() > 0.5).then_some(target)
}

/// The offset that centres the row `index` of height `row` in a viewport, clamped to the range:
/// `index * row - (viewport - row) / 2`. The reference app's `ComputeCenteredOffsetY`.
pub fn centre_target(index: usize, row: f64, viewport: f64, extent: f64) -> f64 {
    let raw = index as f64 * row - (viewport - row) / 2.0;
    if raw.is_nan() {
        return 0.0;
    }
    raw.clamp(0.0, max_offset(extent, viewport))
}

/// The offset that shows the span `start .. start + len` in a viewport that is at `current`,
/// before the clamp. A span taller than the viewport aligns its start unless it already covers it.
pub fn reveal_offset(start: f64, len: f64, viewport: f64, current: f64, how: Reveal) -> f64 {
    match how {
        Reveal::Center => start - (viewport - len) / 2.0,
        Reveal::Nearest => {
            let end = start + len;
            if len >= viewport {
                if start <= current && end >= current + viewport {
                    current
                } else {
                    start
                }
            } else if start < current {
                start
            } else if end > current + viewport {
                end - viewport
            } else {
                current
            }
        }
    }
}

/// What happens past an end of the content under a finger (D350).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Overscroll {
    /// A pull past the end shows a rubber-banded distance of it ([`rubber_band`]); a fling that
    /// reaches the end bounces off it, and both spring back.
    #[default]
    RubberBand,
    /// The offset stops at the end: nothing shows past it and a fling that reaches it stops dead.
    Clamp,
}

/// How a scroll area moves under a finger: how a fling decays, what the ends do, and the spring
/// that brings an overscroll back. A fling and a bounce ignore the motion tier (D350): they are
/// the finger's own motion. Set for the areas inside a scope by [`Ui::with_scroll_physics`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ScrollPhysics {
    /// How a fling slows down.
    pub decay: Decay,
    /// What an end does.
    pub overscroll: Overscroll,
    /// The spring that takes an overscroll back to the end.
    pub spring: DampedSpring,
}

impl Default for ScrollPhysics {
    /// Rubber-banding ends, [`Decay::NORMAL`], [`DampedSpring::BOUNCE`].
    fn default() -> Self {
        Self {
            decay: Decay::NORMAL,
            overscroll: Overscroll::RubberBand,
            spring: DampedSpring::BOUNCE,
        }
    }
}

impl ScrollPhysics {
    /// These physics with `decay`.
    pub const fn with_decay(self, decay: Decay) -> Self {
        Self { decay, ..self }
    }

    /// These physics with `overscroll` at the ends.
    pub const fn with_overscroll(self, overscroll: Overscroll) -> Self {
        Self { overscroll, ..self }
    }

    /// These physics with `spring` for the way back.
    pub const fn with_spring(self, spring: DampedSpring) -> Self {
        Self { spring, ..self }
    }
}

/// The slowest release, in px/s, that still flings.
const FLING_MIN: f32 = 50.0;

/// What moves an axis other than the wheel's transition: the finger, a fling or a bounce.
#[derive(Debug, Clone, Copy, Default)]
enum Touch {
    /// Nothing: the offset is `base + rel`.
    #[default]
    None,
    /// The finger holds it. `u` is the unbounded coordinate the finger has reached (past an end it
    /// is the pull; what shows past the end is its rubber band).
    Pan { u: f64 },
    /// A fling from `from` at `v` px/s (offset direction), `t0` its start.
    Fling {
        t0: Time,
        from: f64,
        v: f32,
        physics: ScrollPhysics,
    },
    /// The spring back from `over0` px past the high or the low end, at `v` px/s, `t0` its start.
    Bounce {
        t0: Time,
        high: bool,
        over0: f32,
        v: f32,
        spring: DampedSpring,
    },
}

/// What an axis shows at one instant.
#[derive(Debug, Clone, Copy)]
struct Sample {
    /// The offset, clamped to the range.
    offset: f64,
    /// The overscroll: how far the content is drawn past the offset, in the offset's direction.
    over: f32,
    /// A fling or a bounce, a wheel step or a reveal is in flight.
    moving: bool,
}

/// One axis' offset: where the user or app wants it (`wanted`, never lowered by a short frame of
/// a preserved document), and the animation towards it.
#[derive(Debug, Clone, Copy)]
struct AxisState {
    wanted: f64,
    /// The exact base the animation is relative to.
    base: f64,
    /// The animated part: the shown offset is `base + rel`.
    rel: Transition<f32>,
    /// The finger, a fling or a bounce; `base` and `rel` are idle while it holds the axis.
    touch: Touch,
}

impl Default for AxisState {
    fn default() -> Self {
        Self {
            wanted: 0.0,
            base: 0.0,
            rel: Transition::settled(0.0),
            touch: Touch::None,
        }
    }
}

impl AxisState {
    /// Rebases onto `wanted` once the animation has ended, so a settled offset is exact.
    fn settle(&mut self, now: Time) {
        if !self.rel.in_flight(now) {
            self.base = self.wanted;
            self.rel = Transition::settled(0.0);
        }
    }

    fn value(&self, now: Time) -> (f64, bool) {
        let (rel, moving) = self.rel.sample(now);
        (self.base + f64::from(rel), moving)
    }

    /// Moves the whole offset by `by`, keeping any animation's course relative to it (and a
    /// finger's, a fling's).
    fn shift(&mut self, by: f64) {
        self.wanted += by;
        self.base += by;
        match &mut self.touch {
            Touch::Pan { u } => *u += by,
            Touch::Fling { from, .. } => *from += by,
            Touch::Bounce { .. } | Touch::None => {}
        }
    }

    /// Sets the offset at once, cancelling any animation and any touch motion.
    fn jump(&mut self, to: f64) {
        self.wanted = to;
        self.base = to;
        self.rel = Transition::settled(0.0);
        self.touch = Touch::None;
    }

    /// Moves to `to` per `motion` (retargeting keeps the velocity), or at once without one. A touch
    /// motion on the axis was stopped by [`Self::halt`] first.
    fn step(&mut self, now: Time, to: f64, motion: Option<Resolved>) {
        match motion {
            Some(r) => {
                self.touch = Touch::None;
                self.wanted = to;
                self.rel
                    .retarget(now, (to - self.base) as f32, r.duration, r.easing);
            }
            None => self.jump(to),
        }
    }

    /// Clamps the target to `to`, the range the measured extent allows. A touch motion keeps its
    /// course: what shows is clamped on every read, and a fling's `wanted` is its landing.
    fn clamp_to(&mut self, to: f64) {
        if matches!(self.touch, Touch::None) {
            self.jump(to);
        } else {
            self.wanted = to;
        }
    }

    /// What the axis shows at `now`, for content `ext` long in a viewport `vp` long.
    fn sample(&self, now: Time, ext: f64, vp: f64, physics: &ScrollPhysics) -> Sample {
        let bound = |high: bool| if high { max_offset(ext, vp) } else { 0.0 };
        match self.touch {
            Touch::None => {
                let (v, moving) = self.value(now);
                Sample {
                    offset: clamp_offset(v, ext, vp),
                    over: 0.0,
                    moving,
                }
            }
            Touch::Pan { u } => {
                let c = clamp_offset(u, ext, vp);
                let over = match physics.overscroll {
                    Overscroll::RubberBand => rubber_band((u - c) as f32, vp as f32),
                    Overscroll::Clamp => 0.0,
                };
                Sample {
                    offset: c,
                    over,
                    moving: false,
                }
            }
            Touch::Fling {
                t0,
                from,
                v,
                physics,
            } => {
                let at = from + f64::from(physics.decay.offset(v, now.saturating_sub(t0)));
                Sample {
                    offset: clamp_offset(at, ext, vp),
                    over: 0.0,
                    moving: true,
                }
            }
            Touch::Bounce {
                t0,
                high,
                over0,
                v,
                spring,
            } => Sample {
                offset: bound(high),
                over: spring.sample(over0, 0.0, v, now.saturating_sub(t0)).0,
                moving: true,
            },
        }
    }

    /// Stops a fling, a bounce or a pan where it shows at `now` (the overscroll dropped), so the
    /// code that sets the offset by input runs from there. Nothing changes without a touch motion.
    fn halt(&mut self, now: Time, ext: f64, vp: f64, physics: &ScrollPhysics) {
        if !matches!(self.touch, Touch::None) {
            let at = self.sample(now, ext, vp, physics).offset;
            self.jump(at);
        }
    }

    /// Ends a fling that has run out or reached its end, and turns one that reached an end into a
    /// bounce (or stops it at the end under [`Overscroll::Clamp`]); ends a bounce that has settled.
    /// Everything is read off the times since the start, so no frame rate changes where it lands.
    fn advance(&mut self, now: Time, ext: f64, vp: f64) {
        if let Touch::Fling {
            t0,
            from,
            v,
            physics,
        } = self.touch
        {
            let t = now.saturating_sub(t0);
            let up = v > 0.0;
            let bound = if up { max_offset(ext, vp) } else { 0.0 };
            let dist = bound - from;
            let reach = if dist == 0.0 || (dist > 0.0) != up {
                Some(Time::ZERO)
            } else if dist.abs() > 1e5 {
                None
            } else {
                physics.decay.time_to(v, dist as f32)
            };
            let duration = physics.decay.duration(v);
            match reach {
                Some(th) if t >= th => match physics.overscroll {
                    Overscroll::Clamp => self.jump(bound),
                    Overscroll::RubberBand => {
                        self.touch = Touch::Bounce {
                            t0: t0 + th,
                            high: up,
                            over0: 0.0,
                            v: physics.decay.velocity(v, th),
                            spring: physics.spring,
                        };
                    }
                },
                None if t >= duration => {
                    let end = from + f64::from(physics.decay.offset(v, duration));
                    self.jump(clamp_offset(end, ext, vp));
                }
                _ => {}
            }
        }
        if let Touch::Bounce {
            t0,
            high,
            over0,
            v,
            spring,
        } = self.touch
            && now.saturating_sub(t0) >= spring.duration(over0, 0.0, v)
        {
            self.jump(if high { max_offset(ext, vp) } else { 0.0 });
        }
    }

    /// The finger moved `d` px along the axis (its direction): the content follows it, past an end
    /// as far as the unbounded coordinate goes. A pan starts from what shows, overscroll included.
    fn pan(&mut self, now: Time, ext: f64, vp: f64, d: f32, physics: &ScrollPhysics) {
        let u = match self.touch {
            Touch::Pan { u } => u,
            _ => {
                let s = self.sample(now, ext, vp, physics);
                s.offset + f64::from(rubber_band_inverse(s.over, vp as f32))
            }
        };
        let u = u - f64::from(d);
        let c = clamp_offset(u, ext, vp);
        // Under `Clamp` nothing shows past an end, so the coordinate stays on it: the finger
        // reversing moves the content at once.
        let u = if physics.overscroll == Overscroll::Clamp {
            c
        } else {
            u
        };
        self.wanted = c;
        self.base = c;
        self.rel = Transition::settled(0.0);
        self.touch = Touch::Pan { u };
    }

    /// The finger lifted at `v` px/s (the offset's direction), or the pan was cut (`v` 0): a pull
    /// past an end springs back, a fast enough release flings, anything else rests.
    fn lift(&mut self, now: Time, ext: f64, vp: f64, v: f32, physics: &ScrollPhysics) {
        let Touch::Pan { u } = self.touch else { return };
        let c = clamp_offset(u, ext, vp);
        let over = self.sample(now, ext, vp, physics).over;
        self.base = c;
        self.rel = Transition::settled(0.0);
        if over != 0.0 {
            self.wanted = c;
            self.touch = Touch::Bounce {
                t0: now,
                high: u > c,
                over0: over,
                v,
                spring: physics.spring,
            };
        } else if v.is_finite() && v.abs() >= FLING_MIN {
            let landing = c + f64::from(physics.decay.offset(v, physics.decay.duration(v)));
            self.wanted = clamp_offset(landing, ext, vp);
            self.touch = Touch::Fling {
                t0: now,
                from: c,
                v,
                physics: *physics,
            };
        } else {
            self.jump(c);
        }
    }

    /// A touch landed on the area: a motion in flight stops where the last frame drew it
    /// (`drawn`), and an overscroll that was showing (`over`) keeps its place and springs back
    /// from there, since nothing holds it.
    fn catch(&mut self, now: Time, drawn: f64, over: f32, ext: f64, vp: f64) {
        match self.touch {
            Touch::Bounce { high, spring, .. } if over != 0.0 => {
                self.touch = Touch::Bounce {
                    t0: now,
                    high,
                    over0: over,
                    v: 0.0,
                    spring,
                };
            }
            Touch::Pan { .. } => {}
            Touch::None if !self.rel.in_flight(now) => {}
            _ => self.jump(clamp_offset(drawn, ext, vp)),
        }
    }
}

/// A scroll link's retained state (D8), under the group's id: the value the panes share, the
/// value each pane last took, and how many panes drew this frame.
#[derive(Debug, Default)]
struct LinkState {
    value: Option<f64>,
    taken: std::collections::BTreeMap<WidgetId, f64>,
    frame: u64,
    drawn: u32,
}

/// The retained state of one scroll area.
#[derive(Debug, Default)]
struct ScrollState {
    /// Horizontal, then vertical.
    a: [AxisState; 2],
    /// The extent the previous frame's content had.
    extent: [f64; 2],
    key: Option<u64>,
    rendered: bool,
    /// The focused child the last frame saw inside the area.
    last_focus: Option<WidgetId>,
    /// What the last frame ended showing: the offset and the overscroll, which a touch that lands
    /// on a motion in flight stops at.
    drawn: [f64; 2],
    over: [f32; 2],
    /// The keyboard's bottom inset the last frame saw, so a change reveals the caret (D365).
    last_inset: f32,
}

/// `Ui::scroll_to` and `Ui::scroll_shift` requests, taken by the area on its next call.
#[derive(Debug, Default)]
struct Requests {
    reveals: Vec<(Target, Reveal)>,
    shift_x: f64,
    shift_y: f64,
    /// What the area left behind when it last ran.
    ran: Option<Ran>,
}

/// One area's axes as its last call ended, so a request made later in the same frame can tell
/// whether it changes the target.
#[derive(Debug, Clone, Copy)]
struct Ran {
    frame: u64,
    live: [bool; 2],
    wanted: [f64; 2],
    ext: [f64; 2],
    vp: [f64; 2],
}

impl Ran {
    /// Whether `target` moves either axis from where the area is heading.
    fn moved_by(&self, target: &Target, how: Reveal) -> bool {
        [target.x, target.y].iter().enumerate().any(|(ax, span)| {
            let cur = clamp_offset(self.wanted[ax], self.ext[ax], self.vp[ax]);
            self.live[ax] && reveal_axis(*span, cur, self.ext[ax], self.vp[ax], how).is_some()
        })
    }
}

/// The offset a reveal of `span` (start and length, in content coordinates) sends an axis to from
/// `cur`, or `None` when it changes nothing: no span, one outside the content, or already showing.
fn reveal_axis(span: Option<(f64, f64)>, cur: f64, ext: f64, vp: f64, how: Reveal) -> Option<f64> {
    let (start, len) = span?;
    if !start.is_finite() || !len.is_finite() || len < 0.0 || start >= ext || start + len < 0.0 {
        return None;
    }
    let to = clamp_offset(reveal_offset(start, len, vp, cur, how), ext, vp);
    (to != cur).then_some(to)
}

/// `r`, drawn in the area's space, as a box in content coordinates: the content's origin is the
/// viewport's top-left corner at offset 0.
fn content_rect(r: Rect, viewport: Rect) -> Rect {
    Rect::new(r.x - viewport.x, r.y - viewport.y, r.w, r.h)
}

/// The height of the area at `rect` an on-screen keyboard `inset` tall covers, in the area's own
/// space: 0 with no keyboard, otherwise the part of the area's window rect below the keyboard's
/// top, clamped to the area (D365).
fn keyboard_cut(ui: &mut Ui<'_>, rect: Rect, inset: f32) -> f32 {
    if inset.is_nan() || inset <= 0.0 || inset.is_infinite() || rect.h.is_nan() || rect.h <= 0.0 {
        return 0.0;
    }
    let top = ui.painter().window.h - inset;
    let win = ui.painter().transformed(rect);
    let covered = (win.bottom() - top).clamp(0.0, win.h);
    // The window height the area spans, back to the area's own space.
    if win.h > 0.0 {
        (covered * rect.h / win.h).clamp(0.0, rect.h)
    } else {
        0.0
    }
}

/// `child`, the focused widget's rect in the content space, narrowed to the caret the widget
/// published as the frame's input method area, when that lies inside `shown`, the child's rect
/// where it was drawn (the area's own space); `child` itself otherwise. The caller asks for it
/// only for a child taller than the visible part: one that fits is brought in whole (D365).
fn caret_in(ui: &mut Ui<'_>, child: Rect, shown: Rect) -> Rect {
    let Some(ime) = ui.frame_ime_area() else {
        return child;
    };
    let win = ui.painter().transformed(shown);
    if !(ime.w > 0.0 && ime.h > 0.0 && within(ime, win)) {
        return child;
    }
    Rect::new(
        child.x + (ime.x - win.x),
        child.y + (ime.y - win.y),
        ime.w,
        ime.h,
    )
}

/// Whether `inner` lies wholly inside `outer`, both in one space.
fn within(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

/// What an area hands on to the area around it for a child at `child`: its viewport on the axes it
/// scrolls itself. On a translated axis it cannot move (its offset there is 0), only the outer area
/// can bring the child into view, so the child's own span goes on (a tall table that scrolls only
/// sideways, in a document). An untranslated area's child is a region of its own, not content.
fn handed_on(child: Rect, viewport: Rect, live: [bool; 2], translate: bool) -> Rect {
    let span = |axis_live: bool, own: (f32, f32), child: (f32, f32)| {
        if axis_live || !translate { own } else { child }
    };
    let (x, w) = span(live[0], (viewport.x, viewport.w), (child.x, child.w));
    let (y, h) = span(live[1], (viewport.y, viewport.h), (child.y, child.h));
    Rect::new(x, y, w, h)
}

fn clean(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 { v } else { 0.0 }
}

impl Ui<'_> {
    /// A scroll area under `key` over `rect`. `content` draws in content coordinates (the area
    /// translates it by the negative offset, clipped to `rect`) and returns its value and its
    /// [`Extent`]; with [`ScrollOptions::extent`] given, the returned extent is ignored.
    pub fn scroll_area<R>(
        &mut self,
        key: impl Hash,
        rect: Rect,
        opts: &ScrollOptions,
        content: impl FnOnce(&mut Self, &ScrollView) -> (R, Extent),
    ) -> ScrollResponse<R> {
        self.scroll_area_with(key, rect, opts, true, content)
    }

    /// [`Self::scroll_area`] without the translation: the content places itself from
    /// [`ScrollView::offset`] (a virtual list subtracts in `f64` and casts the small result, which
    /// an `f32` translation could not do at a million rows). The clip, the input, the thumb and the
    /// node are the same. A keyboard focus move to a child that shows only in part brings it into
    /// view by the least movement, as in a translated area; a child placed over the whole viewport
    /// (a list's own region) has nothing to reveal, and a scroll-into-view stays the content's
    /// business.
    pub fn scroll_area_untranslated<R>(
        &mut self,
        key: impl Hash,
        rect: Rect,
        opts: &ScrollOptions,
        content: impl FnOnce(&mut Self, &ScrollView) -> (R, Extent),
    ) -> ScrollResponse<R> {
        self.scroll_area_with(key, rect, opts, false, content)
    }

    /// Asks the scroll area under `key` (of the current scope) to bring `target`, a box in its
    /// content coordinates, into view. Applied when the area is next called (this frame if it
    /// comes later, or from inside its content). A target outside the content changes nothing.
    ///
    /// Made after the area in the frame, it asks for the frame that applies it, unless the target
    /// already shows, so an app may call this every frame and still settle to `Repaint::Idle`.
    pub fn scroll_to(&mut self, key: impl Hash, target: Target, how: Reveal) {
        let id = self.id(key);
        let frame = self.frame_number();
        let requests = self.transient::<Requests>(id);
        requests.reveals.push((target, how));
        let late = requests
            .ran
            .is_some_and(|r| r.frame == frame && r.moved_by(&target, how));
        if late {
            self.request_catch_up();
        }
    }

    /// Asks the area under `key` (of the current scope) to move its vertical offset by `by` when
    /// it is next called, before anything else: the content above the viewport changed height by
    /// that much, so what is on screen stays put. Any animation keeps its course.
    pub(crate) fn scroll_shift(&mut self, key: impl Hash, by: f64) {
        let id = self.id(key);
        self.transient::<Requests>(id).shift_y += by;
    }

    /// The vertical shift [`Self::scroll_shift`] has queued for the area under `key` and the area
    /// has not taken yet. A pane of a scroll link adds it to the offset it drew, since the area
    /// moves by it first: asked of the group's value without it, a pane that measured the same
    /// rows as the pane that published would move twice (D8).
    pub(crate) fn scroll_pending_shift(&mut self, key: impl Hash) -> f64 {
        let id = self.id(key);
        self.transient::<Requests>(id).shift_y
    }

    /// Drops the shifts queued for the area under `key`. A host calls it on a new identity, before
    /// it queues anything of its own: what the last frame left there measured the last document.
    pub(crate) fn scroll_forget_shift(&mut self, key: impl Hash) {
        let id = self.id(key);
        let requests = self.transient::<Requests>(id);
        requests.shift_x = 0.0;
        requests.shift_y = 0.0;
    }

    /// [`Self::scroll_shift`] on the horizontal axis: an edit moves its caret into view at once,
    /// since typing and caret motion never animate.
    pub(crate) fn scroll_shift_x(&mut self, key: impl Hash, by: f64) {
        let id = self.id(key);
        self.transient::<Requests>(id).shift_x += by;
    }

    /// What the area under `id` showed past its ends when its last call ended, as the area stores
    /// it: the content is drawn translated by the negative of it, so a pull past the top is
    /// negative on y. `(0, 0)` for an area that has not run or sits in range. The pull to
    /// refresh reads it (D375).
    pub(crate) fn scroll_overscroll(&mut self, id: WidgetId) -> (f32, f32) {
        let over = self.persistent::<ScrollState>(id).over;
        (over[0], over[1])
    }

    /// The state of the scroll link `group`, with the asking pane counted among the frame's.
    fn link_drawn(&mut self, group: WidgetId) -> &mut LinkState {
        let frame = self.frame_number();
        let link = self.persistent::<LinkState>(group);
        if link.frame != frame {
            link.frame = frame;
            link.drawn = 0;
        }
        link.drawn += 1;
        link
    }

    /// A pane of a scroll link (D8) asks what to apply before its area runs: the shift that takes
    /// `own_offset`, where the last frame left it, to the group's value. Zero when the group has
    /// no value yet, when the pane already took that value (a pane that clamps does not ask every
    /// frame), or when they are equal. A `first` draw (the pane's state is new: its first frame, or
    /// the first after it was not drawn and its state swept) ignores what it took before, since
    /// `taken` lives under the group and outlasts the pane: it joins where the group stands. The
    /// pane is the current scope.
    pub(crate) fn scroll_link_pull(
        &mut self,
        group: WidgetId,
        own_offset: f64,
        first: bool,
    ) -> f64 {
        let pane = self.scope_id();
        let link = self.link_drawn(group);
        let Some(value) = link.value else {
            return 0.0;
        };
        if !first && link.taken.get(&pane) == Some(&value) {
            return 0.0;
        }
        link.taken.insert(pane, value);
        if value.is_finite() {
            let by = value - own_offset;
            if by != 0.0 {
                // The pane's area stops any fling or bounce before the pull applies (D350).
                self.mark_link_pulled(pane);
            }
            by
        } else {
            0.0
        }
    }

    /// A pane that opened another document asks this in place of [`Self::scroll_link_pull`]. It
    /// takes nothing, since the group's value measured the last document, and that value goes,
    /// wherever the pane stood: the group starts over at the top with it (D203). A pane that drew
    /// the old value before this one asks a catch-up frame. A first draw opens nothing; such a
    /// pane pulls, as any pane that joins.
    pub(crate) fn scroll_link_open(&mut self, group: WidgetId) {
        let link = self.link_drawn(group);
        let left = link.value.replace(0.0);
        let stale = link.drawn > 1 && left.is_some_and(|v| v != 0.0);
        if stale {
            self.request_catch_up();
        }
    }

    /// A pane tells the group where it ended: `before` is what it drew last frame, `after` what it
    /// is at now. Only a move publishes; equal values do nothing, so a pane that took the group's
    /// value does not echo it. A pane that moved after another one already drew this frame asks a
    /// catch-up frame, since that one drew the old value.
    pub(crate) fn scroll_link_push(&mut self, group: WidgetId, before: f64, after: f64) {
        if before == after {
            return;
        }
        let pane = self.scope_id();
        let link = self.persistent::<LinkState>(group);
        link.value = Some(after);
        link.taken.insert(pane, after);
        let stale = link.drawn > 1;
        if stale {
            self.request_catch_up();
        }
    }

    fn scroll_area_with<R>(
        &mut self,
        key: impl Hash,
        rect: Rect,
        opts: &ScrollOptions,
        translate: bool,
        content: impl FnOnce(&mut Self, &ScrollView) -> (R, Extent),
    ) -> ScrollResponse<R> {
        let id = self.id(key);
        let now = self.time();
        let physics = self.scroll_physics();
        let live = [opts.bars.0 != Bars::Disabled, opts.bars.1 != Bars::Disabled];
        // The part of the area an on-screen keyboard covers adds to its y range (D365): the
        // viewport the scroll arithmetic uses is the uncovered part, and the app's `ScrollView`
        // keeps the whole rect. No keyboard: `cut` is 0 and every value is as before.
        let inset = self.keyboard_inset().bottom;
        let cut = keyboard_cut(self, rect, inset);
        let vp = [f64::from(rect.w), f64::from(rect.h - cut)];
        let own = self.registration_mark();
        let pulled = self.take_link_pulled(self.scope_id());
        let resp = self.interact(
            id,
            rect,
            if live[0] || live[1] {
                Sense::SCROLL
            } else {
                Sense::default()
            },
        );
        // A sideways-wheel strip reads the whole delta as its own axis (`sideways`), so it takes
        // both; any other area takes an axis only where it is live (D178).
        self.set_scroll_axes(if opts.wheel_x {
            (live[0], live[0])
        } else {
            (live[0], live[1])
        });
        let mut st = std::mem::take(self.persistent::<ScrollState>(id));
        // A finger pans the axes that have content past the viewport, as the last frame measured
        // (D349); a sideways-wheel strip's wheel axes are not widened into pan axes.
        self.set_pan_axes((
            live[0] && st.extent[0] > vp[0],
            live[1] && st.extent[1] > vp[1],
        ));
        let Requests {
            mut reveals,
            shift_x,
            shift_y,
            ran: _,
        } = std::mem::take(self.transient::<Requests>(id));

        // Identity: a new key starts at the top; the same key keeps `wanted` through short frames.
        if st.rendered && opts.identity != st.key {
            st = ScrollState::default();
        }
        let preserve = should_restore(st.rendered, st.key, opts.identity);
        st.key = opts.identity;
        st.rendered = true;
        let known = opts.extent.map(|(w, h)| [clean(w), clean(h)]);
        let ext_pre = known.unwrap_or(st.extent);
        if pulled {
            for ax in 0..2 {
                st.a[ax].halt(now, ext_pre[ax], vp[ax], &physics);
            }
        }
        st.a[0].shift(shift_x);
        st.a[1].shift(shift_y);
        let wheel = self.theme().motion.resolve(&WHEEL, self.tier());
        let into_view = self.theme().motion.resolve(&INTO_VIEW, self.tier());

        // Input: wanted offsets are clamped here, against the extent last known (a wheel step at
        // the end must not leave `wanted` past it).
        if resp.enabled {
            let gesture = |d: (f32, f32)| if opts.wheel_x { sideways(d) } else { d };
            let (scroll_lines, scroll_pixels) =
                (gesture(resp.scroll_lines), gesture(resp.scroll_pixels));
            for ax in 0..2 {
                if !live[ax] {
                    continue;
                }
                let lines = [scroll_lines.0, scroll_lines.1][ax];
                let pixels = [scroll_pixels.0, scroll_pixels.1][ax];
                let a = &mut st.a[ax];
                if (lines.is_finite() && lines != 0.0) || (pixels.is_finite() && pixels != 0.0) {
                    a.halt(now, ext_pre[ax], vp[ax], &physics);
                }
                let mut cur = clamp_offset(a.wanted, ext_pre[ax], vp[ax]);
                if lines.is_finite() && lines != 0.0 {
                    cur = clamp_offset(cur - f64::from(lines) * LINE, ext_pre[ax], vp[ax]);
                    a.step(now, cur, wheel);
                }
                if pixels.is_finite() && pixels != 0.0 {
                    cur = clamp_offset(cur - f64::from(pixels), ext_pre[ax], vp[ax]);
                    a.jump(cur);
                }
            }
            let pax = primary_axis(&live, &ext_pre, &vp);
            for (nk, dir) in [(NamedKey::ArrowUp, 1.0), (NamedKey::ArrowDown, -1.0)] {
                if live[pax] && self.consume_synthetic_key(id, nk) {
                    let a = &mut st.a[pax];
                    a.halt(now, ext_pre[pax], vp[pax], &physics);
                    let cur = clamp_offset(a.wanted, ext_pre[pax], vp[pax]);
                    let to = clamp_offset(cur + dir * LINE, ext_pre[pax], vp[pax]);
                    a.step(now, to, wheel);
                }
            }
        }
        // A finger: a touch that landed on a motion in flight stops it where it was drawn, a pan
        // moves the axes it locked, and a lift hands them on to a fling or a bounce (D350).
        let pan = self.pan(id);
        if pan.caught {
            for ax in 0..2 {
                let (drawn, over) = (st.drawn[ax], st.over[ax]);
                st.a[ax].catch(now, drawn, over, ext_pre[ax], vp[ax]);
            }
        }
        let finger = self.release_velocity(id);
        for ax in 0..2 {
            let a = &mut st.a[ax];
            let d = [pan.delta.0, pan.delta.1][ax];
            if live[ax] && d.is_finite() && d != 0.0 {
                a.pan(now, ext_pre[ax], vp[ax], d, &physics);
            }
            if matches!(a.touch, Touch::Pan { .. }) && !pan.active {
                // The finger offset's direction is the opposite of the content's.
                let v = if pan.ended {
                    -[finger.0, finger.1][ax]
                } else {
                    0.0
                };
                a.lift(now, ext_pre[ax], vp[ax], v, &physics);
            }
            a.advance(now, ext_pre[ax], vp[ax]);
        }
        for a in &mut st.a {
            a.settle(now);
        }
        // A preserved document keeps `wanted` through a short frame, but not through a vertical
        // shift: that is the host placing the area, and a `wanted` past the end would hold it
        // there when the next shift brings it back (D203).
        let keep = [preserve, preserve && shift_y == 0.0];
        if let Some(k) = known {
            for ax in 0..2 {
                let c = clamp_offset(st.a[ax].wanted, k[ax], vp[ax]);
                if !keep[ax] && c != st.a[ax].wanted {
                    st.a[ax].clamp_to(c);
                }
            }
        }
        let look = |st: &ScrollState, ext: &[f64; 2]| -> [Sample; 2] {
            [0, 1].map(|ax| st.a[ax].sample(now, ext[ax], vp[ax], &physics))
        };
        let shown =
            |st: &ScrollState, ext: &[f64; 2]| -> [f64; 2] { look(st, ext).map(|s| s.offset) };
        let pre = shown(&st, &ext_pre);
        let over_pre = look(&st, &ext_pre).map(|s| s.over);

        // The content, in the clip, translated by the negative offset.
        let view = ScrollView {
            viewport: rect,
            offset: (pre[0], pre[1]),
        };
        let watch_mark = self.watch_mark();
        let reg_mark = self.registration_mark();
        let mut node = Node::new(id, Role::ScrollArea, "", rect);
        if !resp.enabled {
            node.states.insert(State::Disabled);
        }
        let (value, measured) = self.node_container(node, |ui| {
            ui.painter().push_clip(rect);
            // The overscroll: the content, hit regions included, drawn past its end (D350).
            let stretched = over_pre != [0.0, 0.0];
            if stretched {
                ui.painter().push_translate(-over_pre[0], -over_pre[1]);
            }
            if translate {
                ui.painter()
                    .push_translate(-(pre[0] as f32), -(pre[1] as f32));
            }
            let out = content(ui, &view);
            if translate {
                ui.painter().pop_translate();
            }
            if stretched {
                ui.painter().pop_translate();
            }
            ui.painter().pop_clip();
            out
        });
        let focusable_child = self.focusable_since(reg_mark);
        let watched = self.watched_since(watch_mark);
        // Requests the content made: nothing else would run them on a quiet frame.
        reveals.append(&mut self.transient::<Requests>(id).reveals);

        // The measured extent: follow-tail, then the clamp (a preserved document keeps `wanted`).
        let ext = known.unwrap_or([clean(measured.width), clean(measured.height)]);
        if opts.follow_tail && live[1] && ext[1] > st.extent[1] {
            st.a[1].jump(max_offset(ext[1], vp[1]));
        }
        st.extent = ext;
        if !preserve {
            for ax in 0..2 {
                let c = clamp_offset(st.a[ax].wanted, ext[ax], vp[ax]);
                if c != st.a[ax].wanted {
                    st.a[ax].clamp_to(c);
                }
            }
        }

        // Reveals: the app's requests, an accessibility scroll-into-view on a child (centred) and
        // a keyboard focus move to a child (the least movement).
        let (focused, ring) = self.focus_state();
        let inside = focused.and_then(|f| watched.iter().rev().find(|(w, _)| *w == f).copied());
        // With a keyboard up, a child that gained focus (a touch focus has no ring) or a keyboard
        // that changed its height reveals the caret, in the part the keyboard leaves (D365).
        let kb_up = inset > 0.0;
        let kb_reveal = kb_up
            && inside.is_some_and(|(f, _)| st.last_focus != Some(f) || st.last_inset != inset);
        let visible = Rect::new(rect.x, rect.y, rect.w, rect.h - cut);
        if translate {
            for (w, r) in &watched {
                if self.is_scroll_target(*w) {
                    reveals.push((Target::rect(content_rect(*r, rect)), Reveal::Center));
                }
            }
            if let Some((f, r)) = inside
                && ((ring && st.last_focus != Some(f)) || kb_reveal)
            {
                let r = if kb_up && f64::from(r.h) > vp[1] {
                    let shown = Rect::new(
                        r.x - (pre[0] + f64::from(over_pre[0])) as f32,
                        r.y - (pre[1] + f64::from(over_pre[1])) as f32,
                        r.w,
                        r.h,
                    );
                    caret_in(self, r, shown)
                } else {
                    r
                };
                reveals.push((Target::rect(content_rect(r, rect)), Reveal::Nearest));
            }
        } else if let Some((f, r)) = inside
            && ((ring && st.last_focus != Some(f)) || kb_reveal)
            && (kb_up || !within(r, rect))
        {
            let r = if kb_up && f64::from(r.h) > vp[1] {
                caret_in(self, r, r)
            } else {
                r
            };
            if !kb_up || !within(r, visible) {
                // Drawn at the offset the content was given, in the viewport's own space.
                let at = |start: f32, origin: f32, ax: usize| f64::from(start - origin) + pre[ax];
                reveals.push((
                    Target {
                        x: Some((at(r.x, rect.x, 0), f64::from(r.w))),
                        y: Some((at(r.y, rect.y, 1), f64::from(r.h))),
                    },
                    Reveal::Nearest,
                ));
            }
        }
        st.last_inset = inset;
        st.last_focus = inside.map(|(f, _)| f);
        for (target, how) in reveals {
            for ax in 0..2 {
                let a = &mut st.a[ax];
                let span = [target.x, target.y][ax];
                if span.is_none() || !live[ax] {
                    continue;
                }
                // A motion in flight stops where it is drawn first: `wanted` is its landing, and
                // the reveal is judged against what shows.
                a.halt(now, ext[ax], vp[ax], &physics);
                let cur = clamp_offset(a.wanted, ext[ax], vp[ax]);
                let Some(to) = reveal_axis(span, cur, ext[ax], vp[ax], how) else {
                    continue;
                };
                if (to - cur).abs() > SNAP_DISTANCE * vp[ax] {
                    a.jump(to);
                } else {
                    a.step(now, to, into_view);
                }
            }
        }

        // Page keys and Home and End, when the focused child inside the area declined them;
        // while it composes they are the input method's.
        // A focused child in a disabled scope is not watched (it registered inert), but its keys
        // are still the area's to take: they are consumed and nothing scrolls.
        let keyed = inside.map(|(f, _)| (f, true)).or_else(|| {
            focused
                .filter(|f| self.disabled_registered_since(reg_mark, *f))
                .map(|f| (f, false))
        });
        if let Some((f, acts)) = keyed.filter(|_| !self.composing()) {
            let pax = primary_axis(&live, &ext, &vp);
            if live[pax] && ext[pax] > vp[pax] {
                let mut at_start = false;
                for (index, ev) in self.focus_events(f) {
                    let FocusEvent::Key {
                        key: Key::Named(k),
                        modifiers,
                        ..
                    } = ev
                    else {
                        continue;
                    };
                    if modifiers.control || modifiers.alt || modifiers.logo {
                        continue;
                    }
                    let a = &mut st.a[pax];
                    if acts
                        && matches!(
                            k,
                            NamedKey::PageUp | NamedKey::PageDown | NamedKey::Home | NamedKey::End
                        )
                    {
                        a.halt(now, ext[pax], vp[pax], &physics);
                    }
                    let cur = clamp_offset(a.wanted, ext[pax], vp[pax]);
                    let to = match k {
                        NamedKey::PageUp => cur - vp[pax],
                        NamedKey::PageDown => cur + vp[pax],
                        NamedKey::Home => 0.0,
                        NamedKey::End => max_offset(ext[pax], vp[pax]),
                        _ => continue,
                    };
                    if acts {
                        a.jump(clamp_offset(to, ext[pax], vp[pax]));
                        at_start = k == NamedKey::Home;
                    }
                    self.consume_focus_event(index);
                }
                // A shift the content queued in this frame is what grew above the view it drew.
                // Home lands where nothing is above, so it takes none: it rests at the start,
                // whatever that frame measured. Every other key keeps it, as the place it moved
                // from moved by as much.
                if at_start {
                    let requests = self.transient::<Requests>(id);
                    if pax == 1 {
                        requests.shift_y = 0.0;
                    } else {
                        requests.shift_x = 0.0;
                    }
                }
            }
        }

        // The overlay bars, hit after the content so a press on a thumb reaches no child.
        let now_off = shown(&st, &ext);
        let bar = [
            opts.bars.0 == Bars::Auto && live[0],
            opts.bars.1 == Bars::Auto && live[1],
        ];
        let room = opts.bar_room;
        let has_y = bar[1]
            && bar_geometry(Axis::Vertical, visible, now_off[1], ext[1], false, room.1).is_some();
        let has_x = bar[0]
            && bar_geometry(Axis::Horizontal, visible, now_off[0], ext[0], false, room.0).is_some();
        let reveal = self.pointer_in(rect);
        if has_y
            && let Some(g) =
                bar_geometry(Axis::Vertical, visible, now_off[1], ext[1], has_x, room.1)
        {
            let drag = self.scrollbar(id.with("bar-y"), Axis::Vertical, &g, reveal);
            if drag != 0.0 {
                st.a[1].halt(now, ext[1], vp[1], &physics);
                let cur = clamp_offset(st.a[1].wanted, ext[1], vp[1]);
                st.a[1].jump(clamp_offset(cur + g.offset_delta(drag), ext[1], vp[1]));
            }
        }
        if has_x
            && let Some(g) =
                bar_geometry(Axis::Horizontal, visible, now_off[0], ext[0], has_y, room.0)
        {
            let drag = self.scrollbar(id.with("bar-x"), Axis::Horizontal, &g, reveal);
            if drag != 0.0 {
                st.a[0].halt(now, ext[0], vp[0], &physics);
                let cur = clamp_offset(st.a[0].wanted, ext[0], vp[0]);
                st.a[0].jump(clamp_offset(cur + g.offset_delta(drag), ext[0], vp[0]));
            }
        }

        // Settled or moving; a change after the content drew needs a catch-up frame.
        let end = look(&st, &ext);
        let moving = end.iter().any(|s| s.moving);
        if moving {
            self.mark_moving();
        }
        // Hit regions are recorded as the area's call began, so the flag is set on its own region.
        self.set_scroll_moving(own, moving);
        let fin = end.map(|s| s.offset);
        let over_fin = end.map(|s| s.over);
        if fin != pre || over_fin != over_pre {
            self.request_catch_up();
        }
        st.drawn = fin;
        st.over = over_fin;

        let nax = usize::from(ext[1] > vp[1] || ext[0] <= vp[0]);
        let max = max_offset(ext[nax], vp[nax]);
        let scrollable = resp.enabled && (0..2).any(|ax| live[ax] && ext[ax] > vp[ax]);
        let off = fin[nax];
        self.patch_node(id, |n| {
            n.value = Some(Value::Number {
                value: off,
                min: 0.0,
                max,
                step: LINE,
            });
            if scrollable {
                n.actions.insert(Action::Increment);
                n.actions.insert(Action::Decrement);
            }
            if focusable_child {
                n.states.insert(State::Focusable);
            }
        });
        // A nested area hands the widgets it dealt with to the outer one as its own viewport.
        let keep: Vec<(WidgetId, Rect)> = watched
            .iter()
            .map(|(w, r)| (*w, handed_on(*r, rect, live, translate)))
            .collect();
        self.watch_replace(watch_mark, keep);

        let ran = Ran {
            frame: self.frame_number(),
            live,
            wanted: [st.a[0].wanted, st.a[1].wanted],
            ext,
            vp,
        };
        self.transient::<Requests>(id).ran = Some(ran);
        *self.persistent::<ScrollState>(id) = st;
        ScrollResponse {
            value,
            response: resp,
            offset: (fin[0], fin[1]),
            extent: (ext[0], ext[1]),
            viewport: rect,
        }
    }
}

/// [`thumb_geometry`] with the track `room` px short at its far end: the bar of a viewport `room`
/// shorter over content `room` shorter, so the range, and so where an offset puts the thumb, is
/// the same, and at the far end the thumb ends where the track does.
fn bar_geometry(
    axis: Axis,
    rect: Rect,
    offset: f64,
    extent: f64,
    corner: bool,
    room: f32,
) -> Option<ThumbGeometry> {
    let room = if room.is_finite() { room.max(0.0) } else { 0.0 };
    if room == 0.0 {
        return thumb_geometry(axis, rect, offset, extent, corner);
    }
    let short = match axis {
        Axis::Horizontal => Rect::new(rect.x, rect.y, rect.w - room, rect.h),
        Axis::Vertical => Rect::new(rect.x, rect.y, rect.w, rect.h - room),
    };
    thumb_geometry(axis, short, offset, extent - f64::from(room), corner)
}

/// A gesture `d` turned onto the horizontal axis: its larger component, so a plain wheel scrolls
/// sideways and a trackpad's small vertical drift does not fight a horizontal swipe.
fn sideways(d: (f32, f32)) -> (f32, f32) {
    (if d.1.abs() > d.0.abs() { d.1 } else { d.0 }, 0.0)
}

/// The axis keys and the node's actions scroll: the vertical one unless only the horizontal one
/// overflows.
fn primary_axis(live: &[bool; 2], ext: &[f64; 2], vp: &[f64; 2]) -> usize {
    usize::from(!(live[0] && ext[0] > vp[0] && !(live[1] && ext[1] > vp[1])))
}
