//! Glass: six frost cards at three depths float over the moving background, each a small widget
//! made of the toolkit's components (a stat that counts, a progress ring, a mini list with avatars
//! and badges, a "now playing" card, quick settings, storage bars). Each bobs on its own slow path,
//! shifts with the pointer by its depth, and sits on a shadow that grows with it; a primary button
//! opens the toolkit's dialog over its blurred scrim.
//!
//! The autopilot changes one thing per step (a number, a toggle, a person's status, the sync ring, a
//! storage bar), each through the component's own motion; a person's hand holds it back for
//! [`RESUME`](super::RESUME). Under Reduced, Off and `--still` the cards do not move at all.

use std::f32::consts::{FRAC_PI_2, PI};
use std::time::Duration;

use lune_ui::color::with_alpha;
use lune_ui::icon;
use lune_ui::motion::{Kind, Motion};
use lune_ui::paint::{Rect, StrokeStyle};
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::badge;
use lune_ui::widgets::button::{ButtonKind, ButtonOptions, IconButton, button_width_with};
use lune_ui::widgets::dialog::{Cancel, Dialog, DialogGlass};
use lune_ui::widgets::progress::ProgressOptions;
use lune_ui::widgets::stat::StatOptions;
use lune_ui::widgets::tone::Tone;

use super::{CARD_PAD, Env, GAP, Pilot, Targets, caps, glyph, line, scrolled, tile};

/// The autopilot's step.
pub const STEP: Duration = Duration::from_millis(1600);
/// The pointer's pull on the nearest cards, at the stage's edge, in logical px.
pub const PARALLAX_MAX: f32 = 16.0;
/// The room kept above the first card.
const TOP: f32 = 24.0;
/// A card's bob: the half-extent on x and y in px and the periods in seconds (6 to 12 s, none the
/// same), and the phases in turns.
#[derive(Debug, Clone, Copy)]
pub struct Bob {
    /// Half-extents of the two axes, px (at most 8).
    pub amp: (f32, f32),
    /// Periods of the two axes, seconds.
    pub period: (f32, f32),
    /// Phases of the two axes, in turns.
    pub phase: (f32, f32),
}

/// One of the cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Card {
    /// The people and their status.
    Team,
    /// The "now playing" card.
    Player,
    /// Two toggles and the button that opens the dialog.
    Quick,
    /// A number that counts, and a sparkline.
    Revenue,
    /// A progress ring.
    Sync,
    /// Three bars.
    Storage,
}

impl Card {
    /// Every card, nearest first.
    pub const ALL: [Card; 6] = [
        Card::Player,
        Card::Quick,
        Card::Team,
        Card::Revenue,
        Card::Sync,
        Card::Storage,
    ];

    /// How near the card is: 1.0 is nearest, 0.3 furthest. It scales the pointer's pull and the
    /// shadow.
    pub fn depth(self) -> f32 {
        match self {
            Card::Player | Card::Quick => 1.0,
            Card::Team | Card::Revenue => 0.6,
            Card::Sync | Card::Storage => 0.3,
        }
    }

    /// The card's height at scale 1, logical px.
    fn base_h(self) -> f32 {
        match self {
            Card::Player => 184.0,
            Card::Team => 232.0,
            Card::Quick => 196.0,
            Card::Revenue => 200.0,
            Card::Sync => 176.0,
            Card::Storage => 176.0,
        }
    }

    /// The card's bob.
    pub fn bob(self) -> Bob {
        let (amp, period, phase) = match self {
            Card::Player => ((4.0, 6.0), (9.0, 7.0), (0.00, 0.20)),
            Card::Quick => ((5.0, 5.0), (11.0, 8.0), (0.35, 0.60)),
            Card::Team => ((6.0, 7.0), (12.0, 9.0), (0.70, 0.05)),
            Card::Revenue => ((5.0, 6.0), (8.0, 10.5), (0.15, 0.45)),
            Card::Sync => ((7.0, 8.0), (10.0, 6.5), (0.50, 0.85)),
            Card::Storage => ((6.0, 7.0), (7.0, 11.5), (0.90, 0.30)),
        };
        Bob { amp, period, phase }
    }

