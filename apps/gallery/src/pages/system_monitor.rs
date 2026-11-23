//! The `system-monitor` page: a menu-bar style dropdown of readings, built from toolkit parts only
//! (D440). A panel holds a scroll area; in it come the sections CPU, GPU, RAM, Disk, Sensors,
//! Network, Battery and Clock, each a label with a gear button at its right, then rings, rows with
//! legend swatches, graphs, a battery and a card whose analog face the page draws from the time the
//! app passes in. Every reading is a constant or a function of that time and the CPU knob, so a
//! dump is the same every time. The domain words live here, in `apps/`, not in the library.
//!
//! The page lays out from the rect it is given: the panel is as wide as the card allows up to
//! [`COLUMN_W`], so a phone-size window shows it whole.

use std::f32::consts::TAU;
use std::time::Duration;

use lune_ui::icon;
use lune_ui::motion::MotionTier;
use lune_ui::paint::{Rect, StrokeStyle};
use lune_ui::text::{Spec, Style};
use lune_ui::theme::Theme;
use lune_ui::ui::Ui;
use lune_ui::widgets::battery::BatteryOptions;
use lune_ui::widgets::button::IconButton;
use lune_ui::widgets::kv_list::{KvOptions, KvRow, ROW};
use lune_ui::widgets::label::LabelRole;
use lune_ui::widgets::panel::{PanelKind, PanelOptions, PanelSlot};
use lune_ui::widgets::ring_gauge::{RingGaugeOptions, RingSegment};
use lune_ui::widgets::scroll_area::{Extent, Reveal, ScrollOptions, Target};
use lune_ui::widgets::sparkline::{SparkStyle, SparklineOptions};

use super::card;
use crate::page::Stage;
use crate::page::{Expect, Page, PageKind, PageState, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "system-monitor",
    title: "System monitor",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: &[],
    }),
};

/// The panel's widest width.
const COLUMN_W: f32 = 320.0;
/// The card's margin around the panel.
const MARGIN: f32 = 16.0;
/// The room the scroll bar keeps at the content's right.
const BAR_ROOM: f32 = 8.0;
/// A section header: its label and the gear button at its right.
const HEADER_H: f32 = 28.0;
const GEAR: f32 = 28.0;
/// The space under a header, and between two sections (a hairline sits in the middle of it).
const HEADER_GAP: f32 = 8.0;
const SECTION_GAP: f32 = 20.0;
/// A ring's side, its thickness, and the gap between it and the rows beside it.
const RING: f32 = 96.0;
const RING_THICK: f32 = 10.0;
const RING_GAP: f32 = 16.0;
/// The per-core strip and a small caption over it.
const STRIP_H: f32 = 40.0;
const CAPTION_H: f32 = 18.0;
/// The sensors list's box, its inset and the room between a graph and the rows beside it.
const SENSORS_H: f32 = 132.0;
const SENSORS_INSET: (f32, f32) = (8.0, 4.0);
/// A traffic graph's height and the width of the swatched pair beside the disk's graph.
const GRAPH_H: f32 = 56.0;
const PAIR_W: f32 = 132.0;
/// The clock card's height, its face's radius and the face's gap from the text.
const CLOCK_H: f32 = 112.0;
const FACE_R: f32 = 38.0;
/// A battery's size.
const BATTERY: (f32, f32) = (64.0, 30.0);
/// The graphs' slots, and the cores shown.
const HISTORY: usize = 60;
const CORES: usize = 12;
/// The fake machine: memory in GB, the disk in GB, the full scale of a disk graph in MB/s and of
/// a network graph in MB/s.
const RAM_GB: f32 = 16.0;
const DISK_GB: f32 = 500.0;
const DISK_MAX: f32 = 40.0;
const NET_MAX: f32 = 12.0;
/// The time of day at app time 0, in seconds, and the date shown.
const CLOCK_START: f64 = 14.0 * 3600.0 + 8.0 * 60.0 + 32.0;
const DATE: &str = "Tue, 24 Nov 2026";

/// The sensors' rows: the name, then the value (the first is replaced by the CPU's own reading).
const SENSORS: [(&str, &str); 14] = [
    ("CPU die", ""),
    ("CPU efficiency cores", "47 \u{b0}C"),
    ("GPU", "55 \u{b0}C"),
    ("Memory", "44 \u{b0}C"),
    ("Storage", "39 \u{b0}C"),
    ("Battery", "31 \u{b0}C"),
    ("Ambient", "27 \u{b0}C"),
    ("Fan left", "1210 rpm"),
    ("Fan right", "1195 rpm"),
    ("Power in", "12.4 W"),
    ("CPU power", "4.2 W"),
    ("GPU power", "1.1 W"),
    ("Memory power", "0.6 W"),
    ("Total power", "9.8 W"),
];

