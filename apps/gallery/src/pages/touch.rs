//! The `touch` page: what a finger does to the toolkit (`core.gestures`, `layout.scroll-area`), laid
//! out for a phone-width window. A row of buttons with a tap counter, a tile whose `context`
//! opens a menu (a long press answers it), a box a pinch scales, and a virtual list of 200 rows
//! on a scroll area that follows the finger, flings and rubber-bands (D348 to D350). The page
//! honours the window's safe area. Its `phone-touch-*` scenes run in the [`PHONE`] window at
//! scale 3 with a [`Ev::SafeArea`] of 62 above and 34 below; the touches are scripted at 60 Hz,
//! and `phone-touch-fling-120` is the same gesture at 8 ms.
//!
//! The feel of a real finger is not checked here (no finger can be driven): the scenes show the
//! states the motion reaches, which `tests/scroll_touch.rs` proves rate-independent.

use lune_ui::layout::Insets;
use lune_ui::motion::MotionTier;
use lune_ui::paint::Rect;
use lune_ui::ui::{Sense, Ui};
use lune_ui::widgets::button::{ButtonKind, ButtonOptions};
use lune_ui::widgets::label::LabelRole;
use lune_ui::widgets::menu::MenuRow;
use lune_ui::widgets::scroll_area::{Overscroll, ScrollPhysics};
use lune_ui::widgets::virtual_list::{Heights, ListOptions, RowKey, SelectionMode};

use super::{card, heading};
use crate::page::{Ev, Expect, PHONE, Page, PageKind, PageState, Stage, Step, UiPage, UiScene};
use crate::scenes::{CARD_MARGIN, H, TITLE_BAND, W};
use crate::showcase::knobs::Knobs;

pub(crate) static PAGE: Page = Page {
    id: "touch",
    title: "Touch",
    kind: PageKind::Ui(UiPage {
        state: live,
        scenes: SCENES,
        bench: BENCH,
    }),
};

fn live() -> Box<dyn PageState> {
    Box::new(Live::new())
}

/// The padding inside the card and the gap between two rows of the page.
const PAD: f32 = 12.0;
const GAP: f32 = 8.0;
/// A tap target: 44 logical pixels, the least a finger hits reliably.
const TARGET: f32 = 44.0;
/// The height of a list row.
const ROW: f32 = 56.0;
/// The rows of the list.
const ROWS: usize = 200;
/// The height of the pinch box's area; the box is 100 at scale 1 and at most 1.8 of it.
const AREA_H: f32 = 190.0;
const BOX: f32 = 100.0;
const SCALE_RANGE: (f32, f32) = (0.5, 1.8);
/// The width of the pin button in a row, and its distance from the row's right edge.
const PIN_W: f32 = 64.0;
const PIN_INSET: f32 = 16.0;

