//! The bottom sheet of a phone screen (`overlay.bottom-sheet`, D371): a modal surface that slides
//! up from the window's bottom edge to a detent, follows a finger that drags it, and springs to the
//! detent the release picks, or off the screen to dismiss.
//!
//! [`Ui::bottom_sheet`] is an overlay ([`Ui::overlay`]) the app calls every frame while its `open`
//! flag is set and for as long as the sheet slides out after it is cleared. It draws a scrim over
//! the window, then a frosted surface (`popover::glass_surface`) the width of the window whose
//! top edge is the one moving part, then the app's content under a grabber band.
//!
//! The sheet moves by its own track (`Track`), not by the overlay's open and close motions
//! (those are none): opening and an app close are a programmatic move under [`Kind::Drawer`]
//! (D3), a release after a finger continues by the track's spring at every motion tier. Under
//! the Reduced tier the slide is replaced by a fade, and under Off the sheet snaps.
//!
//! A touch pan on the sheet drags it, except where content that scrolls vertically sits on top of
//! the sheet's region and takes the pan itself. A mouse or a key never drags: a click on the
//! grabber steps to the next detent, Escape and a click on the scrim close.

use std::hash::Hash;

use super::dialog::{fade, paint_scrim};
use super::gesture_track::{Bounds, Track};
use super::nav_bar::{button_node, ghost_look, paint_ghost};
use crate::access::{Action, Node, Role, State};
use crate::color::premul;
use crate::input::NamedKey;
use crate::motion::{Easing, Kind, Motion, Reduce, Token};
use crate::paint::{Painter, Rect};
use crate::scene::LayerId;
use crate::ui::{OverlayOptions, Sense, Ui};

/// The corner radius of the sheet's top edge.
const RADIUS: f32 = 12.0;
/// The height of the grabber band at the sheet's top.
const BAND: f32 = 20.0;
/// The grabber pill's size.
const GRABBER: (f32, f32) = (36.0, 5.0);
/// The grabber button's width (its height is the band's); a finger's hit is 44 by 44 anyway.
const GRABBER_HIT_W: f32 = 72.0;
/// The room kept between the tallest detent and the window's top safe area.
const TOP_GAP: f32 = 10.0;

/// The overlay's own open and close: none, the sheet moves itself.
const NO_MOTION: Motion = Motion {
    kind: Kind::Drawer,
    token: Token::Zero,
    easing: Easing::Linear,
    reduced: Reduce::Keep,
    reduced_token: None,
};

/// A height a sheet rests at.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum Detent {
    /// This share of the largest height (the window's height less the top safe area and 10 px).
    Fraction(f32),
    /// This height in logical px, at most the largest height.
    Height(f32),
}

/// What [`Ui::bottom_sheet`] draws.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct BottomSheet<'a> {
    /// The accessible name of the sheet and of its grabber.
    pub name: &'a str,
    /// The heights the sheet rests at, in any order; empty means one detent of the largest height.
    pub detents: &'a [Detent],
    /// Draws the grabber and its 20 px band at the sheet's top; off, the content starts at the top.
    pub grabber: bool,
}

impl<'a> BottomSheet<'a> {
    /// A sheet named `name` with a half-height detent and a full-height one, and a grabber.
    #[must_use]
    pub const fn new(name: &'a str) -> Self {
        const DEFAULT: &[Detent] = &[Detent::Fraction(0.5), Detent::Fraction(1.0)];
        Self {
            name,
            detents: DEFAULT,
            grabber: true,
        }
    }

    /// The sheet resting at `detents`.
    #[must_use]
    pub const fn with_detents(mut self, detents: &'a [Detent]) -> Self {
        self.detents = detents;
        self
    }

    /// The sheet with the grabber on or off.
    #[must_use]
    pub const fn with_grabber(mut self, on: bool) -> Self {
        self.grabber = on;
        self
    }
}

