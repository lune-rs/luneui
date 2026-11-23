//! Controls: the input components on five frost cards, all live (toggles, sliders with a readout,
//! a stepper, a star rating, a radio group that swaps a preview with the page-swap motion, checkboxes
//! and a button pair) and a strip of figures under them that follows the controls. The autopilot
//! walks the controls round-robin, one change every 1.2 s, each through the control's own motion; a
//! person's hand holds it back for [`RESUME`](super::RESUME).

use std::time::Duration;

use lune_ui::color::with_alpha;
use lune_ui::motion::{Kind, Motion};
use lune_ui::paint::Rect;
use lune_ui::ui::Ui;
use lune_ui::widgets::badge;
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::radio_group::RadioOption;
use lune_ui::widgets::slider::{self, SliderOptions};
use lune_ui::widgets::star_rating;
use lune_ui::widgets::stat::StatOptions;
use lune_ui::widgets::stepper::{self, StepRange};
use lune_ui::widgets::tone::Tone;

use super::{CARD_PAD, Env, GAP, Pilot, Targets, caps, line, scrolled, tile};

/// The autopilot's step.
pub const STEP: Duration = Duration::from_millis(1200);

/// One of the cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Card {
    /// Three toggles.
    Toggles,
    /// Three sliders with a readout each.
    Sliders,
    /// A stepper and a star rating.
    Stepper,
    /// A radio group and the preview it swaps.
    View,
    /// Checkboxes and a button pair.
    Options,
    /// The figures that follow the controls.
    Strip,
}

/// Where the cards sit in a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Every card and its rect, in the body's coordinates.
    pub cards: Vec<(Card, Rect)>,
    /// The content's height from the body's top.
    pub height: f32,
    /// How many columns there are.
    pub columns: usize,
}

/// The least height of a card at its natural spacing.
fn min_h(card: Card, stacked_view: bool) -> f32 {
    match card {
        Card::Toggles | Card::Sliders => 210.0,
        Card::Stepper => 170.0,
        Card::View if stacked_view => 276.0,
        Card::View => 196.0,
        Card::Options => 190.0,
        Card::Strip => 112.0,
    }
}

/// Lays the cards out: three columns when there is room, two, or one stacked column (which
/// scrolls). Spare height stretches the first rows by at most 40 % and the rest is left as margin
/// above and below.
pub fn plan(body: Rect, wide: bool) -> Plan {
    let n = if !wide || body.w < 560.0 {
        1
    } else if body.w < 880.0 {
        2
    } else {
        3
    };
    // The card rows: each is a list of (card, columns spanned).
    let rows: Vec<Vec<(Card, usize)>> = match n {
        3 => vec![
            vec![(Card::Toggles, 1), (Card::Sliders, 1), (Card::Stepper, 1)],
            vec![(Card::View, 2), (Card::Options, 1)],
            vec![(Card::Strip, 3)],
        ],
        2 => vec![
            vec![(Card::Toggles, 1), (Card::Sliders, 1)],
            vec![(Card::Stepper, 1), (Card::Options, 1)],
            vec![(Card::View, 2)],
            vec![(Card::Strip, 2)],
        ],
        _ => vec![
            vec![(Card::Toggles, 1)],
            vec![(Card::Sliders, 1)],
            vec![(Card::Stepper, 1)],
            vec![(Card::View, 1)],
            vec![(Card::Options, 1)],
            vec![(Card::Strip, 1)],
        ],
    };
    let cw = (body.w - (n as f32 - 1.0) * GAP) / n as f32;
    let stacked = n == 1 || cw * 2.0 + GAP < 420.0;
    let mins: Vec<f32> = rows
        .iter()
        .map(|r| r.iter().map(|(c, _)| min_h(*c, stacked)).fold(0.0, f32::max))
        .collect();
    let gaps = GAP * (rows.len() as f32 - 1.0);
    let need: f32 = mins.iter().sum::<f32>() + gaps;
    // The strip row keeps its height; the rest of the rows stretch.
    let strip_h = if n > 1 { mins[mins.len() - 1] } else { 0.0 };
    let stretchable = need - gaps - strip_h;
    let scale = if n > 1 {
        ((body.h - gaps - strip_h) / stretchable).clamp(1.0, 1.4)
    } else {
        1.0
    };
    let heights: Vec<f32> = mins
        .iter()
        .enumerate()
        .map(|(i, h)| if n > 1 && i + 1 < mins.len() { h * scale } else { *h })
        .collect();
    let used = heights.iter().sum::<f32>() + gaps;
    let margin = if n > 1 { ((body.h - used) / 2.0).max(0.0) } else { 0.0 };
    let mut y = body.y + margin;
    let mut cards = Vec::new();
    for (row, h) in rows.iter().zip(&heights) {
        let mut col = 0;
        for (card, span) in row {
            let x = body.x + col as f32 * (cw + GAP);
            let w = cw * *span as f32 + GAP * (*span as f32 - 1.0);
            cards.push((*card, Rect::new(x, y, w, *h)));
            col += span;
        }
        y += h + GAP;
    }
    Plan {
        cards,
        height: used + 2.0 * margin,
        columns: n,
    }
}

