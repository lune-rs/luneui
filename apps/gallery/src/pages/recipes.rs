//! The `recipes` page: seven realistic screens built from the toolkit's composites and
//! `lune_ui::layout`, each in one function an app can copy (`docs/design-recipes.md`). Every
//! recipe lays out from the rect it is given, so `gallery --dump <dir> --only recipe-form
//! --window 402x874 --scale 3` draws it at phone size. Every colour comes from `ui.theme()`;
//! every spacing is a literal here or a const of the composite it quotes (the theme holds none).

use lune_ui::icon;
use lune_ui::layout::{Align, Child, Insets, Size, Track, grid, row};
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::dialog::{Cancel, Dialog, DialogGlass, DialogOptions};
use lune_ui::widgets::field::{FieldOptions, FieldState, Variant};
use lune_ui::widgets::form_field::FormField;
use lune_ui::widgets::label::LabelRole;
use lune_ui::widgets::list_row::{ListRow, SIDE_PANEL_PADDING, height};
use lune_ui::widgets::nav_item::NavEntry;
use lune_ui::widgets::page::{PageOptions, PageSlot};
use lune_ui::widgets::panel::{PanelKind, PanelOptions, PanelSlot, TEXT_INSET};
use lune_ui::widgets::sidebar::{Sidebar, SidebarBody, SidebarFooter, WIDTH as SIDEBAR_WIDTH};
use lune_ui::widgets::stat::StatOptions;
use lune_ui::widgets::table::{Cell, CellTone, ColumnWidth, TableColumn, TableOptions};
use lune_ui::widgets::tone::Tone;
use lune_ui::widgets::virtual_list::{Heights, ListOptions, RowKey, SelectionMode};

use crate::page::Stage;
use crate::page::{Expect, Page, PageKind, PageState, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "recipes",
    title: "Recipes",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: &[],
    }),
};

/// The space between the window's edge and a recipe's content.
const MARGIN: f32 = 16.0;
/// The space between two blocks of a recipe.
const GAP: f32 = 12.0;
/// The window width under which the sidebar recipe shows one figure instead of two: the 208 px
/// column leaves the content less than two cells need.
const NARROW: f32 = 560.0;
/// The widest a form grows: a wider window leaves the rest empty.
const FORM_W: f32 = 480.0;
/// Where a mono text's top sits in a 32 px row, so its middle matches the sans text beside it
/// (the sans text sits 6 px down).
const MONO_DROP: f32 = 4.5;
/// A title line (`LabelRole::Title`) and a control's height.
const TITLE_H: f32 = 24.0;
const CONTROL_H: f32 = 32.0;

/// The upstreams the list and table recipes show: name, address, status, latency in ms.
const UPSTREAMS: [(&str, &str, &str, u32); 12] = [
    ("qwen2.5-coder", "10.0.0.4:8443", "up", 118),
    ("llama3.1", "10.0.0.5:8443", "up", 142),
    ("mistral", "10.0.0.6:8443", "up", 97),
    ("phi-4", "10.0.0.7:8443", "slow", 612),
    ("gemma2", "10.0.0.8:8443", "up", 131),
    ("deepseek-r1", "10.0.0.9:8443", "down", 0),
    ("command-r", "10.0.0.10:8443", "up", 156),
    ("yi-coder", "10.0.0.11:8443", "up", 109),
    ("granite-3", "10.0.0.12:8443", "up", 124),
    ("codestral", "10.0.0.13:8443", "slow", 540),
    ("starcoder2", "10.0.0.14:8443", "up", 101),
    ("smollm2", "10.0.0.15:8443", "up", 88),
];

/// The screens of the sidebar recipe.
const SCREENS: [(u64, &str, &str); 4] = [
    (1, "Overview", icon::INFO),
    (2, "Requests", icon::SEARCH),
    (3, "Upstreams", icon::SYNC),
    (4, "Settings", icon::GEAR),
];

/// Which recipe a state draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    List,
    Table,
    Panel,
    Page,
    Sidebar,
    Form,
    Dialog,
}

/// The recipes, in the order the knob lists them, with their names.
const KINDS: [(&str, Kind); 7] = [
    ("List", Kind::List),
    ("Table", Kind::Table),
    ("Panel", Kind::Panel),
    ("Page", Kind::Page),
    ("Sidebar", Kind::Sidebar),
    ("Form", Kind::Form),
    ("Dialog", Kind::Dialog),
];

