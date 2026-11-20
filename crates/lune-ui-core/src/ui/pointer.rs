//! The pointer state machine: presses, capture, clicks, double clicks, drags and scroll, resolved
//! against the previous frame's layered hit list (D4).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::input::{Modifiers, PointerButton, PointerKind, ScrollDelta};
use crate::motion::Time;
use crate::paint::Rect;

use super::{Sense, WidgetId};

/// Travel from a mouse press point at which a drag starts.
pub(crate) const DRAG_THRESHOLD: f32 = 4.0;
/// Travel from a touch press point at which a drag starts: a finger wobbles more than a mouse (D348).
pub(crate) const TOUCH_SLOP: f32 = 10.0;
/// How long a touch press holds, within the slop, before it is a long press (D348).
pub(crate) const LONG_PRESS: Duration = Duration::from_millis(500);
/// The release velocity is fitted over the samples this close before the release.
const VELOCITY_WINDOW: Duration = Duration::from_millis(100);
/// A finger that has not moved this long before the release leaves with no velocity.
const VELOCITY_STILL: Duration = Duration::from_millis(40);
/// The largest release velocity, in logical px/s.
const VELOCITY_MAX: f32 = 8000.0;
/// Two clicks on one id closer than this (frame time) are a double click.
pub(crate) const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// One hit region of a finished frame, in draw order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HitRecord {
    pub(crate) id: WidgetId,
    pub(crate) rect: Rect,
    pub(crate) layer: usize,
    pub(crate) sense: Sense,
    pub(crate) tag: u32,
    pub(crate) enabled: bool,
    /// The axes a scroll region is live on, `(x, y)`: a wheel delta on an axis goes only to a region
    /// live on it (D178). Both for every other region.
    pub(crate) scroll_axes: (bool, bool),
    /// The [`Ui::tab_scope`](super::Ui::tab_scope) order the region was registered in: 0 outside
    /// any scope.
    pub(crate) order: i32,
}

/// The topmost region of `hits` at `pos` that `eligible` admits: highest layer, then the overlay opened last (tag 0, the base, is below every overlay of a
/// layer), then latest call.
pub(crate) fn topmost<'h>(
    hits: &'h [HitRecord],
    pos: (f32, f32),
    eligible: &dyn Fn(&HitRecord) -> bool,
) -> Option<&'h HitRecord> {
    hits.iter()
        .enumerate()
        .filter(|(_, h)| eligible(h) && h.rect.contains(pos))
        .max_by_key(|(i, h)| (h.layer, h.tag, *i))
        .map(|(_, h)| h)
}

fn senses_capture(h: &HitRecord) -> bool {
    h.enabled && (h.sense.click || h.sense.drag)
}

fn senses_drag(hits: &[HitRecord], id: WidgetId) -> bool {
    hits.iter().any(|h| h.id == id && h.sense.drag && h.enabled)
}

/// A primary press a region captured this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PressInfo {
    pub(crate) pos: (f32, f32),
    pub(crate) count: u8,
    pub(crate) modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Press {
    /// The captured id; `None` when the press landed on nothing that captures.
    id: Option<WidgetId>,
    at: (f32, f32),
    last: (f32, f32),
    dragging: bool,
    /// What made the press: a touch has the larger slop and a long press.
    kind: PointerKind,
    /// The frame time the press began at.
    t0: Time,
    /// The press travelled the slop from `at` at some point.
    passed: bool,
    /// The long press fired: the release clicks nothing.
    spent: bool,
}

impl Press {
    fn new(id: Option<WidgetId>, pos: (f32, f32), kind: PointerKind, t0: Time) -> Self {
        Self {
            id,
            at: pos,
            last: pos,
            dragging: false,
            kind,
            t0,
            passed: false,
            spent: false,
        }
    }

    /// Whether a long press is still to fire: a touch that holds a widget, has neither dragged nor
    /// left the slop, and has not fired yet.
    fn long_pending(&self) -> bool {
        self.kind == PointerKind::Touch
            && self.id.is_some()
            && !self.dragging
            && !self.passed
            && !self.spent
    }

    fn slop(&self) -> f32 {
        if self.kind == PointerKind::Touch {
            TOUCH_SLOP
        } else {
            DRAG_THRESHOLD
        }
    }
}

/// The primary press's positions by frame, for the release velocity (D348).
#[derive(Debug, Clone, Default, PartialEq)]
struct Tracker {
    samples: Vec<(Time, (f32, f32))>,
}

