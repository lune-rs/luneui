//! The six scenes and what each one draws into the stage's body. Each scene draws from the rect it
//! is given and takes every colour from `ui.theme()`; the frame around it (header, glass, the push
//! between scenes) is the stage's.

use std::time::Duration;

use lune_ui::color::{Rgba, clear, premul, with_alpha};
use lune_ui::input::Event;
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;
use lune_ui::widgets::scroll_area::{Extent, ScrollOptions};

pub mod controls;
pub mod glass;
pub mod motion;

pub use controls::Controls;
pub use glass::Glass;
pub use motion::Motions;

/// Codicons (as mapped into the Nerd Font) the scenes and the frame use.
pub mod glyph {
    /// `cod-layers`.
    pub const LAYERS: &str = "\u{ebd2}";
    /// `cod-settings`.
    pub const SETTINGS: &str = "\u{eb52}";
    /// `cod-pulse`.
    pub const PULSE: &str = "\u{eb31}";
    /// `cod-graph`.
    pub const GRAPH: &str = "\u{eb03}";
    /// `cod-code`.
    pub const CODE: &str = "\u{eac4}";
    /// `cod-symbol-color`.
    pub const PALETTE: &str = "\u{eb5c}";
    /// Material Design `music-note` (in the Nerd Font).
    pub const MUSIC: &str = "\u{f0387}";
    /// `cod-play`.
    pub const PLAY: &str = "\u{eb2c}";
    /// `cod-debug-pause`.
    pub const PAUSE: &str = "\u{ead1}";
    /// `cod-chevron-left`.
    pub const PREV: &str = "\u{eab5}";
    /// `cod-chevron-right`.
    pub const NEXT: &str = "\u{eab6}";
}

/// One of the six scenes, in rail and tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneId {
    /// Floating frost cards over the moving background, and a dialog.
    Glass,
    /// The input controls, all driven.
    Controls,
    /// Pager, sheet, navigation stack and the physics behind them.
    Motion,
    /// Charts, a table, a board and counters.
    Data,
    /// A code view, a text field, markdown and selection.
    Text,
    /// Two themes and an accent sweep.
    Theme,
}

impl SceneId {
    /// Every scene, in order.
    pub const ALL: [SceneId; 6] = [
        SceneId::Glass,
        SceneId::Controls,
        SceneId::Motion,
        SceneId::Data,
        SceneId::Text,
        SceneId::Theme,
    ];

    /// Its place in [`SceneId::ALL`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// The scene at `i`, the last one for an index past the end.
    pub fn from_index(i: usize) -> SceneId {
        Self::ALL[i.min(Self::ALL.len() - 1)]
    }

    /// The name the rail, the tab bar and the title bar show.
    pub fn name(self) -> &'static str {
        match self {
            SceneId::Glass => "Glass",
            SceneId::Controls => "Controls",
            SceneId::Motion => "Motion",
            SceneId::Data => "Data",
            SceneId::Text => "Text",
            SceneId::Theme => "Theme",
        }
    }

    /// The lower-case word the command line and the page's query use.
    pub fn slug(self) -> &'static str {
        match self {
            SceneId::Glass => "glass",
            SceneId::Controls => "controls",
            SceneId::Motion => "motion",
            SceneId::Data => "data",
            SceneId::Text => "text",
            SceneId::Theme => "theme",
        }
    }

    /// The scene a `slug` names.
    pub fn from_slug(slug: &str) -> Option<SceneId> {
        Self::ALL.into_iter().find(|s| s.slug() == slug)
    }

    /// The scene's icon.
    pub fn glyph(self) -> &'static str {
        match self {
            SceneId::Glass => glyph::LAYERS,
            SceneId::Controls => glyph::SETTINGS,
            SceneId::Motion => glyph::PULSE,
            SceneId::Data => glyph::GRAPH,
            SceneId::Text => glyph::CODE,
            SceneId::Theme => glyph::PALETTE,
        }
    }

    /// One line on what the scene shows, under its title.
    pub fn subtitle(self) -> &'static str {
        match self {
            SceneId::Glass => {
                "Frost cards float over the moving background; a dialog blurs what is behind it"
            }
            SceneId::Controls => {
                "Toggles, sliders, steppers, ratings and radios, all driven by themselves"
            }
            SceneId::Motion => {
                "A pager, a bottom sheet and a navigation stack, with the springs behind them"
            }
            SceneId::Data => {
                "Charts that grow, a table that sorts, a board and counters that count"
            }
            SceneId::Text => "A code view, a field that is typed into, markdown and a selection",
            SceneId::Theme => {
                "Two themes and an accent that sweeps, applied to every widget at once"
            }
        }
    }
}