/// What the page keeps: the CPU's load and the scene's own scroll.
struct Live {
    /// The CPU's load, 0 to 1; it moves the ring, the rows and the strip together.
    cpu: f32,
    /// From this time on the scene scrolls to the sensors list and down its rows, once.
    reveal_at: Option<Duration>,
    revealed: bool,
    /// Where the sensors header sat in the content last frame, for the scroll to it.
    sensors_y: f32,
}

fn state(reveal_at: Option<Duration>) -> Box<dyn PageState> {
    Box::new(Live {
        cpu: 0.36,
        reveal_at,
        revealed: false,
        sensors_y: 0.0,
    })
}

fn live() -> Box<dyn PageState> {
    state(None)
}

fn scrolled() -> Box<dyn PageState> {
    state(Some(REVEAL))
}

/// The scene scrolls on the frame at this time.
const REVEAL: Duration = Duration::from_millis(300);

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let card = card(ui, stage, "System monitor");
        let w = (card.w - 2.0 * MARGIN).clamp(0.0, COLUMN_W);
        let rect = Rect::new(
            card.x + (card.w - w) / 2.0,
            card.y + MARGIN,
            w,
            (card.h - 2.0 * MARGIN).max(0.0),
        );
        let reading = Reading::at(self.cpu, ui.time());
        let reveal = !self.revealed && self.reveal_at.is_some_and(|t| ui.time() >= t);
        if reveal {
            self.revealed = true;
        }
        let sensors_y = &mut self.sensors_y;
        let sensors_at = *sensors_y;
        let opts = PanelOptions {
            title: Some("Combined modules"),
            meta: Some("sample readings"),
            kind: PanelKind::Plain,
            name: "Combined modules",
            ..PanelOptions::default()
        };
        ui.panel("panel", rect, &opts, |ui, slot| {
            let PanelSlot::Body(body) = slot else {
                return 0.0;
            };
            if reveal {
                let target = Target::span_y(
                    f64::from(sensors_at),
                    f64::from(HEADER_H + HEADER_GAP + SENSORS_H),
                );
                ui.scroll_to("column", target, Reveal::Center);
            }
            ui.scroll_area("column", body, &ScrollOptions::default(), |ui, view| {
                let vp = view.viewport;
                let area = Rect::new(vp.x, vp.y, (vp.w - BAR_ROOM).max(0.0), vp.h);
                let height = sections(ui, area, &reading, sensors_y, reveal);
                ((), Extent::new(f64::from(vp.w), f64::from(height)))
            });
            body.h
        });
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.number(ui, "CPU load", &mut self.cpu, (0.0, 1.0, 0.0));
    }
}

/// Every reading of one frame: the CPU's load and the whole seconds of the app's time (a sample
/// series is a function of both).
struct Reading {
    cpu: f32,
    /// The app's time in seconds, fractions kept for the clock's hands.
    secs: f64,
    /// The whole seconds: sample `k` of a history is a function of `tick - n + 1 + k`.
    tick: i64,
}

impl Reading {
    fn at(cpu: f32, time: Duration) -> Self {
        let cpu = if cpu.is_finite() {
            cpu.clamp(0.0, 1.0)
        } else {
            0.0
        };
        Self {
            cpu,
            secs: time.as_secs_f64(),
            tick: time.as_secs() as i64,
        }
    }

    /// `n` samples of a wave 0 to 1, oldest first, ending at this reading's second: the same
    /// `seed` and time draw the same series, and a second later it has moved by one slot.
    fn series(&self, seed: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|k| {
                let i = (self.tick - n as i64 + 1 + k as i64) as f32;
                let v = 0.5
                    + 0.28 * (i * 0.31 + seed).sin()
                    + 0.17 * (i * 0.83 + seed * 2.1).sin()
                    + 0.05 * (i * 2.9 + seed * 3.7).sin();
                v.clamp(0.0, 1.0)
            })
            .collect()
    }

    /// Core `i`'s load: the CPU's load spread over the cores, with a ripple in the time.
    fn core(&self, i: usize) -> f32 {
        let spread = 0.55 + 0.9 * ((i as f32 * 0.618).fract());
        let ripple = 0.08 * (self.tick as f32 * 0.9 + i as f32 * 1.3).sin();
        (self.cpu * spread + ripple * self.cpu.max(0.2)).clamp(0.0, 1.0)
    }
}

