//! The navigation bar of a phone screen (`widget.nav-bar`, D370): a back button, a title and up
//! to three action buttons on a 44 px row.
//!
//! [`Ui::nav_bar`] fills the rect it is given with the bar's background and lays [`HEIGHT`] px of
//! row at the rect's bottom, so the app gives it `safe_area().top + HEIGHT` tall at the window's
//! top and the row sits under the notch. The bar never reads the safe area itself: a bar inside a
//! sheet or a split pads for nothing. It owns no string: the title, the back button's label and
//! each action's name are the app's.
//!
//! The title is centred on the bar's width when the back button and the actions leave room for it
//! at that place, else it takes the space between them; either way it is elided, never wrapped.
//! The buttons are the ghost look in the theme's accent; a finger's hit is the touch target's 44
//! px, so a button needs no hit area of its own.

use std::hash::Hash;

use super::button::{ButtonKind, ButtonLook, ButtonState, FOCUS, HOVER};
use super::focus_ring::{self, RingKind};
use crate::access::{Action, Node, Role, State};
use crate::color::Rgba;
use crate::icon;
use crate::input::NamedKey;
use crate::paint::{Painter, Rect};
use crate::text::{Spec, Style};
use crate::ui::{Response, Sense, Ui, WidgetId};

/// The height of the bar's row: the bottom [`HEIGHT`] px of the rect.
pub const HEIGHT: f32 = 44.0;
/// The glyph of the back button and of an action.
const GLYPH: f32 = 20.0;
/// The space left of the back button's glyph and right of an action, and right of the back label.
const INSET: f32 = 8.0;
/// The space between the back glyph and its label.
const GAP: f32 = 4.0;
/// The cell of an action: its width, and how many a bar takes.
const ACTION_W: f32 = 44.0;
const MAX_ACTIONS: usize = 3;
/// The least clear space between the title and a button.
const CLEAR: f32 = 8.0;
/// The back button's width as a share of the bar's, label included.
const BACK_SHARE: f32 = 0.35;

/// What [`Ui::nav_bar`] draws.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct NavBar<'a> {
    /// The title, in the middle.
    pub title: &'a str,
    /// The back button's label; `None` draws no back button.
    pub back: Option<&'a str>,
    /// The action buttons at the right, left to right; only the first three are drawn.
    pub actions: &'a [NavAction<'a>],
}

impl<'a> NavBar<'a> {
    /// A bar titled `title` with no back button and no actions.
    #[must_use]
    pub const fn new(title: &'a str) -> Self {
        Self {
            title,
            back: None,
            actions: &[],
        }
    }

    /// The bar with a back button labelled `label`.
    #[must_use]
    pub const fn with_back(mut self, label: &'a str) -> Self {
        self.back = Some(label);
        self
    }

    /// The bar with `actions` at the right.
    #[must_use]
    pub const fn with_actions(mut self, actions: &'a [NavAction<'a>]) -> Self {
        self.actions = actions;
        self
    }
}

/// One action button of a [`NavBar`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct NavAction<'a> {
    /// A codicon glyph (`icon::*`).
    pub glyph: &'a str,
    /// The accessible name of the button.
    pub label: &'a str,
}

impl<'a> NavAction<'a> {
    /// An action drawn as `glyph` and named `label`.
    #[must_use]
    pub const fn new(glyph: &'a str, label: &'a str) -> Self {
        Self { glyph, label }
    }
}

/// What a frame of [`Ui::nav_bar`] reports.
#[derive(Debug, Default, Clone, PartialEq)]
#[non_exhaustive]
pub struct NavBarResponse {
    /// The back button was tapped, clicked or activated by Enter or Space.
    pub back: bool,
    /// The index of the action activated this frame.
    pub action: Option<usize>,
}

/// The fill and the focus ring opacity of a ghost button that follows `r`, animated.
pub(super) fn ghost_look(ui: &mut Ui<'_>, id: WidgetId, r: &Response) -> (Rgba, f32) {
    let state = ButtonState {
        hovered: r.hovered,
        pressed: r.pressed,
        focus_visible: r.focus_visible,
    };
    let look = ButtonLook::of(ui.theme(), ButtonKind::Ghost, state);
    let fill = ui.animate_color(id.with("fill"), look.fill, &HOVER);
    let ring = ui.animate_f32(id.with("ring"), look.ring, &FOCUS);
    (fill, ring)
}

/// Draws the fill and ring of a ghost button in `rect`.
pub(super) fn paint_ghost(ui: &mut Ui<'_>, rect: Rect, fill: Rgba, ring: f32) {
    let radius = ui.theme().item_radius;
    let p = ui.painter();
    if fill[3] > 0 {
        p.fill(rect, radius, fill);
    }
    focus_ring::paint(p, rect, RingKind::Button, ring);
}

/// The node of a button the bar draws: `Button` named `name`, focusable, acting on default.
pub(super) fn button_node(id: WidgetId, name: &str, rect: Rect, r: &Response) -> Node {
    let mut node = Node::new(id, Role::Button, name, rect);
    if r.enabled {
        node.states.insert(State::Focusable);
        node.actions.insert(Action::Default);
    } else {
        node.states.insert(State::Disabled);
    }
    node
}

/// The one device pixel of a hairline at scale `s`, in logical px.
pub(super) fn hairline<H: Copy + PartialEq>(p: &Painter<'_, H>) -> f32 {
    1.0 / p.s.max(1.0)
}

