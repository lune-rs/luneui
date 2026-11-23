//! The sparkline (`data.sparkline` acceptance, D436): the scale, the slots (right-aligned, newest
//! at the right edge, clamped), the baseline of one series and of a mirrored pair, the line with
//! its fill, the bars, the frame, the clamp that keeps the fill polygon drawable, no animation,
//! and the semantic node.

use lune_ui_core::access::Role;
use lune_ui_core::color::{Rgba, premul, with_alpha};
use lune_ui_core::input::Repaint;
use lune_ui_core::paint::Rect;
use lune_ui_core::scene::{Scene, Shape};
use lune_ui_core::testing::{UiHarness, disabled_violations};
use lune_ui_core::theme::DARK;
use lune_ui_core::ui::WidgetId;
use lune_ui_core::widgets::sparkline::{MAX_CAPACITY, SparkStyle, SparklineOptions, scale};

const WIN: (f32, f32) = (1200.0, 300.0);
const RECT: Rect = Rect::new(10.0, 10.0, 100.0, 40.0);
/// The graph's inset from the frame.
const INSET: f32 = 4.0;
const UP: Rgba = [200, 30, 30, 255];
const DOWN: Rgba = [30, 30, 200, 255];

fn draw_in(h: &mut UiHarness, rect: Rect, o: &SparklineOptions<'_>) -> (Scene, Repaint) {
    let (scene, out) = h.frame(|ui| ui.sparkline("s", rect, o));
    (scene, out.repaint)
}

fn draw_rect(rect: Rect, o: &SparklineOptions<'_>) -> Scene {
    draw_in(&mut UiHarness::new(WIN), rect, o).0
}

fn draw(o: &SparklineOptions<'_>) -> Scene {
    draw_rect(RECT, o)
}

fn under(scene: &Scene) -> Vec<Shape> {
    scene.layers[0].geometry.under.clone()
}

/// The shapes of `color` (premultiplied from straight).
fn shapes_of(scene: &Scene, color: Rgba) -> Vec<Shape> {
    let want = premul(color);
    under(scene)
        .into_iter()
        .filter(|s| s.color == want)
        .collect()
}

fn stroke_xs(scene: &Scene, color: Rgba) -> Vec<f32> {
    let want = premul(color);
    scene.layers[0]
        .geometry
        .strokes
        .iter()
        .filter(|v| v.color == want)
        .map(|v| v.pos[0])
        .collect()
}

fn stroke_ys(scene: &Scene, color: Rgba) -> Vec<f32> {
    let want = premul(color);
    scene.layers[0]
        .geometry
        .strokes
        .iter()
        .filter(|v| v.color == want)
        .map(|v| v.pos[1])
        .collect()
}