/// The hit rects of every control, for the tests and the tour: they are what the scene draws.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// The three toggles' switches.
    pub toggles: [Rect; 3],
    /// The three sliders' rects (the track spans the width).
    pub sliders: [Rect; 3],
    /// The stepper's box.
    pub stepper: Rect,
    /// The first star's button (the others follow at the button's pitch).
    pub stars: Rect,
    /// The radio group's rect; its rows are [`radio::ROW`](lune_ui::widgets::radio_group::ROW_HEIGHT) high.
    pub radios: Rect,
    /// Where the preview is drawn.
    pub preview: Rect,
    /// The three checkboxes.
    pub checks: [Rect; 3],
    /// The Reset button, then the Apply button.
    pub buttons: [Rect; 2],
    /// The figures' strip, and how many cells it shows.
    pub strip: Rect,
    /// How many cells the strip shows.
    pub strip_cells: usize,
    /// Every card's inner rect (inside its padding).
    pub inner: Vec<(Card, Rect)>,
}

/// The rows of `n` equal slots under a card's heading.
fn slots(inner: Rect, n: usize) -> Vec<Rect> {
    let y0 = inner.y + 24.0;
    let pitch = (inner.bottom() - y0) / n as f32;
    (0..n)
        .map(|i| Rect::new(inner.x, y0 + pitch * i as f32, inner.w, pitch))
        .collect()
}

/// Where every control is in `body`.
pub fn geometry(body: Rect, wide: bool) -> Geometry {
    let plan = plan(body, wide);
    let inner = |c: Card| {
        plan.cards
            .iter()
            .find(|(k, _)| *k == c)
            .map(|(_, r)| r.inset(CARD_PAD, CARD_PAD))
            .unwrap_or_default()
    };
    let toggles = slots(inner(Card::Toggles), 3);
    let sliders = slots(inner(Card::Sliders), 3);
    let step = slots(inner(Card::Stepper), 2);
    let opts = inner(Card::Options);
    let view = inner(Card::View);
    let strip = plan
        .cards
        .iter()
        .find(|(k, _)| *k == Card::Strip)
        .map(|(_, r)| *r)
        .unwrap_or_default();
    let side = view.w >= 380.0;
    let (radios, preview) = if side {
        (
            Rect::new(view.x, view.y + 30.0, 104.0, 3.0 * 28.0),
            Rect::new(view.x + 120.0, view.y + 24.0, view.w - 120.0, view.h - 24.0),
        )
    } else {
        let top = view.y + 28.0;
        (
            Rect::new(view.x, top, view.w, 3.0 * 28.0),
            Rect::new(view.x, top + 3.0 * 28.0 + 10.0, view.w, (view.bottom() - top - 94.0).max(0.0)),
        )
    };
    let button_y = opts.bottom() - 32.0;
    let half = (opts.w - 8.0) / 2.0;
    let check_pitch = ((button_y - 8.0) - (opts.y + 26.0)) / 3.0;
    Geometry {
        toggles: std::array::from_fn(|i| {
            Rect::new(
                toggles[i].right() - 40.0,
                toggles[i].y + toggles[i].h / 2.0 - 12.0,
                40.0,
                24.0,
            )
        }),
        sliders: std::array::from_fn(|i| {
            let r = sliders[i];
            let off = ((r.h - 46.0) / 2.0).max(0.0);
            Rect::new(r.x, r.y + off + 22.0, r.w, 24.0)
        }),
        stepper: Rect::new(
            step[0].right() - 140.0,
            step[0].y + step[0].h / 2.0 - 16.0,
            140.0,
            32.0,
        ),
        stars: Rect::new(
            step[1].right() - star_rating::row_width(5),
            step[1].y + step[1].h / 2.0 - 12.0,
            star_rating::BUTTON,
            star_rating::BUTTON,
        ),
        radios,
        preview,
        checks: std::array::from_fn(|i| {
            Rect::new(opts.x, opts.y + 26.0 + check_pitch * i as f32 + (check_pitch - 24.0) / 2.0, opts.w, 24.0)
        }),
        buttons: [
            Rect::new(opts.x, button_y, half, 32.0),
            Rect::new(opts.x + half + 8.0, button_y, half, 32.0),
        ],
        strip,
        strip_cells: if strip.w >= 520.0 { 4 } else { 2 },
        inner: plan
            .cards
            .iter()
            .map(|(c, r)| (*c, r.inset(CARD_PAD, CARD_PAD)))
            .collect(),
    }
}

