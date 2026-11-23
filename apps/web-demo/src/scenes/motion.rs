//! Motion: four demos of how things move. A pager of four pages, a bottom sheet that steps through
//! its detents, a navigation stack that pushes and pops a detail page, and the physics behind them
//! (a damped spring and a fling's decay) drawn as curves with a dot riding each.
//!
//! The phone demos sit in phone-like frames of one size and are clipped to their own screens. The
//! toolkit's bottom sheet is a window-wide modal and cannot be confined to a rect, so the frame
//! shows a miniature of it (the same detents, the grabber's click and drag, the drawer motion), and
//! a button opens the real one over the whole window. The autopilot makes one change a second, in
//! turn: the pager, the sheet, the stack, each every 3 s; a person's hand holds it back for
//! [`RESUME`](super::RESUME).

use std::f32::consts::FRAC_PI_2;
use std::time::Duration;

use lune_ui::color::{Rgba, clear, mix, premul, with_alpha, with_alpha_byte};
use lune_ui::icon;
use lune_ui::input::NamedKey;
use lune_ui::motion::{DampedSpring, Decay, Kind, Motion};
use lune_ui::paint::{Rect, StrokeStyle};
use lune_ui::text::{Spec, Style};
use lune_ui::ui::{Sense, Ui};
use lune_ui::widgets::bottom_sheet::{BottomSheet, Detent};
use lune_ui::widgets::button::{ButtonKind, ButtonOptions};
use lune_ui::widgets::nav_bar::NavBar;
use lune_ui::widgets::pager::PagerOptions;

use super::{CARD_PAD, Env, GAP, Pilot, Targets, caps, glyph, line, scrolled, tile};

/// The autopilot's step: one change a second.
pub const STEP: Duration = Duration::from_secs(1);
/// How often each demo changes, in steps.
pub const EVERY: u64 = 3;
/// The height of a demo's caption row.
const CAPTION: f32 = 26.0;
/// The status bar's height at the top of a phone screen.
const STATUS: f32 = 26.0;
/// The detents of the miniature sheet, as shares of the screen's height (peek, medium, large).
pub const DETENTS: [f32; 3] = [0.2, 0.5, 0.85];
/// The pager's autopilot: the pages in turn, there and back.
const PAGER_SEQ: [usize; 6] = [0, 1, 2, 3, 2, 1];
/// The sheet's autopilot: the detents in turn, there and back.
const SHEET_SEQ: [usize; 4] = [0, 1, 2, 1];
/// The physics cycle, seconds: a run starts 0.4 s before the still instant.
const CYCLE: f64 = 5.0;
/// The spring's response and damping, and the fling's speed and decay rate.
pub const SPRING: DampedSpring = DampedSpring::new(Duration::from_millis(450), 0.55);
/// The distance the spring travels in px (a spring is at rest within half a px, so the curve is
/// drawn for a real distance and normalised).
pub const SPRING_SPAN: f32 = 240.0;
/// The fling's start speed in px/s.
pub const FLING_V0: f32 = 1800.0;
/// The fling's decay (the fraction of the speed kept per millisecond).
pub const FLING: Decay = Decay::new(0.998);

/// One of the four demos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Demo {
    /// The pager.
    Pager,
    /// The bottom sheet.
    Sheet,
    /// The navigation stack.
    Stack,
    /// The spring and the decay.
    Physics,
}

impl Demo {
    /// All four, in reading order.
    pub const ALL: [Demo; 4] = [Demo::Pager, Demo::Sheet, Demo::Stack, Demo::Physics];

    fn title(self) -> &'static str {
        match self {
            Demo::Pager => "Pager",
            Demo::Sheet => "Bottom sheet",
            Demo::Stack => "Navigation stack",
            Demo::Physics => "Physics",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Demo::Pager => "swipe or wait",
            Demo::Sheet => "drag or click",
            Demo::Stack => "tap a row",
            Demo::Physics => "spring and fling",
        }
    }
}

/// Where the demos sit in a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Every demo and the rect of its frame (the caption row is above it).
    pub frames: Vec<(Demo, Rect)>,
    /// The content's height from the body's top.
    pub height: f32,
    /// How many columns there are.
    pub columns: usize,
}

