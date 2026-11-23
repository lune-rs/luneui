//! Data: counters that count, a stacked bar chart that grows in on every entry and then updates, a
//! pie chart whose slices retarget, a table the autopilot re-sorts (the rows slide to their new
//! places), a kanban board where a card moves from column to column, and meters. Every figure is
//! the component's own motion (`value`, `settle`, `move`) or the app's easing over the first
//! second after the page has landed.
//!
//! The autopilot changes one thing per step (a few bars, the pie, the table's order, the counters,
//! a card, the meters), one at a time; a person's hand holds it back for
//! [`RESUME`](super::RESUME). Under Reduced, Off and `--still` the scene shows its state at rest.

use std::time::Duration;

use lune_ui::color::{Rgba, with_alpha};
use lune_ui::motion::{Kind, Motion};
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::bar_chart::{self, BarChartOptions, BarColumn, BarSeries};
use lune_ui::widgets::kanban_board::{BoardOptions, KanbanCard, KanbanColumn};
use lune_ui::widgets::pie_chart::{self, PieOptions, PieSlice};
use lune_ui::widgets::progress::ProgressOptions;
use lune_ui::widgets::stat::StatOptions;
use lune_ui::widgets::table::{
    self, Cell, CellCtx, ColumnWidth, ROW, SortKey, TableColumn, TableOptions,
};
use lune_ui::widgets::tone::Tone;
use lune_ui::widgets::virtual_list::{RowKey, SelectionMode};

use super::{CARD_PAD, Entry, Env, GAP, Pilot, Targets, caps, grown, line, scrolled, tile};

/// The autopilot's step.
pub const STEP: Duration = Duration::from_millis(1500);
/// How long the grow-in takes from the page's landing.
pub const GROW: Duration = Duration::from_millis(1100);
/// How many bars the chart has.
pub const BARS: usize = 14;
/// The bar chart's largest value (the plot's top).
const BAR_MAX: f64 = 500.0;

/// One of the cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Card {
    /// The counters.
    Stats,
    /// The bar chart.
    Bars,
    /// The pie chart.
    Pie,
    /// The table.
    Table,
    /// The board.
    Board,
    /// The meters.
    Meters,
}

/// Where the cards sit in a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Every card and its rect, in the body's coordinates.
    pub cards: Vec<(Card, Rect)>,
    /// The content's height from the body's top.
    pub height: f32,
}

/// The least height of a card.
fn min_h(card: Card) -> f32 {
    match card {
        Card::Stats => 100.0,
        Card::Bars | Card::Pie | Card::Table => 292.0,
        Card::Board | Card::Meters => 336.0,
    }
}

/// The board's own width: three columns of the toolkit's width and its margins.
const BOARD_W: f32 = 3.0 * 272.0 + 2.0 * 10.0 + 2.0 * 12.0 + 8.0;

