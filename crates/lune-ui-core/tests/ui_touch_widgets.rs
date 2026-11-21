//! The core rules of touch under widgets (mobile M5 step 1, D361): a touch press picks its widget
//! with a 44 x 44 target around a small control, focus moves at the tap, the long press or the drag
//! start rather than at the press, and a drag that grows out of a long press dismisses the menu
//! that long press opened. Scripted time and touches through `UiHarness`; nothing here reads a
//! clock.

use lune_ui_core::input::PointerButton;
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::{UiHarness, hit_regions};
use lune_ui_core::ui::{Response, Sense, Ui};
use lune_ui_core::widgets::menu::MenuRow;
use lune_ui_core::widgets::scroll_area::{Extent, ScrollOptions};

const P: PointerButton = PointerButton::Primary;
const WIN: (f32, f32) = (400.0, 300.0);

fn harness() -> UiHarness {
    UiHarness::new(WIN)
}

/// One frame of `keys` laid out at `rects` with `sense`, in call order: their responses.
fn frame_of(h: &mut UiHarness, regions: &[(&str, Rect, Sense)]) -> Vec<Response> {
    let mut out = Vec::new();
    h.frame(|ui| {
        for (key, rect, sense) in regions {
            let id = ui.id(key);
            out.push(ui.interact(id, *rect, *sense));
        }
    });
    out
}

/// A harness that has drawn `regions` once, so the next frame's input has regions to hit.
fn ready(regions: &[(&str, Rect, Sense)]) -> UiHarness {
    let mut h = harness();
    frame_of(&mut h, regions);
    h
}

/// A touch tap at `at` over `regions`: the press frame and the lift frame.
fn tap(
    h: &mut UiHarness,
    at: (f32, f32),
    regions: &[(&str, Rect, Sense)],
) -> (Vec<Response>, Vec<Response>) {
    h.input.touch_start(1, at.0, at.1);
    let press = frame_of(h, regions);
    h.input.touch_end(1, at.0, at.1);
    (press, frame_of(h, regions))
}

/// A mouse click at `at` over `regions`: the press frame and the release frame.
fn click(
    h: &mut UiHarness,
    at: (f32, f32),
    regions: &[(&str, Rect, Sense)],
) -> (Vec<Response>, Vec<Response>) {
    h.input.pointer_to(at.0, at.1).press(P);
    let press = frame_of(h, regions);
    h.input.release(P);
    (press, frame_of(h, regions))
}

const SMALL: Rect = Rect::new(100.0, 100.0, 20.0, 20.0);

#[test]
fn a_touch_outside_a_small_button_clicks_it_and_a_mouse_press_there_does_not() {
    let regions = [("b", SMALL, Sense::CLICK)];
    // 10 px right of the button's edge, level with its middle.
    let at = (130.0, 110.0);
    let mut touch = ready(&regions);
    let (press, lift) = tap(&mut touch, at, &regions);
    assert!(press[0].pressed && press[0].press_started);
    assert!(lift[0].clicked);
    let mut mouse = ready(&regions);
    let (press, release) = click(&mut mouse, at, &regions);
    assert!(!press[0].pressed && !press[0].press_started);
    assert!(!release[0].clicked);
    // 23 px away is outside the 44 px target.
    let mut far = ready(&regions);
    let (press, lift) = tap(&mut far, (143.0, 110.0), &regions);
    assert!(!press[0].pressed && !lift[0].clicked);
}

#[test]
fn between_two_close_buttons_a_touch_picks_the_nearer() {
    let regions = [
        ("a", Rect::new(100.0, 100.0, 24.0, 24.0), Sense::CLICK),
        ("b", Rect::new(128.0, 100.0, 24.0, 24.0), Sense::CLICK),
    ];
    let mut h = ready(&regions);
    let (_, lift) = tap(&mut h, (110.0, 112.0), &regions);
    assert!(lift[0].clicked && !lift[1].clicked, "on A's own rect");
    let mut h = ready(&regions);
    let (_, lift) = tap(&mut h, (127.0, 112.0), &regions);
    assert!(
        lift[1].clicked && !lift[0].clicked,
        "in the gap, 1 px from B"
    );
    let mut h = ready(&regions);
    let (_, lift) = tap(&mut h, (126.0, 112.0), &regions);
    assert!(
        lift[1].clicked && !lift[0].clicked,
        "in the gap, level: the later call"
    );
}

