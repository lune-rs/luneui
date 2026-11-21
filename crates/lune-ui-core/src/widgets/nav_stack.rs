//! The navigation stack of a phone app (`layout.nav-stack`, D372): the levels of a drill-down, the
//! top one drawn, pushed in from the right edge, popped back out by a call or by a finger's swipe
//! from the left edge.
//!
//! The app owns the levels: it passes how many there are (`depth`) and draws each one in the
//! `page` closure, which the stack calls with the level's index and the rect to draw it in. The
//! stack owns only the motion between levels. While it moves, both levels are drawn: the lower one
//! 30 % of the width to the left of where it rests (the parallax), dimmed by a shade that fades
//! as the top level covers it, and the top level one soft shadow wide along its leading edge.
//!
//! A level draws inside the rect it is given and fills it with an opaque background: the levels
//! overlap while the stack moves, and the stack paints no background of its own. Each level is
//! drawn inside its own id scope, kept alive while it is covered, so a covered level keeps its
//! scroll offset and its other persistent state until the app removes it.
//!
//! * A `depth` one higher than the last frame's is a push: the new top slides in from the right
//!   under the page-swap motion ([`Kind::PageSwap`], D3). Any other change of `depth`, and the
//!   first frame, cuts to the new top with no motion. Under the Reduced and Off tiers a push and
//!   a pop cut too.
//! * [`Ui::nav_stack_pop`] asks for an animated pop. [`NavStackResponse::popped`] is set once, on
//!   the frame the pop ends: the app then lowers its `depth` (the level that was the top has left
//!   the window; the frame shows the one below at rest). An app that keeps its `depth` sees the
//!   top drawn again at rest.
//! * A touch pan that starts in the 24 px band at the rect's left edge, while `depth` is at least
//!   2, drags the top level 1:1 across; the lift springs it to the right edge (a pop) or back,
//!   by the release's speed and the position it projects to. A press on the moving stack catches
//!   it. A mouse, a wheel and a key never pan: the app keeps its own back button.
//! * While the stack moves, a region over the whole rect takes every press, so nothing under
//!   the sliding levels is tapped or scrolled.

use std::hash::Hash;

use super::gesture_track::{Bounds, Track};
use crate::color::{Rgba, premul};
use crate::motion::{Kind, Motion};
use crate::paint::Rect;
use crate::ui::{Sense, Ui};

/// The width of the band at the rect's left edge a touch pan starts a swipe in.
const EDGE: f32 = 24.0;
/// How far left of its resting place the lower level sits when the top is fully in, as a share of
/// the stack's width.
const PARALLAX: f32 = 0.3;
/// The most of the scrim's alpha the shade over the lower level takes (when the top level has
/// just begun to cover it).
const SHADE: f32 = 1.0;
/// The soft shadow along the top level's leading edge: its blur, in logical px, and the share of
/// the scrim's alpha it takes at its strongest.
const SHADOW_SIGMA: f32 = 10.0;
const SHADOW: f32 = 0.7;

/// What a frame of [`Ui::nav_stack`] reports.
#[derive(Debug, Default, Clone, PartialEq)]
#[non_exhaustive]
pub struct NavStackResponse {
    /// An animated pop (a queued [`Ui::nav_stack_pop`] or an edge swipe) ended this frame, once:
    /// the app removes its top level.
    pub popped: bool,
    /// A push, a pop or a swipe is moving the levels (both are drawn).
    pub moving: bool,
}

/// What the stack is doing with its track, the top level's left edge in px from the rect's.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    /// At rest on 0: the top level alone.
    Rest,
    /// Moving to 0: a push, or a swipe that springs back.
    ToRest,
    /// Moving to the right edge: a pop that reports `popped` when it gets there.
    Pop,
    /// Held by a finger (or caught by one that has not moved yet).
    Finger,
    /// A pop ended and was reported: the top is out of the window, and the app takes it away.
    Popped,
}

#[derive(Debug)]
struct StackState {
    track: Track,
    mode: Mode,
    /// The depth the last frame drew; 0 before the first.
    depth: usize,
    /// A finger caught the stack in motion and has not moved it yet.
    caught: bool,
}