/// What the recipes keep between frames: the app's own state, one place for all seven.
struct Data {
    /// The list's filter text.
    filter: String,
    /// The sidebar's selected screen.
    nav: u64,
    /// The form's values.
    name: String,
    email: String,
    port: String,
    /// The dialog is open.
    open: bool,
}

impl Data {
    fn new() -> Self {
        Self {
            filter: String::new(),
            nav: 1,
            name: "staging".to_owned(),
            email: "ops-example.com".to_owned(),
            port: "8443".to_owned(),
            open: true,
        }
    }
}

/// One recipe at a time: the window's page (the knob picks) and every scene.
struct Recipes {
    kind: Kind,
    data: Data,
}

impl Recipes {
    fn of(kind: Kind) -> Box<dyn PageState> {
        Box::new(Self {
            kind,
            data: Data::new(),
        })
    }
}

fn live() -> Box<dyn PageState> {
    Recipes::of(Kind::List)
}

fn list() -> Box<dyn PageState> {
    Recipes::of(Kind::List)
}

fn table() -> Box<dyn PageState> {
    Recipes::of(Kind::Table)
}

fn panel() -> Box<dyn PageState> {
    Recipes::of(Kind::Panel)
}

fn page() -> Box<dyn PageState> {
    Recipes::of(Kind::Page)
}

fn sidebar() -> Box<dyn PageState> {
    Recipes::of(Kind::Sidebar)
}

fn form() -> Box<dyn PageState> {
    Recipes::of(Kind::Form)
}

fn dialog() -> Box<dyn PageState> {
    Recipes::of(Kind::Dialog)
}

impl PageState for Recipes {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let window = stage.area(ui);
        let background = ui.theme().glass.window.tint;
        ui.painter().fill(window, 0.0, background);
        let area = shrink(window, MARGIN);
        let d = &mut self.data;
        match self.kind {
            Kind::List => recipe_list(ui, area, d),
            Kind::Table => recipe_table(ui, area),
            Kind::Panel => recipe_panel(ui, area),
            Kind::Page => recipe_page(ui, area),
            Kind::Sidebar => recipe_sidebar(ui, shrink(window, MARGIN / 2.0), d),
            Kind::Form => recipe_form(ui, area, d),
            Kind::Dialog => recipe_dialog(ui, window, d),
        }
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        let names: Vec<&str> = KINDS.iter().map(|(n, _)| *n).collect();
        let mut at = KINDS.iter().position(|(_, k)| *k == self.kind).unwrap_or(0);
        if panel.choice(ui, "Recipe", &names, &mut at) {
            self.kind = KINDS[at].1;
            self.data.open = true;
        }
    }
}

/// `r` less `by` on every side.
fn shrink(r: Rect, by: f32) -> Rect {
    Rect::new(
        r.x + by,
        r.y + by,
        (r.w - 2.0 * by).max(0.0),
        (r.h - 2.0 * by).max(0.0),
    )
}

/// A title line over a block: `area` split into the title's row and the block under it.
fn titled(ui: &mut Ui<'_>, area: Rect, title: &str) -> Rect {
    let rows = [Track::Fixed(TITLE_H), Track::Star(1.0)];
    let g = grid(area, &[Track::Star(1.0)], &rows, 0.0, GAP, Insets::ZERO);
    let at = g.cell(0, 0);
    ui.label("title", (at.x, at.y), title, LabelRole::Title);
    g.cell(0, 1)
}

/// The status word of `UPSTREAMS` as the tone its dot and cell take.
fn tone_of(status: &str) -> Tone {
    match status {
        "up" => Tone::Ok,
        "slow" => Tone::Warn,
        _ => Tone::Err,
    }
}

