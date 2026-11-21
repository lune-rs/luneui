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