#[test]
fn a_small_control_inside_a_larger_one_wins_within_reach_of_a_finger() {
    let regions = [
        ("tab", Rect::new(100.0, 100.0, 120.0, 32.0), Sense::CLICK),
        ("close", Rect::new(196.0, 108.0, 16.0, 16.0), Sense::CLICK),
    ];
    // 8 px left of the close, on the tab.
    let at = (188.0, 116.0);
    let mut touch = ready(&regions);
    let (press, lift) = tap(&mut touch, at, &regions);
    assert!(press[1].pressed && !press[0].pressed);
    assert!(lift[1].clicked && !lift[0].clicked);
    let mut mouse = ready(&regions);
    let (press, _) = click(&mut mouse, at, &regions);
    assert!(
        press[0].pressed && !press[1].pressed,
        "a mouse picks the tab"
    );
    // On the tab, far from the close: the tab.
    let mut far = ready(&regions);
    let (press, _) = tap(&mut far, (120.0, 116.0), &regions);
    assert!(press[0].pressed && !press[1].pressed);
}

#[test]
fn the_target_stops_at_the_clip_the_region_was_registered_under() {
    let frame = |h: &mut UiHarness| {
        let mut out = None;
        h.frame(|ui| {
            ui.painter().push_clip(Rect::new(0.0, 0.0, 200.0, 100.0));
            let id = ui.id("low");
            out = Some(ui.interact(id, Rect::new(10.0, 70.0, 100.0, 40.0), Sense::CLICK));
            ui.painter().pop_clip();
        });
        out.expect("the frame ran")
    };
    let mut h = harness();
    frame(&mut h);
    h.input.touch_start(1, 50.0, 104.0);
    assert!(!frame(&mut h).pressed, "4 px below the viewport");
    h.input.touch_end(1, 50.0, 104.0);
    assert!(!frame(&mut h).clicked);
    h.input.touch_start(1, 50.0, 98.0);
    assert!(
        frame(&mut h).pressed,
        "2 px inside the viewport, above its edge"
    );
}

/// A page, a menu opened by `open`, and its response.
fn page_and_menu(ui: &mut Ui<'_>, open: bool) -> (Response, Option<bool>) {
    let id = ui.id("page");
    let page = ui.interact(id, Rect::new(0.0, 0.0, WIN.0, WIN.1), Sense::CLICK);
    let menu = open.then(|| {
        let rows = [
            MenuRow::Item {
                glyph: None,
                text: "One",
                shortcut: "",
            },
            MenuRow::Item {
                glyph: None,
                text: "Two",
                shortcut: "",
            },
        ];
        let m = ui.menu("menu", &rows, (150.0, 100.0));
        assert_eq!(m.picked, None, "no row picked");
        m.dismissed
    });
    (page, menu)
}

#[test]
fn a_touch_just_outside_an_open_menu_dismisses_it_and_presses_no_row() {
    let mut h = harness();
    for _ in 0..3 {
        h.frame(|ui| {
            page_and_menu(ui, true);
        });
    }
    let menu_hits: Vec<Rect> = hit_regions(&h.store)
        .iter()
        .filter(|r| r.tag != 0)
        .map(|r| r.rect)
        .collect();
    assert!(!menu_hits.is_empty(), "the menu registered its rows");
    // The panel, and the last row inside it, which a touch grows to 44 px high.
    let panel = menu_hits
        .iter()
        .fold(menu_hits[0], |a, r| if r.h > a.h { *r } else { a });
    let row = *menu_hits.last().expect("a row");
    let at = (row.x + row.w / 2.0, panel.bottom() + 1.0);
    assert!(
        at.1 < row.bottom() + (44.0 - row.h) / 2.0,
        "the point is in the last row's grown target"
    );
    h.input.touch_start(1, at.0, at.1);
    let mut dismissed = None;
    h.frame(|ui| dismissed = page_and_menu(ui, true).1);
    assert_eq!(dismissed, Some(true), "the touch dismissed the menu");
    h.input.touch_end(1, at.0, at.1);
    h.frame(|ui| {
        page_and_menu(ui, false);
    });
}

