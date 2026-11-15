//! The `gradients` page: the painter's per-pixel linear and radial gradients and its analytic soft
//! shadow (D323 to D325) over the whole window. The wash is a linear ramp, two radial lights and a
//! card with a coloured shadow and a glow. Each part has a scene of its own; the knob picks one
//! part, or all of them.

use std::f32::consts::FRAC_PI_2;

use lune_ui::motion::MotionTier;
use lune_ui::paint::{Falloff, Rect};
use lune_ui::ui::Ui;

use crate::page::Stage;
use crate::page::{Ev, Expect, Page, PageKind, PageState, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "gradients",
    title: "Gradients",
    kind: PageKind::Ui(UiPage {
        state: sheet,
        scenes: SCENES,
        bench: BENCH,
    }),
};

/// `#141a33`, the top of the wash.
const TOP: [u8; 4] = [0x14, 0x1a, 0x33, 0xff];
/// `#0d0f1a`, the bottom of the wash.
const BOTTOM: [u8; 4] = [0x0d, 0x0f, 0x1a, 0xff];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    All,
    Radial,
    Linear,
    Shadow,
}

/// The knob's choices, in its order: `All` is the default.
const PARTS: [Part; 4] = [Part::All, Part::Radial, Part::Linear, Part::Shadow];

fn name(part: Part) -> &'static str {
    match part {
        Part::All => "All",
        Part::Radial => "Radial light",
        Part::Linear => "Linear fade",
        Part::Shadow => "Shadow",
    }
}

/// The vertical ramp over the whole area: `TOP` at the top edge, `BOTTOM` at the bottom edge.
fn draw_wash(ui: &mut Ui<'_>, area: Rect) {
    ui.painter().linear(area, 0.0, TOP, BOTTOM, FRAC_PI_2);
}

/// Two large lights, each fading to transparent at its rim.
fn draw_lights(ui: &mut Ui<'_>, area: Rect) {
    let p = ui.painter();
    p.radial(
        [area.x + 0.28 * area.w, area.y + 0.32 * area.h],
        [0.46 * area.w, 0.7 * area.h],
        [0x4f, 0x6b, 0xff, 0x40],
        [0x4f, 0x6b, 0xff, 0x00],
        Falloff::Gaussian,
    );
    p.radial(
        [area.x + 0.78 * area.w, area.y + 0.7 * area.h],
        [0.4 * area.w, 0.6 * area.h],
        [0xb0, 0x6b, 0xff, 0x33],
        [0xb0, 0x6b, 0xff, 0x00],
        Falloff::Smooth,
    );
}

/// A card at the centre: a blue shadow offset downwards, a light glow around it, then the card.
fn draw_card(ui: &mut Ui<'_>, area: Rect) {
    let c = [area.x + 0.5 * area.w, area.y + 0.5 * area.h];
    let card = Rect::new(c[0] - 180.0, c[1] - 110.0, 360.0, 220.0);
    let p = ui.painter();
    let offset = Rect::new(card.x, card.y + 14.0, card.w, card.h);
    p.shadow(offset, 18.0, 22.0, [0x3a, 0x6f, 0xff, 0x90]);
    p.shadow(card, 18.0, 36.0, [0x8f, 0xb4, 0xff, 0x55]);
    p.fill(card, 18.0, [0x1d, 0x25, 0x4a, 0xff]);
}

/// The part filling the stage's area, clipped to it: a light's box may reach past the window.
fn draw(ui: &mut Ui<'_>, stage: &Stage, part: Part) {
    let area = stage.area(ui);
    ui.painter().push_clip(area);
    match part {
        Part::All => {
            draw_wash(ui, area);
            draw_lights(ui, area);
            draw_card(ui, area);
        }
        Part::Radial => {
            ui.painter().fill(area, 0.0, BOTTOM);
            draw_lights(ui, area);
        }
        Part::Linear => draw_wash(ui, area),
        Part::Shadow => {
            ui.painter().fill(area, 0.0, BOTTOM);
            draw_card(ui, area);
        }
    }
    ui.painter().pop_clip();
}

/// The page itself: the knob picks the part, `All` at first.
#[derive(Default)]
struct Sheet {
    /// An index into [`PARTS`].
    pick: usize,
}

impl PageState for Sheet {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let part = PARTS.get(self.pick).copied().unwrap_or(Part::All);
        draw(ui, stage, part);
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        let names: Vec<&str> = PARTS.iter().map(|p| name(*p)).collect();
        panel.choice(ui, "Show", &names, &mut self.pick);
    }
}

/// One part of the page, for a scene.
struct Show(Part);

impl PageState for Show {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        draw(ui, stage, self.0);
    }
}

fn sheet() -> Box<dyn PageState> {
    Box::new(Sheet::default())
}

fn wash() -> Box<dyn PageState> {
    Box::new(Show(Part::All))
}

fn radial_light() -> Box<dyn PageState> {
    Box::new(Show(Part::Radial))
}

fn linear_fade() -> Box<dyn PageState> {
    Box::new(Show(Part::Linear))
}

fn shadow() -> Box<dyn PageState> {
    Box::new(Show(Part::Shadow))
}

const fn scene(id: &'static str, scale: f32, state: fn() -> Box<dyn PageState>) -> UiScene {
    UiScene {
        id,
        scale,
        tier: MotionTier::Full,
        state,
        script: &[],
        capture_ms: 0,
        expect: Expect::Settled,
    }
}

const SCENES: &[UiScene] = &[
    scene("gradient-wash", 2.0, wash),
    scene("gradient-wash-1x", 1.0, wash),
    scene("gradient-radial-light", 2.0, radial_light),
    scene("gradient-linear-fade", 2.0, linear_fade),
    scene("gradient-shadow", 2.0, shadow),
];

/// A static page: the bench moves the pointer across it.
const BENCH: &[Step] = &[
    Step {
        at_ms: 0,
        events: &[Ev::PointerAt(120.0, 120.0)],
    },
    Step {
        at_ms: 200,
        events: &[Ev::PointerAt(480.0, 360.0)],
    },
];