/// List: a title, a filter field and a list of rows with a status dot, a name, an address and a
/// status badge. The filter narrows the rows; the list scrolls inside its rect.
fn recipe_list(ui: &mut Ui<'_>, area: Rect, d: &mut Data) {
    let body = titled(ui, area, "Upstreams");
    let rows = [Track::Fixed(CONTROL_H), Track::Star(1.0)];
    let g = grid(body, &[Track::Star(1.0)], &rows, 0.0, GAP, Insets::ZERO);
    let filter = FieldOptions {
        placeholder: "Filter upstreams",
        variant: Variant::Search,
        ..FieldOptions::default()
    };
    ui.text_field_with("filter", g.cell(0, 0), &mut d.filter, &filter);
    let shown = matching(&d.filter);
    let row_of = |u: &(&'static str, &'static str, &'static str, u32)| ListRow {
        name: u.0,
        sub: Some(u.1),
        badge: Some((u.2, tone_of(u.2))),
        dot: Some(tone_of(u.2)),
        ..ListRow::default()
    };
    // A row is keyed by its upstream, not by its place among the matches, so the selection stays
    // on the same upstream when the filter changes; `index_of` finds a key's place again.
    let index_of = |k: RowKey| shown.iter().position(|&u| u as u64 == k.0);
    let opts = ListOptions {
        count: shown.len(),
        heights: Heights::Fixed(height(&row_of(&UPSTREAMS[0]), SIDE_PANEL_PADDING)),
        selection: SelectionMode::Single,
        index_of: Some(&index_of),
        label: "Upstreams",
    };
    if shown.is_empty() {
        // An empty state says why it is empty.
        let at = g.cell(0, 1);
        let text = "No upstream matches the filter.";
        ui.label("empty", (at.x + 8.0, at.y + 8.0), text, LabelRole::Small);
    }
    let key = |i: usize| RowKey(shown[i] as u64);
    ui.virtual_list("list", g.cell(0, 1), &opts, key, |ui, ctx| {
        ui.list_row(
            ctx,
            &row_of(&UPSTREAMS[shown[ctx.index]]),
            SIDE_PANEL_PADDING,
        )
    });
}

/// The indices into `UPSTREAMS` of the upstreams whose name or address holds `filter`.
fn matching(filter: &str) -> Vec<usize> {
    let needle = filter.to_lowercase();
    (0..UPSTREAMS.len())
        .filter(|&i| UPSTREAMS[i].0.contains(&needle) || UPSTREAMS[i].1.contains(&needle))
        .collect()
}

/// The table's columns: the name fills what the fixed two leave.
const COLUMNS: [TableColumn<'static>; 3] = [
    TableColumn {
        key: 1,
        title: "Upstream",
        width: ColumnWidth::Fill(1.0),
        num: false,
        sortable: false,
    },
    TableColumn {
        key: 2,
        title: "Status",
        width: ColumnWidth::Fixed(72.0),
        num: false,
        sortable: false,
    },
    TableColumn {
        key: 3,
        title: "Latency ms",
        width: ColumnWidth::Fixed(96.0),
        num: true,
        sortable: false,
    },
];

/// Table: a title and a data table with a header row, one Fill column and numbers right-aligned in
/// mono. Cells are text; a long name is elided by the table.
fn recipe_table(ui: &mut Ui<'_>, area: Rect) {
    let body = titled(ui, area, "Latency by upstream");
    let opts = TableOptions {
        columns: &COLUMNS,
        count: UPSTREAMS.len(),
        selection: SelectionMode::Single,
        wrap: false,
        index_of: None,
        label: "Latency by upstream",
    };
    ui.table(
        "table",
        body,
        &opts,
        |i| RowKey(i as u64),
        |_, c| {
            let u = &UPSTREAMS[c.row];
            match c.column {
                0 => Cell::text(u.0),
                1 => Cell::text(u.2).tone(match tone_of(u.2) {
                    Tone::Ok => CellTone::Default,
                    Tone::Warn => CellTone::Warn,
                    _ => CellTone::Err,
                }),
                _ if u.3 == 0 => Cell::text("-").tone(CellTone::Dim),
                _ => Cell::text(u.3.to_string()),
            }
        },
    );
}

