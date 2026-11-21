//! The core rules of touch under widgets (mobile M5 step 1, D361): a touch press picks its widget
//! with a 44 x 44 target around a small control, focus moves at the tap, the long press or the drag
//! start rather than at the press, and a drag that grows out of a long press dismisses the menu
//! that long press opened. Scripted time and touches through `UiHarness`; nothing here reads a
//! clock.

use lune_ui_core::input::PointerButton;
use lune_ui_core::layout::Axis;
use lune_ui_core::paint::Rect;
use lune_ui_core::testing::{UiHarness, hit_regions};
use lune_ui_core::ui::{Response, Sense, Ui};
use lune_ui_core::widgets::menu::MenuRow;
use lune_ui_core::widgets::scroll_area::{Extent, ScrollOptions};
use lune_ui_core::widgets::slider::SliderOptions;
use lune_ui_core::widgets::split_panes::{SplitOptions, SplitSide, geometry};
use lune_ui_core::widgets::tab_strip::{Tab, TabEvent, TabKey, TabStripOptions, TabStripResponse};
use lune_ui_core::widgets::tone::Tone;

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

// Step 2 (D362): drag claims and acting on the tap.

const AREA: Rect = Rect::new(0.0, 0.0, 400.0, 300.0);

/// One frame of a vertical scroll area over `AREA` (content 400 x 1500) running `content`: what it
/// returned and the area's y offset after the frame.
fn in_area<R>(h: &mut UiHarness, content: impl FnOnce(&mut Ui<'_>) -> R) -> (R, f64) {
    let mut out = None;
    h.frame(|ui| {
        let res = ui.scroll_area("area", AREA, &ScrollOptions::default(), |ui, _| {
            (content(ui), Extent::new(400.0, 1500.0))
        });
        out = Some((res.value, res.offset.1));
    });
    out.expect("the frame ran")
}

/// A touch that starts at `from`, travels to `to` in three moves, one frame each, and lifts: the
/// frame outputs of `run`, in order (press, moves, lift).
fn swipe<R>(
    h: &mut UiHarness,
    from: (f32, f32),
    to: (f32, f32),
    mut run: impl FnMut(&mut UiHarness) -> R,
) -> Vec<R> {
    let mut out = Vec::new();
    h.input.touch_start(1, from.0, from.1);
    out.push(run(h));
    for k in 1..=3 {
        let t = k as f32 / 3.0;
        h.input.touch_move(
            1,
            from.0 + (to.0 - from.0) * t,
            from.1 + (to.1 - from.1) * t,
        );
        out.push(run(h));
    }
    h.input.touch_end(1, to.0, to.1);
    out.push(run(h));
    out
}

const SLIDER: Rect = Rect::new(20.0, 100.0, 300.0, 24.0);
const SLIDER_OPTS: SliderOptions<'static> = SliderOptions {
    min: 0.0,
    max: 100.0,
    step: 0.0,
    name: "Level",
};

fn slider_frame(h: &mut UiHarness, value: &mut f32) -> (Response, f64) {
    let (r, offset) = in_area(h, |ui| ui.slider("level", SLIDER, value, &SLIDER_OPTS));
    (r, offset)
}

#[test]
fn a_slider_claims_x_only_a_vertical_pan_scrolls_and_a_horizontal_drag_moves_it() {
    let ready = || {
        let mut h = harness();
        let mut v = 10.0;
        slider_frame(&mut h, &mut v);
        slider_frame(&mut h, &mut v);
        h
    };
    // A vertical pan from the slider scrolls the area and leaves the value.
    let mut h = ready();
    let mut v = 10.0;
    let seen = swipe(&mut h, (170.0, 112.0), (170.0, 60.0), |h| {
        slider_frame(h, &mut v)
    });
    assert_eq!(v, 10.0, "the pan changed no value");
    assert!(seen.last().is_some_and(|(_, off)| *off > 20.0), "{seen:?}");
    assert!(seen.iter().all(|(r, _)| !r.changed));

    // A horizontal drag moves the value from the slop on, and the area stays.
    let mut h = ready();
    let mut v = 10.0;
    let seen = swipe(&mut h, (170.0, 112.0), (260.0, 112.0), |h| {
        slider_frame(h, &mut v)
    });
    assert!(v > 60.0, "the drag followed: {v}");
    assert!(seen.iter().all(|(_, off)| *off == 0.0));
}

