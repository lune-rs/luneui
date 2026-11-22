//! Swipe row actions of a phone list (`widget.swipe-actions`, D374): a row whose content a finger
//! slides sideways to uncover buttons under it, trailing ones on a leftward swipe and leading ones
//! on a rightward swipe.
//!
//! [`Ui::swipe_actions`] wraps one row. The app draws the row in the rect the closure is given,
//! which moves with the finger and carries the row's own background, so the content slides as one
//! piece and the buttons show in the room it leaves. The component draws the buttons and owns the
//! motion; the app owns what an action does (it reads [`SwipeResponse::action`]) and the row's
//! content.
//!
//! * A touch pan on x over the row follows the finger 1:1. An edge with no actions rubber-bands
//!   ([`rubber_band`](crate::motion::rubber_band)). The lift springs the row open (the edge's
//!   buttons at their own widths) or closed, by the speed and position it projects to. A vertical
//!   pan goes to the list around the row. A mouse, a wheel and a key never swipe: the app's menu
//!   on [`SwipeResponse::context`] holds the same actions for them.
//! * While the row shows part of an edge, the buttons share the room that is uncovered in
//!   proportion to their widths, so the outermost always touches the window edge. A button is at
//!   least 74 px wide, as wide as its label and 12 px on each side, and as tall as the row.
//! * With [`SwipeActions::full_swipe`] the pull continues past the open width; a lift with more
//!   than 60 % of the row's width uncovered, and more than 24 px beyond the edge's open width
//!   (so on a narrow row the open rest position is never a full swipe), springs to the full width, the edge's outermost
//!   button stretching across the row, and reports that button when it arrives. The row then
//!   closes.
//! * A tap on a button reports it and closes the row. While the row is open, a press on its
//!   content closes it and goes no further; a press outside the row closes it and goes on to
//!   whatever it landed on. A press on a row in motion catches it.
//! * A secondary click or a touch long press on the row (not on a button) sets
//!   [`SwipeResponse::context`] and opens no menu (D292).
//!
//! Each edge's buttons are listed left to right as they sit on the screen: the last trailing
//! action is the outermost, and so is the first leading one.

use std::hash::Hash;

use super::gesture_track::{Bounds, Track};
use super::tone::Tone;
use crate::access::{Action, Node, Role};
use crate::color::with_alpha_byte;
use crate::input::{Event, PointerButton, TouchPhase};
use crate::motion::{Kind, Motion};
use crate::paint::Rect;
use crate::text::{Spec, Style};
use crate::ui::{Sense, Ui};

/// A button's least width.
const MIN_W: f32 = 74.0;
/// The room between a button's text and its edge, each side.
const PAD_X: f32 = 12.0;
/// The glyph's size and the gap under it.
const GLYPH: f32 = 20.0;
const GLYPH_GAP: f32 = 2.0;
/// The part of the row's width past which a lift is a full swipe.
const FULL: f32 = 0.6;
/// The room a full swipe needs past an edge's open width: the pull is a full swipe only past this
/// much beyond the buttons' total, so a lift at the open rest position never is one.
const FULL_MARGIN: f32 = 24.0;
/// The alpha of the shade a pressed button takes.
const PRESS_SHADE: u8 = 0x40;

/// One button of an edge.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SwipeAction<'a> {
    /// The button's text, elided to its width; also the accessible name.
    pub label: &'a str,
    /// A codicon glyph (`icon::*`) drawn over the label, if any.
    pub glyph: Option<&'a str>,
    /// The button's fill: [`Tone::Err`] is the theme's error colour, [`Tone::Neutral`] its accent.
    pub tone: Tone,
}

impl<'a> SwipeAction<'a> {
    /// A neutral button reading `label`, with no glyph.
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            glyph: None,
            tone: Tone::Neutral,
        }
    }

    /// The button with `glyph` over its label.
    #[must_use]
    pub const fn with_glyph(mut self, glyph: &'a str) -> Self {
        self.glyph = Some(glyph);
        self
    }

    /// The button filled in `tone`.
    #[must_use]
    pub const fn with_tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }
}

/// The buttons of a row, by edge.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SwipeActions<'a> {
    /// The buttons a rightward swipe uncovers at the row's left edge, left to right: the first is
    /// the outermost.
    pub leading: &'a [SwipeAction<'a>],
    /// The buttons a leftward swipe uncovers at the row's right edge, left to right: the last is
    /// the outermost.
    pub trailing: &'a [SwipeAction<'a>],
    /// Whether a pull past 60 % of the row's width commits the outermost button of its edge.
    pub full_swipe: bool,
}