impl Ui<'_> {
    /// A navigation bar under `key` over `rect`: the background fills the whole rect, a hairline
    /// closes its bottom edge, and the [`HEIGHT`] px row at the bottom holds the back button, the
    /// title and the actions (a rect shorter than the row lays it from the top and clips). The
    /// node is a `Toolbar` named by the title, the buttons its children. Colours come from the
    /// theme.
    pub fn nav_bar(&mut self, key: impl Hash, rect: Rect, bar: &NavBar<'_>) -> NavBarResponse {
        let id = self.id(&key);
        let t = *self.theme();
        let row_y = if rect.h >= HEIGHT {
            rect.bottom() - HEIGHT
        } else {
            rect.y
        };
        let row = Rect::new(rect.x, row_y, rect.w, HEIGHT);
        let cx = rect.x + rect.w / 2.0;

        let hair = hairline(self.painter());
        {
            let p = self.painter();
            p.push_clip(rect);
            p.fill(rect, 0.0, t.surface_overlay);
            p.fill(
                Rect::new(rect.x, rect.bottom() - hair, rect.w, hair),
                0.0,
                t.surface_separator,
            );
        }

        let actions = &bar.actions[..bar.actions.len().min(MAX_ACTIONS)];
        let mut out = NavBarResponse::default();
        let node = Node::new(id, Role::Toolbar, bar.title, rect);
        self.scope(&key, |ui| {
            ui.node_container(node, |ui| {
                // The back button: its glyph, then the label elided to its share of the bar.
                let mut left = rect.x + INSET;
                if let Some(label) = bar.back {
                    let glyph = icon::glyph(icon::ARROW_LEFT, GLYPH, t.accent);
                    let spec = Spec::plain(label, Style::ui(t.font_title, t.accent));
                    let p = ui.painter();
                    let gw = p.measure(&glyph).0;
                    let room = (rect.w * BACK_SHARE - 2.0 * INSET - gw - GAP).max(0.0);
                    let spec = p.elide(&spec, room);
                    let tw = if label.is_empty() {
                        0.0
                    } else {
                        p.measure(&spec).0
                    };
                    let gap = if label.is_empty() { 0.0 } else { GAP };
                    let w = INSET + gw + gap + tw + INSET;
                    let at = Rect::new(rect.x, row.y, w, HEIGHT);
                    let bid = ui.id("back");
                    let mut r = ui.interact(bid, at, Sense::CLICK_FOCUS);
                    if ui.activation_keys(bid, r.enabled, &[NamedKey::Enter, NamedKey::Space]) {
                        r.clicked = true;
                    }
                    let (fill, ring) = ghost_look(ui, bid, &r);
                    paint_ghost(ui, at, fill, ring);
                    let p = ui.painter();
                    p.text_centered(&glyph, Rect::new(rect.x + INSET, row.y, gw, HEIGHT));
                    if tw > 0.0 {
                        let x = rect.x + INSET + gw + gap;
                        p.text_centered(&spec, Rect::new(x, row.y, tw, HEIGHT));
                    }
                    ui.node(button_node(bid, label, at, &r));
                    if r.enabled && r.clicked {
                        out.back = true;
                    }
                    left = at.right();
                }

                // The actions, right to left from the inset, each in a 44 wide cell.
                let mut right = rect.right() - INSET;
                for (i, action) in actions.iter().enumerate() {
                    let at = Rect::new(
                        rect.right() - INSET - (actions.len() - i) as f32 * ACTION_W,
                        row.y,
                        ACTION_W,
                        HEIGHT,
                    );
                    if i == 0 {
                        right = at.x;
                    }
                    let aid = ui.id(("action", i));
                    let mut r = ui.interact(aid, at, Sense::CLICK_FOCUS);
                    if ui.activation_keys(aid, r.enabled, &[NamedKey::Enter, NamedKey::Space]) {
                        r.clicked = true;
                    }
                    let (fill, ring) = ghost_look(ui, aid, &r);
                    paint_ghost(ui, at, fill, ring);
                    let glyph = icon::glyph(action.glyph, GLYPH, t.accent);
                    ui.painter().text_centered(&glyph, at);
                    ui.node(button_node(aid, action.label, at, &r));
                    if r.enabled && r.clicked && out.action.is_none() {
                        out.action = Some(i);
                    }
                }

                // The title: at the bar's middle when it fits there between the buttons, else in
                // the space between them.
                if !bar.title.is_empty() {
                    let lo = left + CLEAR;
                    let hi = right - CLEAR;
                    let between = (hi - lo).max(0.0);
                    let spec = Spec::plain(bar.title, Style::ui(t.font_title, t.fg_primary).bold());
                    let p = ui.painter();
                    let full = p.measure(&spec).0;
                    let centred = 2.0 * (cx - lo).min(hi - cx).max(0.0);
                    let (spec, w) = if full <= centred {
                        (spec, full)
                    } else {
                        let spec = p.elide(&spec, between);
                        let w = p.measure(&spec).0;
                        (spec, w)
                    };
                    let x = if full <= centred {
                        cx - w / 2.0
                    } else {
                        (cx - w / 2.0).clamp(lo, (hi - w).max(lo))
                    };
                    p.text_centered(&spec, Rect::new(x, row.y, w, HEIGHT));
                }
            });
        });
        self.painter().pop_clip();
        out
    }
}