/// Panel: a strip of three figures over a flush panel holding a list. The strip fills its rect and
/// the list panel takes the rest; each draws its own outline.
fn recipe_panel(ui: &mut Ui<'_>, area: Rect) {
    let t = *ui.theme();
    let rows = [Track::Fixed(88.0), Track::Star(1.0)];
    let g = grid(area, &[Track::Star(1.0)], &rows, 0.0, GAP, Insets::ZERO);
    let cells = [
        StatOptions {
            label: "Requests",
            value: "8,412",
            sub: Some("3 failed"),
            sub_tone: Tone::Warn,
            ..StatOptions::default()
        },
        StatOptions {
            label: "Median",
            value: "118 ms",
            sub: Some("-6% vs last hour"),
            sub_tone: Tone::Ok,
            ..StatOptions::default()
        },
        StatOptions {
            label: "Upstreams",
            value: "11/12",
            sub: Some("1 down"),
            ..StatOptions::default()
        },
    ];
    let strip = PanelOptions {
        kind: PanelKind::Strip,
        name: "Summary",
        ..PanelOptions::default()
    };
    ui.panel("strip", g.cell(0, 0), &strip, |ui, slot| match slot {
        PanelSlot::Body(r) => ui.stat_strip("cells", r, &cells),
        PanelSlot::Footer(_) => 0.0,
    });
    let flush = PanelOptions {
        title: Some("Recent requests"),
        meta: Some("last hour"),
        kind: PanelKind::Flush,
        ..PanelOptions::default()
    };
    ui.panel("list", g.cell(0, 1), &flush, |ui, slot| {
        let PanelSlot::Body(r) = slot else {
            return 0.0;
        };
        for (i, u) in UPSTREAMS.iter().enumerate() {
            let line = Rect::new(r.x, r.y + i as f32 * CONTROL_H, r.w, CONTROL_H);
            if line.bottom() > r.bottom() {
                break;
            }
            if i % 2 == 0 {
                ui.painter().fill(line, 0.0, t.surface_hover);
            }
            let status = Spec::plain(u.2, Style::mono(t.font_base, 20.0, t.fg_secondary));
            let p = ui.painter();
            let sw = p.measure(&status).0;
            let sx = line.right() - TEXT_INSET - 8.0 - sw;
            p.text(&status, sx, line.y + MONO_DROP);
            let name = Spec::plain(u.0, Style::ui(t.font_base, t.fg_primary));
            let name = p.elide(&name, (sx - GAP - line.x - TEXT_INSET - 8.0).max(0.0));
            p.text(&name, line.x + TEXT_INSET + 8.0, line.y + 6.0);
        }
        r.h
    });
}

/// Page: the page card with a title, a subtitle, one header action, a scrolling body (a strip of
/// figures and rows) and a pinned footer of two buttons.
fn recipe_page(ui: &mut Ui<'_>, area: Rect) {
    let t = *ui.theme();
    let opts = PageOptions {
        title: "Overview",
        subtitle: Some("Last 30 days, all upstreams"),
        glass: t.glass.panel,
        under: t.glass.card.premultiplied(),
        actions_w: 88.0,
        footer: true,
        swap_key: 0,
        name: "",
    };
    ui.page("page", area, &opts, |ui, slot| match slot {
        PageSlot::Actions(at) => {
            ui.button(
                "export",
                Rect::new(at.x, at.y, at.w, CONTROL_H),
                "Export",
                ButtonKind::Secondary,
            );
            0.0
        }
        PageSlot::Body(_, inner) => {
            let cells = [
                StatOptions {
                    label: "Requests",
                    value: "8,412",
                    ..StatOptions::default()
                },
                StatOptions {
                    label: "Median",
                    value: "118 ms",
                    ..StatOptions::default()
                },
            ];
            let strip = ui.stat_strip("strip", Rect::new(inner.x, inner.y, inner.w, 0.0), &cells);
            let top = inner.y + strip + GAP;
            let mut y = top;
            for (i, u) in UPSTREAMS.iter().enumerate() {
                let line = Rect::new(inner.x, y, inner.w, CONTROL_H);
                if i % 2 == 0 {
                    ui.painter().fill(line, 0.0, t.surface_hover);
                }
                let value = Spec::plain(
                    if u.3 == 0 {
                        "-".to_owned()
                    } else {
                        format!("{} ms", u.3)
                    },
                    Style::mono(t.font_base, 20.0, t.fg_secondary),
                );
                let p = ui.painter();
                let vw = p.measure(&value).0;
                p.text(&value, line.right() - TEXT_INSET - vw, line.y + MONO_DROP);
                let name = Spec::plain(u.0, Style::ui(t.font_base, t.fg_primary));
                let name = p.elide(&name, (line.w - vw - GAP - 2.0 * TEXT_INSET).max(0.0));
                p.text(&name, line.x + TEXT_INSET, line.y + 6.0);
                y += CONTROL_H;
            }
            y - inner.y
        }
        PageSlot::Footer(r) => {
            let cols = [
                Child {
                    main: Size::Fill(1.0),
                    cross: 0.0,
                },
                Child {
                    main: Size::Fixed(96.0),
                    cross: 0.0,
                },
                Child {
                    main: Size::Fixed(96.0),
                    cross: 0.0,
                },
            ];
            let at = row(r, &cols, 8.0, Align::Stretch, Insets::ZERO);
            ui.button("back", at[1], "Back", ButtonKind::Secondary);
            ui.button("next", at[2], "Continue", ButtonKind::Primary);
            0.0
        }
    });
}