/// Lays the demos out: four frames in a row when there is room, a 2 by 2 grid, or one column that
/// scrolls. The frames are one size.
pub fn plan(body: Rect, wide: bool) -> Plan {
    let n = if !wide || body.w < 560.0 {
        1
    } else if body.w < 960.0 {
        2
    } else {
        4
    };
    let cw = if n == 1 {
        body.w.min(340.0)
    } else {
        (body.w - (n as f32 - 1.0) * GAP) / n as f32
    };
    let rows = 4 / n;
    let fh = match n {
        4 => (cw * 2.2).min(body.h - CAPTION),
        2 => ((body.h - rows as f32 * CAPTION - GAP) / rows as f32).clamp(240.0, cw * 1.7),
        _ => 380.0,
    };
    let block = CAPTION + fh;
    let used = rows as f32 * block + (rows as f32 - 1.0) * GAP;
    let margin = if n == 1 { 0.0 } else { ((body.h - used) / 2.0).max(0.0) };
    let x0 = body.x + (body.w - (n as f32 * cw + (n as f32 - 1.0) * GAP)) / 2.0;
    let frames = Demo::ALL
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let (col, row) = (i % n, i / n);
            (
                *d,
                Rect::new(
                    x0 + col as f32 * (cw + GAP),
                    body.y + margin + row as f32 * (block + GAP) + CAPTION,
                    cw,
                    fh,
                ),
            )
        })
        .collect();
    Plan {
        frames,
        height: used + 2.0 * margin,
        columns: n,
    }
}

/// The screen inside a phone frame.
pub fn screen_of(frame: Rect) -> Rect {
    frame.inset(BEZEL, BEZEL)
}

/// The bezel's width.
const BEZEL: f32 = 7.0;

/// The pager's pages: a glyph, a title, a line and the two hues of its gradient.
const PAGES: [(&str, &str, &str); 4] = [
    (glyph::LAYERS, "Frosted", "Every surface blurs what is behind it"),
    (glyph::PULSE, "Moving", "Springs and tweens, one clock for all"),
    (glyph::GRAPH, "Charted", "Numbers that count to their value"),
    (glyph::PALETTE, "Themed", "One token set, every widget follows"),
];
const ROWS: [(&str, &str); 4] = [
    ("Aurora Borealis", "Lune Ensemble, 4:05"),
    ("Glass Hours", "Frosted Lights, 3:12"),
    ("Parallax", "The Backdrops, 5:40"),
    ("Soft Focus", "Gaussian Blur, 2:58"),
];

/// The Motion scene's state.
#[derive(Debug)]
pub struct Motions {
    /// The autopilot.
    pub pilot: Pilot,
    /// The pager's page.
    pub page: usize,
    /// The miniature sheet's detent.
    pub detent: usize,
    /// While a finger or a mouse drags the sheet: its height as a share of the screen.
    pub sheet_drag: Option<f32>,
    /// The stack's depth: 1 is the list, 2 the detail.
    pub depth: usize,
    /// How many times a row was tapped.
    pub opened: u32,
    /// The real bottom sheet is open.
    pub real_sheet: bool,
    /// The controls of the last frame.
    pub targets: Targets,
    pop_wanted: bool,
    last_frame: Option<Duration>,
}

impl Default for Motions {
    fn default() -> Self {
        Self {
            pilot: Pilot::default(),
            page: 0,
            detent: 1,
            sheet_drag: None,
            depth: 1,
            opened: 0,
            real_sheet: false,
            targets: Targets::default(),
            pop_wanted: false,
            last_frame: None,
        }
    }
}

/// The colour of a phone screen: the window's tint with a wash of the theme's `data`.
fn screen_bg(t: &lune_ui::theme::Theme) -> Rgba {
    mix(opaque(t.glass.window.tint), t.data, 0.24)
}

/// `c` made opaque.
fn opaque(c: Rgba) -> Rgba {
    with_alpha_byte(c, 255)
}

