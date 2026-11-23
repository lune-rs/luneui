//! The ring gauge (`data.ring-gauge` acceptance, D435): the arcs of a list of values, the start at
//! the top, the segments end to end, the cap at one turn, the geometry in the rect, the centred
//! text, the value transitions on every tier and the semantic node.
//!
//! A drawn arc is read back from the scene: the stroke's vertices of one colour sit at the angles
//! of its two ends (the tessellator's ends are butt: no vertex lies past the arc's angle), so the
//! smallest and largest angle from the top give its start and end.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use lune_ui_core::access::{Role, Value};
use lune_ui_core::color::{Rgba, premul};
use lune_ui_core::input::Repaint;
use lune_ui_core::motion::{MotionTier, conformance};
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::Scene;
use lune_ui_core::testing::{FakeText, UiHarness, disabled_violations};
use lune_ui_core::text::{Spec, Style, TextSystem};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::WidgetId;
use lune_ui_core::widgets::ring_gauge::{RingGaugeOptions, RingSegment, THICKNESS, VALUE, arcs};

const WIN: (f32, f32) = (400.0, 300.0);
const RECT: Rect = Rect::new(20.0, 30.0, 100.0, 100.0);
const A: Rgba = [200, 30, 30, 255];
const B: Rgba = [30, 200, 30, 255];
const C: Rgba = [30, 30, 200, 255];
const CENTRE: [f32; 2] = [70.0, 80.0];
/// How far an end vertex reads from the end angle: it is offset along the end chord's normal, not
/// the radius (about 0.011 rad here); a round cap would add 0.087.
const ANGLE: f32 = 0.03;

fn seg(values: &[f32]) -> Vec<RingSegment> {
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| RingSegment::new(i as u64 + 1, v, [A, B, C][i % 3]))
        .collect()
}

fn draw_in(h: &mut UiHarness, rect: Rect, o: &RingGaugeOptions<'_>) -> (Scene, Repaint) {
    let (scene, out) = h.frame(|ui| ui.ring_gauge("g", rect, o));
    (scene, out.repaint)
}

fn draw(values: &[f32]) -> Scene {
    let s = seg(values);
    draw_in(
        &mut UiHarness::new(WIN),
        RECT,
        &RingGaugeOptions::new(&s, ""),
    )
    .0
}

/// What the vertices of colour `color` span: the angle from the top (clockwise, radians) of the
/// first and last one, and the smallest and largest distance from the centre.
#[derive(Debug, Clone, Copy)]
struct Span {
    from: f32,
    to: f32,
    near: f32,
    far: f32,
}

fn span(scene: &Scene, color: Rgba) -> Option<Span> {
    let want = premul(color);
    let mut out: Option<Span> = None;
    for layer in scene.in_order() {
        for v in layer.geometry.strokes.iter().filter(|v| v.color == want) {
            let (dx, dy) = (v.pos[0] - CENTRE[0], v.pos[1] - CENTRE[1]);
            let mut a = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU);
            // An end at the top reads a hair below a turn: it is the start.
            if a > TAU - 0.05 {
                a -= TAU;
            }
            let r = (dx * dx + dy * dy).sqrt();
            out = Some(match out {
                None => Span {
                    from: a,
                    to: a,
                    near: r,
                    far: r,
                },
                Some(s) => Span {
                    from: s.from.min(a),
                    to: s.to.max(a),
                    near: s.near.min(r),
                    far: s.far.max(r),
                },
            });
        }
    }
    out
}

