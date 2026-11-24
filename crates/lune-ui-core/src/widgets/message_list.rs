//! Message list (`layout.message-list`): a scrolling list of messages for a popover inbox, and the
//! header that goes above it.
//!
//! Each [`MessageRow`] has a title (one line, elided), an optional age at the right of that line, an
//! optional body (at most two lines, the second ending in an ellipsis when more remain) and an
//! optional meta line. A row that is `unread` shows a dot in the attention colour
//! ([`Theme::attention`](crate::theme::Theme::attention)) in a slot at its left; a read row keeps
//! the slot blank and draws at 0.6 alpha. [`Ui::message_list`] draws the rows in a scroll area of
//! at most [`MessageListOptions::max_height`], one tab stop with roving focus (Up, Down, Home and
//! End; Enter and Space and a click report [`MessageListResponse::clicked`], a secondary click
//! reports `context`), and [`height`] sizes the list before the popover that holds it. The list
//! reports a click and never opens anything; the app decides what a row does (a widget never builds
//! a context menu, D292).
//!
//! The heights of all rows are measured every frame (a body wraps, so a row's height needs its
//! shaping, which the text system caches); only the rows in view are drawn and named by text.

use std::hash::Hash;

use super::button::{ButtonKind, ButtonOptions, FOCUS, HOVER, button_width_with};
use super::focus_ring::{self, RingKind};
use super::scroll_area::{Bars, Extent, Reveal, ScrollOptions, Target};
use crate::access::{Action, Node, Role, State};
use crate::color::{clear, with_alpha};
use crate::input::NamedKey;
use crate::paint::{Painter, Rect};
use crate::text::{Spec, Style};
use crate::theme::Theme;
use crate::ui::{Response, RoveAxis, Sense, Ui, WidgetId};

/// The height of [`Ui::message_list_header`]: its title is centred in it.
pub const HEADER_HEIGHT: f32 = 40.0;

/// A row's horizontal padding.
const PAD_X: f32 = 12.0;
/// A row's vertical padding.
const PAD_Y: f32 = 9.0;
/// The unread dot's diameter.
const DOT: f32 = 6.0;
/// The space between the dot's slot and the text.
const DOT_GAP: f32 = 10.0;
/// The space between a row's lines.
const LINE_GAP: f32 = 2.0;
/// The space between the title and the age.
const AGE_GAP: f32 = 8.0;
/// A row's corner radius.
const RADIUS: f32 = 6.0;
/// The most lines of body a row shows.
const BODY_LINES: usize = 2;
/// The alpha of a read row's drawing.
const READ_ALPHA: f32 = 0.6;
/// The header's horizontal padding.
const HEADER_PAD: f32 = 12.0;
/// The ellipsis that ends a body cut to two lines.
const ELLIPSIS: &str = "\u{2026}";

/// One message of a [`Ui::message_list`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct MessageRow<'a> {
    /// The first line: one line, elided to the room the age leaves.
    pub title: &'a str,
    /// Right-aligned on the title's line (the app formats it and refreshes it).
    pub age: Option<&'a str>,
    /// Wrapped to at most two lines, the second elided with an ellipsis when more remain.
    pub body: Option<&'a str>,
    /// One line under the body, elided.
    pub meta: Option<&'a str>,
    /// The row is unread: it shows the attention dot and draws at full strength.
    pub unread: bool,
}

impl<'a> MessageRow<'a> {
    /// A read row with `title` and nothing else.
    #[must_use]
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            ..Self::default()
        }
    }

    /// The row with the age text `age`.
    #[must_use]
    pub fn with_age(mut self, age: &'a str) -> Self {
        self.age = Some(age);
        self
    }

    /// The row with the body text `body`.
    #[must_use]
    pub fn with_body(mut self, body: &'a str) -> Self {
        self.body = Some(body);
        self
    }

    /// The row with the meta line `meta`.
    #[must_use]
    pub fn with_meta(mut self, meta: &'a str) -> Self {
        self.meta = Some(meta);
        self
    }

    /// The row marked unread, or read.
    #[must_use]
    pub fn with_unread(mut self, unread: bool) -> Self {
        self.unread = unread;
        self
    }
}

/// What [`Ui::message_list`] is given besides its key, rect and rows.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MessageListOptions<'a> {
    /// The most height the list uses; the rows scroll past it. `f32::INFINITY` is no cap.
    pub max_height: f32,
    /// The accessible name of the list.
    pub label: &'a str,
}