fn fills_of(scene: &Scene, color: Rgba) -> usize {
    let want = premul(with_alpha(color, 0.25));
    scene.layers[0]
        .geometry
        .fills
        .iter()
        .filter(|v| v.color == want)
        .count()
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn scale_maps_to_zero_one() {
    assert_eq!(scale(&[], 5.0), Vec::<f32>::new());
    assert_eq!(scale(&[], 0.0), Vec::<f32>::new());
    assert_eq!(scale(&[0.0, 0.0], 0.0), vec![0.0, 0.0]);
    assert_eq!(scale(&[0.0, 0.0], 10.0), vec![0.0, 0.0]);
    assert_eq!(
        scale(&[f32::NAN, -3.0, f32::INFINITY, 5.0], 10.0),
        vec![0.0, 0.0, 0.0, 0.5]
    );
    assert_eq!(
        scale(&[5.0, 20.0, 10.0], 10.0),
        vec![0.5, 1.0, 1.0],
        "above max is clamped"
    );
    // No usable max: the largest finite sample is the top.
    assert_eq!(scale(&[1.0, 4.0, 2.0], 0.0), vec![0.25, 1.0, 0.5]);
    for bad in [f32::NAN, -1.0, f32::INFINITY] {
        assert_eq!(scale(&[1.0, 4.0], bad), vec![0.25, 1.0], "{bad}");
    }
    assert_eq!(
        scale(&[f32::NAN, 2.0, f32::INFINITY], 0.0),
        vec![0.0, 1.0, 0.0]
    );
}

#[test]
fn the_newest_sample_sits_at_the_last_slots_centre_and_a_short_series_leaves_the_left_empty() {
    // Flat lines have exactly vertical normals, so the strokes' x is the points' x.
    let up = [0.5, 0.5, 0.5];
    let o = SparklineOptions::new(&up, UP, "")
        .with_max(1.0)
        .with_capacity(10);
    let xs = stroke_xs(&draw(&o), UP);
    let (graph_x, graph_w) = (RECT.x + INSET, RECT.w - 2.0 * INSET);
    let pitch = graph_w / 10.0;
    let last = graph_x + 9.5 * pitch;
    let first = graph_x + 7.5 * pitch;
    let (lo, hi) = (
        xs.iter().copied().fold(f32::MAX, f32::min),
        xs.iter().copied().fold(f32::MIN, f32::max),
    );
    assert!(
        close(hi, last, 0.01),
        "newest {hi} vs the last slot's centre {last}"
    );
    assert!(
        close(lo, first, 0.01),
        "oldest {lo} vs slot 7's centre {first}"
    );
}

#[test]
fn a_longer_series_shows_only_the_newest_slots() {
    // Old samples are huge; the newest four are 1.0. With the auto top taken over the slots in
    // view only, the line reaches the top of the graph.
    let mut up = vec![1.0e6_f32; 50];
    up.extend([1.0; 4]);
    let o = SparklineOptions::new(&up, UP, "").with_capacity(4);
    let ys = stroke_ys(&draw(&o), UP);
    assert!(!ys.is_empty());
    let top = ys.iter().copied().fold(f32::MAX, f32::min);
    assert!(
        close(top, RECT.y + INSET - 1.5, 0.01),
        "the line sits at the graph's top: {top}"
    );
}

#[test]
fn one_series_has_its_baseline_at_the_bottom_inset() {
    let up = [1.0, 2.0, 3.0];
    let scene = draw(&SparklineOptions::new(&up, UP, ""));
    let base = shapes_of(&scene, DARK.border);
    assert_eq!(base.len(), 1);
    let [_, y, _, h] = base[0].rect;
    assert!(close(y + h / 2.0, RECT.y + RECT.h - INSET, 0.5), "{y}");
    assert!(close(h, 1.0, 1e-3));
}

#[test]
fn a_mirrored_pair_has_its_baseline_in_the_middle_and_grows_both_ways_on_one_scale() {
    let (up, down) = ([1.0, 2.0, 4.0], [4.0, 2.0, 1.0]);
    let o = SparklineOptions::new(&up, UP, "").with_mirror(&down, DOWN);
    let scene = draw(&o);
    let base = shapes_of(&scene, DARK.border);
    assert_eq!(base.len(), 1);
    let [_, y, _, h] = base[0].rect;
    let middle = RECT.y + RECT.h / 2.0;
    assert!(close(y + h / 2.0, middle, 0.5), "{y} vs {middle}");
    // Both series peak at 4 of the shared top 4: the up line reaches the graph's top, the down
    // line its bottom.
    let up_ys = stroke_ys(&scene, UP);
    let down_ys = stroke_ys(&scene, DOWN);
    let top = up_ys.iter().copied().fold(f32::MAX, f32::min);
    let bottom = down_ys.iter().copied().fold(f32::MIN, f32::max);
    // The lines are sloped, so their edge vertices lie up to 1.5 px past the points.
    let (graph_top, graph_bottom) = (RECT.y + INSET, RECT.y + RECT.h - INSET);
    assert!((graph_top - 1.6..=graph_top - 0.5).contains(&top), "{top}");
    assert!(
        (graph_bottom + 0.5..=graph_bottom + 1.6).contains(&bottom),
        "{bottom}"
    );
    // Each series has its own fill.
    assert!(fills_of(&scene, UP) > 0 && fills_of(&scene, DOWN) > 0);
}

#[test]
fn a_line_is_a_stroke_with_a_fill_at_a_lower_alpha_in_the_same_hue() {
    let up = [1.0, 3.0, 2.0, 4.0];
    let scene = draw(&SparklineOptions::new(&up, UP, ""));
    assert!(!stroke_xs(&scene, UP).is_empty());
    assert!(fills_of(&scene, UP) > 0);
    // The fill is the series colour at a quarter of its alpha.
    assert_eq!(with_alpha(UP, 0.25)[3], 64);
}

#[test]
fn a_series_with_zero_runs_still_fills() {
    let up = [0.0, 0.0, 5.0, 3.0, 0.0, 0.0, 2.0, 4.0, 0.0];
    let scene = draw(&SparklineOptions::new(&up, UP, ""));
    assert!(
        fills_of(&scene, UP) > 0,
        "the polygon with points on the baseline is filled"
    );
}

#[test]
fn nothing_to_draw_leaves_the_frame_and_the_baseline() {
    for o in [
        SparklineOptions::new(&[], UP, ""),
        SparklineOptions::new(&[0.0, 0.0, 0.0], UP, ""),
        SparklineOptions::new(&[f32::NAN, -1.0], UP, ""),
    ] {
        let scene = draw(&o);
        assert!(scene.layers[0].geometry.strokes.is_empty());
        assert!(scene.layers[0].geometry.fills.is_empty());
        assert_eq!(shapes_of(&scene, DARK.border).len(), 1, "the baseline");
        assert_eq!(shapes_of(&scene, DARK.surface_input).len(), 1, "the frame");
    }
}

#[test]
fn bars_are_one_cell_per_slot_filled_from_the_baseline() {
    let up = [0.0, 1.0, 2.0, 4.0, 0.0, 3.0, 1.0, 2.0, 4.0, 4.0];
    let o = SparklineOptions::new(&up, UP, "").with_style(SparkStyle::Bars);
    let scene = draw(&o);
    assert!(
        scene.layers[0].geometry.strokes.is_empty(),
        "no line in a strip"
    );
    let cells = shapes_of(&scene, DARK.surface_overlay);
    assert_eq!(cells.len(), 10, "one cell per slot, a zero sample included");
    let bars = shapes_of(&scene, UP);
    assert_eq!(bars.len(), 8, "a zero sample shows only its cell");
    let bottom = RECT.y + RECT.h - INSET;
    for b in &bars {
        assert!(
            close(b.rect[1] + b.rect[3], bottom, 1e-3),
            "fills from the baseline"
        );
    }
    let tallest = bars.iter().map(|b| b.rect[3]).fold(0.0, f32::max);
    assert!(
        close(tallest, RECT.h - 2.0 * INSET, 1e-3),
        "the largest sample fills the cell"
    );
}

#[test]
fn a_short_strip_leaves_empty_cells_on_the_left() {
    let up = [4.0, 4.0, 4.0];
    let o = SparklineOptions::new(&up, UP, "")
        .with_style(SparkStyle::Bars)
        .with_capacity(20);
    let scene = draw(&o);
    assert_eq!(shapes_of(&scene, DARK.surface_overlay).len(), 20);
    let bars = shapes_of(&scene, UP);
    assert_eq!(bars.len(), 3);
    let right_edge = RECT.x + RECT.w - INSET;
    let last = bars
        .iter()
        .map(|b| b.rect[0] + b.rect[2])
        .fold(0.0, f32::max);
    assert!(
        last <= right_edge + 1e-3 && last > right_edge - 6.0,
        "{last}"
    );
}

#[test]
fn a_mirrored_strip_fills_both_ways_from_the_middle() {
    let (up, down) = ([4.0, 2.0], [2.0, 4.0]);
    let o = SparklineOptions::new(&up, UP, "")
        .with_mirror(&down, DOWN)
        .with_style(SparkStyle::Bars);
    let scene = draw(&o);
    let middle = RECT.y + RECT.h / 2.0;
    for b in shapes_of(&scene, UP) {
        assert!(close(b.rect[1] + b.rect[3], middle, 1e-3));
    }
    for b in shapes_of(&scene, DOWN) {
        assert!(close(b.rect[1], middle, 1e-3));
    }
}

#[test]
fn a_capacity_past_the_clamp_draws_the_clamped_slots_and_a_drawable_fill() {
    let series: Vec<f32> = (0..5000).map(|i| 1.0 + (i % 7) as f32).collect();
    let wide = Rect::new(10.0, 10.0, 1100.0, 60.0);
    // A line through 1024 points is 1023 segments of 6 vertices.
    let segments = 6 * (MAX_CAPACITY - 1);
    for capacity in [5000, MAX_CAPACITY, 0] {
        let scene = draw_rect(
            wide,
            &SparklineOptions::new(&series, UP, "").with_capacity(capacity),
        );
        assert_eq!(
            stroke_xs(&scene, UP).len(),
            segments,
            "capacity {capacity}: the line has {MAX_CAPACITY} points"
        );
        // A polygon of over 4096 points draws nothing; the clamp keeps it at most 1026.
        assert!(
            fills_of(&scene, UP) > 0,
            "capacity {capacity}: the fill is drawn"
        );
    }
    let mirrored = SparklineOptions::new(&series, UP, "")
        .with_mirror(&series, DOWN)
        .with_capacity(5000);
    let scene = draw_rect(wide, &mirrored);
    assert!(fills_of(&scene, UP) > 0 && fills_of(&scene, DOWN) > 0);
}

#[test]
fn a_capacity_of_one_slot_draws_the_newest_sample_flat_and_a_zero_graph_draws_nothing() {
    let up = [1.0, 2.0, 3.0];
    let scene = draw(&SparklineOptions::new(&up, UP, "").with_capacity(1));
    let xs = stroke_xs(&scene, UP);
    assert!(!xs.is_empty(), "the one sample in view shows as a segment");
    let (graph_x, graph_w) = (RECT.x + INSET, RECT.w - 2.0 * INSET);
    let (lo, hi) = (
        xs.iter().copied().fold(f32::MAX, f32::min),
        xs.iter().copied().fold(f32::MIN, f32::max),
    );
    assert!(close(lo, graph_x, 0.01) && close(hi, graph_x + graph_w, 0.01));
    for rect in [
        Rect::new(0.0, 0.0, 8.0, 40.0),
        Rect::new(0.0, 0.0, 100.0, 8.0),
        Rect::new(0.0, 0.0, 0.0, 0.0),
        Rect::new(0.0, 0.0, f32::NAN, 40.0),
    ] {
        let scene = draw_rect(rect, &SparklineOptions::new(&up, UP, "x"));
        assert!(scene.layers[0].geometry.under.is_empty(), "{rect:?}");
        assert!(scene.layers[0].geometry.strokes.is_empty(), "{rect:?}");
    }
}

#[test]
fn one_sample_in_view_draws_a_flat_segment_across_its_slot_with_its_fill_at_any_capacity() {
    let (graph_x, graph_w) = (RECT.x + INSET, RECT.w - 2.0 * INSET);
    let (graph_y, graph_h) = (RECT.y + INSET, RECT.h - 2.0 * INSET);
    let base = graph_y + graph_h;
    let up = [0.5];
    for capacity in [0, 1, 10, 60, 1024] {
        let o = SparklineOptions::new(&up, UP, "")
            .with_max(1.0)
            .with_capacity(capacity);
        let scene = draw(&o);
        let slots = capacity.max(1) as f32;
        let pitch = graph_w / slots;
        let (xs, ys) = (stroke_xs(&scene, UP), stroke_ys(&scene, UP));
        assert!(!xs.is_empty(), "capacity {capacity}: a segment is drawn");
        let (lo, hi) = (
            xs.iter().copied().fold(f32::MAX, f32::min),
            xs.iter().copied().fold(f32::MIN, f32::max),
        );
        // The newest sample's slot is the last one: its span ends at the graph's right edge.
        assert!(
            close(hi, graph_x + graph_w, 0.01) && close(lo, graph_x + graph_w - pitch, 0.01),
            "capacity {capacity}: {lo}..{hi} vs the last slot"
        );
        // Flat: the line sits at half the graph's height above the baseline (a 1 px stroke and its
        // antialiasing fringe reach 1.5 px either side of its centre).
        let mid = base - 0.5 * graph_h;
        assert!(
            ys.iter().all(|&y| close(y, mid, 1.6)),
            "capacity {capacity}: {ys:?} vs {mid}"
        );
        assert!(
            !shapes_of(&scene, with_alpha(UP, 0.25)).is_empty(),
            "capacity {capacity}: its fill is drawn"
        );
    }
    // The mirrored series with one sample shows too, below the baseline.
    let down = [0.5];
    let o = SparklineOptions::new(&up, UP, "")
        .with_mirror(&down, DOWN)
        .with_max(1.0)
        .with_capacity(60);
    let scene = draw(&o);
    assert!(!stroke_xs(&scene, UP).is_empty() && !stroke_xs(&scene, DOWN).is_empty());
    let middle = graph_y + graph_h / 2.0;
    assert!(stroke_ys(&scene, UP).iter().all(|&y| y < middle));
    assert!(stroke_ys(&scene, DOWN).iter().all(|&y| y > middle));
    // A single sample of 0 draws nothing but the baseline, as an all-zero history does.
    let zero = [0.0];
    let scene = draw(&SparklineOptions::new(&zero, UP, "").with_capacity(10));
    assert!(stroke_xs(&scene, UP).is_empty());
}

#[test]
fn nothing_is_drawn_outside_the_rect() {
    let up: Vec<f32> = (0..30).map(|i| (i % 5) as f32).collect();
    let down: Vec<f32> = (0..30).map(|i| (i % 3) as f32).collect();
    for style in [SparkStyle::Line, SparkStyle::Bars] {
        let o = SparklineOptions::new(&up, UP, "")
            .with_mirror(&down, DOWN)
            .with_style(style);
        let scene = draw(&o);
        for s in under(&scene) {
            let [x, y, w, h] = s.rect;
            assert!(
                x >= RECT.x - 1e-3
                    && y >= RECT.y - 1e-3
                    && x + w <= RECT.x + RECT.w + 1e-3
                    && y + h <= RECT.y + RECT.h + 1e-3,
                "{style:?}: {:?}",
                s.rect
            );
        }
        for v in scene.layers[0]
            .geometry
            .strokes
            .iter()
            .chain(&scene.layers[0].geometry.fills)
        {
            assert!(
                v.pos[0] >= RECT.x - 1e-3
                    && v.pos[0] <= RECT.x + RECT.w + 1e-3
                    && v.pos[1] >= RECT.y - 1e-3
                    && v.pos[1] <= RECT.y + RECT.h + 1e-3,
                "{style:?}: {:?}",
                v.pos
            );
        }
    }
}

#[test]
fn a_new_sample_redraws_at_once_and_nothing_animates() {
    let mut h = UiHarness::new(WIN);
    h.at_ms(1000);
    let first = [1.0, 2.0, 3.0];
    let (a, repaint) = draw_in(
        &mut h,
        RECT,
        &SparklineOptions::new(&first, UP, "").with_max(10.0),
    );
    assert_eq!(repaint, Repaint::Idle);
    let second = [1.0, 2.0, 3.0, 9.0];
    h.at_ms(1010);
    let (b, repaint) = draw_in(
        &mut h,
        RECT,
        &SparklineOptions::new(&second, UP, "").with_max(10.0),
    );
    assert_eq!(repaint, Repaint::Idle);
    assert_ne!(a, b);
    h.at_ms(5000);
    let (c, repaint) = draw_in(
        &mut h,
        RECT,
        &SparklineOptions::new(&second, UP, "").with_max(10.0),
    );
    assert_eq!(repaint, Repaint::Idle);
    assert_eq!(b, c);
}

#[test]
fn the_sparkline_is_an_image_named_by_the_app_with_a_description() {
    let up = [1.0, 2.0];
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| {
        ui.sparkline(
            "s",
            RECT,
            &SparklineOptions::new(&up, UP, "History").with_description("Two samples, rising"),
        )
    });
    assert_eq!(out.tree.nodes.len(), 1);
    let n = &out.tree.nodes[0];
    assert_eq!(n.role, Role::Image);
    assert_eq!(n.name, "History");
    assert_eq!(n.description.as_deref(), Some("Two samples, rising"));
    assert_eq!(n.bounds, RECT);
    assert!(n.children.is_empty());
}

#[test]
fn an_empty_label_falls_to_the_toolkit_string_through_the_table() {
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| ui.sparkline("s", RECT, &SparklineOptions::new(&[], UP, "")));
    assert_eq!(out.tree.nodes[0].name, "Sparkline");
    assert_eq!(out.tree.nodes[0].description, None);
    let mut h = UiHarness::new(WIN);
    h.strings
        .insert("luneui.sparkline.name", "Garis sejarah")
        .expect("a toolkit key");
    let (_, out) = h.frame(|ui| ui.sparkline("s", RECT, &SparklineOptions::new(&[], UP, "")));
    assert_eq!(out.tree.nodes[0].name, "Garis sejarah");
}

#[test]
fn a_disabled_scope_marks_the_node() {
    let mut h = UiHarness::new(WIN);
    let (_, out) = h.frame(|ui| {
        ui.with_enabled(false, |ui| {
            ui.sparkline("s", RECT, &SparklineOptions::new(&[], UP, ""))
        })
    });
    assert_eq!(
        disabled_violations(&out.tree, WidgetId::ROOT.with("s")),
        vec![]
    );
}
