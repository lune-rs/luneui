//! The battery (`data.battery` acceptance, D437): the level clamped, the colour bands and their
//! boundaries, the fill's width, the nub, the bolt only while charging, no animation, and the
//! semantic node.

use lune_ui_core::access::{Role, Value};
use lune_ui_core::color::{Rgba, premul};
use lune_ui_core::input::Repaint;
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::{Scene, Shape};
use lune_ui_core::testing::{UiHarness, disabled_violations};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::WidgetId;
use lune_ui_core::widgets::battery::{BatteryOptions, level_color};

const WIN: (f32, f32) = (300.0, 200.0);
const RECT: Rect = Rect::new(10.0, 20.0, 50.0, 24.0);
/// The gap between the outline and the fill.
const INSET: f32 = 2.0;
const CUSTOM: Rgba = [10, 20, 200, 255];

fn draw_in(h: &mut UiHarness, rect: Rect, o: &BatteryOptions<'_>) -> (Scene, Repaint) {
    let (scene, out) = h.frame(|ui| ui.battery("b", rect, o));
    (scene, out.repaint)
}

fn draw(o: &BatteryOptions<'_>) -> Scene {
    draw_in(&mut UiHarness::new(WIN), RECT, o).0
}

fn shapes(scene: &Scene, color: Rgba) -> Vec<Shape> {
    let want = premul(color);
    scene.layers[0]
        .geometry
        .under
        .iter()
        .filter(|s| s.color == want && s.params[0] == 0.0)
        .copied()
        .collect()
}

/// The first plain fill of `color`.
fn fill_of(scene: &Scene, color: Rgba) -> Option<Shape> {
    shapes(scene, color).into_iter().next()
}

fn bolt_points(scene: &Scene) -> Vec<[f32; 2]> {
    let want = premul(DARK.on_accent);
    scene.layers[0]
        .geometry
        .fills
        .iter()
        .filter(|v| v.color == want)
        .map(|v| v.pos)
        .collect()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn the_colour_bands_hold_at_each_boundary() {
    let c = |level, charging| level_color(&DARK, level, charging);
    assert_eq!(c(1.0, false), DARK.ok);
    assert_eq!(c(0.21, false), DARK.ok);
    assert_eq!(c(0.2, false), DARK.warn, "0.2 is the top of the warn band");
    assert_eq!(c(0.15, false), DARK.warn);
    assert_eq!(c(0.1001, false), DARK.warn);
    assert_eq!(c(0.1, false), DARK.err, "0.1 is the top of the err band");
    assert_eq!(c(0.05, false), DARK.err);
    assert_eq!(c(0.0, false), DARK.err);
    assert_eq!(c(f32::NAN, false), DARK.err, "not finite is 0");
    assert_eq!(c(-2.0, false), DARK.err);
    assert_eq!(c(7.0, false), DARK.ok);
    for level in [1.0, 0.15, 0.05, 0.0, f32::NAN] {
        assert_eq!(c(level, true), DARK.ok, "charging is ok at {level}");
    }
}

#[test]
fn the_fill_is_the_level_of_the_inner_width_in_the_level_colour() {
    let body_w = RECT.w - nub_width(&draw(&BatteryOptions::new(0.5, "")));
    let inner_w = body_w - 2.0 * INSET;
    for (level, color) in [(0.5, DARK.ok), (0.15, DARK.warn), (0.05, DARK.err)] {
        let scene = draw(&BatteryOptions::new(level, ""));
        let fill = fill_of(&scene, color).unwrap_or_else(|| panic!("a fill at {level}"));
        assert!(
            close(fill.rect[2], inner_w * level, 1e-3),
            "{level}: {:?}",
            fill.rect
        );
        assert!(close(fill.rect[0], RECT.x + INSET, 1e-3));
        assert!(close(fill.rect[3], RECT.h - 2.0 * INSET, 1e-3));
    }
}

/// The nub's width: the one shape right of the body, in the outline's colour.
fn nub_width(scene: &Scene) -> f32 {
    shapes(scene, DARK.fg_tertiary)[0].rect[2]
}

#[test]
fn a_level_not_usable_is_clamped_and_zero_draws_no_fill() {
    let inner_w = RECT.w - nub_width(&draw(&BatteryOptions::new(1.0, ""))) - 2.0 * INSET;
    let full = fill_of(&draw(&BatteryOptions::new(1.0, "")), DARK.ok).expect("full");
    assert!(close(full.rect[2], inner_w, 1e-3));
    let over = fill_of(&draw(&BatteryOptions::new(4.0, "")), DARK.ok).expect("over");
    assert!(close(over.rect[2], inner_w, 1e-3), "past 1 is cut to 1");
    for level in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let scene = draw(&BatteryOptions::new(level, ""));
        for color in [DARK.ok, DARK.warn, DARK.err] {
            assert!(fill_of(&scene, color).is_none(), "level {level}: no fill");
        }
    }
}