/// Whether a vertex of `color` lies within `ANGLE` of `angle` from the top.
fn has_vertex_at(scene: &Scene, color: Rgba, angle: f32) -> bool {
    let want = premul(color);
    scene.in_order().iter().any(|l| {
        l.geometry
            .strokes
            .iter()
            .filter(|v| v.color == want)
            .any(|v| {
                let (dx, dy) = (v.pos[0] - CENTRE[0], v.pos[1] - CENTRE[1]);
                let a = (dy.atan2(dx) + FRAC_PI_2).rem_euclid(TAU);
                (a - angle).abs() <= ANGLE
            })
    })
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn arcs_start_at_the_top_and_turn_end_to_end() {
    assert_eq!(arcs(&[]), vec![]);
    let one = arcs(&[0.25]);
    assert_eq!(one.len(), 1);
    assert!(close(one[0].0, -FRAC_PI_2, 1e-6) && close(one[0].1, FRAC_PI_2, 1e-5));
    let three = arcs(&[0.5, 0.25, 0.125]);
    assert!(close(three[0].1, PI, 1e-5));
    assert!(close(three[1].0, -FRAC_PI_2 + PI, 1e-5));
    assert!(close(three[2].0, -FRAC_PI_2 + 0.75 * TAU, 1e-5));
}

#[test]
fn a_value_that_is_not_finite_or_below_zero_sweeps_nothing() {
    let a = arcs(&[f32::NAN, -0.5, f32::INFINITY, f32::NEG_INFINITY, 0.0, 0.25]);
    let sweeps: Vec<f32> = a.iter().map(|x| x.1).collect();
    assert_eq!(&sweeps[..5], &[0.0; 5]);
    assert!(close(sweeps[5], FRAC_PI_2, 1e-5));
    // The ones that sweep nothing add nothing to where the next starts.
    assert!(close(a[5].0, -FRAC_PI_2, 1e-6));
}

#[test]
fn the_running_total_is_capped_at_one_turn() {
    let a = arcs(&[0.6, 0.6]);
    assert!(close(a[0].1, 0.6 * TAU, 1e-5));
    assert!(close(a[1].1, 0.4 * TAU, 1e-5), "cut: {}", a[1].1);
    let b = arcs(&[0.6, 0.6, 0.3]);
    assert_eq!(b[2].1, 0.0, "after the cap");
    let c = arcs(&[3.0, 0.2]);
    assert!(close(c[0].1, TAU, 1e-5), "one segment cannot pass a turn");
    assert_eq!(c[1].1, 0.0);
    let total: f32 = arcs(&[0.3, 0.5, 0.9, 2.0]).iter().map(|x| x.1).sum();
    assert!(total <= TAU + 1e-4, "{total}");
}

#[test]
fn the_first_segment_starts_at_the_top_and_turns_clockwise() {
    let scene = draw(&[0.25]);
    let s = span(&scene, A).expect("the segment is drawn");
    assert!(close(s.from, 0.0, ANGLE), "starts at the top: {}", s.from);
    assert!(close(s.to, FRAC_PI_2, ANGLE), "ends a quarter on: {}", s.to);
}

#[test]
fn segments_lie_end_to_end_and_the_track_shows_past_the_last() {
    let scene = draw(&[0.5, 0.25, 0.125]);
    let (a, b, c) = (
        span(&scene, A).expect("a"),
        span(&scene, B).expect("b"),
        span(&scene, C).expect("c"),
    );
    assert!(close(a.from, 0.0, ANGLE) && close(a.to, PI, ANGLE), "{a:?}");
    assert!(
        close(b.from, PI, ANGLE) && close(b.to, 1.5 * PI, ANGLE),
        "{b:?}"
    );
    assert!(
        close(c.from, 1.5 * PI, ANGLE) && close(c.to, 1.75 * PI, ANGLE),
        "{c:?}"
    );
    // The track is one full circle under them.
    let track = span(&scene, DARK.surface_overlay).expect("the track");
    assert!(track.far > 0.0);
}

#[test]
fn butt_ends_do_not_pass_the_segments_angle() {
    // A 36 percent segment ends at 0.36 turns, to the tessellator's chord (the end vertices
    // sit on the radius through the end angle, not past it).
    let scene = draw(&[0.36]);
    let s = span(&scene, A).expect("drawn");
    assert!(close(s.to, 0.36 * TAU, ANGLE), "{} vs {}", s.to, 0.36 * TAU);
}

#[test]
fn values_past_one_turn_are_cut_and_the_later_ones_draw_nothing() {
    let scene = draw(&[0.6, 0.6, 0.3]);
    let a = span(&scene, A).expect("a");
    let b = span(&scene, B).expect("b");
    assert!(close(a.to, 0.6 * TAU, ANGLE));
    // The cut segment runs from where the first ended to the top again; its end at the top reads
    // as a start, so only its far end (a chord short of the top) is compared.
    assert!(b.to > 0.95 * TAU - 0.2 && b.to < TAU, "{b:?}");
    assert!(
        has_vertex_at(&scene, B, 0.6 * TAU),
        "starts where the first ended"
    );
    assert!(b.from < ANGLE && b.from > -ANGLE, "ends at the top: {b:?}");
    assert!(span(&scene, C).is_none(), "the third is past the cap");
}

#[test]
fn a_whole_turn_in_one_segment_is_a_closed_ring() {
    // A closed polyline has no vertex at the turn itself: its last one is a chord short of it.
    let scene = draw(&[1.0]);
    let s = span(&scene, A).expect("drawn");
    assert!(s.to - s.from > TAU - 0.3, "{s:?}");
}

#[test]
fn no_segments_draws_the_track_only() {
    let scene = draw(&[]);
    assert!(span(&scene, DARK.surface_overlay).is_some());
    for c in [A, B, C] {
        assert!(span(&scene, c).is_none());
    }
    let zero = draw(&[0.0, 0.0]);
    assert!(span(&zero, A).is_none());
}

#[test]
fn the_ring_touches_the_squares_sides_and_has_the_default_thickness() {
    let scene = draw(&[0.5]);
    let s = span(&scene, A).expect("drawn");
    // The stroke's vertices lie a pixel past each edge (the antialiasing fringe).
    assert!(close(s.far, 50.0 + 1.0, 0.1), "outer edge: {}", s.far);
    assert!(
        close(s.near, 50.0 - THICKNESS - 1.0, 0.1),
        "inner edge: {}",
        s.near
    );
}

#[test]
fn a_thickness_not_usable_falls_to_the_default_and_is_clamped_to_half_the_diameter() {
    let s = seg(&[0.5]);
    let near = |t: f32| {
        let o = RingGaugeOptions::new(&s, "").with_thickness(t);
        let (scene, _) = draw_in(&mut UiHarness::new(WIN), RECT, &o);
        span(&scene, A).expect("drawn").near
    };
    let default = near(THICKNESS);
    for bad in [f32::NAN, 0.0, -3.0, f32::INFINITY] {
        assert!(close(near(bad), default, 1e-3), "{bad}");
    }
    assert!(near(4.0) > default, "thinner is nearer the outer edge");
    // A thickness past half the diameter fills the disc to its centre, no further.
    assert!(near(500.0) < 2.0, "{}", near(500.0));
    assert!(close(near(500.0), near(50.0), 1e-3));
}

#[test]
fn the_ring_is_the_largest_circle_in_the_rect_centred() {
    let s = seg(&[0.5]);
    let o = RingGaugeOptions::new(&s, "");
    // A wide rect: the circle fits the height, centred along the width.
    let wide = Rect::new(0.0, 0.0, 300.0, 100.0);
    let (scene, _) = draw_in(&mut UiHarness::new(WIN), wide, &o);
    let right = scene.layers[0]
        .geometry
        .strokes
        .iter()
        .map(|v| v.pos[0])
        .fold(f32::MIN, f32::max);
    let left = scene.layers[0]
        .geometry
        .strokes
        .iter()
        .map(|v| v.pos[0])
        .fold(f32::MAX, f32::min);
    assert!(close(left, 100.0 - 1.0, 0.2), "{left}");
    assert!(close(right, 200.0 + 1.0, 0.2), "{right}");
}

#[test]
fn a_rect_with_no_size_draws_nothing_but_still_has_a_node() {
    let s = seg(&[0.5]);
    for rect in [
        Rect::new(0.0, 0.0, 0.0, 50.0),
        Rect::new(0.0, 0.0, 50.0, -1.0),
        Rect::new(0.0, 0.0, f32::NAN, 50.0),
    ] {
        let mut h = UiHarness::new(WIN);
        let (scene, out) = h.frame(|ui| ui.ring_gauge("g", rect, &RingGaugeOptions::new(&s, "x")));
        assert!(scene.layers[0].geometry.strokes.is_empty(), "{rect:?}");
        assert_eq!(out.tree.nodes.len(), 1);
    }
}

#[test]
fn the_text_is_centred_in_the_inner_square_and_empty_draws_none() {
    let s = seg(&[0.36]);
    let mut h = UiHarness::new(WIN);
    let (scene, _) = draw_in(
        &mut h,
        RECT,
        &RingGaugeOptions::new(&s, "").with_text("36%"),
    );
    let texts: Vec<_> = scene
        .in_order()
        .into_iter()
        .flat_map(|l| l.texts.clone())
        .collect();
    assert_eq!(texts.len(), 1);
    // The size is 24 percent of the diameter (24 px here, above the base 13).
    let spec = Spec::plain("36%", Style::ui(24.0, DARK.fg_primary));
    let (w, h_) = FakeText.measure(&spec, 1.0);
    assert!(
        close(texts[0].x + w / 2.0, CENTRE[0], 0.01),
        "{}",
        texts[0].x
    );
    assert!(
        close(texts[0].y + h_ / 2.0, CENTRE[1], 0.01),
        "{}",
        texts[0].y
    );
    let (none, _) = draw_in(
        &mut UiHarness::new(WIN),
        RECT,
        &RingGaugeOptions::new(&s, ""),
    );
    assert!(none.in_order().iter().all(|l| l.texts.is_empty()));
}

#[test]
fn a_small_ring_keeps_the_base_text_size_and_a_long_text_is_elided_to_the_inner_width() {
    let s = seg(&[0.36]);
    let small = Rect::new(0.0, 0.0, 40.0, 40.0);
    let long = "a very long reading that cannot fit";
    let mut h = UiHarness::new(WIN);
    let (scene, _) = draw_in(
        &mut h,
        small,
        &RingGaugeOptions::new(&s, "").with_text(long),
    );
    let texts: Vec<_> = scene
        .in_order()
        .into_iter()
        .flat_map(|l| l.texts.clone())
        .collect();
    assert_eq!(texts.len(), 1);
    // The elided spec is not the whole text: its key differs from the unelided one's.
    let whole = Spec::plain(long, Style::ui(DARK.font_base, DARK.fg_primary));
    let (key, _) = FakeText.prepare(&whole, 1.0);
    assert_ne!(texts[0].key, key, "the text was cut to the inner width");
}

#[test]
fn a_change_animates_once_and_ends_exactly_at_the_value() {
    let resolved = DARK
        .motion
        .resolve(&VALUE, MotionTier::Full)
        .expect("value animates on Full");
    let half = resolved.duration.as_millis() as u64 / 2;
    let eased = resolved.easing.eval(0.5);
    let mut h = UiHarness::new(WIN);
    h.at_ms(1000);
    let (rest, repaint) = draw_in(&mut h, RECT, &RingGaugeOptions::new(&seg(&[0.2]), ""));
    assert_eq!(repaint, Repaint::Idle);
    assert!(close(span(&rest, A).expect("a").to, 0.2 * TAU, ANGLE));
    let target = seg(&[0.8]);
    let o = RingGaugeOptions::new(&target, "");
    let (start, repaint) = draw_in(&mut h, RECT, &o);
    assert_eq!(repaint, Repaint::Now);
    assert!(
        close(span(&start, A).expect("a").to, 0.2 * TAU, ANGLE),
        "starts where it was"
    );
    h.at_ms(1000 + half);
    let (mid, repaint) = draw_in(&mut h, RECT, &o);
    assert_eq!(repaint, Repaint::Now);
    let want = (0.2 + 0.6 * eased) * TAU;
    let got = span(&mid, A).expect("a").to;
    assert!(close(got, want, 0.02), "mid {got} vs the curve {want}");
    assert!(got > 0.2 * TAU + 0.05 && got < 0.8 * TAU - 0.05);
    h.at_ms(1000 + 2 * half + 10);
    let (end, repaint) = draw_in(&mut h, RECT, &o);
    assert_eq!(repaint, Repaint::Idle);
    assert!(close(span(&end, A).expect("a").to, 0.8 * TAU, ANGLE));
}

#[test]
fn live_snaps_and_off_and_reduced_snap() {
    let (low, high) = (seg(&[0.2]), seg(&[0.8]));
    let mut h = UiHarness::new(WIN);
    h.at_ms(1000);
    draw_in(
        &mut h,
        RECT,
        &RingGaugeOptions::new(&low, "").with_live(true),
    );
    let (now, repaint) = draw_in(
        &mut h,
        RECT,
        &RingGaugeOptions::new(&high, "").with_live(true),
    );
    assert_eq!(repaint, Repaint::Idle);
    assert!(close(span(&now, A).expect("a").to, 0.8 * TAU, ANGLE));
    for tier in [MotionTier::Off, MotionTier::Reduced] {
        let mut h = UiHarness::new(WIN);
        h.tier = tier;
        h.at_ms(1000);
        draw_in(&mut h, RECT, &RingGaugeOptions::new(&low, ""));
        let (now, repaint) = draw_in(&mut h, RECT, &RingGaugeOptions::new(&high, ""));
        assert_eq!(repaint, Repaint::Idle, "{tier:?}");
        assert!(
            close(span(&now, A).expect("a").to, 0.8 * TAU, ANGLE),
            "{tier:?}"
        );
    }
}

#[test]
fn a_segment_a_gauge_stops_passing_leaves_at_once() {
    let mut h = UiHarness::new(WIN);
    h.at_ms(1000);
    draw_in(&mut h, RECT, &RingGaugeOptions::new(&seg(&[0.3, 0.3]), ""));
    let (after, _) = draw_in(&mut h, RECT, &RingGaugeOptions::new(&seg(&[0.3]), ""));
    assert!(span(&after, B).is_none());
}

#[test]
fn the_value_row_conforms_under_every_tier() {
    for tier in [MotionTier::Full, MotionTier::Reduced, MotionTier::Off] {
        conformance(&VALUE, &DARK.motion, tier, 0.0_f32, 1.0)
            .unwrap_or_else(|e| panic!("value {tier:?}: {e}"));
    }
}

#[test]
fn idle_at_rest() {
    let mut h = UiHarness::new(WIN);
    h.at_ms(1000);
    let s = seg(&[0.4, 0.2]);
    let o = RingGaugeOptions::new(&s, "").with_text("60%");
    let (a, repaint) = draw_in(&mut h, RECT, &o);
    assert_eq!(repaint, Repaint::Idle);
    h.at_ms(5000);
    let (b, repaint) = draw_in(&mut h, RECT, &o);
    assert_eq!(repaint, Repaint::Idle);
    assert_eq!(a, b);
}

#[test]
fn the_gauge_is_a_progress_named_by_the_app_whose_value_is_the_covered_fraction() {
    let s = seg(&[0.2, 0.3]);
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| {
        ui.ring_gauge(
            "g",
            RECT,
            &RingGaugeOptions::new(&s, "Used").with_text("50%"),
        )
    });
    assert_eq!(out.tree.nodes.len(), 1);
    let n = &out.tree.nodes[0];
    assert_eq!(n.role, Role::Progress);
    assert_eq!(n.name, "Used");
    assert_eq!(n.description.as_deref(), Some("50%"));
    assert_eq!(n.bounds, RECT);
    match &n.value {
        Some(Value::Number {
            value, min, max, ..
        }) => {
            assert!((value - 0.5).abs() < 1e-6, "{value}");
            assert_eq!((*min, *max), (0.0, 1.0));
        }
        other => panic!("a number value, got {other:?}"),
    }
    assert!(n.children.is_empty());
}