#[test]
fn a_tap_on_the_slider_track_sets_the_value_on_the_lift_not_on_the_press() {
    let mut h = harness();
    let mut v = 10.0;
    slider_frame(&mut h, &mut v);
    slider_frame(&mut h, &mut v);
    h.input.touch_start(1, 290.0, 112.0);
    let (press, _) = slider_frame(&mut h, &mut v);
    assert!(press.pressed && !press.changed);
    assert_eq!(v, 10.0, "the press set nothing");
    h.input.touch_end(1, 290.0, 112.0);
    let (lift, _) = slider_frame(&mut h, &mut v);
    assert!(lift.clicked && lift.changed);
    assert!(v > 85.0, "the lift jumped to the tap: {v}");

    // A mouse press still sets it on the press.
    let mut h = harness();
    let mut v = 10.0;
    slider_frame(&mut h, &mut v);
    slider_frame(&mut h, &mut v);
    h.input.pointer_to(290.0, 112.0).press(P);
    let (press, _) = slider_frame(&mut h, &mut v);
    assert!(press.changed && v > 85.0);
}

const SPLIT: Rect = Rect::new(20.0, 60.0, 300.0, 200.0);

fn split_frame(h: &mut UiHarness) -> (f32, bool, f64) {
    let o = SplitOptions {
        axis: Axis::Horizontal,
        sized: SplitSide::First,
        default: 100.0,
        min: 20.0,
        max: 280.0,
        min_other: 20.0,
        hidden: false,
        name: "Split",
    };
    let (r, off) = in_area(h, |ui| ui.split_panes("split", SPLIT, &o, |_, _, _| {}));
    (r.size, r.changed, off)
}

#[test]
fn a_split_divider_claims_its_axis_a_vertical_pan_scrolls_and_a_horizontal_drag_moves_it() {
    let mid = {
        let g = geometry(SPLIT, Axis::Horizontal, SplitSide::First, 100.0).hit;
        (g.x + g.w / 2.0, g.y + g.h / 2.0)
    };
    let ready = || {
        let mut h = harness();
        split_frame(&mut h);
        split_frame(&mut h);
        h
    };
    let mut h = ready();
    let seen = swipe(&mut h, mid, (mid.0, mid.1 - 50.0), split_frame);
    assert!(
        seen.iter()
            .all(|(size, changed, _)| *size == 100.0 && !*changed)
    );
    assert!(
        seen.last().is_some_and(|(_, _, off)| *off > 20.0),
        "{seen:?}"
    );
    // The next press works as the first: nothing is left held.
    let (size, _, _) = split_frame(&mut h);
    assert_eq!(size, 100.0);
    h.input.touch_start(1, mid.0, mid.1 - 50.0);
    split_frame(&mut h);

    // A press that wobbles inside the slop and lifts moves nothing: the strip follows a drag, not
    // the held press.
    let mut h = ready();
    let seen = swipe(&mut h, mid, (mid.0 + 6.0, mid.1), split_frame);
    assert!(seen.iter().all(|(size, changed, _)| *size == 100.0 && !*changed));

    let mut h = ready();
    let seen = swipe(&mut h, mid, (mid.0 + 40.0, mid.1), split_frame);
    assert!(
        seen.last().is_some_and(|(size, _, _)| *size > 130.0),
        "{seen:?}"
    );
    assert!(seen.iter().all(|(_, _, off)| *off == 0.0));
}

#[test]
fn a_scrollbar_thumb_keeps_a_drag_along_its_axis_under_a_finger() {
    let frame = |h: &mut UiHarness| in_area(h, |_| ()).1;
    let mut h = harness();
    frame(&mut h);
    frame(&mut h);
    // A finger pulling the thumb down scrolls the area down (a pan down would clamp at the top).
    let seen = swipe(&mut h, (394.0, 20.0), (394.0, 70.0), frame);
    assert!(seen.last().is_some_and(|off| *off > 100.0), "{seen:?}");

    // Across its axis the thumb gives the touch up: in an area that scrolls both ways, a drag
    // along x from the vertical bar's thumb pans the area sideways.
    let wide = |h: &mut UiHarness| {
        let mut out = (0.0, 0.0);
        h.frame(|ui| {
            let res = ui.scroll_area("area", AREA, &ScrollOptions::default(), |_, _| {
                ((), Extent::new(900.0, 1500.0))
            });
            out = res.offset;
        });
        out
    };
    let mut h = harness();
    wide(&mut h);
    wide(&mut h);
    let seen = swipe(&mut h, (394.0, 20.0), (300.0, 20.0), wide);
    let end = seen.last().copied().unwrap_or_default();
    assert!(end.0 > 50.0 && end.1 == 0.0, "{seen:?}");
}

/// One frame of a strip of `n` tabs at (40, 40) `w` wide.
fn strip_frame(h: &mut UiHarness, w: f32, n: u64) -> TabStripResponse {
    let mut out = None;
    h.frame(|ui| {
        let tabs: Vec<Tab<'_>> = (0..n)
            .map(|k| Tab {
                key: TabKey(k),
                title: "A tab",
                closable: false,
                progress: None,
            })
            .collect();
        let o = TabStripOptions {
            active: Some(TabKey(0)),
            panel_focused: false,
            busy: None,
            busy_tone: Tone::Neutral,
            card: Rect::new(40.0, 40.0, w, 200.0),
            name: "Sessions",
            header_tint: None,
        };
        out = Some(ui.tab_strip("s", Rect::new(40.0, 40.0, w, 32.0), &tabs, &o));
    });
    out.expect("the frame ran")
}