/// What a scene is told each frame besides the `Ui`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Env {
    /// The wide frame (a rail beside the stage) or the narrow one (a phone's).
    pub wide: bool,
    /// The scene's ambient clock: the app's time, or [`crate::stage::STILL_AT`] when nothing moves.
    pub now: Duration,
    /// Motion is on: the Full tier and not `--still`. Off it, a scene shows its state at rest and
    /// asks for no frame.
    pub animating: bool,
}

impl Env {
    /// The padding between the stage card's edge and the scene.
    pub fn pad(&self) -> f32 {
        if self.wide {
            crate::stage::PAD
        } else {
            crate::stage::NARROW_PAD
        }
    }
}

/// How long a scene's autopilot stays out of the way after a real input in it.
pub const RESUME: Duration = Duration::from_secs(5);

/// A scene's autopilot: a metronome that says when the next scripted change is due, and that stays
/// quiet for [`RESUME`] after a person touches the scene, so it never fights the hand.
#[derive(Debug, Default)]
pub struct Pilot {
    last_input: Option<Duration>,
    step: Option<u64>,
    /// How many scripted changes have run.
    pub acted: u64,
}

impl Pilot {
    /// Reads this frame's events: a pointer move or press, a touch or a wheel inside `area`, or any
    /// key or text, is a real input.
    pub fn observe(&mut self, ui: &Ui<'_>, area: Rect, env: &Env) {
        let hit = ui.input().events.iter().any(|e| match e {
            Event::PointerMoved { pos }
            | Event::PointerButton { pos, .. }
            | Event::Scroll { pos, .. } => area.contains(*pos),
            Event::Touch(t) => area.contains(t.pos),
            Event::Key { .. } | Event::Text(_) | Event::Ime(_) => true,
            _ => false,
        });
        if hit {
            self.last_input = Some(env.now);
        }
    }

    /// Whether a real input is still recent enough to hold the autopilot back.
    pub fn paused(&self, env: &Env) -> bool {
        self.last_input
            .is_some_and(|t| env.now.saturating_sub(t) < RESUME)
    }

    /// Whether the autopilot runs: motion is on and nobody has touched the scene lately.
    pub fn live(&self, env: &Env) -> bool {
        env.animating && !self.paused(env)
    }

    /// The index of the change that is due, once per `period` of the clock: `None` at the first
    /// frame (the scene starts at rest), while it is held back and while the clock stands still. A
    /// scene that was away, or held, takes only the latest step.
    pub fn tick(&mut self, env: &Env, period: Duration) -> Option<u64> {
        let n = (env.now.as_secs_f64() / period.as_secs_f64()) as u64;
        let first = self.step.is_none();
        let moved = self.step != Some(n);
        self.step = Some(n);
        if moved && !first && self.live(env) {
            self.acted += 1;
            Some(n)
        } else {
            None
        }
    }
}

/// The radius of a scene's cards.
pub(crate) const CARD_R: f32 = 12.0;
/// The padding inside a card.
pub(crate) const CARD_PAD: f32 = 16.0;
/// The gap between cards.
pub(crate) const GAP: f32 = 12.0;