/// Lays the cards out: three rows with room (the counters; the bars, the pie and the table; the
/// board and the meters), two columns on a middle window, one stacked column (which scrolls) on a
/// narrow one. Spare height stretches the rows below the counters by at most 40 %.
pub fn plan(body: Rect, wide: bool) -> Plan {
    let rows: Vec<Vec<(Card, f32)>> = if wide && body.w >= 1000.0 {
        let avail = body.w - 2.0 * GAP;
        let (a, b) = (avail * 0.40, avail * 0.27);
        let board = BOARD_W.min(body.w - GAP - 240.0);
        vec![
            vec![(Card::Stats, body.w)],
            vec![
                (Card::Bars, a),
                (Card::Pie, b),
                (Card::Table, avail - a - b),
            ],
            vec![(Card::Board, board), (Card::Meters, body.w - board - GAP)],
        ]
    } else if wide && body.w >= 600.0 {
        let half = (body.w - GAP) / 2.0;
        vec![
            vec![(Card::Stats, body.w)],
            vec![(Card::Bars, half), (Card::Pie, half)],
            vec![(Card::Table, half), (Card::Meters, half)],
            vec![(Card::Board, body.w)],
        ]
    } else {
        [
            Card::Stats,
            Card::Bars,
            Card::Pie,
            Card::Table,
            Card::Meters,
            Card::Board,
        ]
        .iter()
        .map(|c| vec![(*c, body.w)])
        .collect()
    };
    let mins: Vec<f32> = rows
        .iter()
        .map(|r| r.iter().map(|(c, _)| min_h(*c)).fold(0.0, f32::max))
        .collect();
    let gaps = GAP * (rows.len() as f32 - 1.0);
    let need = mins.iter().sum::<f32>() + gaps;
    let first = mins[0];
    let k = if rows.len() <= 3 {
        ((body.h - gaps - first) / (need - gaps - first)).clamp(1.0, 1.4)
    } else {
        1.0
    };
    let mut y = body.y;
    let mut cards = Vec::new();
    for (i, (row, h)) in rows.iter().zip(&mins).enumerate() {
        let h = if i == 0 { *h } else { h * k };
        let mut x = body.x;
        for (card, w) in row {
            cards.push((*card, Rect::new(x, y, *w, h)));
            x += w + GAP;
        }
        y += h + GAP;
    }
    Plan {
        cards,
        height: y - GAP - body.y,
    }
}

/// One service of the table.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Service {
    name: &'static str,
    requests: u32,
    latency: u32,
}

const SERVICES: [Service; 7] = [
    Service {
        name: "Search",
        requests: 18_420,
        latency: 41,
    },
    Service {
        name: "Checkout",
        requests: 9_210,
        latency: 118,
    },
    Service {
        name: "Auth",
        requests: 15_730,
        latency: 27,
    },
    Service {
        name: "Billing",
        requests: 2_860,
        latency: 164,
    },
    Service {
        name: "Media",
        requests: 11_380,
        latency: 96,
    },
    Service {
        name: "Notify",
        requests: 6_140,
        latency: 58,
    },
    Service {
        name: "Mail",
        requests: 4_470,
        latency: 73,
    },
];

const NAME: u64 = 1;
const REQUESTS: u64 = 2;
const LATENCY: u64 = 3;

const COLUMNS: [TableColumn<'static>; 3] = [
    TableColumn {
        key: NAME,
        title: "Service",
        width: ColumnWidth::Fill(1.0),
        num: false,
        sortable: true,
    },
    TableColumn {
        key: REQUESTS,
        title: "Requests",
        width: ColumnWidth::Fixed(92.0),
        num: true,
        sortable: true,
    },
    TableColumn {
        key: LATENCY,
        title: "p95 ms",
        width: ColumnWidth::Fixed(76.0),
        num: true,
        sortable: true,
    },
];

/// The orders the autopilot walks the table through.
const SORTS: [SortKey; 5] = [
    SortKey {
        column: REQUESTS,
        descending: true,
    },
    SortKey {
        column: LATENCY,
        descending: false,
    },
    SortKey {
        column: NAME,
        descending: false,
    },
    SortKey {
        column: LATENCY,
        descending: true,
    },
    SortKey {
        column: REQUESTS,
        descending: false,
    },
];

/// The pie's slices: key, name.
const SOURCES: [(u64, &str); 4] = [(1, "Search"), (2, "Direct"), (3, "Social"), (4, "Email")];

/// The board's columns.
const LANES: [(u64, &str); 3] = [(1, "To do"), (2, "In progress"), (3, "Done")];

/// One card of the board.
#[derive(Debug, Clone, Copy)]
struct Task {
    key: u64,
    title: &'static str,
    tags: &'static [&'static str],
    who: &'static str,
    lane: usize,
}