impl Motions {
    /// What the autopilot does at step `n`: one change, each demo in turn.
    fn act(&mut self, n: u64) {
        let k = (n / EVERY) as usize;
        match n % EVERY {
            0 => self.page = PAGER_SEQ[k % PAGER_SEQ.len()],
            1 => self.detent = SHEET_SEQ[k % SHEET_SEQ.len()],
            _ => {
                if k % 2 == 0 {
                    if self.depth == 1 {
                        self.depth = 2;
                    }
                } else if self.depth == 2 {
                    self.pop_wanted = true;
                }
            }
        }
    }

    /// Draws the scene into `body`.
    pub fn draw(&mut self, ui: &mut Ui<'_>, body: Rect, env: &Env) {
        if self
            .last_frame
            .is_some_and(|t| env.now.saturating_sub(t) > Duration::from_secs(1))
        {
            self.real_sheet = false;
        }
        self.last_frame = Some(env.now);
        self.pilot.observe(ui, body.inset(-env.pad(), 0.0), env);
        if let Some(n) = self.pilot.tick(env, STEP) {
            self.act(n);
        }
        if env.animating {
            ui.request_busy_frame(body);
        }
        let plan = plan(body, env.wide);
        self.targets.clear();
        scrolled(ui, body, env.pad(), plan.height, |ui| {
            for (demo, frame) in &plan.frames {
                let t = *ui.theme();
                caps(ui, demo.title(), frame.x + 4.0, frame.y - CAPTION + 5.0);
                if *demo == Demo::Sheet {
                    // The toolkit's own sheet is a modal over the whole window: a button opens it.
                    let open = Rect::new(frame.right() - 112.0, frame.y - CAPTION - 1.0, 112.0, 24.0);
                    let small = ButtonOptions {
                        small: true,
                        ..ButtonOptions::default()
                    };
                    self.targets.note("real-open", open);
                    if ui.button_with("real-open", open, "Open real sheet", &small).clicked {
                        self.real_sheet = true;
                    }
                } else {
                    let hint = Spec::plain(demo.hint(), Style::ui(t.font_s, t.fg_tertiary));
                    let w = ui.painter().measure(&hint).0;
                    ui.painter()
                        .text(&hint, frame.right() - w - 4.0, frame.y - CAPTION + 5.0);
                }
                match demo {
                    Demo::Pager => self.pager(ui, *frame),
                    Demo::Sheet => self.sheet(ui, *frame),
                    Demo::Stack => self.stack(ui, *frame),
                    Demo::Physics => physics(ui, *frame, env),
                }
            }
        });
        self.real_sheet_overlay(ui);
    }

    /// The phone: a frost bezel on a shadow, the screen inside it, and the island. Returns the
    /// screen's rect and radius.
    fn phone(&self, ui: &mut Ui<'_>, frame: Rect) -> (Rect, f32) {
        let t = *ui.theme();
        let radius = (frame.w * 0.12).clamp(16.0, 30.0);
        let screen = screen_of(frame);
        let p = ui.painter();
        p.shadow(
            Rect::new(frame.x, frame.y + 8.0, frame.w, frame.h),
            radius,
            14.0,
            with_alpha(t.dialog_backdrop, 0.45),
        );
        p.frost(frame, radius, premul(with_alpha(t.menu_glass, 0.6)), true);
        p.ring(frame, radius, 1.0, t.menu_popup_border, false);
        let r = radius - BEZEL;
        p.fill(screen, r, screen_bg(&t));
        p.linear(screen, r, t.surface_hover, clear(t.surface_hover), FRAC_PI_2);
        (screen, r)
    }

    /// The island over the top of a screen.
    fn island(&self, ui: &mut Ui<'_>, screen: Rect) {
        let t = *ui.theme();
        let w = (screen.w * 0.28).clamp(40.0, 72.0);
        ui.painter().fill(
            Rect::new(screen.x + (screen.w - w) / 2.0, screen.y + 8.0, w, 14.0),
            7.0,
            opaque(t.dialog_backdrop),
        );
    }

