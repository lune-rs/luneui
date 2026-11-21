//! The bottom tab bar of a phone screen (`widget.tab-bar`, D370): equal-width items of a glyph, a
//! label and an optional badge on a 49 px row.
//!
//! [`Ui::tab_bar`] is [`nav_bar`](super::nav_bar)'s mirror: the row is the rect's top [`HEIGHT`] px
//! and the background fills the whole rect, so the app gives it `HEIGHT + safe_area().bottom`
//! tall at the window's bottom and the bar's colour runs down behind the home indicator. The bar
//! never reads the safe area itself.
//!
//! The bar is one tab stop with roving focus (`core.focus`): Left and Right move focus through
//! the items, Home and End go to the ends, Enter and Space select the focused one. A tap or click
//! selects an item and sets `changed`; a tap on the selected item sets `reselected` (a tab bar's
//! "back to the top"); a secondary click, the show-menu action or a long press sets `context` and
//! opens no menu (D292).

use std::hash::Hash;

use super::nav_bar::{ghost_look, hairline};
use super::nav_item::SELECTION;
use crate::access::{Action, Node, Role, State};
use crate::icon;
use crate::input::NamedKey;
use crate::paint::Rect;
use crate::text::{Spec, Style};
use crate::ui::{RoveAxis, Sense, Ui, WidgetId};

/// The height of the bar's row: the top [`HEIGHT`] px of the rect.
pub const HEIGHT: f32 = 49.0;
/// An item's glyph size, and the distance from the row's top to its line box.
const GLYPH: f32 = 24.0;
const GLYPH_TOP: f32 = 2.0;
/// The label's line box: this high, its bottom this far above the row's bottom.
const LABEL_H: f32 = 14.0;
const LABEL_BOTTOM: f32 = 5.0;
/// The highlight behind the selected item: inset from the item's cell.
const CELL_INSET: (f32, f32) = (6.0, 4.0);
/// The badge: its height, the least width, the text's padding and the text size.
const BADGE_H: f32 = 16.0;
const BADGE_MIN_W: f32 = 16.0;
const BADGE_PAD: f32 = 4.0;
const BADGE_FONT: f32 = 11.0;
/// The badge's left edge from the glyph's centre, and its top from the row's top.
const BADGE_DX: f32 = 6.0;
const BADGE_TOP: f32 = 2.0;

/// One item of a [`Ui::tab_bar`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct TabBarItem<'a> {
    /// A codicon glyph (`icon::*`).
    pub glyph: &'a str,
    /// The label under the glyph, elided to the item's width; also the accessible name.
    pub label: &'a str,
    /// The badge's text ("3", "99+"), the app's own; `None` draws none.
    pub badge: Option<&'a str>,
}

impl<'a> TabBarItem<'a> {
    /// An item drawn as `glyph` over `label`.
    #[must_use]
    pub const fn new(glyph: &'a str, label: &'a str) -> Self {
        Self {
            glyph,
            label,
            badge: None,
        }
    }

    /// The item with a badge reading `text`.
    #[must_use]
    pub const fn with_badge(mut self, text: &'a str) -> Self {
        self.badge = Some(text);
        self
    }
}

/// What a frame of [`Ui::tab_bar`] reports.
#[derive(Debug, Default, Clone, PartialEq)]
#[non_exhaustive]
pub struct TabBarResponse {
    /// A tap, click or key selected another item: `*selected` is now it.
    pub changed: bool,
    /// The selected item was tapped or clicked again.
    pub reselected: bool,
    /// The item a secondary click, the show-menu action or a long press asked a menu for.
    pub context: Option<usize>,
}