const TOGGLES: [(&str, &str, &str); 3] = [
    ("Notifications", "Banners and badges", "Silenced"),
    ("Sounds", "Chimes and alerts", "Muted"),
    ("Auto-update", "Install in the background", "Manual"),
];
const SLIDERS: [&str; 3] = ["Brightness", "Volume", "Warmth"];
const VIEWS: [(&str, &str, &str, Tone, usize); 3] = [
    ("Today", "8,420", "+12.0% on yesterday", Tone::Ok, 12),
    ("This week", "52,310", "+4.1% on last week", Tone::Ok, 7),
    ("This month", "214,900", "-1.8% on last month", Tone::Warn, 10),
];
const CHECKS: [&str; 3] = ["Reduce transparency", "Show previews", "Sync on Wi-Fi only"];

/// The Controls scene's state.
#[derive(Debug)]
pub struct Controls {
    /// The autopilot.
    pub pilot: Pilot,
    /// The three toggles.
    pub toggles: [bool; 3],
    /// The three sliders, 0 to 1.
    pub sliders: [f32; 3],
    /// The stepper's value, 1 to 20.
    pub qty: u32,
    /// The star rating, 0 to 5.
    pub stars: u32,
    /// The preview the radio group chose: 0, 1 or 2.
    pub view: u64,
    /// The three checkboxes.
    pub checks: [bool; 3],
    /// How many times Apply was pressed.
    pub applied: u32,
    /// The controls of the last frame.
    pub targets: Targets,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            pilot: Pilot::default(),
            toggles: [true, false, true],
            sliders: [0.72, 0.40, 0.55],
            qty: 3,
            stars: 4,
            view: 0,
            checks: [false, true, true],
            applied: 0,
            targets: Targets::default(),
        }
    }
}

/// A pseudo-random number in 0..1 from `n`.
fn noise(n: u64) -> f32 {
    let x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
    (x % 1000) as f32 / 1000.0
}

impl Controls {
    /// What the autopilot does at step `n`: one change, round-robin over the controls.
    fn act(&mut self, n: u64) {
        match n % 12 {
            0 => self.toggles[0] = !self.toggles[0],
            1 => self.sliders[0] = 0.15 + 0.8 * noise(n),
            2 => self.qty = if self.qty >= 9 { 2 } else { self.qty + 1 },
            3 => self.stars = 1 + (n / 12 % 5) as u32,
            4 => self.view = (self.view + 1) % 3,
            5 => self.checks[0] = !self.checks[0],
            6 => self.sliders[1] = 0.1 + 0.85 * noise(n),
            7 => self.toggles[1] = !self.toggles[1],
            8 => self.qty = self.qty.saturating_sub(2).max(1),
            9 => self.sliders[2] = 0.1 + 0.85 * noise(n),
            10 => self.toggles[2] = !self.toggles[2],
            _ => self.checks[2] = !self.checks[2],
        }
    }