fn strip_ready(w: f32, n: u64) -> UiHarness {
    let mut h = UiHarness::new((500.0, 300.0));
    strip_frame(&mut h, w, n);
    strip_frame(&mut h, w, n);
    h
}

fn events(seen: &[TabStripResponse]) -> Vec<TabEvent> {
    seen.iter().flat_map(|r| r.events.clone()).collect()
}

#[test]
fn a_pan_on_a_tab_of_an_overflowing_strip_scrolls_it_and_activates_nothing() {
    let mut h = strip_ready(250.0, 5);
    let seen = swipe(&mut h, (100.0, 56.0), (40.0, 56.0), |h| {
        strip_frame(h, 250.0, 5)
    });
    assert_eq!(events(&seen), vec![], "no activation, no reorder");
    // The strip moved: a tap at the same place, once the fling has come to rest, lands on
    // another tab.
    h.at_ms(5000);
    strip_frame(&mut h, 250.0, 5);
    h.input.touch_start(1, 120.0, 56.0);
    strip_frame(&mut h, 250.0, 5);
    h.input.touch_end(1, 120.0, 56.0);
    let tap = strip_frame(&mut h, 250.0, 5);
    assert_eq!(tap.events, vec![TabEvent::Activate(TabKey(1))]);

    // A tap on the first tab activates it on the lift, not on the press.
    let mut h = strip_ready(250.0, 5);
    h.input.touch_start(1, 100.0, 56.0);
    assert_eq!(strip_frame(&mut h, 250.0, 5).events, vec![]);
    h.input.touch_end(1, 100.0, 56.0);
    assert_eq!(
        strip_frame(&mut h, 250.0, 5).events,
        vec![TabEvent::Activate(TabKey(0))]
    );

    // A mouse press still activates on the press.
    let mut h = strip_ready(250.0, 5);
    h.input.pointer_to(100.0, 56.0).press(P);
    assert_eq!(
        strip_frame(&mut h, 250.0, 5).events,
        vec![TabEvent::Activate(TabKey(0))]
    );
}

#[test]
fn a_long_press_on_a_tab_reports_context_and_a_move_after_it_reorders() {
    let mut h = strip_ready(250.0, 5);
    h.at_ms(0).input.touch_start(1, 70.0, 56.0);
    strip_frame(&mut h, 250.0, 5);
    h.at_ms(500);
    let held = strip_frame(&mut h, 250.0, 5);
    assert!(held.context.is_some_and(|(k, _)| k == TabKey(0)));
    assert_eq!(held.events, vec![]);
    let mut all = Vec::new();
    for (ms, x) in [(520, 110.0), (540, 160.0), (560, 220.0), (580, 220.0)] {
        h.at_ms(ms).input.touch_move(1, x, 56.0);
        all.push(strip_frame(&mut h, 250.0, 5));
    }
    h.at_ms(600).input.touch_end(1, 220.0, 56.0);
    all.push(strip_frame(&mut h, 250.0, 5));
    let ev = events(&all);
    assert!(
        ev.iter()
            .any(|e| matches!(e, TabEvent::Reorder { from: 0, .. })),
        "{ev:?}"
    );
    assert!(!ev.iter().any(|e| matches!(e, TabEvent::Activate(_))));
}

#[test]
fn in_a_strip_that_fits_a_horizontal_touch_drag_reorders_as_a_mouse_drag_does() {
    let mut touch = strip_ready(500.0, 3);
    let seen = swipe(&mut touch, (70.0, 56.0), (240.0, 56.0), |h| {
        strip_frame(h, 500.0, 3)
    });
    let t = events(&seen);
    assert!(
        t.iter()
            .any(|e| matches!(e, TabEvent::Reorder { from: 0, .. })),
        "{t:?}"
    );
    let mut mouse = strip_ready(500.0, 3);
    mouse.input.pointer_to(70.0, 56.0).press(P);
    let mut m = vec![strip_frame(&mut mouse, 500.0, 3)];
    for k in 1..=3 {
        mouse.input.pointer_to(70.0 + 170.0 * k as f32 / 3.0, 56.0);
        m.push(strip_frame(&mut mouse, 500.0, 3));
    }
    mouse.input.release(P);
    m.push(strip_frame(&mut mouse, 500.0, 3));
    let reorder = |v: &[TabEvent]| {
        v.iter()
            .find(|e| matches!(e, TabEvent::Reorder { .. }))
            .cloned()
    };
    assert_eq!(reorder(&t), reorder(&events(&m)));
}