const TASKS: [Task; 5] = [
    Task {
        key: 1,
        title: "Move the compositor to wgpu 30",
        tags: &["gpu"],
        who: "Mara",
        lane: 0,
    },
    Task {
        key: 2,
        title: "Frame pacing on the 120 Hz laptop",
        tags: &["perf", "web"],
        who: "Jun",
        lane: 0,
    },
    Task {
        key: 3,
        title: "Text bridge for phone keyboards",
        tags: &["input"],
        who: "Sofia",
        lane: 1,
    },
    Task {
        key: 4,
        title: "Pie chart slices retarget",
        tags: &["charts"],
        who: "Tomas",
        lane: 1,
    },
    Task {
        key: 5,
        title: "Release notes for 0.7.7",
        tags: &["docs"],
        who: "Mara",
        lane: 2,
    },
];

const METERS: [(&str, Tone); 5] = [
    ("Build", Tone::Ok),
    ("Tests", Tone::Neutral),
    ("Lint", Tone::Ok),
    ("Deploy", Tone::Warn),
    ("Docs", Tone::Neutral),
];

/// A pseudo-random number in 0..1 from `n`.
fn noise(n: u64) -> f32 {
    let x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
    (x % 1000) as f32 / 1000.0
}

/// The Data scene's state.
#[derive(Debug)]
pub struct Data {
    /// The autopilot.
    pub pilot: Pilot,
    /// When the page was entered.
    pub entry: Entry,
    /// The four counters: orders, visitors, revenue, latency.
    pub stats: [f64; 4],
    /// The bars: visits and signups of each day.
    pub bars: Vec<[f64; 2]>,
    /// The pie's slices.
    pub pie: [f64; 4],
    /// The table's rows by position: indices into the services.
    pub order: Vec<usize>,
    /// The meters, 0 to 1.
    pub meters: [f32; 5],
    /// The lane each task is in, and the order they were put in it.
    pub lanes: Vec<(u64, usize)>,
    /// How many days the chart has rolled.
    pub rolled: u64,
    /// The time since the page landed, as the last frame's grow-in saw it (`None` at rest).
    pub grew: Option<Duration>,
    /// How many sorts the autopilot has asked for.
    sorts: usize,
    /// The controls of the last frame.
    pub targets: Targets,
}

impl Default for Data {
    fn default() -> Self {
        let bars = (0..BARS as u64)
            .map(|i| {
                [
                    150.0 + 230.0 * f64::from(noise(i * 7 + 1)),
                    30.0 + 80.0 * f64::from(noise(i * 13 + 5)),
                ]
            })
            .collect();
        Self {
            pilot: Pilot::default(),
            entry: Entry::default(),
            stats: [1284.0, 8420.0, 42_810.0, 128.0],
            bars,
            pie: [38.0, 27.0, 19.0, 16.0],
            order: (0..SERVICES.len()).collect(),
            meters: [0.82, 0.64, 0.93, 0.38, 0.55],
            lanes: TASKS.iter().map(|t| (t.key, t.lane)).collect(),
            rolled: 0,
            grew: None,
            sorts: 0,
            targets: Targets::default(),
        }
    }
}

impl Data {
    /// Orders the table by `sort` (a stable order: ties by index).
    pub fn resort(&mut self, sort: SortKey) {
        self.order.sort_by(|a, b| {
            let (x, y) = (&SERVICES[*a], &SERVICES[*b]);
            match sort.column {
                NAME => x.name.cmp(y.name),
                REQUESTS => x.requests.cmp(&y.requests),
                _ => x.latency.cmp(&y.latency),
            }
            .then(a.cmp(b))
        });
        if sort.descending {
            self.order.reverse();
        }
    }

    /// The lane (0 to 2) of task `key`.
    pub fn lane_of(&self, key: u64) -> usize {
        self.lanes
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(0, |(_, l)| *l)
    }

    /// Moves task `key` to the end of `lane`.
    pub fn move_task(&mut self, key: u64, lane: usize) {
        self.lanes.retain(|(k, _)| *k != key);
        self.lanes.push((key, lane));
    }