    /// Draws the scene into `body`.
    pub fn draw(&mut self, ui: &mut Ui<'_>, body: Rect, env: &Env) {
        self.pilot.observe(ui, body.inset(-env.pad(), 0.0), env);
        if let Some(n) = self.pilot.tick(env, STEP) {
            self.act(n);
        }
        if env.animating {
            ui.request_busy_frame(body);
        }
        let plan = plan(body, env.wide);
        let g = geometry(body, env.wide);
        self.targets.clear();
        for i in 0..3 {
            self.targets.note(format!("toggle{i}"), g.toggles[i]);
            self.targets.note(format!("slider{i}"), g.sliders[i]);
            self.targets.note(format!("radio{i}"), Rect::new(g.radios.x, g.radios.y + 28.0 * i as f32, g.radios.w, 28.0));
            // The box and the start of the label: a click on the empty part of the row misses.
            let c = g.checks[i];
            self.targets.note(format!("check{i}"), Rect::new(c.x, c.y, 24.0, c.h));
        }
        self.targets.note("minus", stepper::minus_rect(g.stepper));
        self.targets.note("plus", stepper::plus_rect(g.stepper));
        for n in 0..5 {
            self.targets.note(
                format!("star{}", n + 1),
                star_rating::button_rect((g.stars.x, g.stars.y), n),
            );
        }
        self.targets.note("reset", g.buttons[0]);
        self.targets.note("apply", g.buttons[1]);
        scrolled(ui, body, env.pad(), plan.height, |ui| {
            for (_, r) in &plan.cards {
                tile(ui, *r, 0.0);
            }
            for (card, inner) in &g.inner {
                match card {
                    Card::Toggles => self.toggles_card(ui, *inner, &g),
                    Card::Sliders => self.sliders_card(ui, *inner, &g),
                    Card::Stepper => self.stepper_card(ui, *inner, &g),
                    Card::View => self.view_card(ui, *inner, &g),
                    Card::Options => self.options_card(ui, *inner, &g),
                    Card::Strip => {}
                }
            }
            self.strip_card(ui, &g);
        });
    }

    fn toggles_card(&mut self, ui: &mut Ui<'_>, inner: Rect, g: &Geometry) {
        let t = *ui.theme();
        caps(ui, "Toggles", inner.x, inner.y);
        for (i, (name, hint, off)) in TOGGLES.iter().enumerate() {
            let sw = g.toggles[i];
            let cy = sw.y + sw.h / 2.0;
            let w = inner.w - 56.0;
            line(ui, name, inner.x, cy - 8.0, w, t.font_m, t.fg_primary);
            let sub = if self.toggles[i] { hint } else { off };
            line(ui, sub, inner.x, cy + 10.0, w, t.font_s, t.fg_secondary);
            ui.toggle(("toggle", i), sw, &mut self.toggles[i], name);
        }
    }

    fn sliders_card(&mut self, ui: &mut Ui<'_>, inner: Rect, g: &Geometry) {
        let t = *ui.theme();
        caps(ui, "Sliders", inner.x, inner.y);
        for (i, name) in SLIDERS.iter().enumerate() {
            let r = g.sliders[i];
            let id = ui.id(("readout", i));
            let shown = ui.animate_f32(id, self.sliders[i], &slider::VALUE);
            let text = match i {
                2 => format!("{} K", (3000.0 + shown * 4000.0).round() as u32),
                _ => format!("{}%", (shown * 100.0).round() as u32),
            };
            let cy = r.y - 10.0;
            line(ui, name, r.x, cy, r.w - 70.0, t.font_m, t.fg_primary);
            let mono = lune_ui::text::Spec::plain(
                text,
                lune_ui::text::Style::mono(t.font_base, t.font_base * 1.3, t.fg_secondary),
            );
            let p = ui.painter();
            let w = p.measure(&mono).0;
            p.text(&mono, r.right() - w, cy - 8.0);
            let opts = SliderOptions {
                min: 0.0,
                max: 1.0,
                step: 0.0,
                name,
            };
            ui.slider(("slider", i), r, &mut self.sliders[i], &opts);
        }
    }

