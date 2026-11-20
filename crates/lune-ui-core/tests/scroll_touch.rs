//! A finger on the scroll area (mobile M3 step 5, R-core-12, D350): the pan follows it 1:1, a lift
//! flings on the closed-form decay, the ends rubber-band and spring back, a touch on a moving area
//! catches it, and every other way the offset moves stops a fling first. Scripted time and
//! touches through `UiHarness`; nothing here reads a clock.

use std::time::Duration;

use lune_ui_core::input::Repaint;
use lune_ui_core::motion::{DampedSpring, Decay, Time, conformance_of_at, rubber_band};
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::UiHarness;
use lune_ui_core::ui::{Response, Sense, Ui};
use lune_ui_core::widgets::scroll_area::{Extent, Overscroll, ScrollOptions, ScrollPhysics};
use lune_ui_core::widgets::virtual_list::{Heights, ListOptions, RowCtx, RowKey, SelectionMode};

const WIN: (f32, f32) = (400.0, 800.0);
const AREA: Rect = Rect::new(0.0, 0.0, 400.0, 800.0);
const CONTENT: f64 = 4000.0;
/// The offset range's end: 4000 over an 800 viewport.
const MAX: f64 = 3200.0;

/// What one frame of the area showed.
struct Seen {
    /// `ScrollResponse::offset`, vertical.
    off: f64,
    /// The overscroll shown, read off a child laid at a fixed place in the area: positive past
    /// the end, negative above the start.
    over: f64,
    /// A child at content y 500: where it is on screen.
    fixed_y: f32,
    /// A child that follows the offset, 400 px below the top of the viewport.
    track: Response,
    repaint: Repaint,
    /// The release velocity the area's region read.
    vel: (f32, f32),
}

fn frame(h: &mut UiHarness, physics: Option<ScrollPhysics>, ext: f64) -> Seen {
    let mut got = None;
    let (_, out) = h.frame(|ui| {
        let run = |ui: &mut Ui<'_>| {
            let res = ui.scroll_area("a", AREA, &ScrollOptions::default(), |ui, view| {
                let fixed = ui.id("fixed");
                let fixed = ui.interact(fixed, Rect::new(0.0, 500.0, 400.0, 40.0), Sense::CLICK);
                let track = ui.id("track");
                let y = 400.0 + view.offset.1 as f32;
                let track = ui.interact(track, Rect::new(0.0, y, 400.0, 40.0), Sense::CLICK);
                ((fixed, track), Extent::new(400.0, ext))
            });
            let id = ui.id("a");
            (res, ui.release_velocity(id))
        };
        got = Some(match physics {
            Some(p) => ui.with_scroll_physics(p, run),
            None => run(ui),
        });
    });
    let (res, vel) = got.expect("the frame ran");
    let (fixed, track) = res.value;
    Seen {
        off: res.offset.1,
        over: 400.0 - f64::from(track.rect.y),
        fixed_y: fixed.rect.y,
        track,
        repaint: out.repaint,
        vel,
    }
}

/// A harness at `hz` frames per second, one area of `CONTENT` px, the frame number kept.
struct Rig {
    h: UiHarness,
    hz: u64,
    k: u64,
    physics: Option<ScrollPhysics>,
}

impl Rig {
    fn new(hz: u64) -> Self {
        Self::with(hz, None)
    }

    fn with(hz: u64, physics: Option<ScrollPhysics>) -> Self {
        let mut r = Self {
            h: UiHarness::new(WIN),
            hz,
            k: 0,
            physics,
        };
        r.frame_at(0);
        r
    }

    /// The app time of frame `k`, in microseconds.
    fn us(&self) -> u64 {
        self.k * 1_000_000 / self.hz
    }

    /// The app time of the frame after this one.
    fn next_us(&self) -> u64 {
        (self.k + 1) * 1_000_000 / self.hz
    }

    fn frame_at(&mut self, micros: u64) -> Seen {
        self.h.input.set_time(Duration::from_micros(micros));
        frame(&mut self.h, self.physics, CONTENT)
    }

    /// The next frame on the grid.
    fn tick(&mut self) -> Seen {
        self.k += 1;
        self.frame_at(self.us())
    }