impl Ui<'_> {
    /// A bottom tab bar under `key` over `rect`: the background fills the whole rect, a hairline
    /// closes its top edge and the [`HEIGHT`] px row at the top holds the `items`, equally wide.
    /// `*selected` is the selected item; it is clamped for drawing and left alone while out of
    /// range. The node is a `TabList`, the items `Tab` nodes, the selected one marked selected.
    pub fn tab_bar(
        &mut self,
        key: impl Hash,
        rect: Rect,
        items: &[TabBarItem<'_>],
        selected: &mut usize,
    ) -> TabBarResponse {
        let id = self.id(&key);
        let t = *self.theme();
        let group_on = self.is_enabled();
        let hair = hairline(self.painter());
        {
            let p = self.painter();
            p.push_clip(rect);
            p.fill(rect, 0.0, t.surface_overlay);
            p.fill(
                Rect::new(rect.x, rect.y, rect.w, hair),
                0.0,
                t.surface_separator,
            );
        }
        let mut out = TabBarResponse::default();
        if items.is_empty() {
            self.painter().pop_clip();
            return out;
        }
        let shown = (*selected).min(items.len() - 1);
        let ids: Vec<(WidgetId, bool)> = (0..items.len()).map(|i| (id.with(i), group_on)).collect();
        let stop = self.rove_stop(&ids, Some(shown));
        let width = rect.w / items.len() as f32;
        let name = self.tr("luneui.tab-bar.name");
        let node = Node::new(id, Role::TabList, name, rect);
        let mut pick = None;
        self.scope(&key, |ui| {
            ui.node_container(node, |ui| {
                for (i, item) in items.iter().enumerate() {
                    let cell = Rect::new(rect.x + i as f32 * width, rect.y, width, HEIGHT);
                    let iid = ui.id(i);
                    let sense = if stop == Some(i) {
                        Sense::CLICK_FOCUS
                    } else {
                        Sense::CLICK_ROVE
                    };
                    let mut r = ui.interact(iid, cell, sense);
                    if ui.activation_keys(iid, r.enabled, &[NamedKey::Enter, NamedKey::Space]) {
                        r.clicked = true;
                    }
                    let on = i == shown;

                    // The selected cell's fill and the press and hover fill over it.
                    let [sr, sg, sb, _] = t.surface_selected;
                    let sel = ui.animate_color(
                        iid.with("sel"),
                        if on {
                            t.surface_selected
                        } else {
                            [sr, sg, sb, 0]
                        },
                        &SELECTION,
                    );
                    let (touch, ring) = ghost_look(ui, iid, &r);
                    let tint = ui.animate_color(
                        iid.with("tint"),
                        if on { t.fg_primary } else { t.fg_tertiary },
                        &SELECTION,
                    );
                    let hi = cell.inset(CELL_INSET.0, CELL_INSET.1);
                    let radius = t.item_radius;
                    let p = ui.painter();
                    if sel[3] > 0 {
                        p.fill(hi, radius, sel);
                    }
                    if touch[3] > 0 {
                        p.fill(hi, radius, touch);
                    }
                    super::focus_ring::paint(p, hi, super::focus_ring::RingKind::Button, ring);

                    let glyph = icon::glyph(item.glyph, GLYPH, tint);
                    let gw = p.measure(&glyph).0;
                    let gx = cell.x + (cell.w - gw) / 2.0;
                    p.text_centered(&glyph, Rect::new(gx, cell.y + GLYPH_TOP, gw, GLYPH * 1.25));
                    let label = Spec::plain(item.label, Style::ui(t.font_s, tint));
                    let label = p.elide(&label, (cell.w - 4.0).max(0.0));
                    let lw = p.measure(&label).0;
                    p.text_centered(
                        &label,
                        Rect::new(
                            cell.x + (cell.w - lw) / 2.0,
                            cell.bottom() - LABEL_BOTTOM - LABEL_H,
                            lw,
                            LABEL_H,
                        ),
                    );
                    if let Some(text) = item.badge {
                        let spec = Spec::plain(text, Style::ui(BADGE_FONT, t.on_accent).bold());
                        let w = (p.measure(&spec).0 + 2.0 * BADGE_PAD).max(BADGE_MIN_W);
                        let x = (gx + gw / 2.0 + BADGE_DX).min(cell.right() - 2.0 - w);
                        let at = Rect::new(x, cell.y + BADGE_TOP, w, BADGE_H);
                        p.fill(at, BADGE_H / 2.0, t.count_badge);
                        p.text_centered(&spec, at);
                    }

                    let mut node = Node::new(iid, Role::Tab, item.label, cell);
                    if r.enabled {
                        node.states.insert(State::Focusable);
                        for action in [Action::Default, Action::Focus, Action::ShowMenu] {
                            node.actions.insert(action);
                        }
                    } else {
                        node.states.insert(State::Disabled);
                    }
                    if on {
                        node.states.insert(State::Selected);
                    }
                    if let Some(text) = item.badge {
                        node.description = Some(text.to_owned());
                    }
                    ui.node(node);

                    if r.enabled && r.clicked && pick.is_none() {
                        pick = Some(i);
                    }
                    if r.enabled && r.secondary_clicked && out.context.is_none() {
                        out.context = Some(i);
                    }
                }
            });
        });
        self.rove_keys(&ids, RoveAxis::Horizontal, true);
        self.painter().pop_clip();
        if let Some(i) = pick {
            if i == shown && *selected == i {
                out.reselected = true;
            } else if *selected != i {
                *selected = i;
                out.changed = true;
            }
        }
        out
    }
}
