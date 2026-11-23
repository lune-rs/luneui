//! A list of key and value rows whose values can be selected (`data.kv-list`).
//!
//! Default: a key column as wide as the longest key (measured), a 16 px gap, rows of 24 with the
//! key (13, tertiary) and the value (13, primary, elided) on a 20 px line 2 px down. Wrap: the
//! same, but a value wraps at the column and its row is the text plus 2 px above and below.
//! Stacked: two columns of cells, the key (11 on a 16 px line, tertiary) over the value (13 mono on
//! a 20 px line), 12 px between rows. Values are [selectable](super::selectable): the keys and
//! the rest of a row take no input, except the optional [copy control](KvOptions::copy).
//!
//! A row can carry a colour swatch before its key ([`KvRow`], [`Ui::kv_rows`]): a legend square
//! the size of the bar chart's, then a gap, then the key. Only rows with a swatch are indented, so
//! a plain row stays flush; the key column is as wide as the widest indent plus key. The swatch is
//! decoration: no node and no hit region. A list of plain pairs ([`Ui::kv_list`]) is the same
//! layout with no swatch.

use std::hash::Hash;
use std::time::Duration;

use super::bar_chart::{SWATCH, SWATCH_RADIUS};
use super::code_block::{COPY_GAP, COPY_SIDE, CopyControl};
use crate::access::{Node, Role, Value};
use crate::color::Rgba;
use crate::motion::Time;
use crate::paint::{Painter, Rect};
use crate::text::{Spec, Style};
use crate::theme::Theme;
use crate::ui::Ui;

/// A row's height in the default mode.
pub const ROW: f32 = 24.0;
/// Between the key column and the value column, and between the two columns of the stacked mode.
pub const GAP: f32 = 16.0;
/// A text line of the default and wrap modes.
pub const LINE: f32 = 20.0;
/// A wrapped value's padding above and below.
pub const WRAP_PAD: f32 = 2.0;
/// The stacked key's line and the gap between its rows.
pub const STACKED_KEY_LINE: f32 = 16.0;
pub const STACKED_ROW_GAP: f32 = 12.0;
/// Between a row's swatch and its key.
pub const SWATCH_GAP: f32 = 8.0;

/// How the pairs are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KvMode {
    /// A key column and an elided value, rows of 24.
    #[default]
    Default,
    /// Two columns of cells, the key over a mono value.
    Stacked,
    /// A value wraps instead of eliding.
    Wrap,
}

/// One row of [`Ui::kv_rows`]: a key, a value and an optional colour swatch before the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct KvRow<'a> {
    /// The key text.
    pub key: &'a str,
    /// The value text.
    pub value: &'a str,
    /// The colour of a legend square before the key; `None` draws none and does not indent the row.
    pub swatch: Option<Rgba>,
}

impl<'a> KvRow<'a> {
    /// A row of `key` and `value` with no swatch.
    #[must_use]
    pub const fn new(key: &'a str, value: &'a str) -> Self {
        Self {
            key,
            value,
            swatch: None,
        }
    }

    /// This row with a swatch of `color` before its key.
    #[must_use]
    pub const fn with_swatch(mut self, color: Rgba) -> Self {
        self.swatch = Some(color);
        self
    }

    /// How far this row's key is indented: the swatch and its gap, or nothing.
    fn indent(&self) -> f32 {
        if self.swatch.is_some() {
            SWATCH + SWATCH_GAP
        } else {
            0.0
        }
    }
}

/// `pairs` as rows with no swatch.
fn plain<'a>(pairs: &[(&'a str, &'a str)]) -> Vec<KvRow<'a>> {
    pairs.iter().map(|(k, v)| KvRow::new(k, v)).collect()
}

/// What [`Ui::kv_list`] is given besides its rect and pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KvOptions {
    pub mode: KvMode,
    /// Values in the mono face (ids, paths, figures). The stacked mode is always mono.
    pub mono: bool,
    /// Each row shows a copy control at its right end while the pointer is on the row or focus is
    /// in it; a click puts that row's value on the clipboard and the control shows a check mark
    /// for [`COPY_FEEDBACK_MS`]. The value's room narrows by the control. Off, nothing moves.
    pub copy: bool,
}

/// How long the check mark shows after a row's copy, in milliseconds of app time.
pub const COPY_FEEDBACK_MS: u64 = 1_500;

/// The room the copy control takes from a value's width: the control and the gap before it, or
/// nothing with `copy` off.
fn copy_room(opts: &KvOptions) -> f32 {
    if opts.copy { COPY_SIDE + COPY_GAP } else { 0.0 }
}

/// What a row keeps for its copy control: when the check mark began, and the value it copied (a
/// row whose value changed shows no check mark).
#[derive(Debug, Default)]
struct RowCopy {
    at: Option<Time>,
    value: String,
}

