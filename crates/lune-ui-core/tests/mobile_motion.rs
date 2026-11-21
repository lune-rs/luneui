//! The motion rows of the mobile components (mobile M6, D369). A row drives a component through a
//! real `Ui` at 60 Hz and again at 120 Hz, reads the moving part's position each frame and passes
//! `conformance_of_at` with the bound `vmax / rate + 0.5` (M3's form, `tests/motion_physics.rs`).
//! The components' rows are added here by steps 2 to 7; this step builds the harness and proves it
//! on a known pass and a known failure, so a row that passes means something.

use std::time::Duration;

use lune_ui_core::motion::{
    Conformance, ConformanceError, DampedSpring, Kind, Motion, Time, conformance_of_at,
};
use lune_ui_core::testing::UiHarness;
use lune_ui_core::ui::{Ui, WidgetId};

/// The two rates every row runs at.
const RATES: [u32; 2] = [60, 120];

/// What a row sampled: one reading per frame, and whether the part is still moving.
type Sampler<'a> = Box<dyn FnMut(Time) -> (f32, bool) + 'a>;

/// The fastest the sampled position moves, in px/s: the largest step between neighbours of a 4000
/// point grid over `0..=end`, over its length. (The instrument is checked in
/// `vmax_reads_a_known_speed`.)
fn vmax(mut sample: Sampler<'_>, end: Time) -> f32 {
    const POINTS: u32 = 4000;
    let dt = end.as_secs_f64() / f64::from(POINTS);
    let mut prev = sample(Time::ZERO).0;
    let mut fastest = 0.0f32;
    for i in 1..=POINTS {
        let x = sample(Duration::from_secs_f64(dt * f64::from(i))).0;
        fastest = fastest.max(((x - prev).abs() / dt as f32).abs());
        prev = x;
    }
    fastest
}

/// Runs a row at each rate: a fresh sampler from `make` per rate, the bound `vmax / rate + 0.5`
/// with `vmax` from a fine scan of its own fresh sampler. Returns `(rate, verdict)` per rate.
fn at_60_and_120<'a>(
    end: Time,
    target: f32,
    make: impl Fn() -> Sampler<'a>,
) -> Vec<(u32, Result<Conformance, ConformanceError>)> {
    let fastest = vmax(make(), end);
    RATES
        .iter()
        .map(|&rate| {
            let bound = fastest / rate as f32 + 0.5;
            (rate, conformance_of_at(rate, make(), end, target, bound))
        })
        .collect()
}

/// Panics, naming the row and the rate, unless the row conforms at both rates.
fn assert_conforms<'a>(row: &str, end: Time, target: f32, make: impl Fn() -> Sampler<'a>) {
    for (rate, verdict) in at_60_and_120(end, target, make) {
        if let Err(e) = verdict {
            panic!("{row} at {rate} Hz: {e}");
        }
    }
}

/// A real `Ui` frame per sample: the component's drawing reads the app time of the sample, and the
/// frame reports whether it asked to be drawn again.
struct UiSampler {
    h: UiHarness,
    /// App time of the sample at grid time zero, so a warm-up frame can come before it.
    origin: Time,
}

impl UiSampler {
    /// A window of `size` whose first sample is at app time `origin`.
    fn new(size: (f32, f32), origin: Time) -> Self {
        UiSampler {
            h: UiHarness::new(size),
            origin,
        }
    }