    fn stepper_card(&mut self, ui: &mut Ui<'_>, inner: Rect, g: &Geometry) {
        let t = *ui.theme();
        caps(ui, "Stepper and rating", inner.x, inner.y);
        let w = inner.w - 140.0 - 8.0;
        let cy = g.stepper.y + g.stepper.h / 2.0;
        line(ui, "Quantity", inner.x, cy - 8.0, w, t.font_m, t.fg_primary);
        line(ui, "Units per order", inner.x, cy + 10.0, w, t.font_s, t.fg_secondary);
        let range = StepRange {
            min: 1.0,
            max: 20.0,
            step: 1.0,
        };
        let s = ui.stepper("stepper", g.stepper, self.qty, range, "Quantity");
        if let Some(set) = s.set {
            self.qty = set as u32;
        } else if let Some(step) = s.step {
            self.qty = self.qty.saturating_add_signed(step).clamp(1, 20);
        }
        let cy = g.stars.y + g.stars.h / 2.0;
        let w = inner.w - star_rating::row_width(5) - 8.0;
        line(ui, "Rating", inner.x, cy - 8.0, w, t.font_m, t.fg_primary);
        line(ui, &format!("{} of 5 stars", self.stars), inner.x, cy + 10.0, w, t.font_s, t.fg_secondary);
        ui.star_rating("stars", (g.stars.x, g.stars.y), &mut self.stars, 5, "Rating");
    }

    fn view_card(&mut self, ui: &mut Ui<'_>, inner: Rect, g: &Geometry) {
        let t = *ui.theme();
        caps(ui, "View", inner.x, inner.y);
        let options: Vec<RadioOption<'_>> = VIEWS
            .iter()
            .enumerate()
            .map(|(i, v)| RadioOption {
                key: i as u64,
                label: match i {
                    0 => "Day",
                    1 => "Week",
                    _ => "Month",
                },
                enabled: !v.0.is_empty(),
            })
            .collect();
        ui.radio_group("view", g.radios, &options, &mut self.view, "View");
        // The preview: the page that the choice swaps in, sliding and fading with the page-swap
        // motion, clipped to its own frame.
        let id = ui.id("swap");
        let pos = ui.animate_f32(id, self.view as f32, &Motion::of(Kind::PageSwap));
        let pv = g.preview;
        let p = ui.painter();
        p.fill(pv, 8.0, with_alpha(t.surface_overlay, 0.8));
        p.push_clip_rounded(pv, 8.0);
        for (i, (label, value, delta, tone, bars)) in VIEWS.iter().enumerate() {
            let near = 1.0 - (pos - i as f32).abs();
            if near <= 0.0 {
                continue;
            }
            let dx = (i as f32 - pos) * pv.w * 0.5;
            let p = ui.painter();
            p.push_translate(dx, 0.0);
            p.push_xform(lune_ui::paint::Xform {
                origin: (0.0, 0.0),
                scale: 1.0,
                dx: 0.0,
                dy: 0.0,
                alpha: near,
            });
            let page = pv.inset(14.0, 12.0);
            let h = ui.stat(
                ("preview", i),
                Rect::new(page.x, page.y, page.w, 0.0),
                &StatOptions {
                    label,
                    value,
                    sub: Some(delta),
                    sub_tone: *tone,
                    ..StatOptions::default()
                },
            );
            let top = page.y + h + 10.0;
            let room = page.bottom() - top;
            if room >= 16.0 {
                let gap = 3.0;
                let bw = (page.w - gap * (*bars as f32 - 1.0)) / *bars as f32;
                for j in 0..*bars {
                    let k = 0.25 + 0.75 * noise((i * 40 + j + 3) as u64 * 7);
                    let bh = (room * k).max(3.0);
                    let col = if j + 1 == *bars { t.data_hi } else { with_alpha(t.data, 0.45) };
                    ui.painter().fill(
                        Rect::new(page.x + j as f32 * (bw + gap), page.bottom() - bh, bw, bh),
                        (bw / 2.0).min(2.5),
                        col,
                    );
                }
            }
            let p = ui.painter();
            p.pop_xform();
            p.pop_translate();
        }
        ui.painter().pop_clip();
    }