impl Default for StackState {
    fn default() -> Self {
        StackState {
            track: Track::new(0.0),
            mode: Mode::Rest,
            depth: 0,
            caught: false,
        }
    }
}

/// A pop the app asked for and the stack has not taken yet.
#[derive(Debug, Default)]
struct PopRequest(bool);

/// `c` with its alpha scaled by `k` (0 to 1).
fn scaled(c: Rgba, k: f32) -> Rgba {
    [
        c[0],
        c[1],
        c[2],
        (f32::from(c[3]) * k.clamp(0.0, 1.0)).round() as u8,
    ]
}

/// Draws level `level` in `r` inside its own kept-alive id scope under the stack's `key`.
fn draw_level<K: Hash>(
    ui: &mut Ui<'_>,
    key: &K,
    level: usize,
    r: Rect,
    page: &mut impl FnMut(&mut Ui<'_>, usize, Rect),
) {
    ui.scope(key, |ui| ui.keep_alive(level, |ui| page(ui, level, r)));
}

impl Ui<'_> {
    /// A navigation stack under `key` over `rect`: call it every frame with the number of levels
    /// the app has (`depth`) and a `page` that draws level `level` (0 the root) in the rect it is
    /// given, which moves while the stack does. Depth 0 draws nothing. See the module docs for the
    /// motion, and for what a level must draw (an opaque background over its whole rect).
    pub fn nav_stack(
        &mut self,
        key: impl Hash,
        rect: Rect,
        depth: usize,
        mut page: impl FnMut(&mut Ui<'_>, usize, Rect),
    ) -> NavStackResponse {
        let id = self.id(&key);
        let mut st = std::mem::take(self.persistent::<StackState>(id));
        let queued = std::mem::take(&mut self.transient::<PopRequest>(id).0);
        let now = self.time();
        let w = rect.w.max(0.0);
        let b = Bounds::new(0.0, w, w.max(1.0));
        let slide = self
            .theme()
            .motion
            .resolve(&Motion::of(Kind::PageSwap), self.tier())
            .filter(|r| r.transforms);
        let prev = st.depth;
        let band_id = id.with("edge");
        let hold_id = id.with("hold");

        // The levels the app took away: what they kept ends.
        for level in depth..prev {
            self.scope(&key, |ui| ui.end_keep_alive(level));
        }
        if st.mode == Mode::Popped {
            st.track = Track::new(0.0);
            st.mode = Mode::Rest;
        }
        if depth == 0 {
            *self.persistent::<StackState>(id) = StackState::default();
            return NavStackResponse::default();
        }

        // The app moved the depth, or asked for a pop.
        if depth != prev {
            st.caught = false;
            if prev >= 1 && depth == prev + 1 && matches!(st.mode, Mode::Rest | Mode::ToRest) {
                st.track = Track::new(w);
                st.track.go_to(now, 0.0, slide, b);
                st.mode = Mode::ToRest;
            } else {
                st.track = Track::new(0.0);
                st.mode = Mode::Rest;
            }
        } else if queued && depth >= 2 && matches!(st.mode, Mode::Rest | Mode::ToRest) {
            st.track.go_to(now, w, slide, b);
            st.mode = Mode::Pop;
        }

        // A finger on the edge band: it catches the stack, drags it, and its lift picks the end.
        if depth >= 2 {
            let pan = self.pan(band_id);
            if pan.caught {
                st.track.halt(now, b);
                st.mode = Mode::Finger;
                st.caught = true;
            }
            let dx = pan.delta.0;
            if dx.is_finite() && dx != 0.0 {
                let (p, _) = st.track.at(now, 0.0, w, b.extent);
                st.track.follow(now, (p + dx).clamp(0.0, w) - p, b);
                st.mode = Mode::Finger;
                st.caught = false;
            }
            let lifted = pan.ended || (st.caught && !pan.active && self.touch_rest().is_none());
            if st.mode == Mode::Finger && lifted {
                let v = if pan.ended {
                    self.release_velocity(band_id).0
                } else {
                    0.0
                };
                let (p, _) = st.track.at(now, 0.0, w, b.extent);
                let current = usize::from(p > w / 2.0);
                let pick = st.track.release(now, v, &[0.0, w], current, None, b);
                st.mode = if pick == 1 { Mode::Pop } else { Mode::ToRest };
                st.caught = false;
            }
        }

        let (x, track_moving) = st.track.at(now, 0.0, w, b.extent);
        let x = x.clamp(0.0, w);
        let mut popped = false;
        match st.mode {
            Mode::ToRest if st.track.settled_at(now, 0.0) => st.mode = Mode::Rest,
            Mode::Pop if st.track.settled_at(now, w) => {
                st.mode = Mode::Popped;
                popped = true;
            }
            _ => {}
        }
        let moving = matches!(st.mode, Mode::ToRest | Mode::Pop | Mode::Finger);
        if track_moving {
            self.mark_moving();
        }
        st.depth = depth;

        // The band is under the levels' widgets, so a pan that starts on one that does not take
        // it still goes to the band.
        if depth >= 2 && st.mode != Mode::Popped {
            let band = Rect::new(rect.x, rect.y, EDGE.min(w), rect.h);
            self.pan_region(band_id, band, (true, false), track_moving);
        }

        let t = *self.theme();
        self.painter().push_clip(rect);
        if st.mode == Mode::Popped {
            // The top has left: the level below, at rest.
            draw_level(self, &key, depth - 2, rect, &mut page);
        } else if moving && depth >= 2 {
            let done = if w > 0.0 { x / w } else { 1.0 };
            let lower = Rect::new(rect.x - PARALLAX * w * (1.0 - done), rect.y, rect.w, rect.h);
            // The lower level shows only left of the top's edge. Its text draws above every
            // fill of the layer, so without this clip it would show through the top level.
            let seen = Rect::new(rect.x, rect.y, x, rect.h);
            self.painter().push_clip(seen);
            draw_level(self, &key, depth - 2, lower, &mut page);
            self.painter().pop_clip();
            if x > 0.0 {
                let p = self.painter();
                // The shade is an over fill: it dims the lower level's text as well as its fills.
                p.fill_over(
                    seen,
                    0.0,
                    premul(scaled(t.dialog_backdrop, SHADE * (1.0 - done))),
                );
                // The shadow is the whole blurred shape: the top level covers its inner half, and
                // the shape runs past the rect's top and bottom so its edge does not fade there.
                p.shadow(
                    Rect::new(
                        rect.x + x,
                        rect.y - 3.0 * SHADOW_SIGMA,
                        w,
                        rect.h + 6.0 * SHADOW_SIGMA,
                    ),
                    0.0,
                    SHADOW_SIGMA,
                    scaled(t.dialog_backdrop, SHADOW * (1.0 - done)),
                );
            }
            let top = Rect::new(rect.x + x, rect.y, rect.w, rect.h);
            draw_level(self, &key, depth - 1, top, &mut page);
        } else {
            draw_level(self, &key, depth - 1, rect, &mut page);
        }
        self.painter().pop_clip();

        // Every press while it moves stops here, and a pan that starts on it stays with it.
        if moving {
            let sense = Sense {
                drag: true,
                ..Sense::CLICK
            };
            self.interact(hold_id, rect, sense);
            self.set_drag_axes(hold_id, true, true);
        }

        *self.persistent::<StackState>(id) = st;
        NavStackResponse { popped, moving }
    }

    /// Asks the navigation stack under `key` to pop its top level with the reverse of a push: its
    /// next call takes the request. `key` is the one the stack was called with, in the same id
    /// scope as the stack's own call (so a page that draws its own back button reports the tap
    /// and the app calls this after the stack call, not from inside the `page` closure, whose
    /// scope is the level's). Ignored while `depth` is below 2, and while a pop or a finger is
    /// already moving the stack. The pop is over when [`NavStackResponse::popped`] is set.
    pub fn nav_stack_pop(&mut self, key: impl Hash) {
        let id = self.id(key);
        self.transient::<PopRequest>(id).0 = true;
    }
}