    /// A frame at app time `at`: `draw` builds it and returns the moving part's position.
    fn frame(&mut self, at: Time, draw: impl FnOnce(&mut Ui<'_>) -> f32) -> (f32, bool) {
        self.h.input.set_time(at);
        let mut x = 0.0;
        let (_, out) = self.h.frame(|ui| x = draw(ui));
        (x, out.repaint != lune_ui_core::input::Repaint::Idle)
    }

    /// The frame at grid time `t` (since the sampler's origin).
    fn sample(&mut self, t: Time, draw: impl FnOnce(&mut Ui<'_>) -> f32) -> (f32, bool) {
        self.frame(self.origin + t, draw)
    }
}

// ---- the harness on known cases ------------------------------------------------------------

/// The settle spring of `gesture_track` (critically damped, 0.3 s), from 120 px with a flick of
/// -1500 px/s to 0: a pure closed form, so the harness is the only thing under test.
#[test]
fn a_known_release_conforms_at_60_and_120() {
    let spring = DampedSpring::new(Duration::from_millis(300), 1.0);
    let (from, to, v0) = (120.0f32, 0.0f32, -1500.0f32);
    let end = spring.duration(from, to, v0);
    let make =
        || -> Sampler<'static> { Box::new(move |t| (spring.sample(from, to, v0, t).0, t < end)) };
    let fastest = vmax(make(), end);
    assert!(fastest >= v0.abs() - 5.0, "the scan reads {fastest}");
    assert_conforms("known release", end, to, make);
}

#[test]
fn a_50_px_jump_fails_with_jump_at_both_rates() {
    let spring = DampedSpring::new(Duration::from_millis(300), 1.0);
    let (from, to, v0) = (120.0f32, 0.0f32, -1500.0f32);
    let end = spring.duration(from, to, v0);
    // The same release with a 50 px step in the middle of it.
    let make = || -> Sampler<'static> {
        Box::new(move |t| {
            let (p, _) = spring.sample(from, to, v0, t);
            let jump = if t >= end / 2 && t < end { 50.0 } else { 0.0 };
            (p + jump, t < end)
        })
    };
    // The bound is the one a smooth release gets: the scan of the smooth sampler.
    let smooth = vmax(
        Box::new(move |t| (spring.sample(from, to, v0, t).0, t < end)),
        end,
    );
    for rate in RATES {
        let verdict = conformance_of_at(rate, make(), end, to, smooth / rate as f32 + 0.5);
        assert!(
            matches!(verdict, Err(ConformanceError::Jump { .. })),
            "{rate} Hz: {verdict:?}"
        );
    }
}

#[test]
fn vmax_reads_a_known_speed() {
    // A straight line at 600 px/s for one second.
    let line = || -> Sampler<'static> { Box::new(|t| (600.0 * t.as_secs_f32(), true)) };
    let v = vmax(line(), Duration::from_secs(1));
    assert!((v - 600.0).abs() < 1.0, "{v}");
}

/// The Ui path: a component's position read through a real frame (here `animate_f32`, the
/// primitive the components' programmatic moves use) is conformant at both rates.
#[test]
fn a_ui_driven_move_conforms_at_60_and_120() {
    let motion = Motion::of(Kind::PageSwap);
    let end = Duration::from_millis(350);
    let origin = Duration::from_secs(1);
    let make = || -> Sampler<'static> {
        let mut s = UiSampler::new((400.0, 300.0), origin);
        let id = |ui: &mut Ui<'_>| -> WidgetId { ui.id("box") };
        // A warm-up frame before the grid's first sample, with the target at 0.
        s.frame(Duration::ZERO, |ui| {
            let id = id(ui);
            ui.animate_f32(id, 0.0, &motion)
        });
        Box::new(move |t| {
            s.sample(t, |ui| {
                let id = id(ui);
                ui.animate_f32(id, 300.0, &motion)
            })
        })
    };
    assert_conforms("ui-driven move", end, 300.0, make);
}

// ---- the bottom sheet (step 3, D371) -------------------------------------------------------

mod sheet {
    use super::*;
    use lune_ui_core::input::Repaint;
    use lune_ui_core::layout::Insets;
    use lune_ui_core::widgets::bottom_sheet::{BottomSheet, Detent};

    /// The phone's window, its safe area 0/62/0/34: the largest height is 802.
    const PHONE: (f32, f32) = (402.0, 874.0);
    const CLOSED: f32 = 874.0;
    /// 16 ms of app time: the warm-up frames' step.
    const STEP: Time = Duration::from_millis(16);
    /// A generous end for every row here: a settle is over well before, and a frame still moving
    /// at or after it fails.
    const END: Time = Duration::from_millis(1800);

    /// The two detents of the plan's rows, and three that put a small one near the bottom.
    const BOTH: &[Detent] = &[Detent::Fraction(0.5), Detent::Fraction(1.0)];
    const THREE: &[Detent] = &[
        Detent::Height(150.0),
        Detent::Height(500.0),
        Detent::Fraction(1.0),
    ];

    /// A sheet in a real `Ui`, driven by app time; reads the sheet node's top.
    struct Rig {
        s: UiSampler,
        open: bool,
        detents: &'static [Detent],
    }

    impl Rig {
        fn new(detents: &'static [Detent]) -> Self {
            let mut s = UiSampler::new(PHONE, Time::ZERO);
            s.h.input.safe_area(Insets::new(0.0, 62.0, 0.0, 34.0));
            Rig {
                s,
                open: false,
                detents,
            }
        }

        /// One frame at app time `at`: the sheet node's top (`None` when it is not drawn) and
        /// whether the frame asked for another.
        fn frame(&mut self, at: Time) -> (Option<f32>, bool) {
            let Rig { s, open, detents } = self;
            s.h.input.set_time(at);
            let id = WidgetId::ROOT.with("sheet");
            let (_, out) = s.h.frame(|ui| {
                let sheet = BottomSheet::new("Sheet").with_detents(detents);
                ui.bottom_sheet("sheet", open, &sheet, |_, _| ());
            });
            (
                out.tree.get(id).map(|n| n.bounds.y),
                out.repaint != Repaint::Idle,
            )
        }