impl Default for SwipeActions<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> SwipeActions<'a> {
    /// No buttons on either edge, and no full swipe.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            leading: &[],
            trailing: &[],
            full_swipe: false,
        }
    }

    /// These actions with `leading` buttons at the row's left edge.
    #[must_use]
    pub const fn with_leading(mut self, leading: &'a [SwipeAction<'a>]) -> Self {
        self.leading = leading;
        self
    }

    /// These actions with `trailing` buttons at the row's right edge.
    #[must_use]
    pub const fn with_trailing(mut self, trailing: &'a [SwipeAction<'a>]) -> Self {
        self.trailing = trailing;
        self
    }

    /// These actions with the full swipe on or off.
    #[must_use]
    pub const fn with_full_swipe(mut self, on: bool) -> Self {
        self.full_swipe = on;
        self
    }
}

/// An edge of the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SwipeEdge {
    /// The left edge, uncovered by a rightward swipe.
    Leading,
    /// The right edge, uncovered by a leftward swipe.
    Trailing,
}

/// What a frame of [`Ui::swipe_actions`] reports.
#[derive(Debug)]
#[non_exhaustive]
pub struct SwipeResponse<R> {
    /// What the app's row closure returned.
    pub value: R,
    /// The button a tap chose, or the outermost one a full swipe reached this frame: its edge and
    /// its index in that edge's list.
    pub action: Option<(SwipeEdge, usize)>,
    /// The edge the row is open on (or opening to) after this frame; `None` when it is closed or
    /// closing.
    pub open: Option<SwipeEdge>,
    /// A secondary click or a long press on the row asked for a menu.
    pub context: bool,
}

/// Where the row rests or is going.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Rest {
    Closed,
    Open(SwipeEdge),
    /// A full swipe in flight to the row's whole width.
    Full(SwipeEdge),
}

#[derive(Debug)]
struct RowState {
    /// How far the content is moved to the left: positive uncovers the trailing edge, negative
    /// the leading one.
    track: Track,
    rest: Rest,
    /// The row's width the track was laid out for.
    w: f32,
    /// A finger holds the row (or caught it and has not moved yet).
    finger: bool,
    caught: bool,
}

impl Default for RowState {
    fn default() -> Self {
        RowState {
            track: Track::new(0.0),
            rest: Rest::Closed,
            w: 0.0,
            finger: false,
            caught: false,
        }
    }
}

/// The buttons' natural widths of one edge.
fn natural(ui: &mut Ui<'_>, items: &[SwipeAction<'_>]) -> Vec<f32> {
    let size = ui.theme().font_s;
    items
        .iter()
        .map(|a| {
            let spec = Spec::plain(a.label, Style::ui(size, [0, 0, 0, 255]));
            (ui.painter().measure(&spec).0 + 2.0 * PAD_X)
                .ceil()
                .max(MIN_W)
        })
        .collect()
}

/// The uncovered width past which a lift is a full swipe, for an edge whose buttons total
/// `total` on a row `row_w` wide: 60 % of the row, and at least `FULL_MARGIN` past the open
/// width (on a row where the buttons are most of it, the open rest position is not a full swipe).
/// At or past `row_w` there is no full swipe.
fn full_at(total: f32, row_w: f32) -> f32 {
    (FULL * row_w).max(total + FULL_MARGIN)
}

/// The widths the buttons of an edge show when `r` px of it are uncovered, in screen order
/// (`outer` is the outermost's index): in proportion to their natural widths up to the total,
/// beyond it the outermost takes what is left, and with `full` the others narrow to nothing at
/// [`full_at`].
fn shown_widths(natural: &[f32], outer: usize, r: f32, row_w: f32, full: bool) -> Vec<f32> {
    let total: f32 = natural.iter().sum();
    if total <= 0.0 {
        return vec![0.0; natural.len()];
    }
    if r <= total {
        return natural.iter().map(|w| w * r / total).collect();
    }
    let edge = full_at(total, row_w);
    let keep = if !full || edge <= total {
        if full { 0.0 } else { 1.0 }
    } else {
        ((edge - r) / (edge - total)).clamp(0.0, 1.0)
    };
    let mut out: Vec<f32> = natural.iter().map(|w| w * keep).collect();
    let others: f32 = out
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != outer)
        .map(|(_, w)| w)
        .sum();
    out[outer] = (r - others).max(0.0);
    out
}