/// `x` (a fraction) as a percentage text.
fn pct(x: f32) -> String {
    format!("{:.0}%", x * 100.0)
}

/// The sections, top to bottom, from the top of `area`. Returns the content's height. A section's
/// header is the label and its gear; `sensors_y` gets the top of the sensors header, for the scene
/// that scrolls to it, and `reveal` scrolls the sensors list's own area to its middle rows.
fn sections(ui: &mut Ui<'_>, area: Rect, r: &Reading, sensors_y: &mut f32, reveal: bool) -> f32 {
    let t = *ui.theme();
    let (x, w) = (area.x, area.w);
    let mut y = area.y;
    // A hairline in the middle of the gap between two sections, then the gap.
    let divide = |ui: &mut Ui<'_>, y: &mut f32| {
        let line = Rect::new(x, *y + SECTION_GAP / 2.0, w, 1.0);
        ui.painter().fill(line, 0.0, t.border_subtle);
        *y += SECTION_GAP;
    };

    y += cpu(ui, x, y, w, r);
    divide(ui, &mut y);
    y += gpu(ui, x, y, w, r);
    divide(ui, &mut y);
    y += ram(ui, x, y, w);
    divide(ui, &mut y);
    y += disk(ui, x, y, w, r);
    divide(ui, &mut y);
    *sensors_y = y - area.y;
    y += sensors(ui, x, y, w, r, reveal);
    divide(ui, &mut y);
    y += network(ui, x, y, w, r);
    divide(ui, &mut y);
    y += battery(ui, x, y, w);
    divide(ui, &mut y);
    y += clock(ui, x, y, w, r);
    y - area.y + 4.0
}

/// A section's header: `title` at the left and a gear button at the right, `HEADER_H` high.
/// The button does nothing here; it is a real one, so Tab and a touch reach it.
fn header(ui: &mut Ui<'_>, key: &str, x: f32, y: f32, w: f32, title: &str) -> f32 {
    ui.label((key, "title"), (x, y + 4.0), title, LabelRole::Section);
    let name = format!("{title} settings");
    let gear = IconButton {
        glyph: icon::GEAR,
        glyph_size: 14.0,
        color: None,
        radius: 4.0,
        name: &name,
    };
    ui.icon_button(
        (key, "gear"),
        Rect::new(x + w - GEAR, y, GEAR, HEADER_H),
        &gear,
    );
    HEADER_H + HEADER_GAP
}