    /// The card's offset at app time `t` seconds.
    pub fn bob_at(self, t: f64) -> (f32, f32) {
        let b = self.bob();
        let x = (t / f64::from(b.period.0) + f64::from(b.phase.0)) * std::f64::consts::TAU;
        let y = (t / f64::from(b.period.1) + f64::from(b.phase.1)) * std::f64::consts::TAU;
        (b.amp.0 * x.sin() as f32, b.amp.1 * y.sin() as f32)
    }
}

/// Where the cards sit in a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Every card and its rect at rest, in the body's coordinates.
    pub cards: Vec<(Card, Rect)>,
    /// The content's height from the body's top.
    pub height: f32,
    /// How many columns the cards are in.
    pub columns: usize,
}

/// Lays the cards out in `body`: three columns when there is room, two, or one stacked (which
/// scrolls). The cards keep the height their content needs and form a staggered cluster centred in
/// the stage.
pub fn plan(body: Rect, wide: bool) -> Plan {
    let n = if !wide || body.w < 560.0 {
        1
    } else if body.w < 880.0 {
        2
    } else {
        3
    };
    let cols: Vec<Vec<Card>> = match n {
        3 => vec![
            vec![Card::Team, Card::Sync],
            vec![Card::Player, Card::Revenue],
            vec![Card::Quick, Card::Storage],
        ],
        2 => vec![
            vec![Card::Team, Card::Sync, Card::Storage],
            vec![Card::Player, Card::Revenue, Card::Quick],
        ],
        _ => vec![vec![
            Card::Player,
            Card::Quick,
            Card::Team,
            Card::Revenue,
            Card::Sync,
            Card::Storage,
        ]],
    };
    // Room for the bob and the pointer's pull (at most 24 px) between a card and the edge.
    let side = if n == 1 { 16.0 } else { 8.0 };
    let body = Rect::new(
        body.x + side,
        body.y + TOP,
        body.w - 2.0 * side,
        body.h - TOP,
    );
    let mut cards = Vec::new();
    let mut height = body.h;
    if n == 1 {
        let cw = body.w.min(400.0);
        let mut y = body.y;
        let x = body.x + (body.w - cw) / 2.0;
        for k in &cols[0] {
            cards.push((*k, Rect::new(x, y, cw, k.base_h())));
            y += k.base_h() + 16.0;
        }
        height = (y - 16.0 - body.y).max(body.h);
    } else {
        // A staggered cluster, centred in the stage: the cards keep the height their content needs
        // and the free space lies between them. The columns start at different heights.
        let stagger = [0.0_f32, if n == 3 { 44.0 } else { 12.0 }, 18.0];
        let raw = (body.w - (n as f32 - 1.0) * GAP) / n as f32;
        let cw = raw.min(340.0);
        let gap_x = ((body.w - n as f32 * cw) / (n as f32 - 1.0)).clamp(GAP, 56.0);
        let x0 = body.x + (body.w - (n as f32 * cw + (n as f32 - 1.0) * gap_x)) / 2.0;
        let tallest = |scale: f32, gap: f32| {
            cols.iter()
                .enumerate()
                .map(|(i, c)| {
                    let sum: f32 = c.iter().map(|k| k.base_h() * scale).sum();
                    stagger[i] + sum + gap * (c.len() as f32 - 1.0)
                })
                .fold(0.0, f32::max)
        };
        let scale = if n == 2 {
            let slack = body.h - TOP - tallest(1.0, 20.0);
            (1.0 + slack / (tallest(1.0, 20.0) - 20.0 * 2.0)).clamp(0.9, 1.0)
        } else {
            1.0
        };
        let most = cols.iter().map(Vec::len).max().unwrap_or(1) as f32;
        let gap_y = if n == 2 {
            20.0
        } else {
            ((body.h * 0.9 - tallest(scale, 0.0)) / (most - 1.0)).clamp(28.0, 96.0)
        };
        let cluster = tallest(scale, gap_y);
        // The bob and the pull need room under the last card as above the first.
        height = height.max(cluster + TOP);
        let top = body.y + ((height - cluster) / 2.0).max(0.0);
        for (i, col) in cols.iter().enumerate() {
            let x = x0 + i as f32 * (cw + gap_x);
            let mut y = top + stagger[i];
            for card in col {
                let h = card.base_h() * scale;
                cards.push((*card, Rect::new(x, y, cw, h)));
                y += h + gap_y;
            }
        }
    }
    Plan {
        cards,
        height: height + TOP,
        columns: n,
    }
}