        /// Frames every 16 ms from `from` through `until`; the time of the last.
        fn run(&mut self, from: Time, until: Time) -> Time {
            let mut t = from;
            while t <= until {
                self.frame(t);
                t += STEP;
            }
            t - STEP
        }

        /// Opens the sheet at `Time::ZERO` and lets it settle for 1.5 s; the next frame's time.
        fn opened(detents: &'static [Detent]) -> (Rig, Time) {
            let mut rig = Rig::new(detents);
            rig.frame(Time::ZERO);
            rig.open = true;
            let last = rig.run(STEP, Duration::from_millis(1500));
            (rig, last + STEP)
        }

        /// A click on the grabber of the sheet resting at `top`, then 1.5 s to settle; the next
        /// frame's time.
        fn step(&mut self, at: Time, top: f32) -> Time {
            self.s.h.input.pointer_to(201.0, top + 10.0);
            self.s
                .h
                .input
                .click(lune_ui_core::input::PointerButton::Primary);
            self.frame(at);
            self.s.h.input.pointer_leave();
            self.run(at + STEP, at + Duration::from_millis(1500)) + STEP
        }
    }

    /// The row of an opening or an app close: `want_open` is what the app sets at grid time zero.
    fn app_row(row: &str, detents: &'static [Detent], want_open: bool, target: f32) {
        let make = move || -> Sampler<'static> {
            let (mut rig, origin) = if want_open {
                let mut rig = Rig::new(detents);
                rig.frame(Time::ZERO);
                (rig, STEP)
            } else {
                Rig::opened(detents)
            };
            Box::new(move |t| {
                rig.open = want_open;
                let (top, moving) = rig.frame(origin + t);
                (top.unwrap_or(CLOSED), moving)
            })
        };
        assert_conforms(row, END, target, make);
    }

    #[test]
    fn opening_to_the_first_detent_conforms_at_60_and_120() {
        app_row("sheet open", BOTH, true, CLOSED - 0.5 * 802.0);
    }

    #[test]
    fn an_app_close_conforms_at_60_and_120() {
        app_row("sheet app close", BOTH, false, CLOSED);
    }

    /// A finger down on the grabber of the sheet resting at `top`, moving by `dy` px every 16 ms for
    /// `moves` frames (a speed of `dy / 0.016` px/s); the next frame's time. The lift is the
    /// grid's first frame, with the last move: `lift` returns its events' position.
    fn drag(rig: &mut Rig, mut at: Time, top: f32, dy: f32, moves: u32) -> (Time, f32) {
        let x = 201.0;
        let mut y = top + 10.0;
        rig.s.h.input.touch_start(1, x, y);
        rig.frame(at);
        for _ in 1..moves {
            at += STEP;
            y += dy;
            rig.s.h.input.touch_move(1, x, y);
            rig.frame(at);
        }
        (at + STEP, y + dy)
    }

    /// The row of a release at about 1200 px/s: the finger drags down from the rest at `top`, and
    /// lifts with its last move at grid time zero.
    fn release_row(row: &str, detents: &'static [Detent], clicks: u32, moves: u32, target: f32) {
        let make = move || -> Sampler<'static> {
            let (mut rig, mut at) = Rig::opened(detents);
            let mut top = CLOSED - 150.0;
            if detents.len() == 3 {
                for _ in 0..clicks {
                    at = rig.step(at, top);
                    top = rig.frame(at).0.unwrap_or(top);
                }
            } else {
                top = CLOSED - 0.5 * 802.0;
            }
            let (origin, last) = drag(&mut rig, at, top, 19.2, moves);
            let mut first = true;
            Box::new(move |t| {
                if first {
                    first = false;
                    rig.s.h.input.touch_move(1, 201.0, last);
                    rig.s.h.input.touch_end(1, 201.0, last);
                }
                let (top, moving) = rig.frame(origin + t);
                (top.unwrap_or(CLOSED), moving)
            })
        };
        assert_conforms(row, END, target, make);
    }

    #[test]
    fn a_touch_release_from_between_the_detents_conforms_at_60_and_120() {
        // From the large detent of three, down 5 frames: a release at 1200 px/s from near 160 lands
        // on the lowest detent (the plan's two-detent setup would close: the coast of 1200 px/s
        // is 600 px, past every detent but the bottom one).
        release_row("sheet release", THREE, 2, 5, CLOSED - 150.0);
    }

    #[test]
    fn a_fling_release_to_closed_conforms_at_60_and_120() {
        release_row("sheet fling to closed", BOTH, 0, 5, CLOSED);
    }
}

// ---- the navigation stack (step 4, D372) ---------------------------------------------------

mod stack {
    use super::*;
    use lune_ui_core::access::{Node, Role};
    use lune_ui_core::input::Repaint;
    use lune_ui_core::paint::Rect;