#[test]
fn a_colour_from_the_app_replaces_the_level_colour() {
    let scene = draw(&BatteryOptions::new(0.05, "").with_color(CUSTOM));
    assert!(fill_of(&scene, CUSTOM).is_some());
    assert!(fill_of(&scene, DARK.err).is_none());
}

#[test]
fn the_nub_is_on_the_right_end_centred_vertically() {
    let scene = draw(&BatteryOptions::new(0.5, ""));
    let nub = shapes(&scene, DARK.fg_tertiary);
    assert_eq!(
        nub.len(),
        1,
        "one plain fill in the outline colour, the nub; the outline is a ring"
    );
    let [x, y, w, h] = nub[0].rect;
    assert!(
        close(x + w, RECT.x + RECT.w, 1e-3),
        "the nub ends at the rect's right edge"
    );
    assert!(close(y + h / 2.0, RECT.y + RECT.h / 2.0, 1e-3));
    assert!((2.0..=4.0).contains(&w), "{w}");
    assert!(h < RECT.h);
    // The outline (a ring of the same colour) spans the body, which keeps the rect's height.
    let ring = scene.layers[0]
        .geometry
        .under
        .iter()
        .find(|s| s.params[0] == 1.0)
        .expect("the outline");
    assert!(close(ring.rect[3], RECT.h, 1e-3));
    assert!(close(ring.rect[0], RECT.x, 1e-3));
    assert!(
        close(ring.rect[0] + ring.rect[2], x, 1e-3),
        "the body ends where the nub starts"
    );
}