/// A frost card over the stage: the theme's glass, a sheen from the top and the popup hairline, on a
/// soft shadow that grows with `lift` (0 for none, 1 for the nearest).
pub(crate) fn tile(ui: &mut Ui<'_>, r: Rect, lift: f32) {
    let t = *ui.theme();
    let p = ui.painter();
    if lift > 0.0 {
        let down = 3.0 + 9.0 * lift;
        p.shadow(
            Rect::new(r.x, r.y + down, r.w, r.h),
            CARD_R,
            6.0 + 12.0 * lift,
            with_alpha(t.dialog_backdrop, 0.30 + 0.30 * lift),
        );
    }
    p.frost(r, CARD_R, premul(with_alpha(t.menu_glass, 0.5)), true);
    p.linear(
        r,
        CARD_R,
        t.surface_hover,
        clear(t.surface_hover),
        std::f32::consts::FRAC_PI_2,
    );
    p.ring(r, CARD_R, 1.0, t.menu_popup_border, false);
}

/// A caption in capitals at `(x, y)` in the small size.
pub(crate) fn caps(ui: &mut Ui<'_>, text: &str, x: f32, y: f32) {
    let t = *ui.theme();
    let spec = Spec::plain(
        text.to_uppercase(),
        Style::ui(t.font_s, t.fg_tertiary).medium(),
    );
    ui.painter().text(&spec, x, y);
}

/// `text` in `colour`, its left edge at `x`, centred on `cy`, elided to `w`.
pub(crate) fn line(ui: &mut Ui<'_>, text: &str, x: f32, cy: f32, w: f32, size: f32, colour: Rgba) {
    let p = ui.painter();
    let spec = p.elide(&Spec::plain(text, Style::ui(size, colour)), w.max(0.0));
    let h = p.measure(&spec).1;
    p.text(&spec, x, cy - h / 2.0);
}

/// `text` wrapped to `width` and drawn with every line centred on `cx`, the first line's top at
/// `y`; returns the height drawn.
pub(crate) fn centred_lines(
    ui: &mut Ui<'_>,
    text: &str,
    cx: f32,
    y: f32,
    width: f32,
    size: f32,
    colour: Rgba,
) -> f32 {
    let p = ui.painter();
    let style = Style::ui(size, colour);
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let joined = match lines.last() {
            Some(l) => format!("{l} {word}"),
            None => word.to_owned(),
        };
        let fits = p.measure(&Spec::plain(joined.clone(), style)).0 <= width;
        match lines.last_mut() {
            Some(last) if fits => *last = joined,
            _ => lines.push(word.to_owned()),
        }
    }
    let mut at = y;
    for l in lines {
        let spec = Spec::plain(l, style);
        let (w, h) = p.measure(&spec);
        p.text(&spec, cx - w / 2.0, at);
        at += h;
    }
    at - y
}

/// Draws a scene's content in a scroll area over `body` widened by `pad` at the sides and the
/// bottom (so a shadow is not cut at the body's edge). `extent_h` is the content's height from the
/// body's top; the content is placed in the body's own coordinates.
pub(crate) fn scrolled(
    ui: &mut Ui<'_>,
    body: Rect,
    pad: f32,
    extent_h: f32,
    draw: impl FnOnce(&mut Ui<'_>),
) {
    let view = Rect::new(body.x - pad, body.y, body.w + 2.0 * pad, body.h + pad);
    let height = extent_h.max(body.h) + pad;
    ui.scroll_area("scroll", view, &ScrollOptions::default(), |ui, _| {
        draw(ui);
        ((), Extent::new(f64::from(view.w), f64::from(height)))
    });
}

/// The rects of a scene's controls as the last frame drew them, by name: what the tests tap and
/// what a tour can aim at. The rects are in the body's coordinates.
#[derive(Debug, Default, Clone)]
pub struct Targets(Vec<(String, Rect)>);

impl Targets {
    /// Forgets the last frame's rects.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Records that `name` was drawn at `rect`.
    pub fn note(&mut self, name: impl Into<String>, rect: Rect) {
        self.0.push((name.into(), rect));
    }

    /// The rect of `name`.
    pub fn get(&self, name: &str) -> Option<Rect> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, r)| *r)
    }

    /// The middle of `name`'s rect.
    pub fn centre(&self, name: &str) -> Option<(f32, f32)> {
        self.get(name).map(|r| (r.x + r.w / 2.0, r.y + r.h / 2.0))
    }

    /// Every name and rect, in the order they were drawn.
    pub fn all(&self) -> &[(String, Rect)] {
        &self.0
    }
}