    /// Frames up to and including the one at `ms` (a multiple of the frame time).
    fn run_to_ms(&mut self, ms: u64) {
        while self.us() < ms * 1000 {
            self.tick();
        }
    }
}

fn near(got: f64, want: f64, tol: f64, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: {got} is not within {tol} of {want}"
    );
}

/// A finger that goes down at 50 ms and moves up the window at 2000 px/s until `end_ms`, where it
/// lifts. Returns the lift frame and its time in microseconds.
fn pan_up(r: &mut Rig, end_ms: u64) -> (Seen, u64) {
    r.run_to_ms(50);
    let y = |us: u64| 780.0 - 2000.0 * (us - 50_000) as f32 / 1e6;
    r.h.input.touch_start(1, 200.0, y(50_000));
    r.tick();
    while r.next_us() < end_ms * 1000 {
        let t = r.next_us();
        r.h.input.touch_move(1, 200.0, y(t));
        r.tick();
    }
    let t = r.next_us();
    assert_eq!(t, end_ms * 1000, "the lift lands on the grid");
    r.h.input.touch_end(1, 200.0, y(t));
    (r.tick(), t)
}

#[test]
fn a_pan_follows_the_finger_one_to_one_less_the_slop() {
    let mut r = Rig::new(60);
    r.h.input.touch_start(1, 200.0, 700.0);
    r.tick();
    let mut s = None;
    for i in 1..=5 {
        r.h.input.touch_move(1, 200.0, 700.0 - 60.0 * i as f32);
        s = Some(r.tick());
    }
    let s = s.expect("moves");
    near(s.off, 290.0, 0.01, "300 px of travel less the 10 px slop");
    near(
        f64::from(s.fixed_y),
        500.0 - 290.0,
        0.01,
        "the content rect moves with it",
    );
    assert_eq!(s.over, 0.0);
}

#[test]
fn a_fling_at_60_and_at_120_hz_lands_where_the_decay_says_at_every_shared_time() {
    let mut at = Vec::new();
    for hz in [60, 120] {
        let mut r = Rig::new(hz);
        let (lift, t_end) = pan_up(&mut r, 300);
        let v = -lift.vel.1;
        near(f64::from(v), 2000.0, 1.0, "the release velocity");
        let released = lift.off;
        let mut samples = Vec::new();
        for after_ms in [100, 400, 1500] {
            r.run_to_ms(t_end / 1000 + after_ms);
            samples.push(r.frame_at(r.us()).off);
        }
        // The landing: the release offset plus the decay's whole travel.
        let decay = Decay::NORMAL;
        let landing = released + f64::from(decay.offset(v, decay.duration(v)));
        r.run_to_ms(t_end / 1000 + 4000);
        let last = r.frame_at(r.us());
        near(last.off, landing, 0.01, "the landing");
        assert_eq!(last.repaint, Repaint::Idle);
        at.push(samples);
    }
    for (a, b) in at[0].iter().zip(&at[1]) {
        near(*a, *b, 0.01, "60 Hz and 120 Hz agree");
    }
}

#[test]
fn the_area_asks_for_frames_while_a_fling_runs_and_idles_at_the_landing() {
    let mut r = Rig::new(60);
    let (lift, t_end) = pan_up(&mut r, 300);
    let decay = Decay::NORMAL;
    let v = -lift.vel.1;
    let end = t_end + decay.duration(v).as_micros() as u64;
    assert_eq!(
        lift.repaint,
        Repaint::Now,
        "the lift frame starts the fling"
    );
    let mut landed = false;
    while r.us() < end + 100_000 {
        let s = r.tick();
        if r.us() < end {
            assert_eq!(s.repaint, Repaint::Now, "in flight at {} us", r.us());
        } else if !landed {
            assert_eq!(
                s.repaint,
                Repaint::Idle,
                "the landing frame at {} us",
                r.us()
            );
            landed = true;
        } else {
            assert_eq!(s.repaint, Repaint::Idle);
        }
    }
    assert!(landed);
}

