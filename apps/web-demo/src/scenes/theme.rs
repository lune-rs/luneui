//! Theme: two presets (`lunegit_dark`, `near_black`) and an accent drawn from the theme's own
//! colours, with a preview of sample controls, an inbox and the palette that shows the result. The
//! scene only chooses a [`Look`]; the stage owns the [`Fx`] that moves the app's own `Theme` copy
//! to it (colours only, `moderate` and sine-in-out, `fast` under Reduced, a cut under Off) and every
//! widget of every scene, the rail, the bars and the background read that copy each frame.
//!
//! The autopilot changes one thing about every 3 s, alternating the preset and the accent, and a
//! person's hand holds it back for [`RESUME`](super::RESUME).

use std::time::Duration;

use lune_ui::color::{Rgba, mix, with_alpha, with_alpha_byte};
use lune_ui::input::NamedKey;
use lune_ui::motion::Easing;
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::theme::{Theme, ThemeGlass};
use lune_ui::ui::{RoveAxis, Sense, Ui, WidgetId};
use lune_ui::widgets::badge;
use lune_ui::widgets::button::ButtonKind;
use lune_ui::widgets::field::FieldState;
use lune_ui::widgets::nav_item::{self, NavLook};
use lune_ui::widgets::progress::ProgressOptions;
use lune_ui::widgets::radio_group::RadioOption;
use lune_ui::widgets::slider::SliderOptions;
use lune_ui::widgets::tone::Tone;
use lune_ui::{GlassLayer, LuminosityCurve, lunegit_dark, near_black};

use super::{CARD_PAD, Env, GAP, Pilot, Targets, caps, line, scrolled, tile};

/// The autopilot's step: the preset and the accent change one after the other.
pub const STEP: Duration = Duration::from_millis(2800);

// ---------------------------------------------------------------------------------------------
// The looks.

/// One of the two presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// Lunegit's dark theme.
    Dark,
    /// The same colours on near-black glass.
    NearBlack,
}

impl Preset {
    /// Both, in the order of the radio group.
    pub const ALL: [Preset; 2] = [Preset::Dark, Preset::NearBlack];

    /// The preset a command-line or query word names (`dark`, `near-black`).
    pub fn from_word(word: &str) -> Option<Preset> {
        match word {
            "dark" => Some(Preset::Dark),
            "near-black" => Some(Preset::NearBlack),
            _ => None,
        }
    }

    /// The name the radio group shows.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Dark => "Lunegit dark",
            Preset::NearBlack => "Near black",
        }
    }

    /// The preset's theme.
    pub fn base(self) -> Theme {
        match self {
            Preset::Dark => lunegit_dark::THEME,
            Preset::NearBlack => near_black::THEME,
        }
    }

    /// How deep the glass is, 0 for the dark preset to 1 for near-black, read from the panel
    /// layer's material opacity (the only number the two presets' glass layers differ in that the
    /// app's frost reads): so it moves smoothly with the theme the stage lerps.
    pub fn depth(t: &Theme) -> f32 {
        let from = lunegit_dark::PANEL.material_opacity;
        let to = near_black::MATERIAL_OPACITY;
        ((from - t.glass.panel.material_opacity) / (from - to)).clamp(0.0, 1.0)
    }
}

/// The accent: one of the theme's own colours (the preset's `accent` for [`Accent::Own`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Accent {
    /// The preset's own `accent` (a grey).
    Own,
    /// The theme's `data` blue.
    Data,
    /// The theme's `ok` green.
    Ok,
    /// The theme's `gold`.
    Gold,
    /// The theme's `err` rose.
    Err,
    /// The theme's `link` sky.
    Link,
}

impl Accent {
    /// Every accent, in the order of the swatches.
    pub const ALL: [Accent; 6] = [
        Accent::Own,
        Accent::Data,
        Accent::Ok,
        Accent::Gold,
        Accent::Err,
        Accent::Link,
    ];

    /// The accent a command-line or query word names (a swatch's name, in any case).
    pub fn from_word(word: &str) -> Option<Accent> {
        Accent::ALL
            .into_iter()
            .find(|a| a.name().eq_ignore_ascii_case(word))
    }