/// Whether a press this frame began outside `rect` (in the current drawing space).
fn pressed_outside(ui: &mut Ui<'_>, rect: Rect) -> bool {
    let rect = ui.painter().hit_rect(rect);
    ui.input().events.iter().any(|e| match e {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            ..
        } => !rect.contains(*pos),
        Event::Touch(t) if t.phase == TouchPhase::Started => !rect.contains(t.pos),
        _ => false,
    })
}

impl Ui<'_> {
    /// A row under `key` over `rect` with `actions` under it. `row(ui, content)` draws the row's
    /// content in `content`, `rect` shifted by the swipe and clipped to `rect`; the content
    /// carries its own background. With no actions on either edge it is just the closure. See the
    /// module docs for the gestures, the motion and the buttons.
    pub fn swipe_actions<R>(
        &mut self,
        key: impl Hash,
        rect: Rect,
        actions: &SwipeActions<'_>,
        row: impl FnOnce(&mut Ui<'_>, Rect) -> R,
    ) -> SwipeResponse<R> {
        let id = self.id(&key);
        let mut out = (None, None, false);
        if actions.leading.is_empty() && actions.trailing.is_empty() {
            *self.persistent::<RowState>(id) = RowState::default();
            let value = self.scope(&key, |ui| {
                ui.painter().push_clip(rect);
                let v = row(ui, rect);
                ui.painter().pop_clip();
                v
            });
            return SwipeResponse {
                value,
                action: None,
                open: None,
                context: false,
            };
        }

        let mut st = std::mem::take(self.persistent::<RowState>(id));
        let now = self.time();
        let w = rect.w.max(0.0);
        let pan_id = id.with("pan");
        let enabled = self.is_enabled();
        let slide = self
            .theme()
            .motion
            .resolve(&Motion::of(Kind::Drawer), self.tier())
            .filter(|r| r.transforms);
        if st.w != w {
            st = RowState {
                w,
                ..RowState::default()
            };
        }

        // The buttons' widths are read only while the row is away from rest or a finger is on it.
        let pan = self.pan(pan_id);
        let live = st.finger
            || !st.track.settled_at(now, 0.0)
            || pan.started
            || pan.caught
            || pan.ended
            || pan.active
            || pan.delta != (0.0, 0.0);
        let (lead_w, trail_w) = if live {
            (
                natural(self, actions.leading),
                natural(self, actions.trailing),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let lead_sum: f32 = lead_w.iter().sum();
        let trail_sum: f32 = trail_w.iter().sum();
        let (lo, hi) = if actions.full_swipe {
            (
                if lead_sum > 0.0 { -w } else { 0.0 },
                if trail_sum > 0.0 { w } else { 0.0 },
            )
        } else {
            (-lead_sum, trail_sum)
        };
        let b = Bounds::new(lo, hi, w.max(1.0));

        if live {
            // A finger catches the row in motion, drags it, and its lift picks where it rests.
            if pan.caught {
                st.track.halt(now, b);
                st.finger = true;
                st.caught = true;
            }
            let dx = pan.delta.0;
            if dx.is_finite() && dx != 0.0 {
                st.track.follow(now, -dx, b);
                st.finger = true;
                st.caught = false;
            }
            let lifted = pan.ended || (st.caught && !pan.active && self.touch_rest().is_none());
            if st.finger && lifted {
                let v = if pan.ended {
                    -self.release_velocity(pan_id).0
                } else {
                    0.0
                };
                let (shown, _) = st.track.at(now, b.lo, b.hi, b.extent);
                let full = if !actions.full_swipe {
                    None
                } else if trail_sum > 0.0 && shown > full_at(trail_sum, w) {
                    Some(SwipeEdge::Trailing)
                } else if lead_sum > 0.0 && shown < -full_at(lead_sum, w) {
                    Some(SwipeEdge::Leading)
                } else {
                    None
                };
                if let Some(edge) = full {
                    let to = if edge == SwipeEdge::Trailing { w } else { -w };
                    st.track.release(now, v, &[to], 0, None, b);
                    st.rest = Rest::Full(edge);
                } else {
                    let mut stops: Vec<(f32, Rest)> = Vec::new();
                    if lead_sum > 0.0 {
                        stops.push((-lead_sum, Rest::Open(SwipeEdge::Leading)));
                    }
                    stops.push((0.0, Rest::Closed));
                    if trail_sum > 0.0 {
                        stops.push((trail_sum, Rest::Open(SwipeEdge::Trailing)));
                    }
                    let current = stops.iter().position(|s| s.1 == st.rest).unwrap_or(0);
                    let at: Vec<f32> = stops.iter().map(|s| s.0).collect();
                    let pick = st.track.release(now, v, &at, current, Some(1), b);
                    st.rest = stops[pick].1;
                }
                st.finger = false;
                st.caught = false;
            }
        }

        // A full swipe that arrived reports its outermost button, and the row closes.
        if let Rest::Full(edge) = st.rest
            && !st.finger
        {
            let to = if edge == SwipeEdge::Trailing { w } else { -w };
            if st.track.settled_at(now, to) {
                let list = match edge {
                    SwipeEdge::Trailing => actions.trailing,
                    SwipeEdge::Leading => actions.leading,
                };
                let outer = match edge {
                    SwipeEdge::Trailing => list.len().saturating_sub(1),
                    SwipeEdge::Leading => 0,
                };
                out.0 = Some((edge, outer));
                st.track.go_to(now, 0.0, slide, b);
                st.rest = Rest::Closed;
            }
        }

        // A press outside the open row closes it; that press goes on where it landed.
        if matches!(st.rest, Rest::Open(_)) && !st.finger && pressed_outside(self, rect) {
            st.track.go_to(now, 0.0, slide, b);
            st.rest = Rest::Closed;
        }

        let (shown, moving) = st.track.at(now, b.lo, b.hi, b.extent);
        if moving {
            self.mark_moving();
        }
        let over = shown.abs() > 0.5 || st.finger;
        let content = Rect::new(rect.x - shown, rect.y, rect.w, rect.h);
        // What a click region over the content covers: the part of the row the buttons leave.
        let cover = if shown >= 0.0 {
            Rect::new(rect.x, rect.y, (rect.w - shown).max(0.0), rect.h)
        } else {
            Rect::new(rect.x - shown, rect.y, (rect.w + shown).max(0.0), rect.h)
        };

        // The pan region is under the content's widgets, so a pan that starts on one that does not
        // take it still goes to the row.
        self.pan_region(pan_id, rect, (true, false), moving);
        let click_id = id.with("row");
        let mut context = false;
        let mut close = false;
        if !over {
            // At rest the row's own region sits under the content: a secondary click or a long
            // press on bare content reaches it, a widget in the content takes its own.
            let r = self.interact(click_id, rect, Sense::CLICK);
            context |= r.enabled && r.secondary_clicked;
        }
        let value = self.scope(&key, |ui| {
            ui.painter().push_clip(rect);
            let v = row(ui, content);
            ui.painter().pop_clip();
            v
        });
        if over {
            // Open or moving, the content takes no press of its own: a tap closes the row.
            let r = self.interact(click_id, cover, Sense::CLICK);
            context |= r.enabled && r.secondary_clicked;
            if r.enabled && r.clicked && !st.finger {
                close = true;
            }
        }

        // The buttons of the edge the row shows.
        let edge = if shown > 0.0 {
            Some(SwipeEdge::Trailing)
        } else if shown < 0.0 {
            Some(SwipeEdge::Leading)
        } else {
            None
        };
        if let Some(edge) = edge {
            let t = *self.theme();
            let (list, widths) = match edge {
                SwipeEdge::Trailing => (actions.trailing, &trail_w),
                SwipeEdge::Leading => (actions.leading, &lead_w),
            };
            let outer = match edge {
                SwipeEdge::Trailing => list.len().saturating_sub(1),
                SwipeEdge::Leading => 0,
            };
            let r = shown.abs();
            let shown_w = shown_widths(widths, outer, r, w, actions.full_swipe);
            let mut x = match edge {
                SwipeEdge::Trailing => rect.right() - r,
                SwipeEdge::Leading => rect.x,
            };
            self.painter().push_clip(rect);
            for (i, a) in list.iter().enumerate() {
                let bw = shown_w.get(i).copied().unwrap_or(0.0);
                let cell = Rect::new(x, rect.y, bw, rect.h);
                x += bw;
                if bw < 0.5 {
                    continue;
                }
                let bid = id.with(("action", edge == SwipeEdge::Trailing, i));
                let resp = self.interact(bid, cell, Sense::CLICK);
                let fill = match a.tone {
                    Tone::Neutral => t.accent,
                    tone => tone.color(&t),
                };
                let nat = widths.get(i).copied().unwrap_or(MIN_W);
                let ink = t.on_accent;
                let p = self.painter();
                p.push_clip(cell);
                p.fill(cell, 0.0, fill);
                if resp.pressed {
                    p.fill(cell, 0.0, with_alpha_byte(t.dialog_backdrop, PRESS_SHADE));
                }
                let label = Spec::plain(a.label, Style::ui(t.font_s, ink));
                let label = p.elide(&label, (nat - 2.0 * PAD_X).max(0.0));
                let (lw, lh) = p.measure(&label);
                let glyph_h = if a.glyph.is_some() {
                    GLYPH * 1.25 + GLYPH_GAP
                } else {
                    0.0
                };
                let top = cell.y + (cell.h - glyph_h - lh) / 2.0;
                let cx = cell.x + cell.w / 2.0;
                if let Some(g) = a.glyph {
                    let spec = crate::icon::glyph(g, GLYPH, ink);
                    let gw = p.measure(&spec).0;
                    p.text_centered(&spec, Rect::new(cx - gw / 2.0, top, gw, GLYPH * 1.25));
                }
                p.text_centered(&label, Rect::new(cx - lw / 2.0, top + glyph_h, lw, lh));
                p.pop_clip();

                let mut node = Node::new(bid, Role::Button, a.label, cell);
                if resp.enabled {
                    node.actions.insert(Action::Default);
                }
                self.node(node);
                if enabled && resp.enabled && resp.clicked && out.0.is_none() {
                    out.0 = Some((edge, i));
                    close = true;
                }
            }
            self.painter().pop_clip();
        }

        if close {
            st.track.go_to(now, 0.0, slide, b);
            st.rest = Rest::Closed;
        }
        out.1 = match st.rest {
            Rest::Open(e) | Rest::Full(e) => Some(e),
            Rest::Closed => None,
        };
        out.2 = context;
        *self.persistent::<RowState>(id) = st;
        SwipeResponse {
            value,
            action: out.0,
            open: out.1,
            context: out.2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_share_the_uncovered_room_in_proportion_up_to_the_total() {
        let w = shown_widths(&[80.0, 120.0], 1, 100.0, 400.0, false);
        assert_eq!(w, vec![40.0, 60.0]);
        let w = shown_widths(&[80.0, 120.0], 1, 200.0, 400.0, false);
        assert_eq!(w, vec![80.0, 120.0]);
    }

    #[test]
    fn past_the_total_the_outermost_takes_the_rest_and_the_row_stays_gap_free() {
        let w = shown_widths(&[80.0, 120.0], 1, 230.0, 400.0, false);
        assert_eq!(w, vec![80.0, 150.0]);
        // With a full swipe the others narrow to nothing at 60 % of the row.
        let w = shown_widths(&[80.0, 120.0], 1, 0.6 * 400.0, 400.0, true);
        assert_eq!(w[0], 0.0);
        assert!((w[1] - 240.0).abs() < 1e-3, "{w:?}");
        let mid = shown_widths(&[80.0, 120.0], 1, 220.0, 400.0, true);
        assert!((mid.iter().sum::<f32>() - 220.0).abs() < 1e-3);
        assert!(mid[0] > 0.0 && mid[0] < 80.0);
    }

    #[test]
    fn the_full_swipe_point_is_past_the_open_width_on_a_narrow_row() {
        // A wide row: 60 % of it. A narrow one, where the buttons are most of it: the margin past
        // them, so the open width is never past the point.
        assert!((full_at(148.0, 400.0) - 240.0).abs() < 1e-3);
        assert_eq!(full_at(148.0, 220.0), 172.0);
        let w = shown_widths(&[74.0, 74.0], 1, 148.0, 220.0, true);
        assert_eq!(
            w,
            vec![74.0, 74.0],
            "the open width shows the buttons whole"
        );
        let w = shown_widths(&[74.0, 74.0], 1, 172.0, 220.0, true);
        assert_eq!(w[0], 0.0);
    }

    #[test]
    fn a_leading_outermost_is_the_first_button() {
        let w = shown_widths(&[80.0, 120.0], 0, 260.0, 400.0, false);
        assert_eq!(w, vec![140.0, 120.0]);
    }
}