#[test]
fn a_pull_past_the_top_rubber_bands_and_springs_back_after_the_lift() {
    let mut r = Rig::new(60);
    r.h.input.touch_start(1, 200.0, 100.0);
    r.tick();
    r.h.input.touch_move(1, 200.0, 200.0);
    r.tick();
    r.h.input.touch_move(1, 200.0, 300.0);
    let held = r.tick();
    let shown = f64::from(rubber_band(-190.0, 800.0));
    near(held.over, shown, 0.01, "the overscroll while held");
    assert_eq!(held.off, 0.0);
    // Held still: the lift leaves no velocity.
    for _ in 0..4 {
        r.tick();
    }
    r.h.input.touch_end(1, 200.0, 300.0);
    let lift = r.tick();
    let t_lift = r.us();
    let over0 = lift.over;
    near(
        over0,
        shown,
        0.01,
        "the lift frame draws where the finger left it",
    );
    let duration = DampedSpring::BOUNCE.duration(over0 as f32, 0.0, 0.0);
    let end = t_lift + duration.as_micros() as u64;
    let mut prev = over0;
    let mut settled = false;
    while r.us() < end + 100_000 {
        let s = r.tick();
        assert!(s.off >= 0.0, "the offset never leaves its range");
        assert!(
            s.over <= 0.0 && s.over >= prev - 0.01,
            "the way back is monotone"
        );
        prev = s.over;
        if r.us() >= end {
            assert_eq!(s.over, 0.0, "back within the spring's duration");
            assert_eq!(s.repaint, Repaint::Idle);
            settled = true;
        }
    }
    assert!(settled);
}

/// Scrolls to `to` with the wheel, which jumps, then runs a frame.
fn wheel_to(r: &mut Rig, to: f64) {
    r.h.input.pointer_to(200.0, 300.0);
    r.h.input.scroll_pixels(0.0, -(to as f32));
    let s = r.tick();
    near(s.off, to, 0.01, "the wheel's jump");
}

/// A finger from the window's bottom, lifting at `v` px/s up after `frames` moves at `hz`.
fn flick(r: &mut Rig, v: f32, frames: u64) -> (Seen, u64) {
    let step = v / r.hz as f32;
    r.h.input.touch_start(1, 200.0, 700.0);
    r.tick();
    for i in 1..=frames {
        r.h.input.touch_move(1, 200.0, 700.0 - step * i as f32);
        r.tick();
    }
    r.h.input
        .touch_end(1, 200.0, 700.0 - step * (frames + 1) as f32);
    let lift = r.tick();
    (lift, r.us())
}

#[test]
fn a_fling_into_the_end_bounces_off_it_and_settles_exactly_at_the_max() {
    let mut r = Rig::new(120);
    wheel_to(&mut r, 2900.0);
    let (lift, t_lift) = flick(&mut r, 3000.0, 5);
    let v = -lift.vel.1;
    assert!(v > 2500.0, "a hard flick: {v}");
    let th = Decay::NORMAL
        .time_to(v, (MAX - lift.off) as f32)
        .expect("it reaches the end");
    let at_end = t_lift + th.as_micros() as u64 + 1;
    let mut peak = 0.0f64;
    while r.us() < t_lift + 4_000_000 {
        let s = r.tick();
        assert!(s.off <= MAX, "the offset is clamped");
        peak = peak.max(s.over);
        if r.us() > at_end + 1_000_000 / 120 {
            assert_eq!(s.off, MAX, "at the end from the crossing on");
        }
    }
    assert!(peak > 10.0, "it bounced: {peak}");
    let last = r.tick();
    assert_eq!((last.off, last.over), (MAX, 0.0));
    assert_eq!(last.repaint, Repaint::Idle);
}