    /// The swatch's name.
    pub fn name(self) -> &'static str {
        match self {
            Accent::Own => "Own",
            Accent::Data => "Blue",
            Accent::Ok => "Green",
            Accent::Gold => "Gold",
            Accent::Err => "Rose",
            Accent::Link => "Sky",
        }
    }

    /// The colour of this accent in `base`.
    pub fn colour(self, base: &Theme) -> Rgba {
        match self {
            Accent::Own => base.accent,
            Accent::Data => base.data,
            Accent::Ok => base.ok,
            Accent::Gold => base.gold,
            Accent::Err => base.err,
            Accent::Link => base.link,
        }
    }
}

/// What the Theme scene chooses: a preset and an accent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Look {
    /// The preset.
    pub preset: Preset,
    /// The accent.
    pub accent: Accent,
}

impl Look {
    /// What the demo starts on: the dark preset and its own accent.
    pub const DEFAULT: Look = Look {
        preset: Preset::Dark,
        accent: Accent::Own,
    };

    /// The theme this look is: the preset with the accent's colour in every field that follows the
    /// accent (the colour, the scrollbar thumb, the selection) and the label colour that reads on
    /// it. The preset's own accent leaves the preset exactly as it is.
    pub fn theme(self) -> Theme {
        let mut t = self.preset.base();
        if self.accent != Accent::Own {
            let c = self.accent.colour(&t);
            t.accent = c;
            t.scroll_thumb = with_alpha_byte(c, t.scroll_thumb[3]);
            t.scroll_thumb_hover = with_alpha_byte(c, t.scroll_thumb_hover[3]);
            t.text_selection = with_alpha_byte(c, t.text_selection[3]);
            t.on_accent = readable_on(c, t.on_accent, t.toast_log_bg);
        }
        t
    }
}