#[test]
fn a_region_in_a_disabled_scope_is_never_a_candidate() {
    let mut out = Vec::new();
    let mut frame = |h: &mut UiHarness| {
        out.clear();
        h.frame(|ui| {
            let e = ui.id("enabled");
            out.push(ui.interact(e, Rect::new(140.0, 100.0, 12.0, 12.0), Sense::CLICK));
            ui.with_enabled(false, |ui| {
                let d = ui.id("disabled");
                out.push(ui.interact(d, SMALL, Sense::CLICK));
            });
        });
        out.clone()
    };
    let mut h = harness();
    frame(&mut h);
    // 5 px from the disabled button, 15 px from the enabled one: the nearer is disabled, and the
    // later call, so it would win a tie or a nearer pick if it were a candidate.
    h.input.touch_start(1, 125.0, 110.0);
    let press = frame(&mut h);
    assert!(press[0].pressed && !press[1].pressed);
    h.input.touch_end(1, 125.0, 110.0);
    let lift = frame(&mut h);
    assert!(lift[0].clicked && !lift[1].clicked);
}

#[test]
fn a_touch_in_the_grown_area_reads_hovered_while_held_and_clicks_on_the_lift_there() {
    let regions = [("b", SMALL, Sense::CLICK)];
    let mut h = ready(&regions);
    h.input.touch_start(1, 130.0, 110.0);
    let press = frame_of(&mut h, &regions);
    assert!(press[0].hovered && press[0].pressed);
    h.input.touch_move(1, 128.0, 112.0);
    let held = frame_of(&mut h, &regions);
    assert!(held[0].hovered && held[0].pressed);
    h.input.touch_end(1, 128.0, 112.0);
    assert!(frame_of(&mut h, &regions)[0].clicked);
    // A lift outside the grown area does not click.
    h.input.touch_start(1, 130.0, 110.0);
    frame_of(&mut h, &regions);
    h.input.touch_move(1, 131.0, 110.0);
    let off = frame_of(&mut h, &regions);
    assert!(off[0].hovered, "still within the 44 px target");
    h.input.touch_end(1, 150.0, 110.0);
    assert!(!frame_of(&mut h, &regions)[0].clicked);
}

/// One frame of a scroll area over `AREA` holding a focusable `field`, and its response.
fn form(h: &mut UiHarness) -> Response {
    let mut out = None;
    h.frame(|ui| {
        let res = ui.scroll_area(
            "area",
            Rect::new(0.0, 0.0, 400.0, 300.0),
            &ScrollOptions::default(),
            |ui, _| {
                let id = ui.id("field");
                let r = ui.interact(id, Rect::new(10.0, 90.0, 200.0, 30.0), Sense::CLICK_FOCUS);
                (r, Extent::new(400.0, 1500.0))
            },
        );
        out = Some(res.value);
    });
    out.expect("the frame ran")
}

fn form_ready() -> UiHarness {
    let mut h = harness();
    form(&mut h);
    form(&mut h);
    h
}