    fn pager(&mut self, ui: &mut Ui<'_>, frame: Rect) {
        let t = *ui.theme();
        let (screen, r) = self.phone(ui, frame);
        ui.painter().push_clip_rounded(screen, r);
        let hues: [(Rgba, Rgba); 4] = [
            (t.data, t.link),
            (t.err, mix(t.data, t.err, 0.5)),
            (t.ok, t.data_hi),
            (t.gold, t.err),
        ];
        let area = Rect::new(screen.x, screen.y + STATUS, screen.w, screen.h - STATUS);
        self.targets.note("pager", area);
        let opts = PagerOptions::new("Features").with_dots(true);
        let mut page = self.page;
        ui.pager("pager", area, PAGES.len(), &mut page, &opts, |ui, i, rect| {
            let card = Rect::new(rect.x + 12.0, rect.y + 8.0, rect.w - 24.0, rect.h - 8.0 - 38.0);
            let (glyph_, title, text) = PAGES[i];
            let (a, b) = hues[i];
            let p = ui.painter();
            p.linear(card, 18.0, with_alpha(a, 0.92), with_alpha(b, 0.92), 0.9);
            p.linear(card, 18.0, t.surface_hover, clear(t.surface_hover), FRAC_PI_2);
            p.ring(card, 18.0, 1.0, t.menu_popup_border, false);
            let cx = card.x + card.w / 2.0;
            let disc = Rect::new(cx - 28.0, card.y + card.h * 0.22, 56.0, 56.0);
            p.fill(disc, 28.0, with_alpha(t.on_accent, 0.22));
            p.text_centered(&icon::glyph(glyph_, 26.0, t.on_accent), disc);
            let spec = Spec::plain(title, Style::ui(t.font_title, t.on_accent).medium());
            let tw = p.measure(&spec).0;
            p.text(&spec, cx - tw / 2.0, disc.bottom() + 18.0);
            let spec = Spec::plain(text, Style::ui(t.font_base, with_alpha(t.on_accent, 0.9)))
                .wrapped(card.w - 40.0);
            p.text_centered(
                &spec,
                Rect::new(card.x + 20.0, disc.bottom() + 44.0, card.w - 40.0, 44.0),
            );
        });
        // A page the person swiped to is the page now (the autopilot's changes are not echoed).
        self.page = page;
        self.island(ui, screen);
        ui.painter().pop_clip();
    }