    const PHONE: (f32, f32) = (402.0, 874.0);
    const W: f32 = 402.0;
    const STEP: Time = Duration::from_millis(16);
    const END: Time = Duration::from_millis(1800);

    /// A stack over the whole window in a real `Ui`, driven by app time; the app lowers its depth
    /// on the frame after `popped`, as an app does. It reads the top level's node rect left edge,
    /// `W` once the top has left (what its right edge is).
    struct Rig {
        s: UiSampler,
        depth: usize,
        pop: bool,
        /// The top level has been popped: what it was drawn at is gone, so it reads `W`.
        gone: bool,
    }

    impl Rig {
        fn new(depth: usize) -> Rig {
            Rig {
                s: UiSampler::new(PHONE, Time::ZERO),
                depth,
                pop: false,
                gone: false,
            }
        }

        /// One frame at app time `at`: the top level's left edge and whether another is wanted.
        fn frame(&mut self, at: Time) -> (f32, bool) {
            self.s.h.input.set_time(at);
            let (depth, pop) = (self.depth, self.pop);
            self.pop = false;
            let mut popped = false;
            let (_, out) = self.s.h.frame(|ui| {
                let r = ui.nav_stack(
                    "stack",
                    Rect::new(0.0, 0.0, PHONE.0, PHONE.1),
                    depth,
                    |ui, level, r| {
                        let t = *ui.theme();
                        ui.painter().fill(r, 0.0, t.surface_overlay);
                        ui.node(Node::new(
                            WidgetId::ROOT.with(("page", level)),
                            Role::Group,
                            "level",
                            r,
                        ));
                    },
                );
                popped = r.popped;
                if pop {
                    ui.nav_stack_pop("stack");
                }
            });
            if popped {
                self.depth -= 1;
                self.gone = true;
            }
            let top = out
                .tree
                .get(WidgetId::ROOT.with(("page", depth - 1)))
                .map(|n| n.bounds.x);
            let x = if self.gone { W } else { top.unwrap_or(W) };
            (x, out.repaint != Repaint::Idle)
        }

        fn run(&mut self, from: Time, until: Time) -> Time {
            let mut t = from;
            while t <= until {
                self.frame(t);
                t += STEP;
            }
            t
        }
    }

    #[test]
    fn a_push_conforms_at_60_and_120() {
        let make = || -> Sampler<'static> {
            let mut rig = Rig::new(1);
            let origin = rig.run(Time::ZERO, Duration::from_millis(100));
            Box::new(move |t| {
                rig.depth = 2;
                rig.frame(origin + t)
            })
        };
        assert_conforms("stack push", END, 0.0, make);
    }

    #[test]
    fn a_queued_pop_conforms_at_60_and_120() {
        let make = || -> Sampler<'static> {
            let mut rig = Rig::new(2);
            let at = rig.run(Time::ZERO, Duration::from_millis(100));
            // The request is taken by the stack's next call, which is the grid's first frame.
            rig.pop = true;
            rig.frame(at);
            let origin = at + STEP;
            Box::new(move |t| rig.frame(origin + t))
        };
        assert_conforms("stack pop", END, W, make);
    }

    /// A finger down in the edge band at 10 px, moving right by `dx` every 16 ms for `moves`
    /// frames; the last move and the lift are the grid's first frame.
    fn swipe_row(row: &str, dx: f32, moves: u32, target: f32) {
        let make = move || -> Sampler<'static> {
            let mut rig = Rig::new(2);
            let mut at = rig.run(Time::ZERO, Duration::from_millis(100));
            let y = 400.0;
            rig.s.h.input.touch_start(1, 10.0, y);
            rig.frame(at);
            for i in 1..moves {
                at += STEP;
                rig.s.h.input.touch_move(1, 10.0 + dx * i as f32, y);
                rig.frame(at);
            }
            let last = 10.0 + dx * moves as f32;
            let origin = at + STEP;
            let mut first = true;
            Box::new(move |t| {
                if first {
                    first = false;
                    rig.s.h.input.touch_move(1, last, y);
                    rig.s.h.input.touch_end(1, last, y);
                }
                rig.frame(origin + t)
            })
        };
        assert_conforms(row, END, target, make);
    }

    #[test]
    fn a_swipe_released_past_the_middle_conforms_at_60_and_120() {
        // 216 px of travel (206 after the slop) at 1500 px/s.
        swipe_row("stack swipe completing", 24.0, 9, W);
    }

    #[test]
    fn a_swipe_released_short_springs_back_and_conforms_at_60_and_120() {
        // 48 px of travel at 250 px/s: the coast does not reach the middle.
        swipe_row("stack swipe springing back", 4.0, 12, 0.0);
    }
}