impl Tracker {
    fn clear(&mut self) {
        self.samples.clear();
    }

    /// Records `pos` at `time`; a second sample of one frame replaces the first. Samples older
    /// than the window before `time` go.
    fn sample(&mut self, time: Time, pos: (f32, f32)) {
        match self.samples.last_mut() {
            Some(last) if last.0 == time => last.1 = pos,
            _ => self.samples.push((time, pos)),
        }
        self.samples
            .retain(|s| time.saturating_sub(s.0) <= VELOCITY_WINDOW);
    }

    /// The least-squares slope of each axis over the samples, in px/s, clamped in magnitude.
    /// Zero with fewer than two samples, with no time across them, or when the finger has not
    /// moved from its last position for the still window.
    fn velocity(&self) -> (f32, f32) {
        let Some(&(end, rest)) = self.samples.last() else {
            return (0.0, 0.0);
        };
        if self.samples.len() < 2
            || self
                .samples
                .iter()
                .filter(|s| end.saturating_sub(s.0) <= VELOCITY_STILL)
                .all(|s| s.1 == rest)
        {
            return (0.0, 0.0);
        }
        let t0 = self.samples[0].0;
        let n = self.samples.len() as f64;
        let ts: Vec<f64> = self
            .samples
            .iter()
            .map(|s| s.0.saturating_sub(t0).as_secs_f64())
            .collect();
        let tm = ts.iter().sum::<f64>() / n;
        let var: f64 = ts.iter().map(|t| (t - tm) * (t - tm)).sum();
        if var <= 0.0 {
            return (0.0, 0.0);
        }
        let slope = |axis: fn(&(f32, f32)) -> f32| {
            let vm = self
                .samples
                .iter()
                .map(|s| f64::from(axis(&s.1)))
                .sum::<f64>()
                / n;
            ts.iter()
                .zip(&self.samples)
                .map(|(t, s)| (t - tm) * (f64::from(axis(&s.1)) - vm))
                .sum::<f64>()
                / var
        };
        let (vx, vy) = (slope(|p| p.0) as f32, slope(|p| p.1) as f32);
        let mag = vx.hypot(vy);
        if mag > VELOCITY_MAX {
            let k = VELOCITY_MAX / mag;
            (vx * k, vy * k)
        } else {
            (vx, vy)
        }
    }
}

/// Pointer state that outlives a frame (held in the store).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PointerState {
    pos: Option<(f32, f32)>,
    press: Option<Press>,
    tracker: Tracker,
    /// The id a secondary press landed on, until its release.
    secondary: Option<Option<WidgetId>>,
    last_click: Option<(WidgetId, Time)>,
    /// The id, time and count of the last captured primary press, for `press_count`.
    last_press: Option<(WidgetId, Time, u8)>,
}

/// One frame's pointer reading.
#[derive(Debug, Clone, Default)]
pub(crate) struct PointerFrame {
    pub(crate) hovered: Option<WidgetId>,
    pub(crate) pressed: Option<WidgetId>,
    pub(crate) dragging: Option<WidgetId>,
    pub(crate) clicked: BTreeSet<WidgetId>,
    pub(crate) double_clicked: BTreeSet<WidgetId>,
    pub(crate) secondary_clicked: BTreeSet<WidgetId>,
    pub(crate) drag_started: BTreeSet<WidgetId>,
    pub(crate) drag_stopped: BTreeSet<WidgetId>,
    pub(crate) press_started: BTreeMap<WidgetId, PressInfo>,
    pub(crate) drag_delta: BTreeMap<WidgetId, (f32, f32)>,
    /// The velocity a dragged press left with, on the frame its `drag_stopped` is set.
    pub(crate) release_velocity: BTreeMap<WidgetId, (f32, f32)>,
    pub(crate) scroll_lines: BTreeMap<WidgetId, (f32, f32)>,
    pub(crate) scroll_pixels: BTreeMap<WidgetId, (f32, f32)>,
}

impl PointerFrame {
    /// Whether the frame's pointer input resolved to something a widget reacts to after it drew:
    /// a click of any kind, a drag start, movement or stop, or a scroll.
    pub(crate) fn acted(&self) -> bool {
        !(self.clicked.is_empty()
            && self.double_clicked.is_empty()
            && self.secondary_clicked.is_empty()
            && self.drag_started.is_empty()
            && self.drag_stopped.is_empty()
            && self.drag_delta.is_empty()
            && self.scroll_lines.is_empty()
            && self.scroll_pixels.is_empty())
    }
}

