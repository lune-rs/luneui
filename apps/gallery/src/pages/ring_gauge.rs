//! The `ring-gauge` page: one segment with a centred reading, three segments end to end, none, a
//! pair past a whole turn (cut at the turn) and a change of the value caught in the middle of its
//! tween (D435). A slider moves the first segment's value.

use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::ui::Ui;
use lune_ui::widgets::ring_gauge::{RingGaugeOptions, RingSegment};

use super::{caption, card, heading};
use crate::page::Stage;
use crate::page::{Expect, Page, PageKind, PageState, Step, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "ring-gauge",
    title: "Ring gauge",
    kind: PageKind::Ui(UiPage {
        state: single,
        scenes: SCENES,
        bench: &[],
    }),
};

/// The large and the small ring's sizes.
const LARGE: f32 = 120.0;
const SMALL: f32 = 56.0;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Single,
    Segments,
    Empty,
    Full,
    /// The value moves from `FROM` to `TO` on the second frame.
    Change,
}

const SHAPES: [(&str, Shape); 4] = [
    ("One segment", Shape::Single),
    ("Three segments", Shape::Segments),
    ("None", Shape::Empty),
    ("Past a turn", Shape::Full),
];

const FROM: f32 = 0.2;
const TO: f32 = 0.8;

struct Live {
    shape: Shape,
    /// The first segment's value in `Single`.
    value: f32,
    frames: u32,
    text: bool,
}

fn state(shape: Shape) -> Box<dyn PageState> {
    Box::new(Live {
        shape,
        value: 0.36,
        frames: 0,
        text: true,
    })
}

fn single() -> Box<dyn PageState> {
    state(Shape::Single)
}
fn segments() -> Box<dyn PageState> {
    state(Shape::Segments)
}
fn empty() -> Box<dyn PageState> {
    state(Shape::Empty)
}
fn full() -> Box<dyn PageState> {
    state(Shape::Full)
}
fn change() -> Box<dyn PageState> {
    state(Shape::Change)
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let card = card(ui, stage, "Ring gauge");
        let (x, y) = (card.x + 24.0, card.y + 24.0);
        let t = *ui.theme();
        let first = match self.shape {
            Shape::Change => {
                self.frames += 1;
                if self.frames > 1 { TO } else { FROM }
            }
            _ => self.value,
        };
        let (list, reading): (Vec<RingSegment>, String) = match self.shape {
            Shape::Single | Shape::Change => (
                vec![RingSegment::new(1, first, t.accent)],
                format!("{:.0}%", first * 100.0),
            ),
            Shape::Segments => (
                vec![
                    RingSegment::new(1, 0.55, t.accent),
                    RingSegment::new(2, 0.15, t.warn),
                    RingSegment::new(3, 0.09, t.data),
                ],
                "79%".to_owned(),
            ),
            Shape::Empty => (Vec::new(), "0%".to_owned()),
            Shape::Full => (
                vec![
                    RingSegment::new(1, 0.6, t.accent),
                    RingSegment::new(2, 0.6, t.err),
                ],
                "100%".to_owned(),
            ),
        };
        let text = if self.text { reading.as_str() } else { "" };
        heading(ui, "LARGE", x, y);
        ui.ring_gauge(
            "large",
            Rect::new(x, y + 20.0, LARGE, LARGE),
            &RingGaugeOptions::new(&list, "Reading").with_text(text),
        );
        let small_x = x + LARGE + 40.0;
        heading(ui, "SMALL, THIN", small_x, y);
        ui.ring_gauge(
            "small",
            Rect::new(small_x, y + 20.0, SMALL, SMALL),
            &RingGaugeOptions::new(&list, "Reading")
                .with_text(text)
                .with_thickness(5.0),
        );
        caption(
            ui,
            "Segments lie end to end from the top.",
            x,
            y + 20.0 + LARGE + 16.0,
        );
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        let names: Vec<&str> = SHAPES.iter().map(|(n, _)| *n).collect();
        let mut at = SHAPES
            .iter()
            .position(|(_, s)| *s == self.shape)
            .unwrap_or(0);
        if panel.choice(ui, "Segments", &names, &mut at) {
            self.shape = SHAPES[at].1;
        }
        panel.number(ui, "Value", &mut self.value, (0.0, 1.0, 0.0));
        panel.toggle(ui, "Centred text", &mut self.text);
    }
}

/// The second frame at time 0 carries the other value, so the capture at 90 ms is the middle of
/// the 180 ms tween.
const CHANGE: Step = Step {
    at_ms: 0,
    events: &[],
};

const fn scene(
    id: &'static str,
    state: fn() -> Box<dyn PageState>,
    script: &'static [Step],
    capture_ms: u32,
    expect: Expect,
) -> UiScene {
    UiScene {
        id,
        scale: 2.0,
        tier: MotionTier::Full,
        state,
        script,
        capture_ms,
        expect,
    }
}

const SCENES: &[UiScene] = &[
    scene("ring-gauge-single", single, &[], 0, Expect::Settled),
    scene("ring-gauge-segments", segments, &[], 0, Expect::Settled),
    scene("ring-gauge-empty", empty, &[], 0, Expect::Settled),
    scene("ring-gauge-full", full, &[], 0, Expect::Settled),
    scene("ring-gauge-mid", change, &[CHANGE], 90, Expect::Moving),
];