    fn sheet(&mut self, ui: &mut Ui<'_>, frame: Rect) {
        let t = *ui.theme();
        let (screen, r) = self.phone(ui, frame);
        let p = ui.painter();
        p.push_clip_rounded(screen, r);
        // The page behind: a list of skeleton rows.
        let mut y = screen.y + STATUS + 8.0;
        let row_h = 44.0;
        let mut i = 0;
        while y + row_h <= screen.bottom() {
            let disc = Rect::new(screen.x + 14.0, y + 6.0, 32.0, 32.0);
            p.fill(disc, 16.0, with_alpha(t.surface_active, 0.9));
            p.fill(
                Rect::new(disc.right() + 12.0, y + 10.0, (screen.w - 90.0) * (0.55 + 0.12 * (i % 3) as f32), 8.0),
                4.0,
                with_alpha(t.fg_faint, 0.8),
            );
            p.fill(
                Rect::new(disc.right() + 12.0, y + 25.0, (screen.w - 90.0) * 0.4, 6.0),
                3.0,
                with_alpha(t.fg_faint, 0.45),
            );
            y += row_h;
            i += 1;
        }
        // The sheet: its top follows the detent, or the hand.
        let id = ui.id("sheet-top");
        let frac = match self.sheet_drag {
            Some(d) => ui.snap_f32(id, d),
            None => ui.animate_f32(id, DETENTS[self.detent], &Motion::of(Kind::Drawer)),
        };
        let top = screen.bottom() - screen.h * frac;
        let p = ui.painter();
        p.fill(screen, 0.0, with_alpha(t.dialog_backdrop, 0.55 * frac));
        let sheet = Rect::new(screen.x, top, screen.w, screen.bottom() - top + 14.0);
        p.fill_radii(sheet, [14.0, 14.0, 0.0, 0.0], mix(screen_bg(&t), t.fg_bright, 0.10));
        p.frost_radii(sheet, [14.0, 14.0, 0.0, 0.0], premul(with_alpha(t.menu_glass, 0.5)), true);
        p.ring(sheet, 14.0, 1.0, t.menu_popup_border, false);
        // The grabber: a pill in a band that takes a click (the next detent) and a drag.
        let band = Rect::new(screen.x, top, screen.w, 28.0);
        let pill = Rect::new(screen.x + (screen.w - 36.0) / 2.0, top + 8.0, 36.0, 5.0);
        self.targets.note("grabber", band);
        let g = ui.interact(
            ui.id("grabber"),
            band,
            Sense {
                focusable: true,
                ..Sense::DRAG
            },
        );
        ui.painter().fill(pill, 2.5, t.fg_faint);
        if g.dragged {
            let was = self.sheet_drag.unwrap_or(frac);
            self.sheet_drag = Some((was - g.drag_delta.1 / screen.h).clamp(0.08, 0.95));
        }
        if g.drag_stopped {
            let at = self.sheet_drag.take().unwrap_or(frac);
            self.detent = (0..DETENTS.len())
                .min_by(|a, b| (DETENTS[*a] - at).abs().total_cmp(&(DETENTS[*b] - at).abs()))
                .unwrap_or(0);
        } else if g.clicked && !g.dragged {
            self.detent = (self.detent + 1) % DETENTS.len();
        }
        if ui.consume_key(g.id, NamedKey::Enter) || ui.consume_key(g.id, NamedKey::Space) {
            self.detent = (self.detent + 1) % DETENTS.len();
        }
        // Its content, clipped to the sheet.
        let content = Rect::new(screen.x, top + 28.0, screen.w, (screen.bottom() - top - 28.0).max(0.0));
        let p = ui.painter();
        p.push_clip(content);
        line(ui, "Options", screen.x + 16.0, top + 46.0, screen.w - 32.0, t.font_m, t.fg_bright);
        for (i, item) in ["Share", "Duplicate", "Move to folder", "Delete"].iter().enumerate() {
            let y = top + 70.0 + i as f32 * 36.0;
            line(ui, item, screen.x + 16.0, y + 18.0, screen.w - 32.0, t.font_base, t.fg_primary);
            ui.painter().fill(
                Rect::new(screen.x + 16.0, y + 35.0, screen.w - 32.0, 1.0),
                0.0,
                t.surface_separator,
            );
        }
        let p = ui.painter();
        p.pop_clip();
        self.island(ui, screen);
        ui.painter().pop_clip();
    }