/// What a frame of [`Ui::bottom_sheet`] reports.
#[derive(Debug)]
#[non_exhaustive]
pub struct SheetResponse<R> {
    /// What the content returned: `Some` while the content is drawn (open, or sliding out).
    pub value: Option<R>,
    /// The index of the detent the sheet rests at or springs to, counting the detents from the
    /// smallest height up; `None` once it is closing or closed.
    pub detent: Option<usize>,
    /// The sheet closed itself this frame (a drag released to closed, Escape, a press on the
    /// scrim or an assistive technology's dismiss): `open` was set false.
    pub dismissed: bool,
}

/// The heights of `detents` in px for a largest height `max`: ascending, without duplicates and
/// without a height under half a px; one detent of `max` when the list names none.
fn detent_heights(detents: &[Detent], max: f32) -> Vec<f32> {
    let mut v: Vec<f32> = detents
        .iter()
        .filter_map(|d| {
            let h = match *d {
                Detent::Fraction(f) => f * max,
                Detent::Height(h) => h,
            };
            h.is_finite().then(|| h.clamp(0.0, max))
        })
        .filter(|h| *h >= 0.5)
        .collect();
    v.sort_by(f32::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    if v.is_empty() {
        v.push(max);
    }
    v
}

/// The sheet's glass: [`super::popover::glass_surface`]'s parts (the blurred scene under `r`, the
/// `menu_glass` frost with the noise, the 1 px `menu_popup_border` ring) with the ring kept to the
/// top `radius` px, so it follows the top corners and the top edge and no line runs down the
/// window's sides, where the sheet is edge to edge.
fn glass_sheet<H: Copy + PartialEq>(p: &mut Painter<'_, H>, r: Rect, radius: f32) {
    let t = p.theme;
    p.backdrop(r, radius);
    p.frost(r, radius, premul(t.menu_glass), true);
    p.push_clip(Rect::new(r.x, r.y, r.w, radius));
    p.ring(r, radius, 1.0, t.menu_popup_border, false);
    p.pop_clip();
}

/// What the sheet keeps between frames while it is up.
#[derive(Debug, Clone, Copy)]
struct Live {
    track: Track,
    /// The detent it rests at or goes to.
    detent: usize,
    /// The top edge the track rests on or goes to; NaN while a finger has caught it.
    target: f32,
    /// Sliding out: `open` was cleared or the sheet dismissed itself.
    closing: bool,
    /// A finger caught the sheet in motion and has not moved it yet.
    caught: bool,
}

#[derive(Debug, Default)]
struct SheetState {
    live: Option<Live>,
}

impl<'a> Ui<'a> {
    /// A bottom sheet under `key`: call it every frame. `open` false to true opens it at the
    /// smallest detent; it stays up, sliding out when `open` is cleared, until it is out of the
    /// window. A close the sheet decides (Escape, the scrim, a drag released to closed, an
    /// assistive technology's dismiss) clears `open` itself and reports `dismissed` once.
    ///
    /// `content` draws inside the content rect: below the grabber band and above the larger of
    /// the safe area's bottom and the keyboard's, clipped to it. A scroll area in it takes the
    /// vertical pans of a finger that starts on it; where it has nothing to scroll the pan drags
    /// the sheet. See the module docs for the motion.
    pub fn bottom_sheet<R>(
        &mut self,
        key: impl Hash,
        open: &mut bool,
        sheet: &BottomSheet<'_>,
        content: impl FnOnce(&mut Ui<'a>, Rect) -> R,
    ) -> SheetResponse<R> {
        let id = self.id(&key);
        let mut state = std::mem::take(self.transient::<SheetState>(id));
        if !*open && state.live.is_none() {
            return SheetResponse {
                value: None,
                detent: None,
                dismissed: false,
            };
        }
        let now = self.time();
        let win = self.painter().window;
        let (safe, kb) = (self.safe_area(), self.keyboard_inset());
        let closed = win.bottom();
        let heights = detent_heights(sheet.detents, (win.h - safe.top - TOP_GAP).max(0.0));
        let tops: Vec<f32> = heights.iter().map(|h| closed - h).collect();
        let highest = tops.last().copied().unwrap_or(closed);
        let bounds = Bounds::new(highest, closed, win.h);
        let drawer = Motion::of(Kind::Drawer);
        let resolved = self.theme().motion.resolve(&drawer, self.tier());
        let slide = resolved.filter(|r| r.transforms);
        let fade_only = resolved.is_some_and(|r| !r.transforms);
        let (fade_id, pan_id) = (id.with("fade"), id.with("pan"));
        let mut dismissed = false;

        // Opening, and a close or a reopen by the app.
        let fresh = state.live.is_none();
        let live = state.live.get_or_insert_with(|| Live {
            track: Track::new(if slide.is_some() { closed } else { tops[0] }),
            detent: 0,
            target: tops[0],
            closing: false,
            caught: false,
        });
        if fresh {
            live.track.go_to(now, tops[0], slide, bounds);
            if fade_only {
                self.snap_f32(fade_id, 0.0);
            }
        } else {
            live.detent = live.detent.min(tops.len() - 1);
            if *open && live.closing {
                live.closing = false;
                live.target = tops[live.detent];
                live.track.go_to(now, live.target, slide, bounds);
            } else if !*open && !live.closing {
                live.closing = true;
                live.caught = false;
                if !fade_only {
                    live.target = closed;
                    live.track.go_to(now, closed, slide, bounds);
                }
            }
        }

        // A finger: it catches a sheet in motion, drags it, and its lift picks where it goes.
        let pan = self.pan(pan_id);
        if !live.closing {
            if pan.caught {
                live.track.halt(now, bounds);
                live.caught = true;
                live.target = f32::NAN;
            }
            let dy = pan.delta.1;
            if dy.is_finite() && dy != 0.0 {
                live.track.follow(now, dy, bounds);
                live.caught = false;
                live.target = f32::NAN;
            }
            if pan.ended {
                let v = self.release_velocity(pan_id).1;
                let stops: Vec<f32> = std::iter::once(closed)
                    .chain(tops.iter().copied())
                    .collect();
                let pick = live
                    .track
                    .release(now, v, &stops, live.detent + 1, None, bounds);
                live.caught = false;
                live.target = stops[pick];
                if pick == 0 {
                    live.closing = true;
                    *open = false;
                    dismissed = true;
                } else {
                    live.detent = pick - 1;
                }
            } else if live.caught && !pan.active && self.touch_rest().is_none() {
                // The finger that caught it lifted without moving it: settle on a detent.
                let pick = live
                    .track
                    .release(now, 0.0, &tops, live.detent, None, bounds);
                live.detent = pick;
                live.target = tops[pick];
                live.caught = false;
            } else if !live.caught
                && !pan.active
                && !live.target.is_nan()
                && live.target != tops[live.detent]
            {
                // The detents moved (the window or the safe area changed): go to where the
                // detent is now, from where the sheet shows.
                live.target = tops[live.detent];
                live.track.go_to(now, live.target, slide, bounds);
            }
        }

        let (y, track_moving) = live.track.at(now, bounds.lo, bounds.hi, bounds.extent);
        let fade_v = fade_only.then(|| {
            let goal = if live.closing { 0.0 } else { 1.0 };
            self.animate_f32(fade_id, goal, &drawer)
        });
        if live.closing
            && live.track.settled_at(now, live.target)
            && fade_v.is_none_or(|f| f <= 0.001)
        {
            // Out of the window: the overlay is not called, so it ends.
            return SheetResponse {
                value: None,
                detent: None,
                dismissed,
            };
        }
        if track_moving {
            self.mark_moving();
        }
        let progress =
            fade_v.unwrap_or_else(|| ((closed - y) / (closed - tops[0]).max(1.0)).clamp(0.0, 1.0));

        let options = OverlayOptions {
            layer: LayerId::Modal,
            modal: true,
            dismiss_on_escape: true,
            dismiss_on_outside_press: false,
            dismiss_on_focus_loss: false,
            anchor: None,
            initial_focus: None,
            open: NO_MOTION,
            close: NO_MOTION,
        };
        let band = if sheet.grabber { BAND } else { 0.0 };
        let visible = Rect::new(win.x, y, win.w, (closed - y).max(0.0));
        let surface = Rect::new(win.x, y, win.w, (closed + RADIUS - y).max(0.0));
        let content_bottom = closed - safe.bottom.max(kb.bottom);
        let rect = Rect::new(
            win.x,
            y + band,
            win.w,
            (content_bottom - (y + band)).max(0.0),
        );
        let live_ref = &mut *live;
        let (frame, (value, scrim_clicked)) = self.overlay(
            &key,
            &options,
            |_, _, _| {},
            |ui, _frame| {
                let t = *ui.theme();
                let scrim = ui.interact(id.with("scrim"), win, Sense::CLICK);
                ui.painter().push_xform(fade(progress));
                paint_scrim(ui.painter(), false);
                ui.painter().pop_xform();
                if let Some(a) = fade_v {
                    ui.painter().push_xform(fade(a));
                }
                glass_sheet(ui.painter(), surface, RADIUS);
                // The sheet takes presses that land on it, so none reaches the scrim.
                ui.interact(id, visible, Sense::CLICK);
                if !live_ref.closing {
                    let moving = track_moving;
                    ui.pan_region(pan_id, visible, (false, true), moving);
                }
                let mut node = Node::new(id, Role::Dialog, sheet.name, visible);
                node.states.insert(State::Modal);
                node.actions.insert(Action::Dismiss);
                let value = ui.node_container(node, |ui| {
                    if sheet.grabber {
                        let gid = id.with("grabber");
                        let cx = win.x + win.w / 2.0;
                        let hit = Rect::new(cx - GRABBER_HIT_W / 2.0, y, GRABBER_HIT_W, BAND);
                        let mut r = ui.interact(gid, hit, Sense::CLICK_FOCUS);
                        if ui.activation_keys(gid, r.enabled, &[NamedKey::Enter, NamedKey::Space]) {
                            r.clicked = true;
                        }
                        let (fill, ring) = ghost_look(ui, gid, &r);
                        paint_ghost(ui, hit, fill, ring);
                        let pill = Rect::new(
                            cx - GRABBER.0 / 2.0,
                            y + (BAND - GRABBER.1) / 2.0,
                            GRABBER.0,
                            GRABBER.1,
                        );
                        ui.painter().fill(pill, GRABBER.1 / 2.0, t.fg_faint);
                        ui.node(button_node(gid, sheet.name, hit, &r));
                        if r.enabled && r.clicked && !live_ref.closing {
                            live_ref.detent = (live_ref.detent + 1) % tops.len();
                            live_ref.target = tops[live_ref.detent];
                            live_ref.track.go_to(now, live_ref.target, slide, bounds);
                        }
                    }
                    ui.painter().push_clip(rect);
                    let value = content(ui, rect);
                    ui.painter().pop_clip();
                    value
                });
                if fade_v.is_some() {
                    ui.painter().pop_xform();
                }
                (value, scrim.clicked)
            },
        );
        if !live.closing && (scrim_clicked || frame.dismissed.is_some()) {
            live.closing = true;
            *open = false;
            dismissed = true;
            if !fade_only {
                live.target = closed;
                live.track.go_to(now, closed, slide, bounds);
            }
        }
        let detent = (!live.closing).then_some(live.detent);
        *self.transient::<SheetState>(id) = state;
        SheetResponse {
            value: Some(value),
            detent,
            dismissed,
        }
    }
}