    /// How many tasks lane `lane` holds.
    pub fn lane_len(&self, lane: usize) -> usize {
        self.lanes.iter().filter(|(_, l)| *l == lane).count()
    }

    /// Moves one task a lane to the right (the last lane wraps to the first), the `round`th in turn
    /// among those that can: a task whose lane keeps another, into a lane with room, so no lane
    /// is ever empty of the board's three and none holds more than three.
    pub fn advance_board(&mut self, round: usize) {
        for k in 0..TASKS.len() {
            let task = TASKS[(round * 2 + k) % TASKS.len()];
            let from = self.lane_of(task.key);
            let to = (from + 1) % LANES.len();
            if self.lane_len(from) > 1 && self.lane_len(to) < 3 {
                self.move_task(task.key, to);
                return;
            }
        }
    }

    /// What the autopilot does at step `n`: one change, round-robin over the parts.
    fn act(&mut self, n: u64, ui: &mut Ui<'_>) {
        match n % 8 {
            0 => {
                for k in 0..3u64 {
                    let at = (noise(n * 5 + k) * BARS as f32) as usize % BARS;
                    self.bars[at] = [
                        120.0 + 280.0 * f64::from(noise(n * 11 + k)),
                        25.0 + 90.0 * f64::from(noise(n * 17 + k)),
                    ];
                }
            }
            1 => {
                for (k, v) in self.pie.iter_mut().enumerate() {
                    *v = 10.0 + 40.0 * f64::from(noise(n * 3 + k as u64 + 9));
                }
            }
            2 | 7 => {
                let sort = if n % 8 == 2 {
                    let s = SORTS[self.sorts % SORTS.len()];
                    self.sorts += 1;
                    s
                } else {
                    let s = ui.table_state("table").sort.unwrap_or(SORTS[0]);
                    SortKey {
                        descending: !s.descending,
                        ..s
                    }
                };
                ui.table_state("table").sort = Some(sort);
                self.resort(sort);
            }
            3 => {
                self.stats[0] += 7.0 + f64::from((noise(n) * 30.0).round());
                self.stats[1] += 36.0 + f64::from((noise(n + 1) * 90.0).round());
                self.stats[2] += 140.0 + f64::from((noise(n + 2) * 600.0).round());
                self.stats[3] = 90.0 + f64::from((noise(n + 3) * 70.0).round());
            }
            4 => self.advance_board((n / 8) as usize),
            5 => {
                for k in 0..2u64 {
                    let at = (noise(n + k * 7) * 5.0) as usize % 5;
                    self.meters[at] = 0.2 + 0.78 * noise(n * 19 + k);
                }
            }
            _ => {
                // The window rolls one day: the bars morph in place.
                self.rolled += 1;
                self.bars.remove(0);
                self.bars.push([
                    150.0 + 250.0 * f64::from(noise(n * 23)),
                    30.0 + 90.0 * f64::from(noise(n * 29)),
                ]);
            }
        }
    }

    /// Draws the scene into `body`.
    pub fn draw(&mut self, ui: &mut Ui<'_>, body: Rect, env: &Env) {
        self.pilot.observe(ui, body.inset(-env.pad(), 0.0), env);
        if let Some(n) = self.pilot.tick(env, STEP) {
            self.act(n, ui);
        }
        let grow = self.entry.elapsed(env);
        self.grew = grow;
        if env.animating {
            ui.request_busy_frame(body);
        }
        let plan = plan(body, env.wide);
        self.targets.clear();
        scrolled(ui, body, env.pad(), plan.height, |ui| {
            for (card, r) in &plan.cards {
                if *card != Card::Board {
                    tile(ui, *r, 0.0);
                }
            }
            for (card, r) in &plan.cards {
                let inner = r.inset(CARD_PAD, CARD_PAD);
                match card {
                    Card::Stats => self.stats_card(ui, *r, grow),
                    Card::Bars => self.bars_card(ui, inner, grow),
                    Card::Pie => self.pie_card(ui, inner, grow),
                    Card::Table => self.table_card(ui, inner),
                    Card::Board => self.board_card(ui, *r),
                    Card::Meters => self.meters_card(ui, inner, grow),
                }
            }
        });
    }