    fn stack(&mut self, ui: &mut Ui<'_>, frame: Rect) {
        let t = *ui.theme();
        let (screen, r) = self.phone(ui, frame);
        ui.painter().push_clip_rounded(screen, r);
        let area = Rect::new(screen.x, screen.y, screen.w, screen.h);
        let bg = screen_bg(&t);
        let (mut push, mut back) = (false, false);
        let mut notes: Vec<(String, Rect)> = Vec::new();
        let depth = self.depth;
        let resp = ui.nav_stack("stack", area, depth, |ui, level, rect| {
            ui.painter().fill(rect, 0.0, bg);
            let bar = Rect::new(rect.x, rect.y + STATUS - 8.0, rect.w, 52.0);
            if level == 0 {
                ui.nav_bar("bar", bar, &NavBar::new("Library"));
                for (i, (title, sub)) in ROWS.iter().enumerate() {
                    let row = Rect::new(rect.x, bar.bottom() + 4.0 + i as f32 * 56.0, rect.w, 56.0);
                    if row.bottom() > rect.bottom() {
                        break;
                    }
                    notes.push((format!("row{i}"), row));
                    let id = ui.id(("row", i));
                    let resp = ui.interact(id, row, Sense::CLICK_FOCUS);
                    let p = ui.painter();
                    if resp.hovered || resp.pressed {
                        p.fill(row.inset(6.0, 2.0), 8.0, t.surface_hover);
                    }
                    let disc = Rect::new(row.x + 14.0, row.y + 12.0, 32.0, 32.0);
                    p.linear(disc, 8.0, t.data, t.err, 0.9);
                    p.text_centered(&icon::glyph(icon::NOTE, 14.0, t.on_accent), disc);
                    line(ui, title, disc.right() + 12.0, row.y + 20.0, rect.w - 100.0, t.font_base, t.fg_primary);
                    line(ui, sub, disc.right() + 12.0, row.y + 38.0, rect.w - 100.0, t.font_s, t.fg_secondary);
                    let chev = icon::glyph(icon::CHEVRON_RIGHT, 14.0, t.fg_tertiary);
                    ui.painter().text_centered(&chev, Rect::new(row.right() - 30.0, row.y, 16.0, row.h));
                    if resp.clicked {
                        push = true;
                    }
                }
            } else {
                let b = ui.nav_bar("bar", bar, &NavBar::new("Aurora").with_back("Library"));
                if b.back {
                    back = true;
                }
                let art = Rect::new(rect.x + 16.0, bar.bottom() + 12.0, rect.w - 32.0, (rect.w * 0.5).min(150.0));
                let p = ui.painter();
                p.linear(art, 14.0, with_alpha(t.data, 0.9), with_alpha(t.err, 0.9), 0.9);
                p.text_centered(&icon::glyph(icon::NOTE, 34.0, t.on_accent), art);
                line(ui, "Aurora Borealis", art.x, art.bottom() + 26.0, art.w, t.font_m, t.fg_bright);
                line(ui, "Lune Ensemble, 4:05", art.x, art.bottom() + 48.0, art.w, t.font_base, t.fg_secondary);
                for j in 0..3 {
                    let y = art.bottom() + 72.0 + j as f32 * 16.0;
                    ui.painter().fill(
                        Rect::new(art.x, y, art.w * (1.0 - 0.18 * j as f32), 6.0),
                        3.0,
                        with_alpha(t.fg_faint, 0.6),
                    );
                }
            }
        });
        for (name, rect) in notes {
            self.targets.note(name, rect);
        }
        if push && self.depth == 1 {
            self.depth = 2;
            self.opened += 1;
        }
        if back {
            self.pop_wanted = true;
        }
        if std::mem::take(&mut self.pop_wanted) {
            ui.nav_stack_pop("stack");
        }
        if resp.popped {
            self.depth = 1;
        }
        self.island(ui, screen);
        ui.painter().pop_clip();
    }

    fn real_sheet_overlay(&mut self, ui: &mut Ui<'_>) {
        if !self.real_sheet {
            return;
        }
        let mut open = self.real_sheet;
        let mut done = false;
        let detents = [Detent::Fraction(0.5), Detent::Fraction(1.0)];
        let sheet = BottomSheet::new("Options").with_detents(&detents).with_grabber(true);
        ui.bottom_sheet("real-sheet", &mut open, &sheet, |ui, rect| {
            let t = *ui.theme();
            let x = rect.x + 16.0;
            let w = (rect.w - 32.0).max(0.0);
            let mut y = rect.y;
            for item in ["Share", "Duplicate", "Move to folder", "Delete"] {
                line(ui, item, x, y + 22.0, w, t.font_m, t.fg_primary);
                ui.painter().fill(Rect::new(x, y + 43.0, w, 1.0), 0.0, t.surface_separator);
                y += 44.0;
            }
            done = ui
                .button("done", Rect::new(x, y + 12.0, w, 44.0), "Done", ButtonKind::Primary)
                .clicked;
        });
        if done {
            open = false;
        }
        self.real_sheet = open;
    }
}