#[test]
fn clamp_physics_stops_a_fling_exactly_at_the_end_and_shows_no_pull() {
    let clamp = ScrollPhysics::default().with_overscroll(Overscroll::Clamp);
    let mut r = Rig::with(120, Some(clamp));
    wheel_to(&mut r, 2900.0);
    let (lift, t_lift) = flick(&mut r, 3000.0, 5);
    let v = -lift.vel.1;
    let th = Decay::NORMAL
        .time_to(v, (MAX - lift.off) as f32)
        .expect("it reaches the end");
    let at_end = t_lift + th.as_micros() as u64 + 1;
    while r.us() < t_lift + 2_000_000 {
        let s = r.tick();
        assert_eq!(s.over, 0.0, "no overscroll at {} us", r.us());
        if r.us() < at_end {
            assert!(s.off < MAX);
            assert_eq!(s.repaint, Repaint::Now);
        } else {
            assert_eq!(s.off, MAX, "stopped exactly at the end by {} us", r.us());
        }
    }
    // The scroll bar's own fade is over by now: nothing asks for a frame.
    assert_eq!(r.tick().repaint, Repaint::Idle);

    // A pull past the start shows nothing under Clamp.
    let mut r = Rig::with(60, Some(clamp));
    r.h.input.touch_start(1, 200.0, 100.0);
    r.tick();
    r.h.input.touch_move(1, 200.0, 300.0);
    let s = r.tick();
    assert_eq!((s.off, s.over), (0.0, 0.0));
}

#[test]
fn a_wheel_step_mid_fling_stops_it_and_steps_from_where_it_was() {
    let mut r = Rig::new(60);
    let (lift, t_end) = pan_up(&mut r, 300);
    let (v, from) = (-lift.vel.1, lift.off);
    for _ in 0..5 {
        r.tick();
    }
    r.h.input.pointer_to(200.0, 300.0);
    r.h.input.scroll_lines(0.0, -1.0);
    r.tick();
    let t = Time::from_micros(r.us() - t_end);
    let stopped = from + f64::from(Decay::NORMAL.offset(v, t));
    // The step settles at the stopped offset plus a line; the fling does not run on.
    for _ in 0..90 {
        r.tick();
    }
    let s = r.tick();
    near(
        s.off,
        stopped + 48.0,
        0.01,
        "the wheel step from the stopped offset",
    );
    assert_eq!(s.repaint, Repaint::Idle);
}

#[test]
fn a_tap_mid_fling_catches_it_and_clicks_nothing() {
    let mut r = Rig::new(120);
    let (_, _) = pan_up(&mut r, 300);
    let mut drawn = 0.0;
    for _ in 0..6 {
        drawn = r.tick().off;
    }
    r.h.input.touch_start(2, 200.0, 420.0);
    let down = r.tick();
    assert_eq!(down.off, drawn, "stopped where the last frame drew it");
    assert!(!down.track.press_started && !down.track.pressed);
    r.h.input.touch_end(2, 200.0, 420.0);
    let up = r.tick();
    assert!(!up.track.clicked && !up.track.pressed);
    for _ in 0..240 {
        assert_eq!(r.tick().off, drawn, "it stays");
    }
    assert_eq!(r.tick().repaint, Repaint::Idle);
}

#[test]
fn a_cancelled_pan_rests_where_it_was_with_no_fling() {
    let mut r = Rig::new(60);
    r.h.input.touch_start(1, 200.0, 700.0);
    r.tick();
    for i in 1..=4 {
        r.h.input.touch_move(1, 200.0, 700.0 - 50.0 * i as f32);
        r.tick();
    }
    r.h.input.touch_cancel(1, 200.0, 500.0);
    let cut = r.tick();
    assert_eq!(cut.vel, (0.0, 0.0));
    for _ in 0..120 {
        assert_eq!(r.tick().off, cut.off);
    }
    assert_eq!(r.tick().repaint, Repaint::Idle);
}

#[test]
fn a_pan_on_content_that_fits_goes_to_the_area_around_it() {
    let opts = ScrollOptions::default();
    let mut h = UiHarness::new(WIN);
    let run = |h: &mut UiHarness, t: u64| {
        h.input.set_time(Duration::from_micros(t));
        let mut got = None;
        h.frame(|ui| {
            let outer = ui.scroll_area("a", AREA, &opts, |ui, _| {
                let inner = ui.scroll_area(
                    "inner",
                    Rect::new(0.0, 100.0, 400.0, 400.0),
                    &opts,
                    |_, _| ((), Extent::new(400.0, 200.0)),
                );
                (inner.offset, Extent::new(400.0, CONTENT))
            });
            got = Some((outer.offset, outer.value));
        });
        got.expect("the frame ran")
    };
    run(&mut h, 0);
    h.input.touch_start(1, 200.0, 300.0);
    run(&mut h, 16_667);
    let mut seen = ((0.0, 0.0), (0.0, 0.0));
    for i in 1..=3 {
        h.input.touch_move(1, 200.0, 300.0 - 60.0 * i as f32);
        seen = run(&mut h, 16_667 * (i + 1));
    }
    near(seen.0.1, 170.0, 0.01, "the outer area took the pan");
    assert_eq!(seen.1, (0.0, 0.0), "the inner one had nothing to scroll");
}