fn add(m: &mut BTreeMap<WidgetId, (f32, f32)>, id: WidgetId, d: (f32, f32)) {
    let e = m.entry(id).or_insert((0.0, 0.0));
    e.0 += d.0;
    e.1 += d.1;
}

/// What the events of one frame resolve against.
pub(crate) struct Ctx<'c> {
    pub(crate) hits: &'c [HitRecord],
    pub(crate) eligible: &'c dyn Fn(&HitRecord) -> bool,
    pub(crate) time: Time,
    /// What made the press this event may begin: set per event by the loop.
    pub(crate) kind: PointerKind,
}

impl PointerState {
    /// The pointer moved to `pos`.
    pub(crate) fn moved(&mut self, pos: (f32, f32), cx: &Ctx<'_>, out: &mut PointerFrame) {
        self.pos = Some(pos);
        if self.press.is_some() {
            self.tracker.sample(cx.time, pos);
        }
        let Some(p) = &mut self.press else { return };
        if let Some(id) = p.id {
            if p.dragging {
                add(
                    &mut out.drag_delta,
                    id,
                    (pos.0 - p.last.0, pos.1 - p.last.1),
                );
            } else {
                let (dx, dy) = (pos.0 - p.at.0, pos.1 - p.at.1);
                if (dx * dx + dy * dy).sqrt() >= p.slop() {
                    p.passed = true;
                    if senses_drag(cx.hits, id) {
                        p.dragging = true;
                        out.drag_started.insert(id);
                        add(&mut out.drag_delta, id, (dx, dy));
                    }
                }
            }
        }
        p.last = pos;
    }

    /// The pointer's last known position; `None` outside the window.
    pub(crate) fn pos(&self) -> Option<(f32, f32)> {
        self.pos
    }

    /// The pointer left the window.
    pub(crate) fn left(&mut self) {
        self.pos = None;
    }