/// The physics card: a spring and a fling drawn as curves, a dot riding each, and a ball on a rail
/// that moves as the curve says.
fn physics(ui: &mut Ui<'_>, frame: Rect, env: &Env) {
    let t = *ui.theme();
    tile(ui, frame, 0.5);
    let inner = frame.inset(CARD_PAD, CARD_PAD);
    // A function of the clock, which stands at the still instant (phase 0.4) when nothing moves.
    let phase = (env.now.as_secs_f64() - 8.6).rem_euclid(CYCLE) as f32;
    let half = (inner.h - GAP) / 2.0;
    let spring_dur = SPRING.duration(0.0, SPRING_SPAN, 0.0).as_secs_f32();
    let fling_dur = FLING.duration(FLING_V0).as_secs_f32();
    let fling_total = FLING.offset(FLING_V0, Duration::from_secs_f32(fling_dur)).max(1.0);
    let spring_curve =
        |s: f32| SPRING.sample(0.0, SPRING_SPAN, 0.0, Duration::from_secs_f32(s)).0 / SPRING_SPAN;
    let fling_curve = |s: f32| FLING.offset(FLING_V0, Duration::from_secs_f32(s)) / fling_total;
    let plots: [(&str, String, f32, &dyn Fn(f32) -> f32, Rgba); 2] = [
        (
            "Spring",
            "response 0.45 s, damping 0.55".to_owned(),
            spring_dur,
            &spring_curve,
            t.data_hi,
        ),
        (
            "Fling",
            format!("speed {FLING_V0:.0} px/s, decay 0.998 per ms"),
            fling_dur,
            &fling_curve,
            t.ok,
        ),
    ];
    for (k, (name, params, dur, curve, colour)) in plots.iter().enumerate() {
        let area = Rect::new(inner.x, inner.y + k as f32 * (half + GAP), inner.w, half);
        caps(ui, name, area.x, area.y);
        let at = phase.min(*dur);
        let value = curve(at);
        let live = format!("{value:.2}");
        let mono = Spec::plain(live, Style::mono(t.font_s, t.font_s * 1.3, t.fg_secondary));
        let w = ui.painter().measure(&mono).0;
        ui.painter().text(&mono, area.right() - w, area.y);
        line(ui, params, area.x, area.y + 26.0, area.w, t.font_s, t.fg_secondary);
        // The plot, then the rail under it.
        let rail_h = 18.0;
        let plot = Rect::new(area.x, area.y + 40.0, area.w, (area.h - 40.0 - rail_h - 8.0).max(24.0));
        let ymax = 1.2;
        let to_px = |s: f32, v: f32| {
            [
                plot.x + (s / dur) * plot.w,
                plot.bottom() - (v / ymax).clamp(0.0, 1.0) * plot.h,
            ]
        };
        let n = 72;
        let pts: Vec<[f32; 2]> = (0..=n)
            .map(|i| {
                let s = dur * i as f32 / n as f32;
                to_px(s, curve(s))
            })
            .collect();
        let done = ((at / dur) * n as f32).round() as usize;
        let p = ui.painter();
        // The axes: the baseline and the target line.
        p.fill(Rect::new(plot.x, plot.bottom(), plot.w, 1.0), 0.0, t.surface_separator);
        let target = to_px(0.0, 1.0)[1];
        p.fill(Rect::new(plot.x, target, plot.w, 1.0), 0.0, with_alpha(t.fg_faint, 0.5));
        p.stroke(&pts, false, StrokeStyle::solid(with_alpha(*colour, 0.28), 2.0));
        if done >= 1 {
            p.stroke(&pts[..=done.min(n)], false, StrokeStyle::solid(*colour, 2.5));
        }
        let fade = (phase / 0.15).min(1.0).min(((CYCLE as f32 - phase) / 0.4).clamp(0.0, 1.0));
        let dot = to_px(at, value);
        p.fill(Rect::new(dot[0] - 9.0, dot[1] - 9.0, 18.0, 18.0), 9.0, with_alpha(*colour, 0.22 * fade));
        p.fill(Rect::new(dot[0] - 4.5, dot[1] - 4.5, 9.0, 9.0), 4.5, with_alpha(t.fg_bright, fade));
        // The rail: the ball goes where the value says.
        let rail = Rect::new(area.x, plot.bottom() + 12.0, area.w, rail_h);
        p.fill(Rect::new(rail.x + 6.0, rail.y + rail.h / 2.0 - 1.0, rail.w - 12.0, 2.0), 1.0, t.surface_active);
        let bx = rail.x + 9.0 + (rail.w - 18.0) / ymax * value.clamp(0.0, ymax);
        p.fill(Rect::new(bx - 8.0, rail.y + 1.0, 16.0, 16.0), 8.0, *colour);
    }
}