    fn options_card(&mut self, ui: &mut Ui<'_>, inner: Rect, g: &Geometry) {
        caps(ui, "Options", inner.x, inner.y);
        let text = format!("{} applied", self.applied);
        let tone = if self.applied > 0 { Tone::Ok } else { Tone::Neutral };
        let bw = badge::width(ui.painter(), &text, false);
        ui.badge("applied", (inner.right() - bw, inner.y - 2.0), &text, tone, false);
        for (i, name) in CHECKS.iter().enumerate() {
            ui.checkbox(("check", i), g.checks[i], &mut self.checks[i], name);
        }
        if ui.button("reset", g.buttons[0], "Reset", ButtonKind::Secondary).clicked {
            self.checks = [false, true, true];
            self.applied = 0;
        }
        if ui.button("apply", g.buttons[1], "Apply", ButtonKind::Primary).clicked {
            self.applied += 1;
        }
    }

    fn strip_card(&mut self, ui: &mut Ui<'_>, g: &Geometry) {
        if g.strip.w <= 0.0 {
            return;
        }
        let r = g.strip;
        let pct = |v: f64| format!("{}%", v.round() as u32);
        let kelvin = |v: f64| format!("{} K", v.round() as u32);
        let count = |v: f64| format!("{}", v.round() as u32);
        let stars = |v: f64| format!("{:.0} / 5", v);
        let b = f64::from(self.sliders[0]) * 100.0;
        let v = f64::from(self.sliders[1]) * 100.0;
        let w = 3000.0 + f64::from(self.sliders[2]) * 4000.0;
        let texts = [
            pct(b),
            pct(v),
            kelvin(w),
            stars(f64::from(self.stars)),
            count(f64::from(self.qty)),
            count(f64::from(self.applied)),
        ];
        let cells: [StatOptions<'_>; 6] = [
            StatOptions {
                label: "Brightness",
                value: &texts[0],
                number: Some((b, &pct)),
                sub: Some("Display"),
                ..StatOptions::default()
            },
            StatOptions {
                label: "Volume",
                value: &texts[1],
                number: Some((v, &pct)),
                sub: Some("Output"),
                ..StatOptions::default()
            },
            StatOptions {
                label: "Warmth",
                value: &texts[2],
                number: Some((w, &kelvin)),
                sub: Some("Colour temperature"),
                ..StatOptions::default()
            },
            StatOptions {
                label: "Rating",
                value: &texts[3],
                number: Some((f64::from(self.stars), &stars)),
                sub: Some("Your score"),
                ..StatOptions::default()
            },
            StatOptions {
                label: "Quantity",
                value: &texts[4],
                number: Some((f64::from(self.qty), &count)),
                sub: Some("Per order"),
                ..StatOptions::default()
            },
            StatOptions {
                label: "Applied",
                value: &texts[5],
                number: Some((f64::from(self.applied), &count)),
                sub: Some("Times"),
                ..StatOptions::default()
            },
        ];
        // The strip shows the figures that matter at the width: four across, two on a narrow card.
        let shown: Vec<StatOptions<'_>> = if g.strip_cells == 4 {
            vec![cells[0], cells[1], cells[3], cells[5]]
        } else {
            vec![cells[0], cells[1]]
        };
        let inner = r.inset(0.0, 0.0);
        let h = ui.stat_strip(
            "figures",
            Rect::new(inner.x, inner.y + ((inner.h - 84.0) / 2.0).max(0.0), inner.w, 0.0),
            &shown,
        );
        let _ = h;
    }
}
