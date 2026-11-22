//! The pager of a phone screen (`layout.pager`, D373): pages side by side that a finger swipes
//! between, with a row of dots under them.
//!
//! The app owns the pages: it passes how many there are (`count`), the current one (`page`) and a
//! `draw` closure that draws page `index` in the rect it is given. The pager owns the motion
//! between pages. A page fills the whole rect and adjacent pages touch, so a swipe in progress
//! shows the edge of one page against the edge of the next. Only the pages that intersect the
//! rect are drawn, each inside its own id scope and clipped to the rect. The pager paints no
//! background of its own: where a pull past the first or the last page uncovers the rect, the
//! window shows.
//!
//! * A touch pan on x over the rect, while `count` is more than one, moves the pages 1:1; past the
//!   first or the last page the pull shows through the rubber band ([`rubber_band`](
//!   crate::motion::rubber_band)). The lift moves at most one page, by the speed and the
//!   position it projects to; `*page` and
//!   [`PagerResponse::changed`] update on that frame. A press on the moving pages catches them.
//!   A vertical pan goes to a scroll area inside a page or around the pager, and a page's own
//!   horizontal scroller takes a pan that starts on it. A mouse, a wheel and a key never pan.
//! * A `*page` the app sets (or a dot or a key) moves the pages under the page-swap motion
//!   ([`Kind::PageSwap`], D3) when it is one page from the last one; a larger step, the first
//!   frame and a change of the rect's width or of `count` cut. Under the Reduced and Off tiers
//!   the move cuts, too. A release after a finger continues by the spring at every tier.
//! * The dots ([`PagerOptions::dots`], with more than one page) are a row at the rect's bottom, the
//!   page nearest the shown position in the accent. They are one tab stop with roving focus:
//!   Left, Right, Home and End move to a page and select it; a click or a tap on a dot selects it.
//!   The row draws over the pages and reserves no room.

use std::hash::Hash;

use super::button::FOCUS;
use super::focus_ring::{self, RingKind};
use super::gesture_track::{Bounds, Track};
use super::nav_item::SELECTION;
use crate::access::{Action, Node, Role, State};
use crate::input::NamedKey;
use crate::motion::{Kind, Motion};
use crate::paint::Rect;
use crate::ui::{RoveAxis, Sense, Ui, WidgetId};

/// A dot's diameter.
const DOT: f32 = 7.0;
/// The distance between the centres of two dots, at most (a long row narrows to fit the rect).
const PITCH: f32 = 16.0;
/// A dot's cell: as wide as the pitch and this high.
const CELL_H: f32 = 24.0;
/// How far above the rect's bottom the dots' centres are.
const ABOVE: f32 = 14.0;
/// The scrim pill behind the row: its height and its padding at each end. It keeps the dots
/// legible on any page fill.
const PILL_H: f32 = 20.0;
const PILL_PAD: f32 = 8.0;

/// What [`Ui::pager`] is given besides its key, rect, page and draw closure.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PagerOptions<'a> {
    /// The accessible name of the pager.
    pub name: &'a str,
    /// Whether the row of dots is drawn (and is a tab stop).
    pub dots: bool,
}

impl<'a> PagerOptions<'a> {
    /// Options for a pager named `name`, with the dots on.
    #[must_use]
    pub const fn new(name: &'a str) -> Self {
        Self { name, dots: true }
    }

    /// These options with the dots on or off.
    #[must_use]
    pub const fn with_dots(mut self, on: bool) -> Self {
        self.dots = on;
        self
    }
}

/// What a frame of [`Ui::pager`] reports.
#[derive(Debug, Default, Clone, PartialEq)]
#[non_exhaustive]
pub struct PagerResponse {
    /// A swipe, a dot or a key changed `*page` this frame (a page the app sets does not).
    pub changed: bool,
}