#[test]
fn a_virtual_list_of_ten_thousand_rows_flings_and_lands_with_its_rows_in_view() {
    const LIST: Rect = Rect::new(20.0, 20.0, 300.0, 400.0);
    const ROW: f64 = 45.0;
    let opts = ListOptions {
        count: 10_000,
        heights: Heights::Fixed(ROW as f32),
        selection: SelectionMode::Multi,
        index_of: None,
        label: "rows",
    };
    let mut h = UiHarness::new((400.0, 600.0));
    let run = |h: &mut UiHarness, us: u64| {
        h.input.set_time(Duration::from_micros(us));
        let (mut off, mut rows) = (0.0, Vec::new());
        let (_, out) = h.frame(|ui| {
            let r = ui.virtual_list(
                "l",
                LIST,
                &opts,
                |i| RowKey(i as u64),
                |_, c: &RowCtx| {
                    rows.push(c.index);
                    ROW as f32
                },
            );
            off = r.offset;
        });
        (off, rows, out.repaint)
    };
    let step = 1_000_000 / 120;
    run(&mut h, 0);
    h.input.touch_start(1, 100.0, 380.0);
    run(&mut h, step);
    // 2000 px/s: 16.67 px a frame, for 20 frames.
    let y = |i: u64| 380.0 - 2000.0 / 120.0 * i as f32;
    for i in 1..=20 {
        h.input.touch_move(1, 100.0, y(i));
        run(&mut h, step * (i + 1));
    }
    h.input.touch_end(1, 100.0, y(21));
    let (released, ..) = run(&mut h, step * 22);
    let v = 2000.0;
    let landing = released + f64::from(Decay::NORMAL.offset(v, Decay::NORMAL.duration(v)));
    assert!(released > 200.0, "the pan moved the list: {released}");
    let mut prev = released;
    let mut last = (released, Repaint::Now);
    for i in 23..=23 + 120 * 4 {
        let (off, rows, repaint) = run(&mut h, step * i);
        assert!(off >= prev, "it only moves on: {prev} then {off}");
        let (first, lastrow) = ((off / ROW) as usize, ((off + 399.0) / ROW) as usize);
        assert!(
            rows.contains(&first) && rows.contains(&lastrow),
            "rows {first} to {lastrow} are built at {off}"
        );
        prev = off;
        last = (off, repaint);
    }
    near(last.0, landing, 3.0, "the landing");
    assert_eq!(last.1, Repaint::Idle);
}

#[test]
fn the_fling_conforms_at_60_and_120_hz() {
    for rate in [60u32, 120] {
        let mut r = Rig::new(u64::from(rate));
        let (lift, t_end) = pan_up(&mut r, 300);
        let v = -lift.vel.1;
        let decay = Decay::NORMAL;
        let landing = (lift.off + f64::from(decay.offset(v, decay.duration(v)))) as f32;
        let bound = v.abs() / rate as f32 + 0.5;
        let result = conformance_of_at(
            rate,
            |t| {
                let s = r.frame_at(t_end + t.as_micros() as u64);
                (s.off as f32, s.repaint != Repaint::Idle)
            },
            decay.duration(v),
            landing,
            bound,
        );
        assert!(result.is_ok(), "{rate} Hz: {result:?}");
    }
}

