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
fn run(layout: Screen, play: Play, cycles: usize, mut next: Next) -> (Passed, usize) {
    let mut h = Headless::new(layout.window, layout.scale, layout.safe);
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

/// A window the tour runs in: its logical size, its scale and its safe area.
#[derive(Debug, Clone, Copy)]
struct Screen {
    window: (f32, f32),
    scale: f32,
    safe: Insets,
}

/// The Android emulator's pixel_7 as `scripts/android-emu.sh --probe` reads it (M8 step 0, reading
/// 4): a 1080x2400 window at scale 2.625 and the content rect `0,136,1080,2337`, so a status bar
/// inset of 136 px and a gesture bar inset of 63 px.
const EMULATOR_SCALE: f32 = 2.625;
const EMULATOR: Screen = Screen {
    window: (1080.0 / EMULATOR_SCALE, 2400.0 / EMULATOR_SCALE),
    scale: EMULATOR_SCALE,
    safe: Insets::new(0.0, 136.0 / EMULATOR_SCALE, 0.0, 63.0 / EMULATOR_SCALE),
};

fn layouts() -> [(&'static str, Screen); 3] {
    let phone = |safe| Screen {
        window: WINDOW,
        scale: 3.0,
        safe,
    };
    [
        ("no insets", phone(Insets::ZERO)),
        ("phone insets", phone(SAFE)),
        ("android emulator", EMULATOR),
    ]
}

fn once() -> Play {
    Play::default()
}

#[test]
fn the_script_runs_between_30_and_60_seconds() {
    for (name, screen) in layouts() {
        let layout = demo::showcase::Layout::new(screen.window, screen.safe);
        let len = script(&layout).length();
        assert!(
            (Duration::from_secs(30)..=Duration::from_secs(60)).contains(&len),
            "{name}: {len:?}"
        );
    }
}

#[test]
fn the_tour_passes_every_mark_at_60_hz() {
    for (name, screen) in layouts() {
        let (passed, restarts) = run(screen, once(), 1, fixed(Duration::from_micros(HZ60)));
        assert_tour(&passed, 1);
        assert_eq!(restarts, 0, "{name}");
    }
}

#[test]
fn the_tour_passes_every_mark_at_120_hz() {
    for (_, screen) in layouts() {
        let (passed, _) = run(screen, once(), 1, fixed(Duration::from_micros(8_333)));
        assert_tour(&passed, 1);
    }
}

#[test]
fn the_tour_passes_every_mark_with_uneven_frames_and_hitches() {
    for (_, screen) in layouts() {
        let (passed, _) = run(screen, once(), 1, jittered());
        assert_tour(&passed, 1);
    }
}

#[test]
fn the_tour_passes_every_mark_drawing_only_when_asked_to() {
    for (_, screen) in layouts() {
        let (passed, _) = run(screen, once(), 1, repaint_driven());
        assert_tour(&passed, 1);
    }
}

#[test]
fn a_repeating_tour_passes_every_mark_twice_and_restarts_once() {
    for (_, screen) in layouts() {
        let play = Play {
            repeat: true,
            touches: false,
        };
        let (passed, restarts) = run(screen, play, 2, fixed(Duration::from_micros(HZ60)));
        assert_tour(&passed, 2);
        assert_eq!(restarts, 1);
    }
}

/// Frames a steady `ms` apart: a software renderer or an emulator draws no faster than this. Past
/// 40 ms the pointer's release speed reads zero (the swipes fling no more), which the tour has to
/// reach its marks without.
fn slow(ms: u64) -> Next {
    fixed(Duration::from_millis(ms))
}

#[test]
fn the_tour_passes_every_mark_with_frames_45_60_and_100_ms_apart() {
    for (name, screen) in layouts() {
        for ms in [45, 60, 100] {
            let (passed, restarts) = run(screen, once(), 1, slow(ms));
            assert_tour(&passed, 1);
            assert_eq!(restarts, 0, "{name} at {ms} ms");
        }
    }
}

#[test]
fn the_tour_passes_every_mark_with_frames_8_and_33_ms_apart() {
    for (_, screen) in layouts() {
        for ms in [8, 33] {
            let (passed, _) = run(screen, once(), 1, slow(ms));
            assert_tour(&passed, 1);
        }
    }
}

/// The gaps, in milliseconds, between the first 165 frames of the tour on the Android emulator
/// (`scripts/android-emu.sh --demo --demo-arg --probe`, SwiftShader, logcat timestamps of the
/// `presented` lines, M8 step 3): a median of 61 ms, a quarter of them past 70, and gaps up to 2.4 s.
/// Replayed in a loop, each gap at most 250 ms.
const EMULATOR_GAPS_MS: [u64; 164] = [
    106, 44, 97, 49, 60, 75, 55, 62, 71, 54, 97, 31, 76, 47, 62, 75, 47, 55, 62, 53, 57, 57, 58,
    66, 48, 55, 57, 61, 126, 40, 56, 59, 61, 57, 59, 62, 55, 58, 2355, 57, 638, 58, 65, 66, 55, 63,
    61, 66, 79, 2253, 58, 637, 50, 60, 57, 134, 50, 63, 110, 73, 36, 59, 59, 88, 46, 58, 56, 55,
    68, 47, 64, 49, 66, 47, 55, 53, 56, 54, 56, 61, 49, 54, 84, 32, 54, 57, 55, 60, 55, 57, 57, 55,
    55, 60, 61, 54, 854, 71, 623, 59, 62, 79, 52, 57, 63, 66, 74, 48, 60, 71, 50, 72, 573, 57, 642,
    55, 63, 55, 90, 23, 57, 749, 40, 649, 59, 59, 80, 154, 113, 142, 694, 18, 658, 40, 71, 180, 66,
    70, 82, 773, 84, 612, 81, 104, 93, 73, 95, 107, 120, 147, 80, 107, 578, 107, 591, 104, 84, 63,
    67, 64, 74, 61, 68, 39,
];

fn emulator_frames() -> Next {
    let mut i = 0usize;
    Box::new(move |t, _| {
        // Gaps past 250 ms were the app idle until its next scheduled frame (the first frame's
        // shader work among them), not drawing slowly: a frame the app asked for comes when asked.
        let gap = EMULATOR_GAPS_MS[i % EMULATOR_GAPS_MS.len()].min(250);
        i += 1;
        Some(t + Duration::from_millis(gap))
    })
}

#[test]
fn the_tour_passes_every_mark_with_the_frame_gaps_the_android_emulator_showed() {
    for (_, screen) in layouts() {
        let (passed, _) = run(screen, once(), 1, emulator_frames());
        assert_tour(&passed, 1);
    }
}