/// The relative luminance of an opaque colour (WCAG 2).
pub fn luminance(c: Rgba) -> f32 {
    let lin = |v: u8| {
        let s = f32::from(v) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// The contrast ratio of two opaque colours (WCAG 2), 1 to 21.
pub fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Whichever of `light` and `dark` reads better on `fill`.
fn readable_on(fill: Rgba, light: Rgba, dark: Rgba) -> Rgba {
    if contrast(light, fill) >= contrast(dark, fill) {
        light
    } else {
        dark
    }
}

/// How far `accent` is from a grey, 0 for a grey and 1 for a colour: what the logo mark and the
/// background lean on, so the preset's own grey accent leaves them as they were.
pub fn accent_share(accent: Rgba) -> f32 {
    let hi = accent[0].max(accent[1]).max(accent[2]);
    let lo = accent[0].min(accent[1]).min(accent[2]);
    ((f32::from(hi - lo) / 255.0 - 0.08) / 0.12).clamp(0.0, 1.0)
}

/// `a` towards `b` by `t`, colours only: every colour (the glass layers' tints and opacities too)
/// is mixed (the glass layers' tints, opacities and luminosity curves too), and the sizes, the radii, the type scale and the motion tokens are `b`'s at once. Exact
/// at 0 and 1. The literal names every field, so a field added to `Theme` stops this compiling.
pub fn lerp_theme(a: &Theme, b: &Theme, t: f32) -> Theme {
    if t >= 1.0 {
        return *b;
    }
    let m = |x: Rgba, y: Rgba| mix(x, y, t);
    let f = |x: f32, y: f32| x + (y - x) * t;
    let d = |x: f64, y: f64| x + (y - x) * f64::from(t);
    let layer = |x: &GlassLayer, y: &GlassLayer| GlassLayer {
        tint: m(x.tint, y.tint),
        tint_opacity: f(x.tint_opacity, y.tint_opacity),
        material_opacity: f(x.material_opacity, y.material_opacity),
        curve: LuminosityCurve {
            mid_point: d(x.curve.mid_point, y.curve.mid_point),
            white_max_opacity: d(x.curve.white_max_opacity, y.curve.white_max_opacity),
            mid_point_max_opacity: d(x.curve.mid_point_max_opacity, y.curve.mid_point_max_opacity),
            black_max_opacity: d(x.curve.black_max_opacity, y.curve.black_max_opacity),
        },
    };
    Theme {
        fg_primary: m(a.fg_primary, b.fg_primary),
        fg_secondary: m(a.fg_secondary, b.fg_secondary),
        fg_tertiary: m(a.fg_tertiary, b.fg_tertiary),
        surface_input: m(a.surface_input, b.surface_input),
        surface_hover: m(a.surface_hover, b.surface_hover),
        surface_selected: m(a.surface_selected, b.surface_selected),
        border: m(a.border, b.border),
        border_subtle: m(a.border_subtle, b.border_subtle),
        tree_guide: m(a.tree_guide, b.tree_guide),
        accent: m(a.accent, b.accent),
        scroll_thumb: m(a.scroll_thumb, b.scroll_thumb),
        scroll_thumb_hover: m(a.scroll_thumb_hover, b.scroll_thumb_hover),
        text_selection: m(a.text_selection, b.text_selection),
        on_accent: m(a.on_accent, b.on_accent),
        dialog_backdrop: m(a.dialog_backdrop, b.dialog_backdrop),
        dialog_footer: m(a.dialog_footer, b.dialog_footer),
        menu_text_primary: m(a.menu_text_primary, b.menu_text_primary),
        menu_text_tertiary: m(a.menu_text_tertiary, b.menu_text_tertiary),
        menu_text_quaternary: m(a.menu_text_quaternary, b.menu_text_quaternary),
        menu_popup_border: m(a.menu_popup_border, b.menu_popup_border),
        menu_item_hover: m(a.menu_item_hover, b.menu_item_hover),
        menu_separator: m(a.menu_separator, b.menu_separator),
        menu_glass: m(a.menu_glass, b.menu_glass),
        menu_item_pressed: m(a.menu_item_pressed, b.menu_item_pressed),
        menu_text_disabled: m(a.menu_text_disabled, b.menu_text_disabled),
        toast_info: m(a.toast_info, b.toast_info),
        toast_success: m(a.toast_success, b.toast_success),
        toast_warning: m(a.toast_warning, b.toast_warning),
        toast_error: m(a.toast_error, b.toast_error),
        toast_log_bg: m(a.toast_log_bg, b.toast_log_bg),
        toast_log_fg: m(a.toast_log_fg, b.toast_log_fg),
        dirty_indicator: m(a.dirty_indicator, b.dirty_indicator),
        count_badge: m(a.count_badge, b.count_badge),
        caption_close_hover: m(a.caption_close_hover, b.caption_close_hover),
        caption_close_fg: m(a.caption_close_fg, b.caption_close_fg),
        fg_muted: m(a.fg_muted, b.fg_muted),
        fg_subtle: m(a.fg_subtle, b.fg_subtle),
        fg_bright: m(a.fg_bright, b.fg_bright),
        fg_quaternary: m(a.fg_quaternary, b.fg_quaternary),
        fg_dim: m(a.fg_dim, b.fg_dim),
        fg_faint: m(a.fg_faint, b.fg_faint),
        surface_overlay: m(a.surface_overlay, b.surface_overlay),
        surface_active: m(a.surface_active, b.surface_active),
        surface_separator: m(a.surface_separator, b.surface_separator),
        surface_divider: m(a.surface_divider, b.surface_divider),
        ok: m(a.ok, b.ok),
        warn: m(a.warn, b.warn),
        err: m(a.err, b.err),
        status_warning: m(a.status_warning, b.status_warning),
        status_warning_light: m(a.status_warning_light, b.status_warning_light),
        status_error_light: m(a.status_error_light, b.status_error_light),
        status_error: m(a.status_error, b.status_error),
        status_warning_dark: m(a.status_warning_dark, b.status_warning_dark),
        data: m(a.data, b.data),
        data_hi: m(a.data_hi, b.data_hi),
        gold: m(a.gold, b.gold),
        paused: m(a.paused, b.paused),
        link: m(a.link, b.link),
        code_inline: m(a.code_inline, b.code_inline),
        diff_added: m(a.diff_added, b.diff_added),
        diff_removed: m(a.diff_removed, b.diff_removed),
        diff_added_mark: m(a.diff_added_mark, b.diff_added_mark),
        diff_removed_mark: m(a.diff_removed_mark, b.diff_removed_mark),
        glass: ThemeGlass {
            window: layer(&a.glass.window, &b.glass.window),
            panel: layer(&a.glass.panel, &b.glass.panel),
            card: layer(&a.glass.card, &b.glass.card),
        },
        font_s: b.font_s,
        font_base: b.font_base,
        font_m: b.font_m,
        font_title: b.font_title,
        line_title: b.line_title,
        item_radius: b.item_radius,
        selection_radius: b.selection_radius,
        selection_pad: b.selection_pad,
        motion: b.motion,
    }
}

/// The run from one look to another: the app's own `Theme` copy, moved a frame at a time.
#[derive(Debug, Clone)]
pub struct Fx {
    look: Look,
    from: Theme,
    to: Theme,
    start: Duration,
    run: Option<(Duration, Easing)>,
    settled: bool,
}

impl Fx {
    /// A run that rests on `look`.
    pub fn new(look: Look) -> Self {
        let to = look.theme();
        Self {
            look,
            from: to,
            to,
            start: Duration::ZERO,
            run: None,
            settled: true,
        }
    }

    /// The look the run goes to.
    pub fn look(&self) -> Look {
        self.look
    }

    /// Whether the run has arrived: the theme is exactly the look's.
    pub fn settled(&self) -> bool {
        self.settled
    }

    /// Rests on `look` at once.
    pub fn snap(&mut self, look: Look) {
        *self = Self::new(look);
    }

    /// Goes to `look` from what is shown at `now`, over `run` (its duration and easing; `None` is
    /// a cut).
    pub fn retarget(&mut self, look: Look, now: Duration, run: Option<(Duration, Easing)>) {
        let shown = self.at(now);
        self.look = look;
        self.to = look.theme();
        match run {
            Some(r) if !r.0.is_zero() => {
                self.from = shown;
                self.start = now;
                self.run = Some(r);
                self.settled = false;
            }
            _ => {
                self.from = self.to;
                self.run = None;
                self.settled = true;
            }
        }
    }

    /// The theme at app time `now`: exactly the look's once the run is over.
    pub fn at(&mut self, now: Duration) -> Theme {
        if self.settled {
            return self.to;
        }
        let Some((duration, easing)) = self.run else {
            self.settled = true;
            return self.to;
        };
        let p = now.saturating_sub(self.start).as_secs_f32() / duration.as_secs_f32();
        if p >= 1.0 {
            self.settled = true;
            return self.to;
        }
        lerp_theme(&self.from, &self.to, easing.eval(p))
    }
}

// ---------------------------------------------------------------------------------------------
// The scene.

/// One of the cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Card {
    /// The preset and the accent.
    Look,
    /// The palette that the theme is made of, live.
    Palette,
    /// Sample controls.
    Controls,
    /// A small inbox.
    Inbox,
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
fn min_h(card: Card, narrow: bool) -> f32 {
    match card {
        Card::Look => 270.0,
        Card::Palette => {
            if narrow {
                196.0
            } else {
                236.0
            }
        }
        Card::Controls => 300.0,
        Card::Inbox => 268.0,
    }
}

/// The widest the left column of the wide frame gets.
const LEFT_W: f32 = 340.0;

/// Lays the cards out: two columns (the look and the palette, the controls and the inbox) with
/// room, one stacked column (which scrolls) without. Spare height stretches each column's cards by
/// at most 40 %.
pub fn plan(body: Rect, wide: bool) -> Plan {
    let two = wide && body.w >= 760.0;
    let stack = |cards: &[Card], x: f32, w: f32, top: f32, fit: Option<f32>| {
        let mins: Vec<f32> = cards.iter().map(|c| min_h(*c, !two)).collect();
        let gaps = GAP * (cards.len() as f32 - 1.0);
        let need: f32 = mins.iter().sum::<f32>() + gaps;
        let k = fit.map_or(1.0, |h| ((h - gaps) / (need - gaps)).clamp(1.0, 1.5));
        let mut y = top;
        let mut out = Vec::new();
        for (c, h) in cards.iter().zip(mins) {
            out.push((*c, Rect::new(x, y, w, h * k)));
            y += h * k + GAP;
        }
        (out, y - GAP - top)
    };
    if two {
        let right_w = body.w - LEFT_W - GAP;
        let (mut cards, lh) = stack(
            &[Card::Look, Card::Palette],
            body.x,
            LEFT_W,
            body.y,
            Some(body.h),
        );
        let (right, rh) = stack(
            &[Card::Controls, Card::Inbox],
            body.x + LEFT_W + GAP,
            right_w,
            body.y,
            Some(body.h),
        );
        cards.extend(right);
        Plan {
            cards,
            height: lh.max(rh),
        }
    } else {
        let (cards, h) = stack(
            &[Card::Look, Card::Controls, Card::Inbox, Card::Palette],
            body.x,
            body.w,
            body.y,
            None,
        );
        Plan { cards, height: h }
    }
}

const NAMES: [(&str, &str); 4] = [
    ("Mara Okafor", "Build 482 is green on every target"),
    ("Jun Park", "Review: glass tint in the dialog card"),
    ("Sofia Reyes", "Release notes for 0.7.7, final pass"),
    ("Tomas Berg", "Frame times on the old laptop"),
];
const TAGS: [(&str, Tone); 4] = [
    ("Passed", Tone::Ok),
    ("Review", Tone::Warn),
    ("Draft", Tone::Neutral),
    ("Blocked", Tone::Err),
];

/// The Theme scene's state.
#[derive(Debug)]
pub struct ThemeScene {
    /// The autopilot.
    pub pilot: Pilot,
    /// The look the scene asks the stage for.
    pub look: Look,
    /// The preview's toggle.
    pub toggle: bool,
    /// The preview's slider.
    pub slider: f32,
    /// The preview's checkboxes.
    pub checks: [bool; 2],
    /// The preview's field.
    pub field: String,
    /// The inbox row that is selected.
    pub selected: usize,
    /// The controls of the last frame.
    pub targets: Targets,
}

impl Default for ThemeScene {
    fn default() -> Self {
        Self {
            pilot: Pilot::default(),
            look: Look::DEFAULT,
            toggle: true,
            slider: 0.62,
            checks: [true, false],
            field: "luneui".to_owned(),
            selected: 1,
            targets: Targets::default(),
        }
    }
}

fn level(on: bool) -> f32 {
    if on { 1.0 } else { 0.0 }
}

impl ThemeScene {
    /// What the autopilot does at step `n`: the preset on an even step, the accent on an odd one.
    fn act(&mut self, n: u64) {
        if n % 2 == 0 {
            self.look.preset = match self.look.preset {
                Preset::Dark => Preset::NearBlack,
                Preset::NearBlack => Preset::Dark,
            };
        } else {
            let at = Accent::ALL
                .iter()
                .position(|a| *a == self.look.accent)
                .unwrap_or(0);
            self.look.accent = Accent::ALL[(at + 1) % Accent::ALL.len()];
        }
    }

    /// Draws the scene into `body`.
    pub fn draw(&mut self, ui: &mut Ui<'_>, body: Rect, env: &Env) {
        self.pilot.observe(ui, body.inset(-env.pad(), 0.0), env);
        if let Some(n) = self.pilot.tick(env, STEP) {
            self.act(n);
        }
        let plan = plan(body, env.wide);
        self.targets.clear();
        scrolled(ui, body, env.pad(), plan.height, |ui| {
            for (_, r) in &plan.cards {
                tile(ui, *r, 0.0);
            }
            for (card, r) in &plan.cards {
                let inner = r.inset(CARD_PAD, CARD_PAD);
                match card {
                    Card::Look => self.look_card(ui, inner),
                    Card::Palette => self.palette_card(ui, inner),
                    Card::Controls => self.controls_card(ui, inner),
                    Card::Inbox => self.inbox_card(ui, inner),
                }
            }
        });
    }

    fn look_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        // What the card has over its content goes between the groups.
        let spare = (inner.h - 232.0).max(0.0);
        caps(ui, "Preset", inner.x, inner.y);
        let rows = Rect::new(inner.x, inner.y + 26.0 + spare * 0.15, inner.w, 56.0);
        let options: Vec<RadioOption<'_>> = Preset::ALL
            .iter()
            .enumerate()
            .map(|(i, p)| RadioOption {
                key: i as u64,
                label: p.name(),
                enabled: true,
            })
            .collect();
        let mut chosen = Preset::ALL
            .iter()
            .position(|p| *p == self.look.preset)
            .unwrap_or(0) as u64;
        for (i, _) in Preset::ALL.iter().enumerate() {
            self.targets.note(
                if i == 0 { "preset-dark" } else { "preset-near" },
                Rect::new(rows.x, rows.y + 28.0 * i as f32, rows.w, 28.0),
            );
        }
        ui.radio_group("preset", rows, &options, &mut chosen, "Preset");
        self.look.preset = Preset::ALL[chosen as usize % Preset::ALL.len()];

        let top = rows.bottom() + 22.0 + spare * 0.3;
        caps(ui, "Accent", inner.x, top);
        // The swatches: six discs on one roving stop; the colour of each is the accent in the
        // preset the scene has chosen.
        let base = self.look.preset.base();
        let n = Accent::ALL.len();
        let pitch = inner.w / n as f32;
        let disc = 30.0_f32.min(pitch - 8.0);
        let cells: Vec<Rect> = (0..n)
            .map(|i| Rect::new(inner.x + pitch * i as f32, top + 24.0, pitch, disc + 26.0))
            .collect();
        let id = ui.id("accent");
        let items: Vec<(WidgetId, bool)> = (0..n).map(|i| (id.with(i), true)).collect();
        let active = Accent::ALL
            .iter()
            .position(|a| *a == self.look.accent)
            .unwrap_or(0);
        let stop = ui.rove_stop(&items, Some(active));
        for (i, cell) in cells.iter().enumerate() {
            let accent = Accent::ALL[i];
            let sense = if stop == Some(i) {
                Sense::CLICK_FOCUS
            } else {
                Sense::CLICK_ROVE
            };
            let kid = items[i].0;
            let r = ui.interact(kid, *cell, sense);
            let enter = ui.consume_key(kid, NamedKey::Enter);
            let space = ui.consume_key(kid, NamedKey::Space);
            if r.clicked || enter || space {
                self.look.accent = accent;
            }
            self.targets.note(format!("accent{i}"), *cell);
            let here = ui.animate_f32(
                kid.with("here"),
                level(self.look.accent == accent),
                &nav_item::HOVER,
            );
            let hov = ui.animate_f32(kid.with("hov"), level(r.hovered), &nav_item::HOVER);
            let focus = ui.animate_f32(kid.with("focus"), level(r.focus_visible), &nav_item::FOCUS);
            let colour = accent.colour(&base);
            let c = Rect::new(cell.x + (cell.w - disc) / 2.0, cell.y, disc, disc);
            let p = ui.painter();
            let grow = c.inset(-2.0 * hov, -2.0 * hov);
            p.fill(grow, grow.w / 2.0, colour);
            // The ring round the chosen one fades in; the keyboard focus draws its own.
            let ring = c.inset(-5.0, -5.0);
            p.ring(
                ring,
                ring.w / 2.0,
                2.0,
                with_alpha(t.fg_bright, here),
                false,
            );
            if focus > 0.0 {
                let f = c.inset(-8.0, -8.0);
                p.ring(f, f.w / 2.0, 1.0, with_alpha(t.accent, focus), false);
            }
            let name = Spec::plain(
                accent.name(),
                Style::ui(t.font_s, with_alpha(t.fg_secondary, 0.65 + 0.35 * here)),
            );
            let w = p.measure(&name).0;
            p.text(&name, cell.x + (cell.w - w) / 2.0, cell.y + disc + 8.0);
            if r.hovered {
                ui.set_cursor(lune_ui::input::CursorIcon::Pointer);
            }
        }
        ui.rove_keys(&items, RoveAxis::Horizontal, false);

        // A line on what the colour fields do, in the theme's own words.
        let y = top + 24.0 + disc + 26.0 + 14.0 + spare * 0.15;
        let hint = "Every widget reads the theme each frame, so the change reaches all six scenes, the rail and the light behind them.";
        let p = ui.painter();
        let spec = Spec::plain(hint, Style::ui(t.font_s, t.fg_tertiary)).wrapped(inner.w);
        if y + 30.0 <= inner.bottom() + 1.0 {
            p.text(&spec, inner.x, y);
        }
    }

    fn palette_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        caps(ui, "Palette", inner.x, inner.y);
        let swatches: [(&str, Rgba); 6] = [
            ("accent", t.accent),
            ("data", t.data),
            ("ok", t.ok),
            ("gold", t.gold),
            ("err", t.err),
            ("link", t.link),
        ];
        let top = inner.y + 26.0;
        let cols = if inner.w >= 300.0 { 2 } else { 3 };
        let rows = swatches.len().div_ceil(cols);
        let gap = 8.0;
        let cw = (inner.w - gap * (cols as f32 - 1.0)) / cols as f32;
        let ch = ((inner.bottom() - top - gap * (rows as f32 - 1.0)) / rows as f32).min(84.0);
        for (i, (name, colour)) in swatches.iter().enumerate() {
            let (c, r) = (i % cols, i / cols);
            let tile = Rect::new(
                inner.x + c as f32 * (cw + gap),
                top + r as f32 * (ch + gap),
                cw,
                ch,
            );
            let p = ui.painter();
            p.fill(tile, 8.0, *colour);
            let on = if luminance(*colour) > 0.4 {
                t.toast_log_bg
            } else {
                t.on_accent
            };
            p.text_in(
                &Spec::plain(*name, Style::ui(t.font_base, on).medium()),
                Rect::new(tile.x + 12.0, tile.y, tile.w - 24.0, tile.h),
                0.0,
            );
        }
    }

    fn controls_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        caps(ui, "Preview", inner.x, inner.y);
        let two = inner.w >= 520.0;
        let gap = 24.0;
        let colw = if two { (inner.w - gap) / 2.0 } else { inner.w };
        let left = Rect::new(inner.x, inner.y + 26.0, colw, inner.h - 26.0);
        let right = Rect::new(inner.x + colw + gap, left.y, colw, left.h);
        // Left: a toggle, a slider and two checkboxes in four slots.
        let slot = |r: Rect, n: usize, i: usize| {
            let pitch = r.h / n as f32;
            Rect::new(r.x, r.y + pitch * i as f32, r.w, pitch)
        };
        let s0 = slot(left, if two { 4 } else { 7 }, 0);
        let mid = s0.y + s0.h / 2.0;
        let sw = Rect::new(left.right() - 40.0, mid - 12.0, 40.0, 24.0);
        line(
            ui,
            "Notifications",
            left.x,
            mid,
            left.w - 52.0,
            t.font_m,
            t.fg_primary,
        );
        ui.toggle("toggle", sw, &mut self.toggle, "Notifications");
        self.targets.note("toggle", sw);
        let s1 = slot(left, if two { 4 } else { 7 }, 1);
        let sl = Rect::new(left.x, s1.y + s1.h / 2.0 + 2.0, left.w, 24.0);
        line(
            ui,
            "Volume",
            left.x,
            sl.y - 10.0,
            left.w - 50.0,
            t.font_m,
            t.fg_primary,
        );
        let pct = format!("{}%", (self.slider * 100.0).round() as u32);
        let pw = ui
            .painter()
            .measure(&Spec::plain(&pct, Style::ui(t.font_base, t.fg_secondary)))
            .0;
        line(
            ui,
            &pct,
            left.right() - pw,
            sl.y - 10.0,
            pw + 2.0,
            t.font_base,
            t.fg_secondary,
        );
        let opts = SliderOptions {
            min: 0.0,
            max: 1.0,
            step: 0.0,
            name: "Volume",
        };
        ui.slider("slider", sl, &mut self.slider, &opts);
        self.targets.note("slider", sl);
        for (i, label) in ["Show previews", "Sync on Wi-Fi only"].iter().enumerate() {
            let s = slot(left, if two { 4 } else { 7 }, 2 + i);
            let r = Rect::new(left.x, s.y + (s.h - 24.0) / 2.0, left.w, 24.0);
            ui.checkbox(("check", i), r, &mut self.checks[i], label);
            self.targets
                .note(format!("check{i}"), Rect::new(r.x, r.y, 24.0, r.h));
        }
        // Right: a field, a progress bar, the badges and the button pair.
        let n = if two { 4 } else { 7 };
        let base = if two { 0 } else { 4 };
        let s = slot(right, n, base);
        let fr = Rect::new(right.x, s.y + s.h / 2.0 - 8.0, right.w, 32.0);
        line(
            ui,
            "Name",
            right.x,
            fr.y - 12.0,
            right.w,
            t.font_m,
            t.fg_primary,
        );
        ui.text_field(
            "field",
            fr,
            &mut self.field,
            "Search",
            FieldState::default(),
        );
        self.targets.note("field", fr);
        let s = slot(right, n, base + 1);
        let fill = self.slider;
        let used = format!("{}%", (fill * 100.0).round() as u32);
        let pr = ProgressOptions {
            value: Some(fill),
            tone: Tone::Neutral,
            head: Some(("Upload", &used)),
            name: "Upload",
        };
        ui.progress(
            "bar",
            Rect::new(right.x, s.y + s.h / 2.0 - 14.0, right.w, 0.0),
            &pr,
        );
        let s = slot(right, n, base + 2);
        let mut x = right.x;
        for (i, (text, tone)) in TAGS.iter().enumerate() {
            let w = badge::width(ui.painter(), text, true);
            if x + w > right.right() {
                break;
            }
            ui.badge(
                ("badge", i),
                (x, s.y + (s.h - 20.0) / 2.0),
                text,
                *tone,
                true,
            );
            x += w + 8.0;
        }
        let s = slot(right, n, base + 3);
        let bw = (right.w - 8.0) / 2.0;
        let by = s.y + (s.h - 32.0) / 2.0;
        let b0 = Rect::new(right.x, by, bw, 32.0);
        let b1 = Rect::new(right.x + bw + 8.0, by, bw, 32.0);
        ui.button("cancel", b0, "Cancel", ButtonKind::Secondary);
        let primary = ui.button("save", b1, "Save", ButtonKind::Primary);
        if primary.clicked {
            self.slider = (self.slider + 0.2).fract();
        }
        self.targets.note("primary", b1);
    }

    fn inbox_card(&mut self, ui: &mut Ui<'_>, inner: Rect) {
        let t = *ui.theme();
        caps(ui, "Inbox", inner.x, inner.y);
        let top = inner.y + 26.0;
        let rows = NAMES.len();
        let pitch = ((inner.bottom() - top) / rows as f32).min(72.0);
        let hues = [t.accent, t.data, t.ok, t.gold];
        for (i, (name, subject)) in NAMES.iter().enumerate() {
            let r = Rect::new(
                inner.x - 6.0,
                top + pitch * i as f32,
                inner.w + 12.0,
                pitch - 4.0,
            );
            let id = ui.id(("mail", i));
            let resp = ui.interact(id, r, Sense::CLICK_FOCUS);
            if resp.clicked {
                self.selected = i;
            }
            self.targets.note(format!("mail{i}"), r);
            let sel = ui.animate_f32(id.with("sel"), level(self.selected == i), &nav_item::HOVER);
            let hov = ui.animate_f32(id.with("hov"), level(resp.hovered), &nav_item::HOVER);
            let focus = ui.animate_f32(
                id.with("focus"),
                level(resp.focus_visible),
                &nav_item::FOCUS,
            );
            let look = NavLook::mixed(&t, sel, hov, 0.0, focus);
            let p = ui.painter();
            p.fill(r, 8.0, look.fill);
            let disc = Rect::new(inner.x + 4.0, r.y + (r.h - 32.0) / 2.0, 32.0, 32.0);
            p.fill(disc, 16.0, with_alpha(hues[i], 0.9));
            let initial: String = name.chars().take(1).collect();
            let on = if luminance(hues[i]) > 0.4 {
                t.toast_log_bg
            } else {
                t.on_accent
            };
            p.text_centered(
                &Spec::plain(initial, Style::ui(t.font_base, on).medium()),
                disc,
            );
            let (tag, tone) = TAGS[i];
            let bw = badge::width(p, tag, false);
            let tx = disc.right() + 12.0;
            let w = (inner.right() - bw - 8.0 - tx).max(0.0);
            let name_c = lune_ui::color::mix(t.fg_primary, t.fg_bright, sel);
            line(ui, name, tx, r.y + r.h / 2.0 - 9.0, w, t.font_base, name_c);
            line(
                ui,
                subject,
                tx,
                r.y + r.h / 2.0 + 9.0,
                w,
                t.font_s,
                t.fg_secondary,
            );
            ui.badge(
                ("tag", i),
                (inner.right() - bw, r.y + (r.h - 20.0) / 2.0),
                tag,
                tone,
                false,
            );
            if resp.focus_visible {
                lune_ui::widgets::focus_ring::paint(
                    ui.painter(),
                    r,
                    lune_ui::widgets::focus_ring::RingKind::Offset { radius: 8.0 },
                    focus,
                );
            }
        }
    }
}