/// The key text's style at `size` on the same 20 px line.
fn key_style_sized(t: &Theme, size: f32) -> Style {
    Style {
        line: LINE,
        ..Style::ui(size, t.fg_tertiary)
    }
}

/// The value text's style.
fn value_style(t: &Theme, mono: bool) -> Style {
    Style {
        line: LINE,
        ..if mono {
            Style::mono(t.font_base, LINE, t.fg_primary)
        } else {
            Style::ui(t.font_base, t.fg_primary)
        }
    }
}

/// The key column's width: the widest of each row's indent and its key measured.
fn key_column<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    rows: &[KvRow<'_>],
    key_size: f32,
) -> f32 {
    let style = key_style_sized(&p.theme, key_size);
    let mut w = 0.0_f32;
    for row in rows {
        w = w.max(row.indent() + p.measure(&Spec::plain(row.key, style)).0);
    }
    w
}

/// A wrapped value at `value_w`: its spec and its text height (at least one line).
fn wrapped_value<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    value: &str,
    style: Style,
    value_w: f32,
) -> (Spec, f32) {
    let s = Spec::plain(value, style).wrapped(value_w);
    let h = p.measure(&s).1.max(LINE);
    (s, h)
}

/// A wrap row's height for a text `text_h` high.
fn wrap_row(text_h: f32) -> f32 {
    (text_h + 2.0 * WRAP_PAD).max(ROW)
}

/// The height [`Ui::kv_list`] returns for `pairs` in a list `w` wide, without drawing.
#[cfg(test)]
pub(crate) fn height<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    pairs: &[(&str, &str)],
    opts: &KvOptions,
    w: f32,
) -> f32 {
    let key_size = p.theme.font_base;
    height_rows(p, &plain(pairs), opts, w, key_size)
}

/// [`height`] for keys set at `key_size` (the flyout's routes use the small size).
pub(crate) fn height_keyed<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    pairs: &[(&str, &str)],
    opts: &KvOptions,
    w: f32,
    key_size: f32,
) -> f32 {
    height_rows(p, &plain(pairs), opts, w, key_size)
}