#[test]
fn a_pan_over_a_field_leaves_focus_alone_and_a_tap_or_a_long_press_focuses_it() {
    // A pan the area takes.
    let mut h = form_ready();
    h.input.touch_start(1, 50.0, 100.0);
    assert!(!form(&mut h).focused, "no focus on the press");
    h.input.touch_move(1, 50.0, 140.0);
    assert!(!form(&mut h).focused, "no focus when the pan takes it");
    h.input.touch_end(1, 50.0, 140.0);
    let after = form(&mut h);
    assert!(!after.focused && !after.clicked);
    assert!(!form(&mut h).focused);

    // A tap.
    let mut h = form_ready();
    h.input.touch_start(1, 50.0, 100.0);
    assert!(!form(&mut h).focused);
    h.input.touch_end(1, 50.0, 100.0);
    let lift = form(&mut h);
    assert!(lift.clicked && lift.focused, "the tap focuses");
    assert!(form(&mut h).focused);

    // A long press.
    let mut h = form_ready();
    h.at_ms(0).input.touch_start(1, 50.0, 100.0);
    assert!(!form(&mut h).focused);
    h.at_ms(600);
    let held = form(&mut h);
    assert!(
        held.secondary_clicked && held.focused,
        "the long press focuses"
    );
    h.input.touch_end(1, 50.0, 100.0);
    let lift = form(&mut h);
    assert!(!lift.clicked && lift.focused);

    // A mouse press still focuses on the press.
    let mut h = form_ready();
    h.input.pointer_to(50.0, 100.0).press(P);
    assert!(form(&mut h).focused);
}

#[test]
fn a_touch_cancel_focuses_nothing() {
    let mut h = form_ready();
    h.input.touch_start(1, 50.0, 100.0);
    form(&mut h);
    h.input.touch_cancel(1, 50.0, 100.0);
    form(&mut h);
    assert!(!form(&mut h).focused);
}

/// A drag widget `tab` whose app opens a menu while `open`: the tab's response and the menu's
/// `dismissed`.
fn tab_and_menu(ui: &mut Ui<'_>, open: bool) -> (Response, Option<bool>) {
    let id = ui.id("tab");
    let tab = ui.interact(id, Rect::new(50.0, 100.0, 100.0, 30.0), Sense::DRAG);
    let menu = open.then(|| {
        let rows = [MenuRow::Item {
            glyph: None,
            text: "Close",
            shortcut: "",
        }];
        ui.menu("menu", &rows, (60.0, 140.0)).dismissed
    });
    (tab, menu)
}

#[test]
fn a_drag_out_of_a_long_press_dismisses_the_menu_the_long_press_opened() {
    let mut h = harness();
    let run = |h: &mut UiHarness, open: bool| {
        let mut out = None;
        h.frame(|ui| out = Some(tab_and_menu(ui, open)));
        out.expect("the frame ran")
    };
    run(&mut h, false);
    h.at_ms(0).input.touch_start(1, 60.0, 110.0);
    run(&mut h, false);
    h.at_ms(600);
    let (tab, _) = run(&mut h, false);
    assert!(tab.secondary_clicked, "the long press fired");
    // The app opens the menu on that frame and the next ones.
    h.at_ms(620);
    assert_eq!(run(&mut h, true).1, Some(false));
    h.at_ms(640);
    assert_eq!(run(&mut h, true).1, Some(false));
    h.at_ms(660).input.touch_move(1, 100.0, 110.0);
    let (tab, menu) = run(&mut h, true);
    assert!(tab.drag_started, "the drag started");
    assert_eq!(menu, Some(true), "the menu was dismissed");
    // The press that dismissed it still drags.
    h.at_ms(680).input.touch_move(1, 120.0, 110.0);
    let (tab, _) = run(&mut h, false);
    assert!(tab.dragged && tab.drag_delta == (20.0, 0.0));

    // The same menu under a mouse drag stays: nothing new is dismissed.
    let mut h = harness();
    run(&mut h, false);
    h.at_ms(0).input.pointer_to(60.0, 110.0).press(P);
    run(&mut h, false);
    h.at_ms(620);
    run(&mut h, true);
    h.at_ms(640);
    assert_eq!(run(&mut h, true).1, Some(false));
    h.at_ms(660).input.pointer_to(100.0, 110.0);
    let (tab, menu) = run(&mut h, true);
    assert!(tab.drag_started);
    assert_eq!(menu, Some(false), "a mouse drag dismisses nothing");
}
