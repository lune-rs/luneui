//! The `sparkline` page: a history as a line with its fill, a mirrored pair, a strip of bars, a
//! short history in a longer graph and none (D436). The knobs switch the style and the slot count.

use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::ui::Ui;
use lune_ui::widgets::sparkline::{SparkStyle, SparklineOptions};

use super::{caption, card, heading};
use crate::page::Stage;
use crate::page::{Expect, Page, PageKind, PageState, UiPage, UiScene};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "sparkline",
    title: "Sparkline",
    kind: PageKind::Ui(UiPage {
        state: line,
        scenes: SCENES,
        bench: &[],
    }),
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Shape {
    Line,
    Mirror,
    Bars,
    Sparse,
    Empty,
}

/// The graph's size and the small one's beside it.
const LARGE: (f32, f32) = (320.0, 96.0);
const SMALL: (f32, f32) = (120.0, 32.0);

struct Live {
    shape: Shape,
    /// Draw as bars whatever the scene's own style.
    bars: bool,
    /// The slot count; 0 is the scene's own.
    capacity: f32,
}

fn state(shape: Shape) -> Box<dyn PageState> {
    Box::new(Live {
        shape,
        bars: shape == Shape::Bars,
        capacity: 0.0,
    })
}

fn line() -> Box<dyn PageState> {
    state(Shape::Line)
}
fn mirror() -> Box<dyn PageState> {
    state(Shape::Mirror)
}
fn bars() -> Box<dyn PageState> {
    state(Shape::Bars)
}
fn sparse() -> Box<dyn PageState> {
    state(Shape::Sparse)
}
fn empty() -> Box<dyn PageState> {
    state(Shape::Empty)
}

/// `n` samples of a fixed wave, from 0 up: a slow swell with a faster ripple, so a dump is the
/// same every time.
fn wave(n: usize, phase: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let x = i as f32 * 0.21 + phase;
            (x.sin() * 0.5 + (x * 2.7).sin() * 0.25 + 0.75).max(0.0) * 40.0
        })
        .collect()
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let card = card(ui, stage, "Sparkline");
        let (x, y) = (card.x + 24.0, card.y + 24.0);
        let t = *ui.theme();
        let style = if self.bars {
            SparkStyle::Bars
        } else {
            SparkStyle::Line
        };
        let (up, down, capacity): (Vec<f32>, Vec<f32>, usize) = match self.shape {
            Shape::Line => (wave(60, 0.0), Vec::new(), 60),
            Shape::Mirror => (wave(60, 0.0), wave(60, 2.0), 60),
            Shape::Bars => (wave(10, 1.0), Vec::new(), 10),
            Shape::Sparse => (wave(3, 0.5), Vec::new(), 20),
            Shape::Empty => (Vec::new(), Vec::new(), 60),
        };
        let capacity = if self.capacity >= 1.0 {
            self.capacity as usize
        } else {
            capacity
        };
        let build = |name| {
            let o = SparklineOptions::new(&up, t.accent, name)
                .with_capacity(capacity)
                .with_style(style);
            if down.is_empty() {
                o
            } else {
                o.with_mirror(&down, t.data)
            }
        };
        // The large graph takes the card's width up to its size; the small one sits beside it
        // when that fits, else under it.
        let room = card.w - 48.0;
        let large = (LARGE.0.min(room), LARGE.1);
        heading(ui, "LARGE", x, y);
        ui.sparkline(
            "large",
            Rect::new(x, y + 20.0, large.0, large.1),
            &build("History"),
        );
        let beside = room >= large.0 + 32.0 + SMALL.0;
        let (small_x, small_y) = if beside {
            (x + large.0 + 32.0, y)
        } else {
            (x, y + 20.0 + large.1 + 16.0)
        };
        heading(ui, "SMALL", small_x, small_y);
        ui.sparkline(
            "small",
            Rect::new(small_x, small_y + 20.0, SMALL.0, SMALL.1),
            &build("Short history"),
        );
        let below = if beside {
            y + 20.0 + large.1
        } else {
            small_y + 20.0 + SMALL.1
        };
        caption(
            ui,
            "The newest sample is at the right edge.",
            x,
            below + 16.0,
        );
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Bars", &mut self.bars);
        panel.number(
            ui,
            "Slots (0: the scene's)",
            &mut self.capacity,
            (0.0, 200.0, 1.0),
        );
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
    scene("sparkline-line", line),
    scene("sparkline-mirror", mirror),
    scene("sparkline-bars", bars),
    scene("sparkline-sparse", sparse),
    scene("sparkline-empty", empty),
];