    /// The topmost eligible hit at `pos`.
    pub(crate) fn target<'h>(&self, pos: (f32, f32), cx: &Ctx<'h>) -> Option<&'h HitRecord> {
        topmost(cx.hits, pos, cx.eligible)
    }

    /// A button changed state at `pos`. Returns the id a primary press captured, if any.
    pub(crate) fn button(
        &mut self,
        pos: (f32, f32),
        button: PointerButton,
        pressed: bool,
        modifiers: Modifiers,
        cx: &Ctx<'_>,
        out: &mut PointerFrame,
    ) -> Option<WidgetId> {
        self.moved(pos, cx, out);
        let target = self.target(pos, cx);
        match (button, pressed) {
            (PointerButton::Primary, true) => {
                let id = target.filter(|h| senses_capture(h)).map(|h| h.id);
                self.press = Some(Press::new(id, pos, cx.kind, cx.time));
                self.tracker.clear();
                self.tracker.sample(cx.time, pos);
                self.last_press = match id {
                    Some(id) => {
                        let count = match self.last_press {
                            Some((last, t0, n))
                                if last == id && cx.time.saturating_sub(t0) < DOUBLE_CLICK =>
                            {
                                n % 3 + 1
                            }
                            _ => 1,
                        };
                        out.press_started.insert(
                            id,
                            PressInfo {
                                pos,
                                count,
                                modifiers,
                            },
                        );
                        Some((id, cx.time, count))
                    }
                    None => None,
                };
                return id;
            }
            (PointerButton::Primary, false) => {
                let p = self.press.take()?;
                let id = p.id?;
                if p.dragging {
                    out.drag_stopped.insert(id);
                    out.release_velocity.insert(id, self.tracker.velocity());
                    return None;
                }
                if p.spent {
                    return None;
                }
                if target.map(|h| h.id) == Some(id) {
                    out.clicked.insert(id);
                    match self.last_click {
                        Some((last, t0))
                            if last == id && cx.time.saturating_sub(t0) < DOUBLE_CLICK =>
                        {
                            out.double_clicked.insert(id);
                            self.last_click = None;
                        }
                        _ => self.last_click = Some((id, cx.time)),
                    }
                }
            }
            (PointerButton::Secondary, true) => {
                self.secondary = Some(target.filter(|h| senses_capture(h)).map(|h| h.id));
            }
            (PointerButton::Secondary, false) => {
                if let Some(Some(id)) = self.secondary.take()
                    && target.map(|h| h.id) == Some(id)
                {
                    out.secondary_clicked.insert(id);
                }
            }
            _ => {}
        }
        None
    }

    /// A scroll gesture at `pos`: each axis of the delta goes to the topmost scroll region live on
    /// that axis that is not on a layer below the topmost region's, nor in an overlay opened
    /// before the topmost region's on the same layer (D178, F11). A region live on both axes takes
    /// both components, as one live on one takes its own.
    pub(crate) fn scroll(
        &mut self,
        pos: (f32, f32),
        delta: ScrollDelta,
        cx: &Ctx<'_>,
        out: &mut PointerFrame,
    ) {
        self.pos = Some(pos);
        let Some(top) = self.target(pos, cx) else {
            return;
        };
        let (layer, tag) = (top.layer, top.tag);
        let region = |on: fn(&HitRecord) -> bool| {
            topmost(cx.hits, pos, &|h| {
                (cx.eligible)(h)
                    && h.enabled
                    && h.sense.scroll
                    && h.layer >= layer
                    && h.tag >= tag
                    && on(h)
            })
        };
        let (map, x, y) = match delta {
            ScrollDelta::Lines { x, y } => (&mut out.scroll_lines, x, y),
            ScrollDelta::Pixels { x, y } => (&mut out.scroll_pixels, x, y),
        };
        if x != 0.0
            && let Some(to) = region(|h| h.scroll_axes.0)
        {
            add(map, to.id, (x, 0.0));
        }
        if y != 0.0
            && let Some(to) = region(|h| h.scroll_axes.1)
        {
            add(map, to.id, (0.0, y));
        }
    }

    /// Fills hover, capture and drag state after the frame's events.
    pub(crate) fn settle(&self, cx: &Ctx<'_>, out: &mut PointerFrame) {
        let captured = self.press.and_then(|p| p.id);
        out.pressed = captured;
        out.dragging = self.press.filter(|p| p.dragging).and_then(|p| p.id);
        out.hovered = match (captured, self.pos) {
            (_, None) => None,
            (Some(id), Some(pos)) => self.target(pos, cx).filter(|h| h.id == id).map(|h| h.id),
            (None, Some(pos)) => self
                .target(pos, cx)
                .filter(|h| h.enabled && h.sense.hover)
                .map(|h| h.id),
        };
    }

    /// Drops a primary press without capturing (an outside press that dismissed an overlay).
    pub(crate) fn press_consumed(&mut self, pos: (f32, f32)) {
        self.pos = Some(pos);
        self.press = Some(Press::new(None, pos, PointerKind::Mouse, Time::ZERO));
        self.tracker.clear();
    }

    /// Drops every press in flight with no click, no drag stop and no secondary click (a touch
    /// that was cancelled, or a touch that takes over from a mouse press).
    pub(crate) fn cancel(&mut self) {
        self.press = None;
        self.secondary = None;
        self.tracker.clear();
    }

    /// Fires the long press that came due by `cx.time`, before the frame's events: the press is
    /// spent, and its widget reads `secondary_clicked` once (D348).
    pub(crate) fn tick(&mut self, cx: &Ctx<'_>, out: &mut PointerFrame) {
        if let Some(p) = &mut self.press
            && p.long_pending()
            && cx.time >= p.t0 + LONG_PRESS
            && let Some(id) = p.id
        {
            p.spent = true;
            out.secondary_clicked.insert(id);
        }
    }

    /// The time a long press still to fire comes due at.
    pub(crate) fn long_press_due(&self) -> Option<Time> {
        self.press
            .filter(Press::long_pending)
            .map(|p| p.t0 + LONG_PRESS)
    }

    /// Whether a press holds a widget's capture now, at this point in the frame's event batch.
    pub(crate) fn captured(&self) -> bool {
        self.held().is_some()
    }

    /// The widget a press holds the capture of now, at this point in the frame's event batch.
    pub(crate) fn held(&self) -> Option<WidgetId> {
        self.press.and_then(|p| p.id)
    }

    /// Releases a capture whose id `lost` says can no longer hold it (it fell under a modal).
    pub(crate) fn drop_capture_if(&mut self, lost: impl Fn(WidgetId) -> bool) {
        if let Some(p) = &mut self.press
            && p.id.is_some_and(&lost)
        {
            p.id = None;
            p.dragging = false;
        }
    }
}