#[test]
fn the_covered_fraction_is_capped_at_one_and_skips_values_not_usable() {
    let s = seg(&[0.8, f32::NAN, 0.8]);
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| ui.ring_gauge("g", RECT, &RingGaugeOptions::new(&s, "x")));
    match &out.tree.nodes[0].value {
        Some(Value::Number { value, .. }) => assert!((value - 1.0).abs() < 1e-6),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_node_is_a_square_in_a_wide_rect() {
    let s = seg(&[0.5]);
    let wide = Rect::new(0.0, 0.0, 300.0, 100.0);
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| ui.ring_gauge("g", wide, &RingGaugeOptions::new(&s, "x")));
    assert_eq!(
        out.tree.nodes[0].bounds,
        Rect::new(100.0, 0.0, 100.0, 100.0)
    );
}

#[test]
fn an_empty_label_falls_to_the_toolkit_string_through_the_table() {
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| ui.ring_gauge("g", RECT, &RingGaugeOptions::new(&[], "")));
    assert_eq!(out.tree.nodes[0].name, "Ring gauge");
    assert_eq!(out.tree.nodes[0].description, None);
    let mut h = UiHarness::new(WIN);
    h.strings
        .insert("luneui.ring-gauge.name", "Tolok cincin")
        .expect("a toolkit key");
    let (_, out) = h.frame(|ui| ui.ring_gauge("g", RECT, &RingGaugeOptions::new(&[], "")));
    assert_eq!(out.tree.nodes[0].name, "Tolok cincin");
}

#[test]
fn a_disabled_scope_marks_the_node() {
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| {
        ui.with_enabled(false, |ui| {
            ui.ring_gauge("g", RECT, &RingGaugeOptions::new(&[], ""))
        })
    });
    assert_eq!(
        disabled_violations(&out.tree, WidgetId::ROOT.with("g")),
        vec![]
    );
}
