//! The demo's screens, headless: no window, no GPU, app time only.

use std::time::Duration;

use demo::dump::{SAFE, STATES, WINDOW};
use demo::showcase::{Headless, Showcase, Tab};
use lune_ui::input::Repaint;
use lune_ui::layout::Insets;
use lune_ui::playback::{Player, Script};
use lune_ui::scene::Scene;

const FRAME: Duration = Duration::from_micros(16_667);

/// Plays `script` for `frames` frames of 60 Hz app time.
fn play(run: &mut Headless, script: Script, frames: u32) {
    let mut player = Player::new(script);
    for _ in 0..frames {
        let mut events = Vec::new();
        player.advance(run.time + FRAME, &mut events);
        run.step(FRAME, events);
    }
}

fn phone() -> Headless {
    Headless::new(WINDOW, 3.0, SAFE)
}

/// The Android emulator's pixel_7 as `scripts/android-emu.sh --probe` reads it (M8 step 0, reading
/// 4): a 1080x2400 window at scale 2.625, the content rect `0,283,1080,2337`.
const EMULATOR_SCALE: f32 = 2.625;
const EMULATOR_WINDOW: (f32, f32) = (1080.0 / EMULATOR_SCALE, 2400.0 / EMULATOR_SCALE);
const EMULATOR_SAFE: Insets = Insets::new(0.0, 283.0 / EMULATOR_SCALE, 0.0, 63.0 / EMULATOR_SCALE);

#[test]
fn the_five_states_lay_out_with_and_without_insets() {
    for (window, scale, insets) in [
        (WINDOW, 3.0, Insets::ZERO),
        (WINDOW, 3.0, SAFE),
        (EMULATOR_WINDOW, EMULATOR_SCALE, EMULATOR_SAFE),
    ] {
        for (name, tab, depth, sheet) in STATES {
            let mut run = Headless::new(window, scale, insets);
            run.demo.showcase = Showcase::with_state(tab, depth, sheet);
            let last = run.run(40, FRAME).expect("a frame");
            assert!(last.scene != Scene::default(), "{name}: nothing drawn");
        }
    }
}

#[test]
fn a_tap_on_the_cards_item_selects_cards() {
    let mut run = phone();
    run.run(10, FRAME);
    let at = run.layout().tab_item_center(1);
    let mut script = Script::new();
    script.wait(Duration::from_millis(100)).tap(at);
    play(&mut run, script, 30);
    assert_eq!(run.demo.showcase.tab, Tab::Cards);
}

#[test]
fn a_tap_on_a_row_pushes_its_detail() {
    let mut run = phone();
    run.run(10, FRAME);
    let at = run.layout().row_center(2, 0.0);
    let mut script = Script::new();
    script.wait(Duration::from_millis(100)).tap(at);
    play(&mut run, script, 30);
    let s = &run.demo.showcase;
    assert_eq!(s.depth, 2);
    assert_eq!(s.selected, Some(2));
    assert_eq!(s.rows[2].title, "Alan Turing");
}

#[test]
fn an_idle_app_asks_for_no_frame() {
    for (name, tab, depth, sheet) in STATES {
        let mut run = phone();
        run.demo.showcase = Showcase::with_state(tab, depth, sheet);
        let last = run.run(180, FRAME).expect("a frame");
        assert_eq!(last.out.repaint, Repaint::Idle, "{name}");
    }
}

#[test]
fn a_tap_on_the_cards_item_selects_cards_on_the_emulator_layout() {
    let mut run = Headless::new(EMULATOR_WINDOW, EMULATOR_SCALE, EMULATOR_SAFE);
    run.run(10, FRAME);
    let at = run.layout().tab_item_center(1);
    let mut script = Script::new();
    script.wait(Duration::from_millis(100)).tap(at);
    play(&mut run, script, 30);
    assert_eq!(run.demo.showcase.tab, Tab::Cards);
}