const SONGS: [(&str, &str); 3] = [
    ("Aurora Borealis", "Lune Ensemble"),
    ("Glass Hours", "Frosted Lights"),
    ("Parallax", "The Backdrops"),
];
/// How long a song is, seconds.
const SONG_LEN: f32 = 245.0;
const REVENUE: [(f64, &str, Tone); 4] = [
    (42_800.0, "+12.4% this week", Tone::Ok),
    (47_350.0, "+10.6% on last month", Tone::Ok),
    (45_100.0, "-4.7% since Monday", Tone::Warn),
    (51_900.0, "+15.1% on last quarter", Tone::Ok),
];
const SYNC: [f32; 4] = [0.72, 0.31, 0.94, 0.58];
const PEOPLE: [(&str, &str); 3] = [
    ("Mira Okafor", "Design"),
    ("Jun Tanaka", "Engineering"),
    ("Sofia Reyes", "Research"),
];
const STATUS: [(&str, Tone); 4] = [
    ("Online", Tone::Ok),
    ("In a call", Tone::Err),
    ("Away", Tone::Warn),
    ("Offline", Tone::Paused),
];
const DRIVES: [(&str, Tone, f32); 3] = [
    ("Photos", Tone::Neutral, 0.62),
    ("Documents", Tone::Ok, 0.28),
    ("Backups", Tone::Warn, 0.84),
];

/// The Glass scene's state.
#[derive(Debug)]
pub struct Glass {
    /// The autopilot.
    pub pilot: Pilot,
    /// The player is playing.
    pub playing: bool,
    /// The song, an index into the list.
    pub song: usize,
    /// How far into the song, 0 to 1.
    pub track: f32,
    /// Quick settings: the first toggle.
    pub wifi: bool,
    /// Quick settings: the second toggle.
    pub focus: bool,
    /// The dialog is open.
    pub dialog: bool,
    /// How many times the dialog was confirmed.
    pub applied: u32,
    /// The revenue shown, an index.
    pub revenue: usize,
    /// The sync ring's value, an index.
    pub sync: usize,
    /// Each person's status, an index.
    pub team: [usize; 3],
    /// Each drive's fill.
    pub storage: [f32; 3],
    /// The controls of the last frame.
    pub targets: Targets,
    last_frame: Option<Duration>,
}

impl Default for Glass {
    fn default() -> Self {
        Self {
            pilot: Pilot::default(),
            playing: true,
            song: 0,
            track: 0.38,
            wifi: true,
            focus: false,
            dialog: false,
            applied: 0,
            revenue: 0,
            sync: 0,
            team: [0, 1, 2],
            storage: [DRIVES[0].2, DRIVES[1].2, DRIVES[2].2],
            targets: Targets::default(),
            last_frame: None,
        }
    }
}

/// A pseudo-random number in 0..1 from `n`: the autopilot's values, the same every run.
fn noise(n: u64) -> f32 {
    let x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
    (x % 1000) as f32 / 1000.0
}

impl Glass {
    /// What the autopilot does at step `n`: one change.
    fn act(&mut self, n: u64) {
        match n % 8 {
            0 | 7 => self.revenue = (self.revenue + 1) % REVENUE.len(),
            1 => self.wifi = !self.wifi,
            2 | 6 => {
                let who = (n / 8 + n % 3) as usize % 3;
                self.team[who] = (self.team[who] + 1) % STATUS.len();
            }
            3 => self.sync = (self.sync + 1) % SYNC.len(),
            4 => {
                let which = (n / 8) as usize % 3;
                self.storage[which] = 0.2 + 0.7 * noise(n);
            }
            _ => self.focus = !self.focus,
        }
    }