    fn stats_card(&mut self, ui: &mut Ui<'_>, r: Rect, grow: Option<Duration>) {
        let counted = |i: usize| {
            grown(
                grow,
                Duration::from_millis(60 * i as u64),
                Duration::from_millis(900),
            )
        };
        let growing = grow.is_some_and(|e| e < GROW + Duration::from_millis(300));
        let plain = |v: f64| format!("{}", v.round() as u64);
        let shown: Vec<f64> = (0..4)
            .map(|i| self.stats[i] * f64::from(counted(i)))
            .collect();
        let texts: Vec<String> = shown.iter().map(|v| plain(*v)).collect();
        let cell = |i: usize, label: &'static str, sub: &'static str| StatOptions {
            label,
            value: &texts[i],
            number: Some((shown[i], &plain)),
            live: growing,
            sub: Some(sub),
            ..StatOptions::default()
        };
        let cells = [
            cell(0, "Orders", "Today"),
            cell(1, "Visitors", "This week"),
            cell(2, "Revenue", "USD, this month"),
            StatOptions {
                meter: Some((shown[3] / 200.0) as f32),
                ..cell(3, "Latency", "ms, p95")
            },
        ];
        let shown_cells: &[StatOptions<'_>] = if r.w >= 520.0 { &cells } else { &cells[..2] };
        ui.stat_strip(
            "stats",
            Rect::new(r.x, r.y + ((r.h - 88.0) / 2.0).max(0.0), r.w, 0.0),
            shown_cells,
        );
    }

    fn bars_card(&mut self, ui: &mut Ui<'_>, inner: Rect, grow: Option<Duration>) {
        let t = *ui.theme();
        caps(ui, "Traffic, 14 days", inner.x, inner.y);
        let y = inner.y + 30.0;
        let total: f64 = self.bars.iter().map(|b| b[0]).sum();
        let signups: f64 = self.bars.iter().map(|b| b[1]).sum();
        let summary = format!(
            "{} visits, {} sign-ups",
            total.round() as u64,
            signups.round() as u64
        );
        let sw = ui
            .painter()
            .measure(&Spec::plain(&summary, Style::ui(t.font_s, t.fg_secondary)))
            .0;
        line(
            ui,
            &summary,
            inner.right() - sw,
            inner.y + 8.0,
            inner.w,
            t.font_s,
            t.fg_secondary,
        );
        let series = [
            BarSeries {
                name: "Visits",
                color: t.data,
            },
            BarSeries {
                name: "Sign-ups",
                color: t.ok,
            },
        ];
        // While the page grows in, each bar's height is the app's own staggered ease and the chart
        // snaps to it; afterwards the chart's `value` motion carries every change.
        let growing = grow.is_some_and(|e| e < GROW + Duration::from_millis(300));
        let shown: Vec<[f64; 2]> = self
            .bars
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let k = f64::from(grown(
                    grow,
                    Duration::from_millis(45 * i as u64),
                    Duration::from_millis(520),
                ));
                [b[0] * k, b[1] * k]
            })
            .collect();
        let columns: Vec<BarColumn<'_>> = shown
            .iter()
            .map(|v| BarColumn {
                values: v.as_slice(),
                tip: "",
            })
            .collect();
        let labels = [(0, "Nov 10"), (4, "Nov 14"), (8, "Nov 18"), (12, "Nov 22")];
        let opts = BarChartOptions {
            series: &series,
            columns: &columns,
            max: BAR_MAX,
            y_labels: ["500", "250", "0"],
            x_labels: &labels,
            label: "Traffic",
            description: "Visits and sign-ups for each of the last fourteen days",
            live: growing,
        };
        let h = ui.bar_chart("bars", Rect::new(inner.x, y, inner.w, 0.0), &opts);
        let _ = h;
        let _ = bar_chart::height();
        self.targets
            .note("bars", Rect::new(inner.x, y, inner.w, bar_chart::height()));
    }

    fn pie_card(&mut self, ui: &mut Ui<'_>, inner: Rect, grow: Option<Duration>) {
        let t = *ui.theme();
        caps(ui, "Sources", inner.x, inner.y);
        let colours: [Rgba; 4] = [t.data, t.ok, t.gold, t.err];
        let side = inner.w >= 260.0;
        let dia = pie_chart::DIAMETER;
        let disc = if side {
            Rect::new(inner.x, inner.y + 28.0, dia, dia)
        } else {
            Rect::new(inner.x + (inner.w - dia) / 2.0, inner.y + 28.0, dia, dia)
        };
        let growing = grow.is_some_and(|e| e < GROW + Duration::from_millis(300));
        let shown: Vec<f64> = self
            .pie
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v * f64::from(grown(
                    grow,
                    Duration::from_millis(140 * i as u64),
                    Duration::from_millis(520),
                ))
            })
            .collect();
        let slices: Vec<PieSlice> = SOURCES
            .iter()
            .enumerate()
            .map(|(i, (key, _))| PieSlice {
                key: *key,
                value: shown[i],
                color: colours[i],
            })
            .collect();
        let opts = PieOptions {
            slices: &slices,
            label: "Traffic sources",
            description: "Search, direct, social and referral shares of the visits",
            live: growing,
        };
        ui.pie_chart("pie", disc, &opts);
        self.targets.note("pie", disc);
        // The legend: a swatch, the name and its share, in the UI face.
        let total: f64 = self.pie.iter().sum();
        let (lx, lw) = if side {
            (disc.right() + 16.0, inner.right() - disc.right() - 16.0)
        } else {
            (inner.x, inner.w)
        };
        let top = if side {
            disc.y + (dia - 4.0 * 24.0) / 2.0
        } else {
            disc.bottom() + 10.0
        };
        for (i, (_, name)) in SOURCES.iter().enumerate() {
            let cy = top + 12.0 + 24.0 * i as f32;
            let p = ui.painter();
            p.fill(Rect::new(lx, cy - 4.0, 8.0, 8.0), 3.0, colours[i]);
            let pct = format!("{}%", (self.pie[i] / total * 100.0).round() as u32);
            let pw = p
                .measure(&Spec::plain(
                    &pct,
                    Style::ui(t.font_base, t.fg_primary).medium(),
                ))
                .0;
            p.text(
                &Spec::plain(&pct, Style::ui(t.font_base, t.fg_primary).medium()),
                lx + lw - pw,
                cy - 8.0,
            );
            line(
                ui,
                name,
                lx + 16.0,
                cy,
                lw - 16.0 - pw - 8.0,
                t.font_base,
                t.fg_secondary,
            );
        }
    }

    fn table_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        caps(ui, "Services", inner.x, inner.y);
        let rect = Rect::new(
            inner.x - CARD_PAD + 4.0,
            inner.y + 26.0,
            inner.w + 2.0 * CARD_PAD - 8.0,
            table::HEADER + ROW * SERVICES.len() as f32,
        );
        // Where each row is drawn: its slot, eased to a new one when the order changes (the
        // `move` kind), so a re-sort slides the rows.
        let slots: Vec<f32> = (0..SERVICES.len())
            .map(|i| {
                let slot = self.order.iter().position(|o| *o == i).unwrap_or(i) as f32;
                let id = ui.id(("slot", i));
                ui.animate_f32(id, slot, &Motion::of(Kind::Move))
            })
            .collect();
        let layout = table::columns(&COLUMNS, rect.w);
        for (i, (x, w)) in layout.iter().enumerate() {
            self.targets.note(
                format!("head{i}"),
                Rect::new(rect.x + x, rect.y, *w, table::HEADER),
            );
        }
        let order = self.order.clone();
        let opts = TableOptions {
            columns: &COLUMNS,
            count: SERVICES.len(),
            selection: SelectionMode::None,
            wrap: false,
            index_of: None,
            label: "Services",
        };
        let resp = ui.table(
            "table",
            rect,
            &opts,
            |row| RowKey(order[row] as u64),
            |ui, c: &CellCtx| {
                let s = SERVICES[order[c.row]];
                let dy = (slots[order[c.row]] - c.row as f32) * ROW;
                let (text, style, right) = match c.column {
                    0 => (
                        s.name.to_owned(),
                        Style::ui(t.font_base, t.fg_primary).medium(),
                        false,
                    ),
                    1 => (
                        s.requests.to_string(),
                        Style::mono(t.font_base, 20.0, t.fg_primary),
                        true,
                    ),
                    _ => (
                        s.latency.to_string(),
                        Style::mono(t.font_base, 20.0, t.fg_secondary),
                        true,
                    ),
                };
                let p = ui.painter();
                let spec = p.elide(&Spec::plain(text, style), c.rect.w);
                let (w, h) = p.measure(&spec);
                let x = if right { c.rect.right() - w } else { c.rect.x };
                p.text(&spec, x, c.rect.y + (c.rect.h - h) / 2.0 + dy);
                Cell::Control
            },
        );
        if let Some(sort) = resp.sort {
            self.resort(sort);
        }
    }

    fn board_card(&mut self, ui: &mut Ui<'_>, r: Rect) {
        let t = *ui.theme();
        let columns_cards: Vec<Vec<KanbanCard<'_>>> = (0..LANES.len())
            .map(|lane| {
                self.lanes
                    .iter()
                    .filter(|(_, l)| *l == lane)
                    .filter_map(|(k, _)| TASKS.iter().find(|t| t.key == *k))
                    .map(|task| KanbanCard {
                        key: task.key,
                        title: task.title,
                        tags: task.tags,
                        assignee: Some(task.who),
                        note: false,
                    })
                    .collect()
            })
            .collect();
        let columns: Vec<KanbanColumn<'_>> = LANES
            .iter()
            .zip(&columns_cards)
            .map(|((key, title), cards)| KanbanColumn {
                key: *key,
                title,
                cards,
            })
            .collect();
        let opts = BoardOptions {
            title: "Sprint",
            columns: &columns,
            error: None,
            glass: t.glass.panel,
            under: t.glass.window.premultiplied(),
            name: "",
        };
        let resp = ui.kanban_board("board", r, &opts);
        self.targets.note("board", r);
        if let Some(m) = resp.moved {
            let lane = LANES.iter().position(|(k, _)| *k == m.column).unwrap_or(0);
            self.move_task(m.card, lane);
        }
    }

    fn meters_card(&mut self, ui: &mut Ui<'_>, inner: Rect, grow: Option<Duration>) {
        let t = *ui.theme();
        caps(ui, "Pipeline", inner.x, inner.y);
        let top = inner.y + 28.0;
        let pitch = ((inner.bottom() - top) / METERS.len() as f32).min(58.0);
        for (i, (name, tone)) in METERS.iter().enumerate() {
            let want = self.meters[i]
                * grown(
                    grow,
                    Duration::from_millis(90 * i as u64),
                    Duration::from_millis(700),
                );
            let id = ui.id(("meter", i));
            let v = ui.animate_f32(id, want, &Motion::of(Kind::Expand));
            let used = format!("{}%", (v * 100.0).round() as u32);
            let opts = ProgressOptions {
                value: Some(v),
                tone: *tone,
                head: Some((name, &used)),
                name,
            };
            let y = top + pitch * i as f32 + (pitch - 28.0).max(0.0) / 2.0;
            ui.progress(("meter", i), Rect::new(inner.x, y, inner.w, 0.0), &opts);
        }
        let _ = with_alpha(t.fg_primary, 1.0);
    }
}