/// Two areas side by side in one frame: `a` in the left half under `physics`, `b` in the right
/// half outside any scope. Returns each one's overscroll.
fn two(h: &mut UiHarness, physics: ScrollPhysics) -> (f64, f64) {
    let (mut over_a, mut over_b) = (0.0, 0.0);
    h.frame(|ui| {
        let one = |ui: &mut Ui<'_>, key: &str, x: f32| {
            let area = Rect::new(x, 0.0, 200.0, 800.0);
            let res = ui.scroll_area(key, area, &ScrollOptions::default(), |ui, view| {
                let id = ui.id((key, "t"));
                let y = 400.0 + view.offset.1 as f32;
                let t = ui.interact(id, Rect::new(x, y, 200.0, 40.0), Sense::CLICK);
                (t.rect.y, Extent::new(200.0, CONTENT))
            });
            400.0 - f64::from(res.value)
        };
        over_a = ui.with_scroll_physics(physics, |ui| one(ui, "a", 0.0));
        over_b = one(ui, "b", 200.0);
    });
    (over_a, over_b)
}

#[test]
fn with_scroll_physics_governs_the_areas_inside_it_only() {
    let clamp = ScrollPhysics::default().with_overscroll(Overscroll::Clamp);
    let mut h = UiHarness::new(WIN);
    two(&mut h, clamp);
    // A pull on the left area, which runs under `Clamp`: nothing shows, here or beside it.
    h.input.set_time(Duration::from_millis(100));
    h.input.touch_start(1, 100.0, 100.0);
    two(&mut h, clamp);
    h.input.set_time(Duration::from_millis(116));
    h.input.touch_move(1, 100.0, 300.0);
    assert_eq!(two(&mut h, clamp), (0.0, 0.0));
    h.input.set_time(Duration::from_millis(500));
    h.input.touch_end(1, 100.0, 300.0);
    two(&mut h, clamp);
    // The same pull on the right one, outside the scope: it keeps the default.
    h.input.set_time(Duration::from_millis(600));
    h.input.touch_start(2, 300.0, 100.0);
    two(&mut h, clamp);
    h.input.set_time(Duration::from_millis(616));
    h.input.touch_move(2, 300.0, 300.0);
    let (inside, outside) = two(&mut h, clamp);
    assert_eq!(inside, 0.0);
    near(
        outside,
        f64::from(rubber_band(-190.0, 800.0)),
        0.01,
        "outside the scope",
    );
}

#[test]
fn the_physics_builders_set_one_field_each() {
    let p = ScrollPhysics::default();
    assert_eq!(p.decay, Decay::NORMAL);
    assert_eq!(p.overscroll, Overscroll::RubberBand);
    assert_eq!(p.spring, DampedSpring::BOUNCE);
    let q = p
        .with_decay(Decay::FAST)
        .with_overscroll(Overscroll::Clamp)
        .with_spring(DampedSpring::new(Duration::from_millis(200), 0.8));
    assert_eq!(q.decay, Decay::FAST);
    assert_eq!(q.overscroll, Overscroll::Clamp);
    assert_eq!(q.spring.damping, 0.8);
}