impl<'a> MessageListOptions<'a> {
    /// Options with the accessible name `label` and no height cap.
    #[must_use]
    pub fn new(label: &'a str) -> Self {
        Self {
            max_height: f32::INFINITY,
            label,
        }
    }

    /// The options with the height capped at `max_height` px.
    #[must_use]
    pub fn with_max_height(mut self, max_height: f32) -> Self {
        self.max_height = max_height;
        self
    }
}

/// What [`Ui::message_list`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct MessageListResponse {
    /// The row a click, Enter or Space activated this frame (an index into the rows).
    pub clicked: Option<usize>,
    /// The row a secondary click or the show-menu action reached this frame; the app opens its
    /// menu (D292).
    pub context: Option<usize>,
    /// The height the list used (what [`height`] says for the same rows).
    pub height: f32,
}

/// The room of a row's text: the width less the padding and the dot's slot.
fn text_width(width: f32) -> f32 {
    (width - 2.0 * PAD_X - DOT - DOT_GAP).max(0.0)
}

fn title_style(t: &Theme, alpha: f32) -> Style {
    Style::ui(t.font_base, with_alpha(t.fg_bright, alpha)).medium()
}

fn age_style(t: &Theme, alpha: f32) -> Style {
    Style::ui(t.font_s, with_alpha(t.fg_muted, alpha))
}

fn body_style(t: &Theme, alpha: f32) -> Style {
    Style::ui(t.font_base, with_alpha(t.fg_primary, alpha))
}

fn meta_style(t: &Theme, alpha: f32) -> Style {
    Style::ui(t.font_s, with_alpha(t.fg_muted, alpha))
}

/// A row measured at a width: what its lines are and where they sit.
struct Measured {
    /// The title line's height (the age shares it).
    first: f32,
    /// The body's lines (the second already cut with its ellipsis) and each one's offset from the
    /// body's top; empty with no body.
    body: Vec<(String, f32)>,
    /// The body's height.
    body_h: f32,
    /// The meta line's height; 0 with no meta.
    meta_h: f32,
}

impl Measured {
    fn of<H: Copy + PartialEq>(p: &mut Painter<'_, H>, row: &MessageRow<'_>, width: f32) -> Self {
        let t = p.theme;
        let room = text_width(width);
        let first = p
            .measure(&Spec::plain(row.title, title_style(&t, 1.0)))
            .1
            .max(
                row.age
                    .map_or(0.0, |a| p.measure(&Spec::plain(a, age_style(&t, 1.0))).1),
            );
        let mut body = Vec::new();
        let mut body_h = 0.0;
        if let Some(text) = row.body.filter(|b| !b.is_empty()) {
            let style = body_style(&t, 1.0);
            let lines = p.text_lines(&Spec::plain(text, style).wrapped(room));
            let more = lines.len() > BODY_LINES;
            for (i, line) in lines.iter().take(BODY_LINES).enumerate() {
                let slice = text.get(line.start..line.end).unwrap_or("").trim_end();
                let line_text = if more && i + 1 == BODY_LINES {
                    let cut = Spec::plain(format!("{slice}{ELLIPSIS}"), style);
                    p.elide(&cut, room).runs.remove(0).0
                } else {
                    slice.to_owned()
                };
                body.push((line_text, line.y));
                body_h = line.y + line.height;
            }
        }
        let meta_h = row
            .meta
            .filter(|m| !m.is_empty())
            .map_or(0.0, |m| p.measure(&Spec::plain(m, meta_style(&t, 1.0))).1);
        Self {
            first,
            body,
            body_h,
            meta_h,
        }
    }

    fn height(&self) -> f32 {
        let mut h = 2.0 * PAD_Y + self.first;
        if !self.body.is_empty() {
            h += LINE_GAP + self.body_h;
        }
        if self.meta_h > 0.0 {
            h += LINE_GAP + self.meta_h;
        }
        h
    }
}

/// The height of `row` at `width`: the padding, the title line, the body (at most two lines) and
/// the meta line, with 2 px between the lines.
pub fn row_height<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    row: &MessageRow<'_>,
    width: f32,
) -> f32 {
    Measured::of(p, row, width).height()
}

/// The height a list of `rows` at `width` uses: the rows' heights added up, at most `max_height`.
/// An app sizes its popover with it before the popover opens; `0` with no rows.
pub fn height<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    rows: &[MessageRow<'_>],
    width: f32,
    max_height: f32,
) -> f32 {
    let total: f32 = rows.iter().map(|r| row_height(p, r, width)).sum();
    total.min(max_height.max(0.0))
}