const fn fmax(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

const fn fmin(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// Where the page puts its parts, from the card, the safe area and the window's height. A `const
/// fn`, so the scenes aim their touches at the same places the page draws.
#[derive(Debug, Clone, Copy)]
struct Layout {
    title: (f32, f32),
    buttons: [Rect; 3],
    counter: (f32, f32),
    hold: Rect,
    area: Rect,
    list: Rect,
}

const fn layout(card: Rect, safe: Insets, window_h: f32) -> Layout {
    let x = card.x + PAD;
    let w = card.w - 2.0 * PAD;
    let top = fmax(card.y, safe.top) + PAD;
    let bottom = fmin(card.y + card.h, window_h - safe.bottom) - PAD;
    let bw = (w - 2.0 * GAP) / 3.0;
    let by = top + 22.0;
    let buttons = [
        Rect::new(x, by, bw, TARGET),
        Rect::new(x + bw + GAP, by, bw, TARGET),
        Rect::new(x + 2.0 * (bw + GAP), by, bw, TARGET),
    ];
    let cy = by + TARGET + GAP;
    let hy = cy + 16.0 + GAP;
    let ay = hy + TARGET + GAP;
    let ly = ay + AREA_H + GAP;
    Layout {
        title: (x, top),
        buttons,
        counter: (x, cy),
        hold: Rect::new(x, hy, w, TARGET),
        area: Rect::new(x, ay, w, AREA_H),
        list: Rect::new(x, ly, w, fmax(bottom - ly, 3.0 * ROW)),
    }
}

/// The card of a window `w` by `h` with nothing but the gallery's own band and margin around it.
const fn window_card(w: u32, h: u32) -> Rect {
    Rect::new(
        CARD_MARGIN,
        TITLE_BAND,
        w as f32 - 2.0 * CARD_MARGIN,
        h as f32 - TITLE_BAND - CARD_MARGIN,
    )
}

/// The safe area the phone scenes tell the page.
const SAFE: Insets = Insets::new(0.0, 62.0, 0.0, 34.0);
/// The page in the phone window.
const P: Layout = layout(window_card(PHONE.0, PHONE.1), SAFE, PHONE.1 as f32);
/// The page in the gallery's own window, which has no safe area (the bench aims at it).
const WIN: Layout = layout(window_card(W, H), Insets::ZERO, H as f32);

const fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// The live page: what the app would keep.
struct Live {
    /// Clicks on the three buttons at the top, and on the pin buttons in the rows.
    taps: u32,
    pins: u32,
    /// The pinch box's scale between pinches.
    scale: f32,
    /// Where the context menu is open, if it is.
    menu: Option<(f32, f32)>,
    /// The ends of the list rubber-band (else they clamp).
    rubber: bool,
}

impl Live {
    fn new() -> Self {
        Self {
            taps: 0,
            pins: 0,
            scale: 1.0,
            menu: None,
            rubber: true,
        }
    }

    fn context_menu(&mut self, ui: &mut Ui<'_>) {
        let Some(at) = self.menu else {
            return;
        };
        let item = |text| MenuRow::Item {
            glyph: None,
            text,
            shortcut: "",
        };
        let rows = [
            item("Copy"),
            item("Share"),
            MenuRow::Separator,
            item("Delete"),
        ];
        let response = ui.menu("hold-menu", &rows, at);
        if response.picked.is_some() || response.dismissed {
            self.menu = None;
        }
    }
}

impl PageState for Live {
    fn draw(&mut self, ui: &mut Ui<'_>, stage: &Stage) {
        let card = card(ui, stage, "Touch");
        let safe = ui.safe_area();
        let window_h = ui.painter().window.h;
        let l = layout(card, safe, window_h);
        heading(ui, "TOUCH", l.title.0, l.title.1);

        for (i, (key, text, kind)) in [
            ("like", "Like", ButtonKind::Primary),
            ("save", "Save", ButtonKind::Secondary),
            ("share", "Share", ButtonKind::Ghost),
        ]
        .into_iter()
        .enumerate()
        {
            if ui.button(key, l.buttons[i], text, kind).clicked {
                self.taps += 1;
            }
        }
        let counts = format!("Taps {}   Pins {}", self.taps, self.pins);
        ui.label("counts", l.counter, &counts, LabelRole::Small);

        // A long press is a secondary click on any widget (D348): the menu opens under the finger.
        let hold = ui.button("hold", l.hold, "Hold for a menu", ButtonKind::Secondary);
        if hold.secondary_clicked {
            // Under the tile, not under the finger: the finger rests on the tile and a menu that
            // opens beneath it would hide it.
            self.menu = Some((l.hold.x, l.hold.y + l.hold.h + 4.0));
        }
        self.context_menu(ui);

        self.pinch_box(ui, l.area);

        let rubber = self.rubber;
        let physics = ScrollPhysics::default().with_overscroll(if rubber {
            Overscroll::RubberBand
        } else {
            Overscroll::Clamp
        });
        let opts = ListOptions {
            count: ROWS,
            heights: Heights::Fixed(ROW),
            selection: SelectionMode::None,
            index_of: None,
            label: "Rows",
        };
        let pins = &mut self.pins;
        ui.with_scroll_physics(physics, |ui| {
            ui.virtual_list(
                "list",
                l.list,
                &opts,
                |i| RowKey(i as u64),
                |ui, row| {
                    let t = *ui.theme();
                    let r = row.rect;
                    let p = ui.painter();
                    p.fill(
                        Rect::new(r.x + 16.0, r.y + ROW - 1.0, r.w - 32.0, 1.0),
                        0.0,
                        t.border_subtle,
                    );
                    ui.label(
                        ("row", row.index),
                        (r.x + 16.0, r.y + 20.0),
                        &format!("Row {}", row.index),
                        LabelRole::Base,
                    );
                    let pin = Rect::new(r.x + r.w - PIN_INSET - PIN_W, r.y + 16.0, PIN_W, 24.0);
                    let options = ButtonOptions {
                        kind: ButtonKind::Ghost,
                        small: true,
                        icon: None,
                    };
                    if ui
                        .button_with(("pin", row.index), pin, "Pin", &options)
                        .clicked
                    {
                        *pins += 1;
                    }
                    ROW
                },
            );
        });
    }

    fn knobs(&mut self, ui: &mut Ui<'_>, panel: &mut Knobs<'_>) {
        panel.toggle(ui, "Rubber band at the ends", &mut self.rubber);
    }
}

impl Live {
    /// The box a pinch scales: it senses drag, so a pan never takes the touch from it.
    fn pinch_box(&mut self, ui: &mut Ui<'_>, area: Rect) {
        let id = ui.id("pinch");
        let reading = ui.pinch(id);
        let shown =
            (self.scale * reading.map_or(1.0, |p| p.scale)).clamp(SCALE_RANGE.0, SCALE_RANGE.1);
        if let Some(p) = reading
            && p.ended
        {
            self.scale = shown;
        }
        let t = *ui.theme();
        let side = BOX * shown;
        let (cx, cy) = centre(area);
        let rect = Rect::new(cx - side / 2.0, cy - side / 2.0, side, side);
        ui.painter().fill(area, t.item_radius, t.surface_hover);
        ui.interact(id, rect, Sense::DRAG);
        ui.painter().fill(rect, t.item_radius, t.accent);
        ui.label(
            "scale",
            (rect.x + 12.0, rect.y + 12.0),
            &format!("{shown:.1}x"),
            LabelRole::Strong,
        );
    }
}

// ---- scenes -----------------------------------------------------------------------------------

/// The first step of every phone scene: the window's safe area.
const SETUP: Step = Step {
    at_ms: 0,
    events: &[Ev::SafeArea(SAFE.left, SAFE.top, SAFE.right, SAFE.bottom)],
};

/// A phone scene: scale 3, the full motion tier, the page's live state.
const fn phone(
    id: &'static str,
    script: &'static [Step],
    capture_ms: u32,
    expect: Expect,
) -> UiScene {
    UiScene {
        id,
        scale: 3.0,
        tier: MotionTier::Full,
        state: live,
        script,
        capture_ms,
        expect,
    }
}

/// A step that moves touch 1 to (x, y) at `ms`.
macro_rules! drag {
    ($ms:expr, $x:expr, $y:expr) => {
        Step {
            at_ms: $ms,
            events: &[Ev::TouchMove(1, $x, $y)],
        }
    };
}

/// A step that puts touch 1 down at (x, y) at `ms`.
macro_rules! down {
    ($ms:expr, $x:expr, $y:expr) => {
        Step {
            at_ms: $ms,
            events: &[Ev::TouchStart(1, $x, $y)],
        }
    };
}

/// A step that lifts touch 1 at (x, y) at `ms`.
macro_rules! up {
    ($ms:expr, $x:expr, $y:expr) => {
        Step {
            at_ms: $ms,
            events: &[Ev::TouchEnd(1, $x, $y)],
        }
    };
}

/// An empty step: a frame at `ms` with no events (what a real loop runs when the core asked for
/// one).
const fn frame(ms: u32) -> Step {
    Step {
        at_ms: ms,
        events: &[],
    }
}

/// The first button's centre: a tap.
const TAP_AT: (f32, f32) = centre(P.buttons[0]);

const TAP: &[Step] = &[
    SETUP,
    Step {
        at_ms: 16,
        events: &[Ev::TouchOn(1, &["like"])],
    },
    up!(80, TAP_AT.0, TAP_AT.1),
];

/// The long press: down on the hold tile, a frame where the core asked for one (500 ms after the
/// press), and the finger stays down.
const LONG_PRESS: &[Step] = &[
    SETUP,
    Step {
        at_ms: 16,
        events: &[Ev::TouchOn(1, &["hold"])],
    },
    frame(516),
];

/// The finger in the list, and where a pan starts.
const FX: f32 = P.list.x + 100.0;
const PAN_Y: f32 = P.list.y + 300.0;

/// Up 200 px in four frames, then held: the offset is 190.
const PAN: &[Step] = &[
    SETUP,
    down!(16, FX, PAN_Y),
    drag!(33, FX, PAN_Y - 50.0),
    drag!(50, FX, PAN_Y - 100.0),
    drag!(67, FX, PAN_Y - 150.0),
    drag!(83, FX, PAN_Y - 200.0),
];

/// A flick at 2000 px/s up (2 px per ms from the press at 16 ms), lifted at 200 ms. It starts near
/// the bottom of the list and ends near its top, so the list moves 358 px under the finger.
const FLICK_Y: f32 = P.list.y + P.list.h - 24.0;

const fn flick(ms: u32) -> f32 {
    FLICK_Y - 2.0 * (ms - 16) as f32
}

macro_rules! flick {
    ($ms:expr) => {
        drag!($ms, FX, flick($ms))
    };
}

/// The flick scripted at 60 Hz (a frame every 16.7 ms, to the millisecond).
const FLING: &[Step] = &[
    SETUP,
    down!(16, FX, FLICK_Y),
    flick!(33),
    flick!(50),
    flick!(67),
    flick!(83),
    flick!(100),
    flick!(117),
    flick!(133),
    flick!(150),
    flick!(167),
    flick!(183),
    up!(200, FX, flick(200)),
];

/// The same flick at 120 Hz-ish (a frame every 8 ms).
const FLING_120: &[Step] = &[
    SETUP,
    down!(16, FX, FLICK_Y),
    flick!(24),
    flick!(32),
    flick!(40),
    flick!(48),
    flick!(56),
    flick!(64),
    flick!(72),
    flick!(80),
    flick!(88),
    flick!(96),
    flick!(104),
    flick!(112),
    flick!(120),
    flick!(128),
    flick!(136),
    flick!(144),
    flick!(152),
    flick!(160),
    flick!(168),
    flick!(176),
    flick!(184),
    flick!(192),
    up!(200, FX, flick(200)),
];

/// A pull down from the top: 160 px of travel, 150 past the slop, then held still.
const PULL_Y: f32 = P.list.y + 100.0;

const PULL: &[Step] = &[
    SETUP,
    down!(16, FX, PULL_Y),
    drag!(33, FX, PULL_Y + 40.0),
    drag!(50, FX, PULL_Y + 80.0),
    drag!(67, FX, PULL_Y + 120.0),
    drag!(83, FX, PULL_Y + 160.0),
];

/// The pull, held still for 100 ms and let go: the list springs back.
const RELEASE: &[Step] = &[
    SETUP,
    down!(16, FX, PULL_Y),
    drag!(33, FX, PULL_Y + 40.0),
    drag!(50, FX, PULL_Y + 80.0),
    drag!(67, FX, PULL_Y + 120.0),
    drag!(83, FX, PULL_Y + 160.0),
    up!(200, FX, PULL_Y + 160.0),
];

/// The pin button of row 1 at offset 0: a pan that starts on it goes to the list.
const PIN_AT: (f32, f32) = (
    P.list.x + P.list.w - PIN_INSET - PIN_W / 2.0,
    P.list.y + ROW + 16.0 + 12.0,
);

const ARBITRATION: &[Step] = &[
    SETUP,
    down!(16, PIN_AT.0, PIN_AT.1),
    drag!(33, PIN_AT.0, PIN_AT.1 - 20.0),
    drag!(50, PIN_AT.0, PIN_AT.1 - 60.0),
    drag!(67, PIN_AT.0, PIN_AT.1 - 100.0),
    drag!(83, PIN_AT.0, PIN_AT.1 - 140.0),
];

/// The box's centre and the two touches 100 px apart that spread to 160.
const BOX_AT: (f32, f32) = centre(P.area);

const PINCH: &[Step] = &[
    SETUP,
    down!(16, BOX_AT.0 - 40.0, BOX_AT.1),
    Step {
        at_ms: 33,
        events: &[Ev::TouchStart(2, BOX_AT.0 + 60.0, BOX_AT.1)],
    },
    Step {
        at_ms: 50,
        events: &[
            Ev::TouchMove(1, BOX_AT.0 - 50.0, BOX_AT.1),
            Ev::TouchMove(2, BOX_AT.0 + 70.0, BOX_AT.1),
        ],
    },
    Step {
        at_ms: 67,
        events: &[
            Ev::TouchMove(1, BOX_AT.0 - 60.0, BOX_AT.1),
            Ev::TouchMove(2, BOX_AT.0 + 80.0, BOX_AT.1),
        ],
    },
    Step {
        at_ms: 83,
        events: &[
            Ev::TouchMove(1, BOX_AT.0 - 70.0, BOX_AT.1),
            Ev::TouchMove(2, BOX_AT.0 + 90.0, BOX_AT.1),
        ],
    },
];

pub(super) const SCENES: &[UiScene] = &[
    // A tap on the first button: the counter reads 1.
    phone("phone-touch-tap", TAP, 600, Expect::Settled),
    // The press on the hold tile lasts past 500 ms: the menu is open under the finger.
    phone("phone-touch-long-press", LONG_PRESS, 900, Expect::Settled),
    // The finger is down on the list and has moved 200 px up: the list moved 190.
    phone("phone-touch-pan", PAN, 400, Expect::Settled),
    // 200 ms after a lift at 2000 px/s: the list is still coasting.
    phone("phone-touch-fling", FLING, 400, Expect::Moving),
    // The same flick with a frame every 8 ms: the same picture.
    phone("phone-touch-fling-120", FLING_120, 400, Expect::Moving),
    // After the decay's end (about 3 s after the lift): at rest, settled.
    phone("phone-touch-landed", FLING, 3600, Expect::Settled),
    // The finger pulled 150 px past the top and holds: the list is stretched, less than the pull.
    phone("phone-touch-rubber-band", PULL, 400, Expect::Settled),
    // 100 ms after that pull is let go: the bounce is back part of the way.
    phone("phone-touch-bounce", RELEASE, 300, Expect::Moving),
    // A pan that started on a pin button: the list moved, the button was not pressed.
    phone("phone-touch-arbitration", ARBITRATION, 400, Expect::Settled),
    // Two touches 100 px apart spread to 160: the box is at 1.6.
    phone("phone-touch-pinch", PINCH, 400, Expect::Settled),
];

/// What the bench replays, in the gallery's own window: a pan with a fling, a pull, and a tap.
const BENCH_X: f32 = WIN.list.x + 200.0;
const BENCH_Y: f32 = WIN.list.y + WIN.list.h - 20.0;

const BENCH: &[Step] = &[
    down!(0, BENCH_X, BENCH_Y),
    drag!(16, BENCH_X, BENCH_Y - 60.0),
    drag!(33, BENCH_X, BENCH_Y - 120.0),
    drag!(50, BENCH_X, BENCH_Y - 180.0),
    up!(67, BENCH_X, BENCH_Y - 180.0),
    down!(1500, BENCH_X, WIN.list.y + 40.0),
    drag!(1516, BENCH_X, WIN.list.y + 100.0),
    drag!(1533, BENCH_X, WIN.list.y + 160.0),
    up!(1550, BENCH_X, WIN.list.y + 160.0),
    Step {
        at_ms: 2500,
        events: &[Ev::TouchOn(1, &["like"])],
    },
    up!(2560, WIN.buttons[0].x + 1.0, WIN.buttons[0].y + 1.0),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Player, run_scene};
    use lune_ui::input::FrameOutput;
    use lune_ui::testing::FakeText;

    fn scene(id: &str) -> &'static UiScene {
        SCENES.iter().find(|s| s.id == id).expect("a scene")
    }

    /// The scene run through the test player: its capture frame.
    fn capture(id: &str) -> (lune_ui::scene::Scene, FrameOutput) {
        run_scene("touch", scene(id), &mut FakeText).expect("the scene runs")
    }

    fn named<'a>(out: &'a FrameOutput, name: &str) -> Option<&'a lune_ui::access::Node> {
        out.tree.nodes.iter().find(|n| n.name == name)
    }

    /// The top of row `n`'s label, which follows the list's offset; `None` when the row is out of
    /// view.
    fn row_top(out: &FrameOutput, n: usize) -> Option<f32> {
        named(out, &format!("Row {n}")).map(|node| node.bounds.y)
    }

    /// C2(d) on the gallery's own page: the same flick at 60 and at 120 Hz builds the same
    /// picture at the capture frame.
    #[test]
    fn the_flick_at_60_and_at_120_hz_builds_equal_scenes() {
        let a = capture("phone-touch-fling");
        let b = capture("phone-touch-fling-120");
        assert_eq!(a.1.repaint, b.1.repaint);
        assert!(
            a.0 == b.0,
            "the capture frames differ: a flick scripted every 16.7 ms and every 8 ms"
        );
        // The instrument: the flick moved the list far (row 3 is at hand at rest, row 0 gone).
        let rest = capture("phone-touch-tap").1;
        assert!(row_top(&rest, 3).is_some() && row_top(&a.1, 0).is_none());
        assert!(row_top(&a.1, 3).is_none());
    }

    #[test]
    fn a_tap_clicks_the_button_once() {
        let (_, out) = capture("phone-touch-tap");
        assert!(named(&out, "Taps 1   Pins 0").is_some());
    }

    #[test]
    fn a_long_press_opens_the_menu_and_a_short_one_does_not() {
        let (_, out) = capture("phone-touch-long-press");
        assert!(named(&out, "Copy").is_some(), "the menu is open");
        let (_, tap) = capture("phone-touch-tap");
        assert!(named(&tap, "Copy").is_none());
    }

    #[test]
    fn a_pan_that_starts_on_a_button_scrolls_the_list_and_presses_nothing() {
        let (_, out) = capture("phone-touch-arbitration");
        assert!(named(&out, "Taps 0   Pins 0").is_some());
        let (_, rest) = capture("phone-touch-tap");
        let moved = row_top(&rest, 3).zip(row_top(&out, 3)).map(|(a, b)| a - b);
        assert!(
            moved.is_some_and(|m| (m - 130.0).abs() < 1.0),
            "the list moved {moved:?}"
        );
    }

    #[test]
    fn a_pull_past_the_top_stretches_the_list_by_less_than_the_pull() {
        let (_, out) = capture("phone-touch-rubber-band");
        let (_, rest) = capture("phone-touch-tap");
        let stretch = row_top(&out, 0).zip(row_top(&rest, 0)).map(|(a, b)| a - b);
        assert!(
            stretch.is_some_and(|s| s > 20.0 && s < 150.0),
            "stretched {stretch:?}"
        );
    }

    #[test]
    fn a_pinch_scales_the_box_to_1_6() {
        let (_, out) = capture("phone-touch-pinch");
        assert!(named(&out, "1.6x").is_some());
    }

    #[test]
    fn every_phone_scene_runs_in_the_phone_window_at_scale_3() {
        for s in SCENES {
            assert_eq!((s.window(), s.scale), (PHONE, 3.0), "{}", s.id);
            assert_eq!(s.size(), (1206, 2622), "{}", s.id);
        }
    }

    /// The bench's touches run through the page in the gallery's window without an error.
    #[test]
    fn the_bench_runs() {
        let mut player = Player::new("touch", live(), (W as f32, H as f32), 1.0, MotionTier::Full);
        player.frame(&mut FakeText, 0, &[]).expect("frame");
        for step in BENCH {
            player
                .frame(&mut FakeText, step.at_ms, step.events)
                .expect("a bench frame");
        }
    }
}