/// One frame of a plain area at `at` microseconds with `identity`, after `before`. Returns the
/// offset and the repaint.
fn plain(
    h: &mut UiHarness,
    at: u64,
    identity: Option<u64>,
    before: impl FnOnce(&mut Ui<'_>),
) -> (f64, Repaint) {
    h.input.set_time(Duration::from_micros(at));
    let mut off = 0.0;
    let opts = ScrollOptions {
        identity,
        ..ScrollOptions::default()
    };
    let (_, out) = h.frame(|ui| {
        before(ui);
        let r = ui.scroll_area("a", AREA, &opts, |_, _| ((), Extent::new(400.0, CONTENT)));
        off = r.offset.1;
    });
    (off, out.repaint)
}

/// A fling in flight on a plain area: the harness, the last frame's time and its offset.
fn flinging() -> (UiHarness, u64, f64) {
    let mut h = UiHarness::new(WIN);
    let step = 1_000_000 / 60;
    plain(&mut h, 0, None, |_| {});
    h.input.touch_start(1, 200.0, 780.0);
    plain(&mut h, step, None, |_| {});
    for i in 1..=8u64 {
        h.input.touch_move(1, 200.0, 780.0 - 33.0 * i as f32);
        plain(&mut h, step * (i + 1), None, |_| {});
    }
    h.input.touch_end(1, 200.0, 780.0 - 33.0 * 9.0);
    let (off, _) = plain(&mut h, step * 10, None, |_| {});
    let (off2, repaint) = plain(&mut h, step * 11, None, |_| {});
    assert!(off2 > off, "it is flinging");
    assert_eq!(repaint, Repaint::Now);
    (h, step * 11, off2)
}

#[test]
fn a_scroll_to_mid_fling_stops_it_and_goes_to_its_own_target() {
    use lune_ui_core::widgets::scroll_area::{Reveal, Target};
    let (mut h, t, _) = flinging();
    let step = 1_000_000 / 60;
    let (mut off, _) = plain(&mut h, t + step, None, |ui| {
        ui.scroll_to("a", Target::span_y(0.0, 40.0), Reveal::Nearest);
    });
    for i in 2..200 {
        off = plain(&mut h, t + step * i, None, |_| {}).0;
    }
    assert_eq!(
        off, 0.0,
        "the reveal took the area to the top; the fling did not run on"
    );
}

/// The offset a `flinging` area shows one frame on, and the one it lands at.
fn shown_and_landing() -> (f64, f64) {
    let (mut h, t, _) = flinging();
    let step = 1_000_000 / 60;
    let shown = plain(&mut h, t + step, None, |_| {}).0;
    let mut landing = shown;
    for i in 2..300 {
        landing = plain(&mut h, t + step * i, None, |_| {}).0;
    }
    (shown, landing)
}

/// A reveal during a fling is judged against the offset on screen, not the landing: a span in view
/// now keeps the area where it is, and a span only in view at the landing still stops the fling.
#[test]
fn a_reveal_mid_fling_is_judged_against_the_shown_offset_not_the_landing() {
    use lune_ui_core::widgets::scroll_area::{Reveal, Target};
    let (shown, landing) = shown_and_landing();
    assert!(
        landing - shown > 700.0,
        "the fling has far to go: {shown} to {landing}"
    );
    let step = 1_000_000 / 60;
    let run = |y: f64| {
        let (mut h, t, _) = flinging();
        let mut off = 0.0;
        for i in 1..200 {
            off = plain(&mut h, t + step * i, None, |ui| {
                if i == 1 {
                    ui.scroll_to("a", Target::span_y(y, 40.0), Reveal::Nearest);
                }
            })
            .0;
            if i == 1 {
                assert!(
                    (off - shown).abs() < 1e-6,
                    "the first frame shows {off}, not {shown}"
                );
            }
        }
        off
    };
    // (A) in view now, above the landing's top: the area stays where it showed.
    assert_eq!(
        run(shown + 100.0),
        shown,
        "a span in view moves nothing and the fling stops"
    );
    // (B) out of view now, in view at the landing: the reveal runs from the shown offset.
    let to = run(landing + 100.0);
    assert!(
        to > shown && to < landing - 1.0,
        "the reveal took the area to {to}, not on to {landing}"
    );
}

/// Under `Clamp` a pull past an end leaves no dead zone: the finger reversing moves the content
/// at once.
#[test]
fn clamp_physics_leaves_no_dead_zone_after_an_overpull() {
    let clamp = ScrollPhysics::default().with_overscroll(Overscroll::Clamp);
    let mut r = Rig::with(60, Some(clamp));
    wheel_to(&mut r, 50.0);
    r.h.input.touch_start(1, 200.0, 100.0);
    r.tick();
    r.h.input.touch_move(1, 200.0, 300.0);
    let pulled = r.tick();
    assert_eq!((pulled.off, pulled.over), (0.0, 0.0));
    // 100 px back up the window: the content is 100 px on from the top at once.
    r.h.input.touch_move(1, 200.0, 200.0);
    let back = r.tick();
    near(
        back.off,
        100.0,
        0.01,
        "the content follows the reversed finger",
    );
}

#[test]
fn a_new_identity_mid_fling_starts_over_at_the_top_and_goes_idle() {
    let (mut h, t, _) = flinging();
    let step = 1_000_000 / 60;
    let (off, _) = plain(&mut h, t + step, Some(9), |_| {});
    assert_eq!(off, 0.0);
    let mut last = (0.0, Repaint::Now);
    for i in 2..200 {
        last = plain(&mut h, t + step * i, Some(9), |_| {});
    }
    assert_eq!(last, (0.0, Repaint::Idle));
}