#[test]
fn the_bolt_is_drawn_only_while_charging_and_centred_in_the_body() {
    let calm = draw(&BatteryOptions::new(0.8, ""));
    assert!(bolt_points(&calm).is_empty(), "no bolt when not charging");
    let live = draw(&BatteryOptions::new(0.8, "").with_charging(true));
    let pts = bolt_points(&live);
    assert!(!pts.is_empty(), "a bolt when charging");
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in &pts {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let body_w = RECT.w - nub_width(&live);
    // The fringe adds a pixel on each side; the centre stays where the body's is.
    assert!(
        close((lo[0] + hi[0]) / 2.0, RECT.x + body_w / 2.0, 0.6),
        "{lo:?} {hi:?}"
    );
    assert!(
        close((lo[1] + hi[1]) / 2.0, RECT.y + RECT.h / 2.0, 0.6),
        "{lo:?} {hi:?}"
    );
    assert!(
        hi[1] - lo[1] < RECT.h - 2.0 * INSET + 2.0,
        "inside the inner height"
    );
    // Charging at a low level is still ok-coloured.
    let low = draw(&BatteryOptions::new(0.05, "").with_charging(true));
    assert!(fill_of(&low, DARK.ok).is_some());
}

#[test]
fn any_aspect_and_a_rect_with_no_size_do_not_draw_outside_or_panic() {
    for rect in [
        Rect::new(10.0, 10.0, 120.0, 12.0),
        Rect::new(10.0, 10.0, 12.0, 60.0),
        Rect::new(10.0, 10.0, 5.0, 5.0),
        Rect::new(10.0, 10.0, 2.0, 2.0),
    ] {
        let o = BatteryOptions::new(0.6, "").with_charging(true);
        let scene = draw_rect(rect, &o);
        for s in &scene.layers[0].geometry.under {
            assert!(
                s.rect[0] >= rect.x - 1e-3
                    && s.rect[0] + s.rect[2] <= rect.x + rect.w + 1e-3
                    && s.rect[1] >= rect.y - 1e-3
                    && s.rect[1] + s.rect[3] <= rect.y + rect.h + 1e-3,
                "{rect:?}: {:?}",
                s.rect
            );
        }
    }
    for rect in [
        Rect::new(0.0, 0.0, 0.0, 10.0),
        Rect::new(0.0, 0.0, 10.0, -1.0),
        Rect::new(0.0, 0.0, f32::NAN, 10.0),
    ] {
        let mut h = UiHarness::new(WIN);
        let (scene, out) = h.frame(|ui| ui.battery("b", rect, &BatteryOptions::new(0.5, "x")));
        assert!(scene.layers[0].geometry.under.is_empty(), "{rect:?}");
        assert_eq!(out.tree.nodes.len(), 1);
    }
}

fn draw_rect(rect: Rect, o: &BatteryOptions<'_>) -> Scene {
    draw_in(&mut UiHarness::new(WIN), rect, o).0
}

#[test]
fn a_new_level_redraws_at_once_and_nothing_animates() {
    let mut h = UiHarness::new(WIN);
    h.at_ms(1000);
    let (a, repaint) = draw_in(&mut h, RECT, &BatteryOptions::new(0.2, ""));
    assert_eq!(repaint, Repaint::Idle);
    h.at_ms(1010);
    let (b, repaint) = draw_in(&mut h, RECT, &BatteryOptions::new(0.9, ""));
    assert_eq!(repaint, Repaint::Idle);
    assert_ne!(a, b);
    let inner_w = RECT.w - nub_width(&b) - 2.0 * INSET;
    let fill = fill_of(&b, DARK.ok).expect("fill");
    assert!(
        close(fill.rect[2], inner_w * 0.9, 1e-3),
        "the new level at once"
    );
}

#[test]
fn the_battery_is_a_progress_with_the_level_as_a_percentage() {
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| {
        ui.battery(
            "b",
            RECT,
            &BatteryOptions::new(0.8, "Main").with_charging(true),
        )
    });
    assert_eq!(out.tree.nodes.len(), 1);
    let n = &out.tree.nodes[0];
    assert_eq!(n.role, Role::Progress);
    assert_eq!(n.name, "Main");
    assert_eq!(n.description.as_deref(), Some("Charging"));
    assert_eq!(n.bounds, RECT);
    match &n.value {
        Some(Value::Number {
            value,
            min,
            max,
            step,
        }) => {
            assert!((value - 80.0).abs() < 1e-4, "{value}");
            assert_eq!((*min, *max, *step), (0.0, 100.0, 1.0));
        }
        other => panic!("a number value, got {other:?}"),
    }
    let (_, out) = h.frame(|ui| ui.battery("b", RECT, &BatteryOptions::new(f32::NAN, "")));
    assert_eq!(
        out.tree.nodes[0].description, None,
        "not charging: no description"
    );
    match &out.tree.nodes[0].value {
        Some(Value::Number { value, .. }) => assert_eq!(*value, 0.0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_empty_label_falls_to_the_toolkit_string_through_the_table() {
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| ui.battery("b", RECT, &BatteryOptions::new(0.5, "")));
    assert_eq!(out.tree.nodes[0].name, "Battery");
    let mut h = UiHarness::new(WIN);
    h.strings
        .insert("luneui.battery.name", "Bateri")
        .expect("a toolkit key");
    h.strings
        .insert("luneui.battery.charging", "Mengecas")
        .expect("a toolkit key");
    let (_, out) =
        h.frame(|ui| ui.battery("b", RECT, &BatteryOptions::new(0.5, "").with_charging(true)));
    assert_eq!(out.tree.nodes[0].name, "Bateri");
    assert_eq!(out.tree.nodes[0].description.as_deref(), Some("Mengecas"));
}

#[test]
fn a_disabled_scope_marks_the_node() {
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| {
        ui.with_enabled(false, |ui| {
            ui.battery("b", RECT, &BatteryOptions::new(0.5, ""))
        })
    });
    assert_eq!(
        disabled_violations(&out.tree, WidgetId::ROOT.with("b")),
        vec![]
    );
}