    /// Draws the scene into `body`.
    pub fn draw(&mut self, ui: &mut Ui<'_>, body: Rect, env: &Env) {
        // A dialog left open when the scene was last on screen does not come back with it.
        if self
            .last_frame
            .is_some_and(|t| env.now.saturating_sub(t) > Duration::from_secs(1))
        {
            self.dialog = false;
        }
        let area = body.inset(-env.pad(), 0.0);
        self.pilot.observe(ui, area, env);
        if let Some(n) = self.pilot.tick(env, STEP) {
            self.act(n);
        }
        if env.animating {
            ui.request_busy_frame(body);
        }
        // The song runs on the app's clock while it plays.
        if let (true, Some(last)) = (self.playing && env.animating, self.last_frame) {
            let dt = env.now.saturating_sub(last).as_secs_f32().min(0.25);
            self.track += dt / SONG_LEN;
            if self.track >= 1.0 {
                self.track = 0.0;
                self.song = (self.song + 1) % SONGS.len();
            }
        }
        self.last_frame = Some(env.now);

        self.targets.clear();
        let plan = plan(body, env.wide);
        // The pointer's pull: the offset of the nearest depth, eased toward the pointer.
        let (tx, ty) = match (env.animating, ui.pointer_pos()) {
            (true, Some(p)) => (
                ((p.0 - (body.x + body.w / 2.0)) / (body.w / 2.0)).clamp(-1.0, 1.0),
                ((p.1 - (body.y + body.h / 2.0)) / (body.h / 2.0)).clamp(-1.0, 1.0),
            ),
            _ => (0.0, 0.0),
        };
        let (idx, idy) = (ui.id("pull-x"), ui.id("pull-y"));
        let pull = (
            ui.animate_f32(idx, tx * PARALLAX_MAX, &Motion::of(Kind::Move)),
            ui.animate_f32(idy, ty * PARALLAX_MAX, &Motion::of(Kind::Move)),
        );
        // The bob is a function of the clock, which stands at the still instant when nothing moves.
        let t = env.now.as_secs_f64();
        let place = |card: Card, r: Rect| {
            let (bx, by) = card.bob_at(t);
            let k = card.depth();
            Rect::new(r.x + bx - pull.0 * k, r.y + by - pull.1 * k, r.w, r.h)
        };
        let placed: Vec<(Card, Rect)> = plan
            .cards
            .iter()
            .map(|(c, r)| (*c, place(*c, *r)))
            .collect();
        scrolled(ui, body, env.pad(), plan.height, |ui| {
            // The glass first, furthest to nearest, so a near card's shadow falls on a far one; then
            // the contents in reading order, so Tab walks the page the way it reads.
            let mut by_depth = placed.clone();
            by_depth.sort_by(|a, b| a.0.depth().total_cmp(&b.0.depth()));
            for (card, r) in &by_depth {
                tile(ui, *r, card.depth());
            }
            let mut reading = placed;
            reading.sort_by(|a, b| {
                (a.1.y / 60.0)
                    .round()
                    .total_cmp(&(b.1.y / 60.0).round())
                    .then(a.1.x.total_cmp(&b.1.x))
            });
            for (card, r) in reading {
                let inner = r.inset(CARD_PAD, CARD_PAD);
                match card {
                    Card::Team => self.team_card(ui, inner),
                    Card::Player => self.player_card(ui, inner),
                    Card::Quick => self.quick_card(ui, inner),
                    Card::Revenue => self.revenue_card(ui, inner),
                    Card::Sync => self.sync_card(ui, inner),
                    Card::Storage => self.storage_card(ui, inner),
                }
            }
        });
        self.dialog_overlay(ui);
    }