/// [`height_keyed`] for rows.
fn height_rows<H: Copy + PartialEq>(
    p: &mut Painter<'_, H>,
    pairs: &[KvRow<'_>],
    opts: &KvOptions,
    w: f32,
    key_size: f32,
) -> f32 {
    if pairs.is_empty() {
        return 0.0;
    }
    match opts.mode {
        KvMode::Default => pairs.len() as f32 * ROW,
        KvMode::Stacked => {
            let rows = pairs.len().div_ceil(2);
            (rows - 1) as f32 * (STACKED_KEY_LINE + LINE + STACKED_ROW_GAP)
                + STACKED_KEY_LINE
                + LINE
        }
        KvMode::Wrap => {
            let value_w = (w - key_column(p, pairs, key_size) - GAP - copy_room(opts)).max(0.0);
            let style = value_style(&p.theme, opts.mono);
            pairs
                .iter()
                .map(|r| wrap_row(wrapped_value(p, r.value, style, value_w).1))
                .sum()
        }
    }
}

impl Ui<'_> {
    /// Key and value `pairs` under `key` with their top left at that of `rect`, wide as `rect`
    /// (its height is ignored). Returns the height drawn. Each pair is a `Group` named by its key
    /// over a `StaticText` key and a selectable `StaticText` value named by the key.
    pub fn kv_list(
        &mut self,
        key: impl Hash,
        rect: Rect,
        pairs: &[(&str, &str)],
        opts: &KvOptions,
    ) -> f32 {
        let key_size = self.theme().font_base;
        self.kv_list_keyed(key, rect, pairs, opts, key_size)
    }

    /// [`Ui::kv_list`] over rows, each of which can carry a colour swatch before its key (a legend
    /// for a chart): a square of the bar chart's legend size, a gap, then the key. Only rows with
    /// a swatch are indented; the key column is as wide as the widest indent plus key. The swatch
    /// is drawn only, with no node and no hit region. Rows with no swatch draw, register and
    /// return exactly what [`Ui::kv_list`] does for the same pairs. Returns the height drawn.
    pub fn kv_rows(
        &mut self,
        key: impl Hash,
        rect: Rect,
        rows: &[KvRow<'_>],
        opts: &KvOptions,
    ) -> f32 {
        let key_size = self.theme().font_base;
        self.kv_rows_keyed(key, rect, rows, opts, key_size)
    }

    /// [`Ui::kv_list`] with the keys set at `key_size` (the stacked mode's keys are always
    /// `font_s`).
    pub(crate) fn kv_list_keyed(
        &mut self,
        key: impl Hash,
        rect: Rect,
        pairs: &[(&str, &str)],
        opts: &KvOptions,
        key_size: f32,
    ) -> f32 {
        self.kv_rows_keyed(key, rect, &plain(pairs), opts, key_size)
    }

    /// [`Ui::kv_rows`] with the keys set at `key_size`.
    fn kv_rows_keyed(
        &mut self,
        key: impl Hash,
        rect: Rect,
        pairs: &[KvRow<'_>],
        opts: &KvOptions,
        key_size: f32,
    ) -> f32 {
        let t = *self.theme();
        let key_style = key_style_sized(&t, key_size);
        let stacked = opts.mode == KvMode::Stacked;
        let key_w = if stacked {
            0.0
        } else {
            key_column(self.painter(), pairs, key_size)
        };
        let col_w = ((rect.w - GAP) / 2.0).max(0.0);
        // The stacked cell's value is narrowed by the control, its key is not.
        let cell_value_w = (col_w - copy_room(opts)).max(0.0);
        let value_x = rect.x + key_w + GAP;
        let room = copy_room(opts);
        let value_w = (rect.w - key_w - GAP - room).max(0.0);

        self.scope(key, |ui| {
            let mut y = rect.y;
            let mut bottom = rect.y;
            for (i, row) in pairs.iter().enumerate() {
                let (k, v) = (&row.key, &row.value);
                let indent = row.indent();
                ui.scope(i.to_string(), |ui| {
                    let gid = ui.id("pair");
                    if stacked {
                        let (col, grid_row) = (i % 2, i / 2);
                        let x = rect.x + col as f32 * (col_w + GAP);
                        let top =
                            rect.y + grid_row as f32 * (STACKED_KEY_LINE + LINE + STACKED_ROW_GAP);
                        let ks = Style {
                            line: STACKED_KEY_LINE,
                            ..Style::ui(t.font_s, t.fg_tertiary)
                        };
                        let key_spec = ui
                            .painter()
                            .elide(&Spec::plain(*k, ks), (col_w - indent).max(0.0));
                        let vs = value_style(&t, true);
                        let val_spec = ui.painter().elide(&Spec::plain(*v, vs), cell_value_w);
                        let cell = Rect::new(x, top, col_w, STACKED_KEY_LINE + LINE);
                        ui.node_container(Node::new(gid, Role::Group, k, cell), |ui| {
                            if let Some(color) = row.swatch {
                                swatch(ui, color, x, top, STACKED_KEY_LINE);
                            }
                            let kr = ui.painter().text(&key_spec, x + indent, top);
                            key_node(ui, gid, k, Rect::new(x + indent, top, kr.0, kr.1));
                            let vr = Rect::new(x, top + STACKED_KEY_LINE, cell_value_w, LINE);
                            ui.selectable_named("value", vr, &val_spec, v, k);
                            let at = Rect::new(
                                x + col_w - COPY_SIDE,
                                top + STACKED_KEY_LINE + (LINE - COPY_SIDE) / 2.0,
                                COPY_SIDE,
                                COPY_SIDE,
                            );
                            ui.row_copy(opts, cell, at, k, v);
                        });
                        bottom = bottom.max(top + STACKED_KEY_LINE + LINE);
                    } else {
                        let wrap = opts.mode == KvMode::Wrap;
                        let vs = value_style(&t, opts.mono);
                        let (val_spec, text_h) = if wrap {
                            wrapped_value(ui.painter(), v, vs, value_w)
                        } else {
                            (ui.painter().elide(&Spec::plain(*v, vs), value_w), LINE)
                        };
                        let row_h = if wrap { wrap_row(text_h) } else { ROW };
                        let top = y + WRAP_PAD;
                        let cell = Rect::new(rect.x, y, rect.w, row_h);
                        ui.node_container(Node::new(gid, Role::Group, k, cell), |ui| {
                            if let Some(color) = row.swatch {
                                swatch(ui, color, rect.x, top, LINE);
                            }
                            let kx = rect.x + indent;
                            let kr = ui.painter().text(&Spec::plain(*k, key_style), kx, top);
                            key_node(ui, gid, k, Rect::new(kx, top, kr.0, kr.1));
                            let vr = Rect::new(value_x, top, value_w, text_h);
                            ui.selectable_named("value", vr, &val_spec, v, k);
                            let at = Rect::new(
                                rect.right() - COPY_SIDE,
                                top + (LINE - COPY_SIDE) / 2.0,
                                COPY_SIDE,
                                COPY_SIDE,
                            );
                            ui.row_copy(opts, cell, at, k, v);
                        });
                        y += row_h;
                        bottom = y;
                    }
                });
            }
            bottom - rect.y
        })
    }
}

impl Ui<'_> {
    /// A row's copy control, under the row's scope: `at` is its rect, `cell` the row. It shows
    /// while the pointer is on the row or focus is on the row's value or on the control, and while
    /// the check mark shows; hidden, it is still a Tab stop, which shows it.
    fn row_copy(&mut self, opts: &KvOptions, cell: Rect, at: Rect, key: &str, value: &str) {
        if !opts.copy {
            return;
        }
        let id = self.scope_id();
        let mut mem = std::mem::take(self.persistent::<RowCopy>(id));
        if mem.value != value {
            mem.value.clear();
            mem.value.push_str(value);
            mem.at = None;
        }
        let focus = self.focus_state().0;
        let within = self.pointer_in(cell)
            || focus == Some(self.id("copy"))
            || focus == Some(self.id("value"));
        let name = self.tr_fmt("luneui.kv-list.copy", &[&key]);
        let said = self.tr("luneui.kv-list.copied");
        let control = CopyControl {
            at,
            name: &name,
            said,
            idle: f32::from(u8::from(within)),
            enabled: !value.is_empty(),
        };
        let hold = Duration::from_millis(COPY_FEEDBACK_MS);
        self.copy_control(&control, &mut mem.at, hold, value);
        *self.persistent::<RowCopy>(id) = mem;
    }
}

/// A row's swatch at (`x`, `top`), centred on a key line `line` high.
fn swatch(ui: &mut Ui<'_>, color: Rgba, x: f32, top: f32, line: f32) {
    let at = Rect::new(x, top + (line - SWATCH) / 2.0, SWATCH, SWATCH);
    ui.painter().fill(at, SWATCH_RADIUS, color);
}

/// The key's `StaticText` node.
fn key_node(ui: &mut Ui<'_>, group: crate::ui::WidgetId, key: &str, at: Rect) {
    let mut node = Node::new(group.with("key"), Role::StaticText, key, at);
    node.value = Some(Value::Text(key.to_owned()));
    ui.node(node);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::UiHarness;

    const LONG: &str = "a value long enough to wrap onto several lines in a narrow column of text";

    /// The measure is the height the draw returns, in every mode, mono or not, with values that
    /// wrap at several widths, for an even and an odd count and for none.
    #[test]
    fn height_is_what_the_list_draws() {
        let pairs: [(&str, &str); 5] = [
            ("Opus", "corp-claude-opus"),
            ("Sonnet", LONG),
            ("A much longer key", "x"),
            ("Haiku", "qwen3-coder-30b-a3b"),
            ("Id", LONG),
        ];
        let mut h = UiHarness::new((600.0, 400.0));
        h.at_ms(0).frame(|ui| {
            for mode in [KvMode::Default, KvMode::Stacked, KvMode::Wrap] {
                for (mono, copy) in [(false, false), (true, false), (false, true), (true, true)] {
                    let opts = KvOptions { mode, mono, copy };
                    for w in [90.0, 200.0, 312.0, 480.0] {
                        for n in [0, 1, 2, 4, 5] {
                            let pairs = &pairs[..n];
                            let measured = height(ui.painter(), pairs, &opts, w);
                            let drawn = ui.kv_list(
                                format!("{mode:?}-{mono}-{copy}-{w}-{n}"),
                                Rect::new(10.0, 10.0, w, 0.0),
                                pairs,
                                &opts,
                            );
                            assert_eq!(
                                measured, drawn,
                                "{mode:?} mono {mono} copy {copy} width {w} pairs {n}"
                            );
                        }
                    }
                }
            }
        });
    }

    /// The measure is the height the draw returns for rows with swatches too: the key column
    /// takes the indent, which narrows a wrapped value.
    #[test]
    fn height_is_what_swatched_rows_draw() {
        let rows = [
            KvRow::new("Usage", LONG),
            KvRow::new("E-cores", "12%").with_swatch([1, 2, 3, 255]),
            KvRow::new("A much longer key", LONG).with_swatch([4, 5, 6, 255]),
            KvRow::new("P-cores", "x"),
        ];
        let mut h = UiHarness::new((600.0, 400.0));
        h.at_ms(0).frame(|ui| {
            for mode in [KvMode::Default, KvMode::Stacked, KvMode::Wrap] {
                for copy in [false, true] {
                    let opts = KvOptions {
                        mode,
                        mono: false,
                        copy,
                    };
                    for w in [120.0, 200.0, 312.0, 480.0] {
                        for n in [0, 1, 2, 4] {
                            let rows = &rows[..n];
                            let key_size = ui.theme().font_base;
                            let measured = height_rows(ui.painter(), rows, &opts, w, key_size);
                            let drawn = ui.kv_rows(
                                format!("{mode:?}-{copy}-{w}-{n}"),
                                Rect::new(10.0, 10.0, w, 0.0),
                                rows,
                                &opts,
                            );
                            assert_eq!(measured, drawn, "{mode:?} copy {copy} width {w} rows {n}");
                        }
                    }
                }
            }
        });
    }
}