#[derive(Debug)]
struct PagerState {
    /// The pages' offset: page `i` rests at `i * w`.
    track: Track,
    /// The page the last frame ended on; `*page` differing from it is a move to start.
    page: usize,
    /// The rect's width and the page count the track was laid out for.
    w: f32,
    count: usize,
    /// A finger holds the pages (or caught them and has not moved yet).
    finger: bool,
    caught: bool,
    init: bool,
}

impl Default for PagerState {
    fn default() -> Self {
        PagerState {
            track: Track::new(0.0),
            page: 0,
            w: 0.0,
            count: 0,
            finger: false,
            caught: false,
            init: false,
        }
    }
}

impl Ui<'_> {
    /// A pager under `key` over `rect`: `count` pages, `*page` the current one (clamped to the
    /// last), `draw(ui, index, rect)` drawing page `index` in the rect it is given, which moves
    /// while the pages do. With no pages it draws nothing. See the module docs for the gestures,
    /// the motion and the dots; the node is a `Group` named by `opts.name`.
    pub fn pager(
        &mut self,
        key: impl Hash,
        rect: Rect,
        count: usize,
        page: &mut usize,
        opts: &PagerOptions<'_>,
        mut draw: impl FnMut(&mut Ui<'_>, usize, Rect),
    ) -> PagerResponse {
        let mut out = PagerResponse::default();
        let id = self.id(&key);
        if count == 0 {
            *self.persistent::<PagerState>(id) = PagerState::default();
            return out;
        }
        let last = count - 1;
        *page = (*page).min(last);
        let mut st = std::mem::take(self.persistent::<PagerState>(id));
        let now = self.time();
        let w = rect.w.max(0.0);
        let b = Bounds::new(0.0, last as f32 * w, w.max(1.0));
        let slide = self
            .theme()
            .motion
            .resolve(&Motion::of(Kind::PageSwap), self.tier())
            .filter(|r| r.transforms);
        let pan_id = id.with("pan");

        if !st.init || st.w != w || st.count != count {
            st = PagerState {
                track: Track::new(*page as f32 * w),
                page: *page,
                w,
                count,
                init: true,
                ..PagerState::default()
            };
        }

        // The page changed by the app, a dot or a key since the last frame.
        if !st.finger && *page != st.page {
            let target = *page as f32 * w;
            let step = *page == st.page + 1 || st.page == *page + 1;
            st.track
                .go_to(now, target, if step { slide } else { None }, b);
            st.page = *page;
        }

        // A finger: it catches pages in motion, drags them, and its lift picks the page.
        if count > 1 {
            let pan = self.pan(pan_id);
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
                let stops: Vec<f32> = (0..count).map(|i| i as f32 * w).collect();
                let pick = st.track.release(now, v, &stops, *page, Some(1), b);
                st.finger = false;
                st.caught = false;
                if pick != *page {
                    *page = pick;
                    out.changed = true;
                }
                st.page = *page;
            }
        }

        let (x, track_moving) = st.track.at(now, b.lo, b.hi, b.extent);
        if track_moving {
            self.mark_moving();
        }
        let near = if w > 0.0 {
            ((x / w).round().max(0.0) as usize).min(last)
        } else {
            *page
        };

        // The pan region is under the pages' widgets, so a pan that starts on one that does not
        // take it still goes to the pager.
        if count > 1 {
            self.pan_region(pan_id, rect, (true, false), track_moving);
        }

        let group = Node::new(id, Role::Group, opts.name, rect);
        let mut pick = None;
        self.scope(&key, |ui| {
            ui.node_container(group, |ui| {
                ui.painter().push_clip(rect);
                if w > 0.0 {
                    let f = x / w;
                    let first = ((f - 1.0).floor() as i64 + 1).max(0) as usize;
                    let end = (((f + 1.0).ceil() as i64 - 1).max(0) as usize).min(last);
                    for i in first..=end {
                        let at = Rect::new(rect.x + i as f32 * w - x, rect.y, w, rect.h);
                        ui.scope(("page", i), |ui| draw(ui, i, at));
                    }
                }
                ui.painter().pop_clip();
                if opts.dots && count > 1 {
                    pick = dots(ui, id, rect, count, *page, near);
                }
            });
        });
        if let Some(i) = pick.filter(|&i| i != *page) {
            *page = i;
            out.changed = true;
        }

        *self.persistent::<PagerState>(id) = st;
        out
    }
}