    fn team_card(&mut self, ui: &mut Ui<'_>, r: Rect) {
        let t = *ui.theme();
        caps(ui, "Team", r.x, r.y);
        let online = self.team.iter().filter(|s| **s == 0).count();
        let head = format!("{online} online");
        let bw = badge::width(ui.painter(), &head, true);
        ui.badge("online", (r.right() - bw, r.y - 2.0), &head, Tone::Ok, true);
        let y0 = r.y + 24.0;
        let pitch = (r.bottom() - y0) / 3.0;
        let hues = [t.data, t.ok, t.err];
        for (i, (name, role)) in PEOPLE.iter().enumerate() {
            let cy = y0 + pitch * (i as f32 + 0.5);
            let disc = Rect::new(r.x, cy - 16.0, 32.0, 32.0);
            let initial: String = name.chars().take(1).collect();
            let p = ui.painter();
            p.fill(disc, 16.0, with_alpha(hues[i], 0.85));
            p.text_centered(
                &Spec::plain(initial, Style::ui(t.font_base, t.on_accent).medium()),
                disc,
            );
            let (word, tone) = STATUS[self.team[i]];
            let bw = badge::width(ui.painter(), word, true);
            ui.badge(("status", i), (r.right() - bw, cy - 9.0), word, tone, true);
            let w = (r.w - 44.0 - bw - 8.0).max(0.0);
            let p = ui.painter();
            let name = p.elide(
                &Spec::plain(*name, Style::ui(t.font_base, t.fg_primary).medium()),
                w,
            );
            p.text(&name, r.x + 44.0, cy - 17.0);
            line(ui, role, r.x + 44.0, cy + 8.0, w, t.font_s, t.fg_secondary);
        }
    }

    fn player_card(&mut self, ui: &mut Ui<'_>, r: Rect) {
        let t = *ui.theme();
        let (title, artist) = SONGS[self.song];
        let spare = r.h - 124.0;
        let side = 48.0 + (spare * 0.4).clamp(0.0, 24.0);
        let art = Rect::new(r.x, r.y, side, side);
        let p = ui.painter();
        p.linear(art, 10.0, t.data, t.err, 0.9);
        p.text_centered(&icon::glyph(glyph::MUSIC, side * 0.5, t.on_accent), art);
        let tx = r.x + side + 12.0;
        let w = r.w - side - 12.0;
        let p = ui.painter();
        let spec = p.elide(
            &Spec::plain(title, Style::ui(t.font_m, t.fg_bright).medium()),
            w,
        );
        p.text(&spec, tx, art.y + side / 2.0 - 20.0);
        line(
            ui,
            artist,
            tx,
            art.y + side / 2.0 + 14.0,
            w,
            t.font_base,
            t.fg_secondary,
        );

        let bar_y = art.bottom() + 14.0 + (spare - (side - 48.0)) * 0.45;
        let opts = ProgressOptions {
            value: Some(self.track),
            name: "Track position",
            ..ProgressOptions::default()
        };
        ui.progress("track", Rect::new(r.x, bar_y, r.w, 0.0), &opts);
        let elapsed = (self.track * SONG_LEN) as u32;
        let left = SONG_LEN as u32 - elapsed;
        // The app's own numbers are drawn in the UI face.
        let small = |s: String| Spec::plain(s, Style::ui(t.font_s, t.fg_secondary));
        let p = ui.painter();
        p.text(
            &small(format!("{}:{:02}", elapsed / 60, elapsed % 60)),
            r.x,
            bar_y + 8.0,
        );
        let rt = small(format!("-{}:{:02}", left / 60, left % 60));
        let rw = p.measure(&rt).0;
        p.text(&rt, r.right() - rw, bar_y + 8.0);

        let cy = r.bottom() - 16.0;
        let cx = r.x + r.w / 2.0;
        let slot =
            |dx: f32, size: f32| Rect::new(cx + dx - size / 2.0, cy - size / 2.0, size, size);
        self.targets.note("back", slot(-52.0, 32.0));
        self.targets.note("play", slot(0.0, 40.0));
        self.targets.note("next", slot(52.0, 32.0));
        let back = ui.icon_button(
            "back",
            slot(-52.0, 32.0),
            &IconButton {
                glyph: glyph::PREV,
                glyph_size: 16.0,
                color: Some(t.fg_primary),
                radius: 16.0,
                name: "Previous",
            },
        );
        let play_glyph = if self.playing {
            glyph::PAUSE
        } else {
            glyph::PLAY
        };
        let play = ui.icon_button(
            "play",
            slot(0.0, 40.0),
            &IconButton {
                glyph: play_glyph,
                glyph_size: 22.0,
                color: Some(t.fg_bright),
                radius: 20.0,
                name: if self.playing { "Pause" } else { "Play" },
            },
        );
        let next = ui.icon_button(
            "next",
            slot(52.0, 32.0),
            &IconButton {
                glyph: glyph::NEXT,
                glyph_size: 16.0,
                color: Some(t.fg_primary),
                radius: 16.0,
                name: "Next",
            },
        );
        if back.clicked {
            self.track = 0.0;
        }
        if play.clicked {
            self.playing = !self.playing;
        }
        if next.clicked {
            self.song = (self.song + 1) % SONGS.len();
            self.track = 0.0;
        }
    }

