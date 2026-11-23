//! The `battery` page: a full battery, one charging, a low one, a critical one and an empty one
//! (D437). The knobs set the level and the charging flag.

use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::ui::Ui;
use lune_ui::widgets::battery::BatteryOptions;

use super::{caption, card, heading};
use crate::page::Stage;
use crate::page::{Expect, Page, PageKind, PageState, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "battery",
    title: "Battery",
    kind: PageKind::Ui(UiPage {
        state: full,
        scenes: SCENES,
        bench: &[],
    }),
};

/// The large and the small battery's sizes.
const LARGE: (f32, f32) = (120.0, 56.0);
const SMALL: (f32, f32) = (40.0, 20.0);

struct Live {
    level: f32,
    charging: bool,
}

fn state(level: f32, charging: bool) -> Box<dyn PageState> {
    Box::new(Live { level, charging })
}

fn full() -> Box<dyn PageState> {
    state(1.0, false)
}
fn charging() -> Box<dyn PageState> {
    state(0.8, true)
}
fn low() -> Box<dyn PageState> {
    state(0.15, false)
}
fn critical() -> Box<dyn PageState> {
    state(0.05, false)
}
fn empty() -> Box<dyn PageState> {
    state(0.0, false)
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let card = card(ui, stage, "Battery");
        let (x, y) = (card.x + 24.0, card.y + 24.0);
        let opts = BatteryOptions::new(self.level, "Charge").with_charging(self.charging);
        heading(ui, "LARGE", x, y);
        ui.battery("large", Rect::new(x, y + 20.0, LARGE.0, LARGE.1), &opts);
        let small_x = x + LARGE.0 + 40.0;
        heading(ui, "SMALL", small_x, y);
        ui.battery(
            "small",
            Rect::new(small_x, y + 20.0, SMALL.0, SMALL.1),
            &opts,
        );
        caption(
            ui,
            "Amber at 20 percent, red at 10; green while charging.",
            x,
            y + 20.0 + LARGE.1 + 16.0,
        );
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.number(ui, "Level", &mut self.level, (0.0, 1.0, 0.0));
        panel.toggle(ui, "Charging", &mut self.charging);
    }
}

const fn scene(id: &'static str, state: fn() -> Box<dyn PageState>) -> UiScene {
    UiScene {
        id,
        scale: 2.0,
        tier: MotionTier::Full,
        state,
        script: &[],
        capture_ms: 0,
        expect: Expect::Settled,
    }
}

const SCENES: &[UiScene] = &[
    scene("battery-full", full),
    scene("battery-charging", charging),
    scene("battery-low", low),
    scene("battery-critical", critical),
    scene("battery-empty", empty),
];
