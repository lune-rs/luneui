//! The showcase tour, headless: the shell's own per-frame function (`Demo::frame`) driven in app
//! time with no window and no GPU, under the frame schedules a real run can have.

use std::time::Duration;

use demo::dump::{SAFE, WINDOW};
use demo::showcase::{Headless, MARKS, Play, Rendered, script};
use lune_ui::input::Repaint;
use lune_ui::layout::Insets;

/// One frame at 60 Hz, in microseconds.
const HZ60: u64 = 16_667;

/// How long a run may last in app time before the test calls it stuck.
const LIMIT: Duration = Duration::from_secs(300);

/// What a schedule says about the next frame: its app time, or `None` when the app asked for none
/// (and the tour must be over by then).
type Next = Box<dyn FnMut(Duration, &Rendered) -> Option<Duration>>;

fn fixed(period: Duration) -> Next {
    Box::new(move |t, _| Some(t + period))
}

/// Frames 8 to 33 ms apart in a fixed pattern, and a 100 ms hitch about every second.
fn jittered() -> Next {
    const PATTERN: [u64; 12] = [8, 17, 33, 12, 25, 16, 9, 30, 16, 21, 14, 27];
    let mut i = 0usize;
    let mut since_hitch = Duration::ZERO;
    Box::new(move |t, _| {
        let mut dt = Duration::from_millis(PATTERN[i % PATTERN.len()]);
        i += 1;
        since_hitch += dt;
        if since_hitch >= Duration::from_secs(1) {
            since_hitch = Duration::ZERO;
            dt = Duration::from_millis(100);
        }
        Some(t + dt)
    })
}

/// The next frame only when the last one asked for it: `Now` a 60 Hz frame on, `At` the first 60 Hz
/// grid time at or after it.
fn repaint_driven() -> Next {
    let grid = Duration::from_micros(HZ60);
    Box::new(move |t, r| match r.out.repaint {
        Repaint::Now => Some(t + grid),
        Repaint::At(at) => {
            let cells = at.as_nanos().div_ceil(grid.as_nanos());
            let on_grid = Duration::from_nanos((cells * grid.as_nanos()) as u64);
            Some(on_grid.max(t + Duration::from_millis(1)))
        }
        Repaint::Idle => None,
    })
}

/// The marks a run passed, in order, with each check's result.
type Passed = Vec<(String, Result<(), String>)>;

/// Plays the tour once or `cycles` times over; returns every mark passed and how many restarts the
/// frames reported.
fn run(safe: Insets, play: Play, cycles: usize, mut next: Next) -> (Passed, usize) {
    let mut h = Headless::new(WINDOW, 3.0, safe);
    h.demo.play(play);
    let (mut passed, mut restarts, mut t) = (Passed::new(), 0, Duration::ZERO);
    loop {
        let r = h.step_to(t, Vec::new());
        restarts += usize::from(r.restarted);
        passed.extend(r.marks.iter().map(|m| (m.name.clone(), m.result.clone())));
        if (!play.repeat && r.finished) || (play.repeat && passed.len() >= cycles * MARKS.len()) {
            return (passed, restarts);
        }
        t = match next(t, &r) {
            Some(n) => n,
            None => panic!(
                "the app went idle at {t:?} with {} marks passed, before the tour ended",
                passed.len()
            ),
        };
        assert!(t < LIMIT, "the tour did not end by {LIMIT:?}");
    }
}

fn assert_tour(passed: &Passed, cycles: usize) {
    let names: Vec<&str> = passed.iter().map(|(n, _)| n.as_str()).collect();
    let want: Vec<&str> = (0..cycles).flat_map(|_| MARKS).collect();
    assert_eq!(names, want, "marks passed out of order");
    for (name, result) in passed {
        assert_eq!(result, &Ok(()), "mark {name}");
    }
}

fn layouts() -> [(&'static str, Insets); 2] {
    [("no insets", Insets::ZERO), ("phone insets", SAFE)]
}

fn once() -> Play {
    Play::default()
}

#[test]
fn the_script_runs_between_30_and_60_seconds() {
    for (name, safe) in layouts() {
        let layout = demo::showcase::Layout::new(WINDOW, safe);
        let len = script(&layout).length();
        assert!(
            (Duration::from_secs(30)..=Duration::from_secs(60)).contains(&len),
            "{name}: {len:?}"
        );
    }
}

#[test]
fn the_tour_passes_every_mark_at_60_hz() {
    for (name, safe) in layouts() {
        let (passed, restarts) = run(safe, once(), 1, fixed(Duration::from_micros(HZ60)));
        assert_tour(&passed, 1);
        assert_eq!(restarts, 0, "{name}");
    }
}

#[test]
fn the_tour_passes_every_mark_at_120_hz() {
    for (_, safe) in layouts() {
        let (passed, _) = run(safe, once(), 1, fixed(Duration::from_micros(8_333)));
        assert_tour(&passed, 1);
    }
}

#[test]
fn the_tour_passes_every_mark_with_uneven_frames_and_hitches() {
    for (_, safe) in layouts() {
        let (passed, _) = run(safe, once(), 1, jittered());
        assert_tour(&passed, 1);
    }
}

#[test]
fn the_tour_passes_every_mark_drawing_only_when_asked_to() {
    for (_, safe) in layouts() {
        let (passed, _) = run(safe, once(), 1, repaint_driven());
        assert_tour(&passed, 1);
    }
}

#[test]
fn a_repeating_tour_passes_every_mark_twice_and_restarts_once() {
    for (_, safe) in layouts() {
        let play = Play {
            repeat: true,
            touches: false,
        };
        let (passed, restarts) = run(safe, play, 2, fixed(Duration::from_micros(HZ60)));
        assert_tour(&passed, 2);
        assert_eq!(restarts, 1);
    }
}