    fn quick_card(&mut self, ui: &mut Ui<'_>, r: Rect) {
        let t = *ui.theme();
        caps(ui, "Quick settings", r.x, r.y);
        let button = Rect::new(r.x, r.bottom() - 32.0, r.w, 32.0);
        let y0 = r.y + 22.0;
        let pitch = (button.y - 6.0 - y0) / 2.0;
        let rows = [
            (
                "Wi-Fi",
                if self.wifi {
                    "Connected to Lune"
                } else {
                    "Off"
                },
            ),
            (
                "Focus",
                if self.focus {
                    "Notifications paused"
                } else {
                    "Notifications on"
                },
            ),
        ];
        for (i, (name, sub)) in rows.iter().enumerate() {
            let cy = y0 + pitch * (i as f32 + 0.5);
            let w = r.w - 56.0;
            line(ui, name, r.x, cy - 8.0, w, t.font_m, t.fg_primary);
            line(ui, sub, r.x, cy + 9.0, w, t.font_s, t.fg_secondary);
            let sw = Rect::new(r.right() - 40.0, cy - 12.0, 40.0, 24.0);
            self.targets.note(["wifi", "focus"][i], sw);
            let on = if i == 0 {
                &mut self.wifi
            } else {
                &mut self.focus
            };
            ui.toggle(("quick", i), sw, on, name);
        }
        self.targets.note("open", button);
        if ui
            .button("open", button, "Review glass settings", ButtonKind::Primary)
            .clicked
        {
            self.dialog = true;
        }
    }

    fn revenue_card(&mut self, ui: &mut Ui<'_>, r: Rect) {
        let t = *ui.theme();
        let (value, sub, tone) = REVENUE[self.revenue];
        let fmt = |v: f64| format!("{}", v.round() as u64);
        let text = fmt(value);
        let opts = StatOptions {
            label: "Revenue, USD",
            value: &text,
            number: Some((value, &fmt)),
            sub: Some(sub),
            sub_tone: tone,
            meter: Some((value / 60_000.0) as f32),
            ..StatOptions::default()
        };
        let h = ui.stat("revenue", Rect::new(r.x, r.y, r.w, 0.0), &opts);
        // A sparkline of bars under it; each eases to its new height when the figure changes.
        let top = r.y + h + 14.0;
        let room = r.bottom() - top;
        if room >= 24.0 {
            let n = if r.w < 230.0 { 10 } else { 16 };
            let gap = 4.0;
            let bw = (r.w - gap * (n as f32 - 1.0)) / n as f32;
            for j in 0..n {
                let want = 0.30 + 0.70 * noise((j as u64 + 1) * 31 + self.revenue as u64 * 977);
                let id = ui.id(("bar", j));
                let k = ui.animate_f32(id, want, &Motion::of(Kind::Expand));
                let bh = (room * k).max(3.0);
                let col = if j + 1 == n {
                    t.data_hi
                } else {
                    with_alpha(t.data, 0.45)
                };
                ui.painter().fill(
                    Rect::new(r.x + j as f32 * (bw + gap), r.bottom() - bh, bw, bh),
                    (bw / 2.0).min(3.0),
                    col,
                );
            }
        }
    }