/// What the scenes keep between frames.
#[derive(Debug, Default)]
pub struct Scenes {
    /// The Glass scene.
    pub glass: Glass,
    /// The Controls scene.
    pub controls: Controls,
    /// The Motion scene.
    pub motion: Motions,
}

impl Scenes {
    /// A fresh set of scenes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Draws `scene` into `body` (logical px).
    pub fn draw(&mut self, scene: SceneId, ui: &mut Ui<'_>, body: Rect, env: &Env) {
        match scene {
            SceneId::Glass => self.glass.draw(ui, body, env),
            SceneId::Controls => self.controls.draw(ui, body, env),
            SceneId::Motion => self.motion.draw(ui, body, env),
            _ => placeholder(scene, ui, body, env.wide),
        }
    }

    /// The controls `scene` drew in its last frame.
    pub fn targets(&self, scene: SceneId) -> Option<&Targets> {
        match scene {
            SceneId::Glass => Some(&self.glass.targets),
            SceneId::Controls => Some(&self.controls.targets),
            SceneId::Motion => Some(&self.motion.targets),
            _ => None,
        }
    }

    /// The autopilot of `scene`, for the scenes that have one.
    pub fn pilot(&self, scene: SceneId) -> Option<&Pilot> {
        match scene {
            SceneId::Glass => Some(&self.glass.pilot),
            SceneId::Controls => Some(&self.controls.pilot),
            SceneId::Motion => Some(&self.motion.pilot),
            _ => None,
        }
    }
}

/// A scene that is not built yet: its icon on a frosted tile with a hint, centred in `body`.
fn placeholder(scene: SceneId, ui: &mut Ui<'_>, body: Rect, wide: bool) {
    let t = *ui.theme();
    let tile_w = if wide { 320.0 } else { body.w.min(320.0) };
    let tile = Rect::new(
        body.x + (body.w - tile_w) / 2.0,
        body.y + (body.h - 168.0).max(0.0) / 2.0,
        tile_w,
        168.0_f32.min(body.h),
    );
    let p = ui.painter();
    p.frost(tile, 12.0, premul(with_alpha(t.menu_glass, 0.5)), true);
    p.linear(
        tile,
        12.0,
        t.surface_hover,
        clear(t.surface_hover),
        std::f32::consts::FRAC_PI_2,
    );
    p.ring(tile, 12.0, 1.0, t.menu_popup_border, false);
    let icon = lune_ui::icon::glyph(scene.glyph(), 44.0, t.data_hi);
    let iw = p.measure(&icon).0;
    p.text_centered(
        &icon,
        Rect::new(tile.x + (tile.w - iw) / 2.0, tile.y + 24.0, iw, 56.0),
    );
    let name = Spec::plain(scene.name(), Style::ui(t.font_title, t.fg_bright).medium());
    let nw = p.measure(&name).0;
    p.text(&name, tile.x + (tile.w - nw) / 2.0, tile.y + 92.0);
    let hint = p.elide(
        &Spec::plain(
            "This scene is being built",
            Style::ui(t.font_base, t.fg_secondary),
        ),
        tile.w - 32.0,
    );
    let hw = p.measure(&hint).0;
    p.text(&hint, tile.x + (tile.w - hw) / 2.0, tile.y + 124.0);
}