/// The accessible description of a row: "Unread" when it is, then the body and the meta line,
/// joined by ", ".
fn description(unread_name: &str, row: &MessageRow<'_>) -> Option<String> {
    let parts: Vec<&str> = [
        row.unread.then_some(unread_name),
        row.body.filter(|b| !b.is_empty()),
        row.meta.filter(|m| !m.is_empty()),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Draws one row in `r`.
fn paint_row<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    r: Rect,
    row: &MessageRow<'_>,
    m: &Measured,
    fill: crate::color::Rgba,
    ring: f32,
) {
    let t = p.theme;
    let alpha = if row.unread { 1.0 } else { READ_ALPHA };
    if fill[3] > 0 {
        p.fill(r, RADIUS, fill);
    }
    let left = r.x + PAD_X;
    let tx = left + DOT + DOT_GAP;
    let right = r.right() - PAD_X;
    let top = r.y + PAD_Y;
    if row.unread {
        let dy = (m.first - DOT) / 2.0;
        p.fill(
            Rect::new(left, top + dy, DOT, DOT),
            DOT / 2.0,
            t.attention(),
        );
    }
    let mut age_w = 0.0;
    if let Some(age) = row.age {
        let spec = Spec::plain(age, age_style(&t, alpha));
        age_w = p.measure(&spec).0;
        p.text_in(&spec, Rect::new(right - age_w, top, age_w, m.first), 0.0);
        age_w += AGE_GAP;
    }
    let title = p.elide(
        &Spec::plain(row.title, title_style(&t, alpha)),
        (right - tx - age_w).max(0.0),
    );
    p.text_in(&title, Rect::new(tx, top, right - tx, m.first), 0.0);
    let mut y = top + m.first;
    if !m.body.is_empty() {
        y += LINE_GAP;
        for (text, dy) in &m.body {
            p.text(
                &Spec::plain(text.as_str(), body_style(&t, alpha)),
                tx,
                y + dy,
            );
        }
        y += m.body_h;
    }
    if let Some(meta) = row.meta.filter(|s| !s.is_empty()) {
        y += LINE_GAP;
        let spec = p.elide(
            &Spec::plain(meta, meta_style(&t, alpha)),
            (right - tx).max(0.0),
        );
        p.text(&spec, tx, y);
    }
    focus_ring::paint(p, r, RingKind::Edge { radius: RADIUS }, ring);
}

impl Ui<'_> {
    /// A message list under `key` at the top left of `rect`, with the width of `rect`: one row per
    /// [`MessageRow`] in a scroll area at most `opts.max_height` high (and `rect.h`), a `List` node
    /// named `opts.label` with a `ListItem` per row (named by its title, described by its body and
    /// meta line, "Unread" first when it is). One tab stop; Up, Down, Home and End move focus
    /// (Down on the last row wraps), Enter and Space and a click report
    /// [`MessageListResponse::clicked`], a secondary click or the show-menu action reports
    /// `context`. The focused row scrolls into view. With no rows nothing is drawn and the height
    /// is 0.
    pub fn message_list(
        &mut self,
        key: impl Hash,
        rect: Rect,
        rows: &[MessageRow<'_>],
        opts: &MessageListOptions<'_>,
    ) -> MessageListResponse {
        let mut out = MessageListResponse::default();
        if rows.is_empty() {
            return out;
        }
        let t = *self.theme();
        let unread_name = self.tr("luneui.message-list.unread");
        let id = self.id(&key);
        let group_on = self.is_enabled();
        let measured: Vec<Measured> = rows
            .iter()
            .map(|r| Measured::of(self.painter(), r, rect.w))
            .collect();
        let heights: Vec<f32> = measured.iter().map(Measured::height).collect();
        let total: f32 = heights.iter().sum();
        let mut tops = Vec::with_capacity(rows.len());
        let mut y = 0.0;
        for h in &heights {
            tops.push(y);
            y += h;
        }
        let used = total.min(opts.max_height.max(0.0)).min(rect.h.max(0.0));
        out.height = used;
        let area = Rect::new(rect.x, rect.y, rect.w, used);

        let items: Vec<(WidgetId, bool)> =
            (0..rows.len()).map(|i| (id.with(i), group_on)).collect();
        let stop = self.rove_stop(&items, None);

        let mut list = Node::new(id, Role::List, opts.label, area);
        list.description = None;
        if !group_on {
            list.states.insert(State::Disabled);
        }
        let mut clicked = None;
        let mut context = None;
        self.node_container(list, |ui| {
            ui.scope(&key, |ui| {
                let scroll = ScrollOptions {
                    bars: (
                        Bars::Disabled,
                        Bars::Auto.unless_fits(f64::from(total), f64::from(used)),
                    ),
                    ..ScrollOptions::default()
                };
                // The keys come first: the area scrolls on Home and End a child left unread.
                if let Some(to) = ui.rove_keys(&items, RoveAxis::Vertical, true) {
                    ui.scroll_to(
                        "list",
                        Target::span_y(f64::from(tops[to]), f64::from(heights[to])),
                        Reveal::Nearest,
                    );
                }
                ui.scroll_area("list", area, &scroll, |ui, view| {
                    let seen = (view.offset.1 as f32, view.offset.1 as f32 + view.viewport.h);
                    for (i, row) in rows.iter().enumerate() {
                        let r = Rect::new(
                            view.viewport.x,
                            view.viewport.y + tops[i],
                            view.viewport.w,
                            heights[i],
                        );
                        let kid = items[i].0;
                        let sense = if stop == Some(i) {
                            Sense::CLICK_FOCUS
                        } else {
                            Sense::CLICK_ROVE
                        };
                        let resp: Response = ui.interact(kid, r, sense);
                        let keyed = ui.activation_keys(
                            kid,
                            resp.enabled,
                            &[NamedKey::Enter, NamedKey::Space],
                        );
                        if resp.enabled && (resp.clicked || keyed) && clicked.is_none() {
                            clicked = Some(i);
                        }
                        if resp.enabled && resp.secondary_clicked && context.is_none() {
                            context = Some(i);
                        }
                        let lit = if resp.pressed {
                            t.surface_active
                        } else if resp.hovered {
                            t.surface_hover
                        } else {
                            clear(t.surface_hover)
                        };
                        let fill = ui.animate_color(kid.with("fill"), lit, &HOVER);
                        let ring = ui.animate_f32(
                            kid.with("ring"),
                            if resp.focus_visible { 1.0 } else { 0.0 },
                            &FOCUS,
                        );
                        if tops[i] + heights[i] > seen.0 && tops[i] < seen.1 {
                            paint_row(ui.painter(), r, row, &measured[i], fill, ring);
                        }
                        let mut node = Node::new(kid, Role::ListItem, row.title, r);
                        node.description = description(unread_name, row);
                        if resp.enabled {
                            node.states.insert(State::Focusable);
                            node.actions.insert(Action::Default);
                        } else {
                            node.states.insert(State::Disabled);
                        }
                        ui.node(node);
                    }
                    (
                        (),
                        Extent::new(f64::from(view.viewport.w), f64::from(total)),
                    )
                });
            });
        });
        out.clicked = clicked;
        out.context = context;
        out
    }

    /// The header of a message list in `rect`: `title` bold and bright at the left, centred in the
    /// rect's height ([`HEADER_HEIGHT`] is what the rect is usually given), and when `action` is
    /// `Some`, a small ghost text button with that label at the right. Returns whether the button
    /// was clicked this frame.
    pub fn message_list_header(
        &mut self,
        key: impl Hash,
        rect: Rect,
        title: &str,
        action: Option<&str>,
    ) -> bool {
        let t = *self.theme();
        let spec = Spec::plain(title, Style::ui(t.font_base, t.fg_bright).bold());
        let mut right = rect.right() - HEADER_PAD;
        let mut clicked = false;
        if let Some(label) = action {
            let opts = ButtonOptions {
                kind: ButtonKind::Ghost,
                small: true,
                icon: None,
            };
            let w = button_width_with(self.painter(), label, &opts);
            let at = Rect::new(
                rect.right() - HEADER_PAD / 2.0 - w,
                rect.y + (rect.h - super::button::SMALL_HEIGHT) / 2.0,
                w,
                super::button::SMALL_HEIGHT,
            );
            clicked = self
                .scope(&key, |ui| ui.button_with("action", at, label, &opts))
                .clicked;
            right = at.x;
        }
        let room = (right - (rect.x + HEADER_PAD)).max(0.0);
        let spec = self.painter().elide(&spec, room);
        self.painter().text_in(
            &spec,
            Rect::new(rect.x + HEADER_PAD, rect.y, room, rect.h),
            0.0,
        );
        clicked
    }
}
