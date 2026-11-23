//! The six scenes and what each one draws into the stage's body. Each scene draws from the rect it
//! is given and takes every colour from `ui.theme()`; the frame around it (header, glass, the push
//! between scenes) is the stage's.

use lune_ui::color::{clear, premul, with_alpha};
use lune_ui::paint::Rect;
use lune_ui::text::{Spec, Style};
use lune_ui::ui::Ui;

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
                "Toggles, sliders, steppers, ratings and tabs, all driven by themselves"
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

/// What the scenes keep between frames.
#[derive(Debug, Default)]
pub struct Scenes {}

impl Scenes {
    /// A fresh set of scenes.
    pub fn new() -> Self {
        Self::default()
    }

    /// Draws `scene` into `body` (logical px); `wide` is the frame's layout.
    pub fn draw(&mut self, scene: SceneId, ui: &mut Ui<'_>, body: Rect, wide: bool) {
        placeholder(scene, ui, body, wide);
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