/// Sidebar: the 208 px navigation column at the left and the screen it selects filling the rest.
/// The column keeps its width, so a narrower window gives the content less.
fn recipe_sidebar(ui: &mut Ui<'_>, area: Rect, d: &mut Data) {
    let cols = [
        Child {
            main: Size::Fixed(SIDEBAR_WIDTH),
            cross: 0.0,
        },
        Child {
            main: Size::Fill(1.0),
            cross: 0.0,
        },
    ];
    let at = row(area, &cols, GAP, Align::Stretch, Insets::ZERO);
    let entries: Vec<NavEntry<'_>> = SCREENS
        .iter()
        .map(|(key, label, glyph)| NavEntry {
            key: *key,
            label,
            icon: Some(glyph),
            enabled: true,
        })
        .collect();
    let footer = SidebarFooter {
        tone: Tone::Ok,
        text: "Proxy on 127.0.0.1:4646",
        mono: true,
    };
    ui.sidebar(
        "nav",
        at[0],
        Sidebar {
            body: SidebarBody::Nav {
                entries: &entries,
                selected: &mut d.nav,
            },
            footer: Some(footer),
            name: "Main navigation",
        },
    );
    let title = SCREENS
        .iter()
        .find(|s| s.0 == d.nav)
        .map_or("Overview", |s| s.1);
    let body = titled(ui, shrink(at[1], 4.0), title);
    let cells = [
        StatOptions {
            label: "Requests",
            value: "8,412",
            ..StatOptions::default()
        },
        StatOptions {
            label: "Median",
            value: "118 ms",
            ..StatOptions::default()
        },
    ];
    // The column keeps its width, so a narrow window leaves the content one cell.
    let cells = if area.w < NARROW {
        &cells[..1]
    } else {
        &cells[..]
    };
    let strip = PanelOptions {
        kind: PanelKind::Strip,
        fit: true,
        name: "Summary",
        ..PanelOptions::default()
    };
    ui.panel("summary", body, &strip, |ui, slot| match slot {
        PanelSlot::Body(r) => ui.stat_strip("cells", r, cells),
        PanelSlot::Footer(_) => 0.0,
    });
}

/// Form: a title, three labelled fields (one in error, one inline), and Cancel and Save at the
/// foot. The form is at most [`FORM_W`] wide and keeps the left edge on a wider window.
fn recipe_form(ui: &mut Ui<'_>, area: Rect, d: &mut Data) {
    let body = titled(
        ui,
        Rect::new(area.x, area.y, area.w.min(FORM_W), area.h),
        "Add upstream",
    );
    let valid = d.email.contains('@') && !d.name.is_empty();
    let mut y = body.y;
    let name = FormField {
        label: "Name",
        hint: Some("Shown in the request log."),
        error: d.name.is_empty().then_some("Name the upstream."),
        inline: false,
        control_size: (0.0, CONTROL_H),
    };
    let value = &mut d.name;
    y += GAP
        + ui.form_field("name", Rect::new(body.x, y, body.w, 0.0), &name, |ui, r| {
            ui.text_field("input", r, value, "Name", FieldState::default())
                .id
        });
    let email = FormField {
        label: "Contact",
        hint: Some("We write here when it goes down."),
        error: (!d.email.contains('@')).then_some("Enter an address like name@example.com."),
        inline: false,
        control_size: (0.0, CONTROL_H),
    };
    let value = &mut d.email;
    let state = FieldState {
        error: !value.contains('@'),
    };
    y += GAP
        + ui.form_field(
            "email",
            Rect::new(body.x, y, body.w, 0.0),
            &email,
            |ui, r| ui.text_field("input", r, value, "Contact", state).id,
        );
    let port = FormField {
        label: "Port",
        hint: None,
        error: None,
        inline: true,
        control_size: (120.0, CONTROL_H),
    };
    let value = &mut d.port;
    y += GAP
        + ui.form_field("port", Rect::new(body.x, y, body.w, 0.0), &port, |ui, r| {
            ui.text_field("input", r, value, "Port", FieldState::default())
                .id
        });
    let cols = [
        Child {
            main: Size::Fill(1.0),
            cross: 0.0,
        },
        Child {
            main: Size::Fixed(88.0),
            cross: 0.0,
        },
        Child {
            main: Size::Fixed(88.0),
            cross: 0.0,
        },
    ];
    let foot = Rect::new(body.x, y + GAP, body.w, CONTROL_H);
    let at = row(foot, &cols, 8.0, Align::Stretch, Insets::ZERO);
    ui.button("cancel", at[1], "Cancel", ButtonKind::Secondary);
    ui.with_enabled(valid, |ui| {
        ui.button("save", at[2], "Save", ButtonKind::Primary);
    });
}