/// Draws the dots over `rect`'s bottom: `near` in the accent, the others dimmer. Returns the page
/// a click, a tap or a key chose.
fn dots(
    ui: &mut Ui<'_>,
    id: WidgetId,
    rect: Rect,
    count: usize,
    page: usize,
    near: usize,
) -> Option<usize> {
    let t = *ui.theme();
    let group_on = ui.is_enabled();
    let ids: Vec<(WidgetId, bool)> = (0..count)
        .map(|i| (id.with(("dot", i)), group_on))
        .collect();
    let stop = ui.rove_stop(&ids, Some(page));
    let pitch = PITCH.min(rect.w / count as f32);
    let x0 = rect.x + (rect.w - pitch * count as f32) / 2.0;
    let cy = rect.bottom() - ABOVE;
    let row = Rect::new(x0, cy - CELL_H / 2.0, pitch * count as f32, CELL_H);
    let name = ui.tr("luneui.pager.dots");
    let list = Node::new(id.with("dots"), Role::TabList, name, row);
    let mut pick = None;
    // The row spans the dots themselves (the centre of the first to the centre of the last).
    let span = (count - 1) as f32 * pitch + DOT;
    let pill = Rect::new(
        x0 + (pitch * count as f32 - span) / 2.0 - PILL_PAD,
        cy - PILL_H / 2.0,
        span + 2.0 * PILL_PAD,
        PILL_H,
    );
    ui.painter().fill(pill, PILL_H / 2.0, t.dialog_backdrop);
    ui.node_container(list, |ui| {
        for (i, &(iid, _)) in ids.iter().enumerate() {
            let cell = Rect::new(x0 + i as f32 * pitch, row.y, pitch, CELL_H);
            let sense = if stop == Some(i) {
                Sense::CLICK_FOCUS
            } else {
                Sense::CLICK_ROVE
            };
            let mut r = ui.interact(iid, cell, sense);
            if ui.activation_keys(iid, r.enabled, &[NamedKey::Enter, NamedKey::Space]) {
                r.clicked = true;
            }
            let tint = ui.animate_color(
                iid.with("tint"),
                if i == near { t.accent } else { t.fg_faint },
                &SELECTION,
            );
            let ring = ui.animate_f32(
                iid.with("ring"),
                if r.focus_visible { 1.0 } else { 0.0 },
                &FOCUS,
            );
            let dot = Rect::new(cell.x + (cell.w - DOT) / 2.0, cy - DOT / 2.0, DOT, DOT);
            let p = ui.painter();
            p.fill(dot, DOT / 2.0, tint);
            let halo = dot.inset(-3.0, -3.0);
            focus_ring::paint(
                p,
                halo,
                RingKind::Edge {
                    radius: halo.h / 2.0,
                },
                ring,
            );

            let label = ui.tr_fmt("luneui.pager.dot", &[&(i + 1), &count]);
            let mut node = Node::new(iid, Role::Tab, &label, cell);
            if r.enabled {
                node.states.insert(State::Focusable);
                for action in [Action::Default, Action::Focus] {
                    node.actions.insert(action);
                }
            } else {
                node.states.insert(State::Disabled);
            }
            if i == page {
                node.states.insert(State::Selected);
            }
            ui.node(node);
            if r.enabled && r.clicked && pick.is_none() {
                pick = Some(i);
            }
        }
    });
    if let Some(i) = ui.rove_keys(&ids, RoveAxis::Horizontal, false) {
        pick = Some(i);
    }
    pick
}