/// A ring at the left with `rows` beside it, centred on the ring's height. Returns the block's
/// height.
fn ring_block(
    ui: &mut Ui<'_>,
    key: &str,
    (x, y, w): (f32, f32, f32),
    ring: (&[RingSegment], &str, &str),
    rows: &[KvRow<'_>],
) -> f32 {
    let (segments, text, name) = ring;
    ui.ring_gauge(
        (key, "ring"),
        Rect::new(x, y, RING, RING),
        &RingGaugeOptions::new(segments, name)
            .with_text(text)
            .with_thickness(RING_THICK),
    );
    let rows_h = rows.len() as f32 * ROW;
    let top = y + ((RING - rows_h) / 2.0).max(0.0);
    let rx = x + RING + RING_GAP;
    ui.kv_rows(
        (key, "rows"),
        Rect::new(rx, top, (w - RING - RING_GAP).max(0.0), rows_h),
        rows,
        &KvOptions::default(),
    );
    RING.max(rows_h)
}

fn cpu(ui: &mut Ui<'_>, x: f32, y: f32, w: f32, r: &Reading) -> f32 {
    let t = *ui.theme();
    let top = y;
    let mut y = y + header(ui, "cpu", x, y, w, "CPU");
    let (user, system) = (r.cpu * 0.68, r.cpu * 0.32);
    let segments = [
        RingSegment::new(1, user, t.accent),
        RingSegment::new(2, system, t.warn),
    ];
    let (u, s, i) = (pct(user), pct(system), pct(1.0 - r.cpu));
    let rows = [
        KvRow::new("User", &u).with_swatch(t.accent),
        KvRow::new("System", &s).with_swatch(t.warn),
        KvRow::new("Idle", &i).with_swatch(t.border),
    ];
    y += ring_block(
        ui,
        "cpu",
        (x, y, w),
        (&segments[..], &pct(r.cpu), "CPU load"),
        &rows,
    );
    y += 12.0;
    ui.label(("cpu", "cores"), (x, y), "Per-core load", LabelRole::Small);
    y += CAPTION_H;
    let cores: Vec<f32> = (0..CORES).map(|i| r.core(i)).collect();
    ui.sparkline(
        ("cpu", "strip"),
        Rect::new(x, y, w, STRIP_H),
        &SparklineOptions::new(&cores, t.accent, "Per-core load")
            .with_style(SparkStyle::Bars)
            .with_capacity(CORES)
            .with_max(1.0),
    );
    y + STRIP_H - top
}

fn gpu(ui: &mut Ui<'_>, x: f32, y: f32, w: f32, r: &Reading) -> f32 {
    let t = *ui.theme();
    let top = y;
    let y = y + header(ui, "gpu", x, y, w, "GPU");
    let load = 0.14 + 0.5 * r.cpu;
    let (render, compute) = (load * 0.72, load * 0.28);
    let segments = [
        RingSegment::new(1, render, t.data),
        RingSegment::new(2, compute, t.data_hi),
    ];
    let (a, b, c) = (pct(render), pct(compute), pct(1.0 - load));
    let rows = [
        KvRow::new("Render", &a).with_swatch(t.data),
        KvRow::new("Compute", &b).with_swatch(t.data_hi),
        KvRow::new("Idle", &c).with_swatch(t.border),
    ];
    let h = ring_block(
        ui,
        "gpu",
        (x, y, w),
        (&segments[..], &pct(load), "GPU load"),
        &rows,
    );
    y + h - top
}

fn ram(ui: &mut Ui<'_>, x: f32, y: f32, w: f32) -> f32 {
    let t = *ui.theme();
    let top = y;
    let y = y + header(ui, "ram", x, y, w, "RAM");
    let (app, wired, packed) = (0.34, 0.12, 0.08);
    let segments = [
        RingSegment::new(1, app, t.accent),
        RingSegment::new(2, wired, t.warn),
        RingSegment::new(3, packed, t.data),
    ];
    let gb = |f: f32| format!("{:.1} GB", f * RAM_GB);
    let (a, b, c, d) = (
        gb(app),
        gb(wired),
        gb(packed),
        gb(1.0 - app - wired - packed),
    );
    let rows = [
        KvRow::new("App", &a).with_swatch(t.accent),
        KvRow::new("Wired", &b).with_swatch(t.warn),
        KvRow::new("Compressed", &c).with_swatch(t.data),
        KvRow::new("Free", &d).with_swatch(t.border),
    ];
    let h = ring_block(
        ui,
        "ram",
        (x, y, w),
        (&segments[..], &pct(app + wired + packed), "Memory in use"),
        &rows,
    );
    y + h - top
}

fn disk(ui: &mut Ui<'_>, x: f32, y: f32, w: f32, r: &Reading) -> f32 {
    let t = *ui.theme();
    let top = y;
    let mut y = y + header(ui, "disk", x, y, w, "Disk");
    let used = 0.62;
    let segments = [RingSegment::new(1, used, t.accent)];
    let gb = |f: f32| format!("{:.0} GB", f * DISK_GB);
    let (a, b) = (gb(used), gb(1.0 - used));
    let rows = [
        KvRow::new("Used", &a).with_swatch(t.accent),
        KvRow::new("Free", &b).with_swatch(t.border),
    ];
    y += ring_block(
        ui,
        "disk",
        (x, y, w),
        (&segments[..], &pct(used), "Disk used"),
        &rows,
    );
    y += 12.0;
    let write = r.series(0.4, HISTORY);
    let read = r.series(2.2, HISTORY);
    let rate = |s: &[f32]| format!("{:.1} MB/s", s.last().copied().unwrap_or(0.0) * DISK_MAX);
    let (wv, rv) = (rate(&write), rate(&read));
    let pair = [
        KvRow::new("Write", &wv).with_swatch(t.data),
        KvRow::new("Read", &rv).with_swatch(t.data_hi),
    ];
    let pair_w = PAIR_W.min(w * 0.5);
    ui.kv_rows(
        ("disk", "pair"),
        Rect::new(x, y + (GRAPH_H - 2.0 * ROW) / 2.0, pair_w, 2.0 * ROW),
        &pair,
        &KvOptions::default(),
    );
    let gx = x + pair_w + 12.0;
    ui.sparkline(
        ("disk", "graph"),
        Rect::new(gx, y, (x + w - gx).max(0.0), GRAPH_H),
        &SparklineOptions::new(&write, t.data, "Disk traffic")
            .with_mirror(&read, t.data_hi)
            .with_capacity(HISTORY),
    );
    y + GRAPH_H - top
}

fn sensors(ui: &mut Ui<'_>, x: f32, y: f32, w: f32, r: &Reading, reveal: bool) -> f32 {
    let t = *ui.theme();
    let y = y + header(ui, "sensors", x, y, w, "Sensors");
    let boxed = Rect::new(x, y, w, SENSORS_H);
    ui.painter().fill(boxed, 6.0, t.surface_input);
    let die = format!("{:.0} \u{b0}C", 38.0 + 42.0 * r.cpu);
    let rows: Vec<KvRow<'_>> = SENSORS
        .iter()
        .enumerate()
        .map(|(i, (k, v))| KvRow::new(k, if i == 0 { &die } else { v }))
        .collect();
    let inner = Rect::new(
        x + SENSORS_INSET.0,
        y + SENSORS_INSET.1,
        (w - 2.0 * SENSORS_INSET.0).max(0.0),
        SENSORS_H - 2.0 * SENSORS_INSET.1,
    );
    if reveal {
        // The middle of the list: row 8 of 14 centred in the box.
        let row = (SENSORS.len() / 2 + 1) as f64 * f64::from(ROW);
        ui.scroll_to(
            "sensors",
            Target::span_y(row, f64::from(ROW)),
            Reveal::Center,
        );
    }
    ui.scroll_area("sensors", inner, &ScrollOptions::default(), |ui, view| {
        let vp = view.viewport;
        let h = ui.kv_rows(
            "rows",
            Rect::new(vp.x, vp.y, (vp.w - BAR_ROOM).max(0.0), vp.h),
            &rows,
            &KvOptions::default(),
        );
        ((), Extent::new(f64::from(vp.w), f64::from(h)))
    });
    HEADER_H + HEADER_GAP + SENSORS_H
}

fn network(ui: &mut Ui<'_>, x: f32, y: f32, w: f32, r: &Reading) -> f32 {
    let t = *ui.theme();
    let top = y;
    let mut y = y + header(ui, "network", x, y, w, "Network");
    let up = r.series(1.1, HISTORY);
    let down = r.series(3.3, HISTORY);
    let rate = |s: &[f32]| format!("{:.1} MB/s", s.last().copied().unwrap_or(0.0) * NET_MAX);
    let (uv, dv) = (rate(&up), rate(&down));
    let pair = [
        KvRow::new("Upload", &uv).with_swatch(t.data),
        KvRow::new("Download", &dv).with_swatch(t.data_hi),
    ];
    ui.kv_rows(
        ("network", "pair"),
        Rect::new(x, y, w, 2.0 * ROW),
        &pair,
        &KvOptions::default(),
    );
    y += 2.0 * ROW + 8.0;
    ui.sparkline(
        ("network", "graph"),
        Rect::new(x, y, w, GRAPH_H),
        &SparklineOptions::new(&up, t.data, "Network traffic")
            .with_mirror(&down, t.data_hi)
            .with_capacity(HISTORY),
    );
    y += GRAPH_H + 8.0;
    let ips = [
        KvRow::new("Local IP", "198.51.100.24"),
        KvRow::new("Public IP", "203.0.113.7"),
    ];
    ui.kv_rows(
        ("network", "ips"),
        Rect::new(x, y, w, 2.0 * ROW),
        &ips,
        &KvOptions::default(),
    );
    y + 2.0 * ROW - top
}

fn battery(ui: &mut Ui<'_>, x: f32, y: f32, w: f32) -> f32 {
    let top = y;
    let mut y = y + header(ui, "battery", x, y, w, "Battery");
    let level = 0.8;
    ui.battery(
        "gauge",
        Rect::new(x, y, BATTERY.0, BATTERY.1),
        &BatteryOptions::new(level, "Charge").with_charging(true),
    );
    let at = (x + BATTERY.0 + 12.0, y + 4.0);
    ui.label(("battery", "level"), at, &pct(level), LabelRole::Title);
    y += BATTERY.1 + 8.0;
    let rows = [
        KvRow::new("State", "Charging"),
        KvRow::new("Time to full", "0:42"),
        KvRow::new("Cycles", "214"),
        KvRow::new("Health", "91%"),
    ];
    ui.kv_rows(
        ("battery", "rows"),
        Rect::new(x, y, w, rows.len() as f32 * ROW),
        &rows,
        &KvOptions::default(),
    );
    y + rows.len() as f32 * ROW - top
}

/// The point at `angle` (0 right, clockwise on the screen) and distance `len` from `c`.
fn polar(c: [f32; 2], len: f32, angle: f32) -> [f32; 2] {
    [c[0] + len * angle.cos(), c[1] + len * angle.sin()]
}

/// The analog face of a clock: a disc with a rim and twelve ticks, the hour and minute hands in
/// the text colour and a second hand in the error colour. `secs` is the time of day.
fn face(ui: &mut Ui<'_>, t: &Theme, c: [f32; 2], radius: f32, secs: f64) {
    let p = ui.painter();
    p.sector(c, radius, 0.0, TAU, t.surface_input);
    p.arc(c, radius - 0.5, 0.0, TAU, StrokeStyle::solid(t.border, 1.0));
    for i in 0..12 {
        let a = i as f32 * TAU / 12.0;
        let quarter = i % 3 == 0;
        let (inner, color) = if quarter {
            (0.78, t.fg_secondary)
        } else {
            (0.86, t.fg_tertiary)
        };
        let pts = [polar(c, radius * inner, a), polar(c, radius * 0.93, a)];
        p.stroke(
            &pts,
            false,
            StrokeStyle::solid(color, if quarter { 1.5 } else { 1.0 }),
        );
    }
    // Turns of the day: the hour hand makes one in 12 hours, the minute hand one in an hour, the
    // second hand one in a minute; each from the top.
    let turn = |period: f64| ((secs / period).fract() as f32) * TAU - TAU / 4.0;
    let hands = [
        (turn(12.0 * 3600.0), 0.5, 3.0, t.fg_primary),
        (turn(3600.0), 0.75, 2.0, t.fg_primary),
        (turn(60.0), 0.86, 1.0, t.err),
    ];
    for (angle, len, width, color) in hands {
        let pts = [
            polar(c, -radius * 0.12, angle),
            polar(c, radius * len, angle),
        ];
        p.stroke(&pts, false, StrokeStyle::solid(color, width));
    }
    p.sector(c, 2.5, 0.0, TAU, t.err);
}

fn clock(ui: &mut Ui<'_>, x: f32, y: f32, w: f32, r: &Reading) -> f32 {
    let t = *ui.theme();
    let top = y;
    let y = y + header(ui, "clock", x, y, w, "Clock");
    let secs = (CLOCK_START + r.secs).rem_euclid(86_400.0);
    let whole = secs as u64;
    let text = format!(
        "{:02}:{:02}:{:02}",
        whole / 3600,
        whole / 60 % 60,
        whole % 60
    );
    let opts = PanelOptions {
        kind: PanelKind::Card,
        name: "Clock",
        ..PanelOptions::default()
    };
    ui.panel("card", Rect::new(x, y, w, CLOCK_H), &opts, |ui, slot| {
        let PanelSlot::Body(body) = slot else {
            return 0.0;
        };
        let c = [body.x + FACE_R, body.y + body.h / 2.0];
        face(ui, &t, c, FACE_R, secs);
        let tx = body.x + 2.0 * FACE_R + 16.0;
        let clip = Rect::new(tx, body.y, (body.right() - tx).max(0.0), body.h);
        let p = ui.painter();
        let time = Spec::plain(&text, Style::mono(t.font_title, t.line_title, t.fg_primary));
        let date = Spec::plain(DATE, Style::ui(t.font_s, t.fg_tertiary));
        let th = p.measure(&time).1;
        let dh = p.measure(&date).1;
        let y0 = clip.y + (clip.h - th - 4.0 - dh) / 2.0;
        p.text(&time, tx, y0);
        p.text(&date, tx, y0 + th + 4.0);
        body.h
    });
    y + CLOCK_H - top
}

const fn scene(
    id: &'static str,
    state: fn() -> Box<dyn PageState>,
    script: &'static [Step],
    capture_ms: u32,
) -> UiScene {
    UiScene {
        id,
        scale: 2.0,
        tier: MotionTier::Full,
        state,
        script,
        capture_ms,
        expect: Expect::Settled,
    }
}

/// The scrolled scene's frame at [`REVEAL`] starts both scrolls; the capture comes long after
/// they ended.
const AT_REVEAL: Step = Step {
    at_ms: 300,
    events: &[],
};

const SCENES: &[UiScene] = &[
    scene("system-monitor", live, &[], 0),
    scene("system-monitor-scrolled", scrolled, &[AT_REVEAL], 1800),
];