/// Dialog: a screen with a confirm dialog open over it. The dialog is a modal overlay: it
/// dims and blurs what is behind it, owns the keys while it is open, and reports `confirmed` or
/// `cancelled` once; the app closes it by not calling it the next frame.
fn recipe_dialog(ui: &mut Ui<'_>, window: Rect, d: &mut Data) {
    let mut behind = Data::new();
    recipe_list(ui, shrink(window, MARGIN), &mut behind);
    if !d.open {
        return;
    }
    let spec = Dialog {
        title: "Remove upstream",
        message: "deepseek-r1 stops receiving requests. You can add it again later.",
        extra_h: 28.0,
        confirm: "Remove",
        cancel: Cancel::Default,
    };
    let glass = DialogGlass::of(ui.theme());
    let r = ui.dialog_with(
        "dialog",
        &spec,
        &DialogOptions::default(),
        &glass,
        |ui, bx, by, _| {
            ui.label("note", (bx, by + 12.0), "10.0.0.9:8443", LabelRole::Mono);
        },
    );
    if r.confirmed || r.cancelled {
        d.open = false;
    }
}

/// A recipe at rest. The dialog is captured after its 250 ms open.
const fn scene(id: &'static str, state: fn() -> Box<dyn PageState>, capture_ms: u32) -> UiScene {
    UiScene {
        id,
        scale: 2.0,
        tier: MotionTier::Full,
        state,
        script: &[],
        capture_ms,
        expect: Expect::Settled,
    }
}

const SCENES: &[UiScene] = &[
    scene("recipe-list", list, 0),
    scene("recipe-table", table, 0),
    scene("recipe-panel", panel, 0),
    scene("recipe-page", page, 0),
    scene("recipe-sidebar", sidebar, 0),
    scene("recipe-form", form, 0),
    scene("recipe-dialog", dialog, 700),
];

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = include_str!("../../../../docs/design-recipes.md");
    const SOURCE: &str = include_str!("recipes.rs");

    /// The `recipe-` ids a text names (a `recipe-` followed by lowercase letters and hyphens).
    fn named_ids(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find("recipe-") {
            let tail = &rest[at..];
            let end = tail
                .find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
                .unwrap_or(tail.len());
            out.push(tail[..end].trim_end_matches('-').to_owned());
            rest = &tail[end..];
        }
        out
    }

    #[test]
    fn every_recipe_scene_is_in_the_design_doc() {
        assert_eq!(SCENES.len(), 7);
        for s in SCENES {
            assert!(DOC.contains(s.id), "the doc names no `{}`", s.id);
            let function = s.id.replace('-', "_");
            assert!(DOC.contains(&function), "the doc names no `{function}`");
            assert!(
                SOURCE.contains(&format!("fn {function}(")),
                "recipes.rs has no `fn {function}`"
            );
        }
        let named = named_ids(DOC);
        assert!(!named.is_empty(), "the doc names no recipe scene at all");
        for id in named {
            assert!(
                SCENES.iter().any(|s| s.id == id),
                "the doc names `{id}`, which is no scene"
            );
        }
    }

    #[test]
    fn a_list_row_keeps_its_key_when_the_filter_changes() {
        let all = matching("");
        assert_eq!(all, (0..UPSTREAMS.len()).collect::<Vec<_>>());
        // `llama3.1` is row 1 unfiltered and row 0 of the matches, but its key is 1 in both.
        let llama = matching("llama");
        assert_eq!(llama, [1]);
        assert_eq!(UPSTREAMS[llama[0]].0, "llama3.1");
        let index_of = |k: u64| llama.iter().position(|&u| u as u64 == k);
        assert_eq!(
            index_of(1),
            Some(0),
            "the key finds its row among the matches"
        );
        assert_eq!(index_of(0), None, "a key the filter hid has no row");
        assert!(matching("zzz").is_empty());
    }

    #[test]
    fn the_scan_for_named_ids_finds_a_known_one_and_stops_at_punctuation() {
        assert_eq!(
            named_ids("see `recipe-form`, and recipe-list."),
            ["recipe-form", "recipe-list"]
        );
    }
}