    fn sync_card(&mut self, ui: &mut Ui<'_>, r: Rect) {
        let t = *ui.theme();
        caps(ui, "Sync", r.x, r.y);
        let area = Rect::new(r.x, r.y + 22.0, r.w, (r.h - 22.0).max(0.0));
        let d = area.h.min(92.0).min(r.w * 0.40);
        let c = [area.x + d / 2.0, area.y + area.h / 2.0];
        let radius = d / 2.0 - 5.0;
        let id = ui.id("ring");
        let frac = ui.animate_f32(id, SYNC[self.sync], &Motion::of(Kind::Expand));
        let p = ui.painter();
        p.arc(
            c,
            radius,
            -FRAC_PI_2,
            2.0 * PI,
            StrokeStyle::solid(t.surface_active, 8.0),
        );
        if frac > 0.004 {
            p.arc(
                c,
                radius,
                -FRAC_PI_2,
                2.0 * PI * frac,
                StrokeStyle::solid(t.data, 8.0),
            );
        }
        let pct = Spec::plain(
            format!("{}%", (frac * 100.0).round() as u32),
            Style::ui(t.font_m, t.fg_bright).medium(),
        );
        p.text_centered(
            &pct,
            Rect::new(c[0] - radius, c[1] - 12.0, radius * 2.0, 24.0),
        );
        let x = area.x + d + 20.0;
        let w = (area.right() - x).max(0.0);
        let done = SYNC[self.sync] >= 0.9;
        let (tone, word) = if done {
            (Tone::Ok, "Up to date")
        } else {
            (Tone::Data, "Syncing")
        };
        // The status, the count and the button side by side with the ring, as one block centred on it.
        let top = c[1] - 38.0;
        ui.status_dot("state", (x, top), tone, word);
        let files = format!("{} of 18 files", (SYNC[self.sync] * 18.0).round() as u32);
        line(ui, &files, x, top + 34.0, w, t.font_base, t.fg_secondary);
        let options = ButtonOptions {
            kind: ButtonKind::Primary,
            small: true,
            ..ButtonOptions::default()
        };
        let bw = button_width_with(ui.painter(), "Sync now", &options).min(w);
        let sync_button = Rect::new(x, top + 52.0, bw, 24.0);
        self.targets.note("sync", sync_button);
        let b = ui.button_with("sync", sync_button, "Sync now", &options);
        if b.clicked {
            self.sync = (self.sync + 1) % SYNC.len();
        }
    }

    fn storage_card(&mut self, ui: &mut Ui<'_>, r: Rect) {
        caps(ui, "Storage", r.x, r.y);
        let y0 = r.y + 24.0;
        let pitch = (r.bottom() - y0) / 3.0;
        for (i, (name, tone, _)) in DRIVES.iter().enumerate() {
            let id = ui.id(("drive", i));
            let v = ui.animate_f32(id, self.storage[i], &Motion::of(Kind::Expand));
            let used = format!("{:.0}GB", v * 256.0);
            let opts = ProgressOptions {
                value: Some(v),
                tone: *tone,
                head: Some((name, &used)),
                name,
            };
            let y = y0 + pitch * i as f32 + (pitch - 28.0).max(0.0) / 2.0;
            ui.progress(("drive", i), Rect::new(r.x, y, r.w, 0.0), &opts);
        }
    }

    fn dialog_overlay(&mut self, ui: &mut Ui<'_>) {
        if !self.dialog {
            return;
        }
        let t = *ui.theme();
        let spec = Dialog {
            title: "Apply glass settings",
            message: "The frost, the tint and the blur reach every surface at once. Nothing is saved.",
            extra_h: 28.0,
            confirm: "Apply",
            cancel: Cancel::Default,
        };
        let r = ui.dialog("dialog", &spec, &DialogGlass::of(&t), |ui, x, y, w| {
            line(
                ui,
                "6 surfaces, 3 of them blurred",
                x,
                y + 14.0,
                w,
                t.font_base,
                t.fg_secondary,
            );
        });
        if r.confirmed {
            self.applied += 1;
        }
        if r.confirmed || r.cancelled {
            self.dialog = false;
        }
    }
}
