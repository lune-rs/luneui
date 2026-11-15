//! The painter's gradient and shadow ops: the shapes they record, the transform, clip and layer
//! they follow, and record and replay.

use lune_ui_core::paint::{Falloff, LayerId, Painter, Rect, Xform};
use lune_ui_core::scene::{Scene, Shape};
use lune_ui_core::testing::FakeText;
use lune_ui_core::theme::DARK as THEME;

type P<'a> = Painter<'a, u8>;

fn painter(text: &mut FakeText) -> P<'_> {
    Painter::new(2.0, Rect::new(0.0, 0.0, 400.0, 300.0), text, None, THEME)
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE_HALF: [u8; 4] = [0, 0, 255, 128];
const R: Rect = Rect::new(10.0, 20.0, 100.0, 50.0);

fn linear(p: &mut P<'_>) {
    p.linear(R, 4.0, RED, BLUE_HALF, 0.5);
}

fn radial(p: &mut P<'_>) {
    p.radial([60.0, 45.0], [30.0, 20.0], RED, BLUE_HALF, Falloff::Smooth);
}

fn shadow(p: &mut P<'_>) {
    p.shadow(R, 4.0, 6.0, RED);
}

fn all(p: &mut P<'_>) {
    linear(p);
    radial(p);
    shadow(p);
}

fn scene_of(f: impl Fn(&mut P<'_>)) -> Scene {
    let mut t = FakeText;
    let mut p = painter(&mut t);
    f(&mut p);
    p.scene
}

fn under(scene: &Scene, layer: LayerId) -> Vec<Shape> {
    scene.layers[layer as usize].geometry.under.clone()
}

fn premul_f(c: [u8; 4]) -> [f32; 4] {
    let a = f32::from(c[3]) / 255.0;
    [
        f32::from(c[0]) / 255.0 * a,
        f32::from(c[1]) / 255.0 * a,
        f32::from(c[2]) / 255.0 * a,
        a,
    ]
}

fn close(a: [f32; 4], b: [f32; 4]) {
    for i in 0..4 {
        assert!((a[i] - b[i]).abs() < 1e-5, "{a:?} != {b:?}");
    }
}

#[test]
fn each_op_lands_on_under_with_its_kind() {
    let scene = scene_of(all);
    let shapes = under(&scene, LayerId::Base);
    assert_eq!(shapes.len(), 3);
    assert!(
        scene.layers[LayerId::Base as usize]
            .geometry
            .over
            .is_empty()
    );

    let [lin, rad, sh] = [shapes[0], shapes[1], shapes[2]];
    assert_eq!(lin.rect, [20.0, 40.0, 200.0, 100.0]);
    assert_eq!(lin.radii, [8.0; 4]);
    assert_eq!(lin.params, [2.0, 0.5, 0.0, 0.0]);
    close(lin.color, premul_f(RED));
    close(lin.under, premul_f(BLUE_HALF));

    // Centre (60, 45), radius (30, 20): rect (30, 25, 60, 40), doubled to physical px.
    assert_eq!(rad.rect, [60.0, 50.0, 120.0, 80.0]);
    assert_eq!(rad.radii, [0.0; 4]);
    assert_eq!(rad.params, [3.0, 1.0, 0.0, 0.0]);
    close(rad.color, premul_f(RED));
    close(rad.under, premul_f(BLUE_HALF));

    assert_eq!(sh.rect, [20.0, 40.0, 200.0, 100.0]);
    assert_eq!(sh.radii, [8.0; 4]);
    assert_eq!(sh.params, [4.0, 12.0, 0.0, 0.0]);
    close(sh.color, premul_f(RED));
    assert_eq!(sh.under, [0.0; 4]);
}

#[test]
fn a_non_finite_angle_counts_as_zero() {
    for angle in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let scene = scene_of(|p| p.linear(R, 0.0, RED, BLUE_HALF, angle));
        assert_eq!(under(&scene, LayerId::Base)[0].params, [2.0, 0.0, 0.0, 0.0]);
    }
}

#[test]
fn the_transform_scales_geometry_and_fades_colours() {
    let scene = scene_of(|p| {
        p.push_xform(Xform {
            origin: (0.0, 0.0),
            scale: 1.5,
            dx: 0.0,
            dy: 0.0,
            alpha: 0.5,
        });
        all(p);
    });
    let [lin, rad, sh] = {
        let s = under(&scene, LayerId::Base);
        [s[0], s[1], s[2]]
    };
    // Scale 2 (display) times 1.5: x3.
    assert_eq!(lin.rect, [30.0, 60.0, 300.0, 150.0]);
    assert_eq!(lin.radii, [12.0; 4]);
    assert_eq!(rad.rect, [90.0, 75.0, 180.0, 120.0]);
    assert_eq!(sh.rect, [30.0, 60.0, 300.0, 150.0]);
    assert_eq!(sh.radii, [12.0; 4]);
    assert_eq!(sh.params[1], 18.0, "sigma x3");

    for s in [lin, rad, sh] {
        close(s.color, premul_f(RED).map(|v| v * 0.5));
    }
    for s in [lin, rad] {
        close(s.under, premul_f(BLUE_HALF).map(|v| v * 0.5));
    }
    assert_eq!(sh.under, [0.0; 4], "a shadow has no second colour to fade");
}

#[test]
fn the_clip_and_rounded_clip_match_a_fill() {
    for rounded in [false, true] {
        let scene = scene_of(|p| {
            let area = Rect::new(15.0, 25.0, 60.0, 30.0);
            if rounded {
                p.push_clip_rounded(area, 6.0);
            } else {
                p.push_clip(area);
            }
            p.fill(R, 4.0, RED);
            all(p);
        });
        let shapes = under(&scene, LayerId::Base);
        assert_eq!(shapes.len(), 4);
        let fill = shapes[0];
        assert!(fill.clip[2] > 0.0);
        assert_eq!(fill.round_radius > 0.0, rounded);
        for s in &shapes[1..] {
            assert_eq!(s.clip, fill.clip, "rounded: {rounded}");
            assert_eq!(s.round, fill.round, "rounded: {rounded}");
            assert_eq!(s.round_radius, fill.round_radius, "rounded: {rounded}");
        }
    }
}

#[test]
fn recorded_and_replayed_equal_the_direct_scene() {
    for transformed in [false, true] {
        let run = |p: &mut P<'_>| {
            if transformed {
                p.push_xform(Xform {
                    origin: (5.0, 5.0),
                    scale: 0.8,
                    dx: 3.0,
                    dy: -2.0,
                    alpha: 0.6,
                });
            }
            all(p);
            if transformed {
                p.pop_xform();
            }
        };
        let direct = scene_of(run);

        let mut t = FakeText;
        let mut rec = painter(&mut t);
        rec.start_recording();
        run(&mut rec);
        let ops = rec.take_recording();
        assert_eq!(rec.scene, direct, "recording must not change the scene");
        assert!(!ops.is_empty());

        let mut t2 = FakeText;
        let mut fresh = painter(&mut t2);
        fresh.replay(&ops);
        assert_eq!(fresh.scene, direct, "replay (transformed: {transformed})");
    }
}

#[test]
fn a_replay_under_another_scale_draws_at_that_scale() {
    let mut t = FakeText;
    let mut rec = painter(&mut t);
    rec.start_recording();
    all(&mut rec);
    let ops = rec.take_recording();

    let mut t2 = FakeText;
    let mut fresh =
        Painter::<u8>::new(1.0, Rect::new(0.0, 0.0, 400.0, 300.0), &mut t2, None, THEME);
    fresh.replay(&ops);
    let s = under(&fresh.scene, LayerId::Base);
    assert_eq!(s[0].rect, [10.0, 20.0, 100.0, 50.0]);
    assert_eq!(s[2].params[1], 6.0);
}

#[test]
fn the_ops_follow_the_layer() {
    let scene = scene_of(|p| {
        p.on(LayerId::Popup);
        all(p);
    });
    assert_eq!(under(&scene, LayerId::Popup).len(), 3);
    assert!(under(&scene, LayerId::Base).is_empty());
}

#[test]
fn degenerate_inputs() {
    let mut radii = Vec::new();
    for radius in [
        [0.0, 10.0],
        [10.0, 0.0],
        [-1.0, 10.0],
        [f32::NAN, 10.0],
        [10.0, f32::INFINITY],
    ] {
        radii.push(radius);
    }
    for radius in radii {
        let scene = scene_of(|p| p.radial([50.0, 50.0], radius, RED, BLUE_HALF, Falloff::Linear));
        assert!(under(&scene, LayerId::Base).is_empty(), "{radius:?}");
    }

    // The op is still recorded for a radial that draws nothing.
    let mut t = FakeText;
    let mut p = painter(&mut t);
    p.start_recording();
    p.radial([50.0, 50.0], [0.0, 10.0], RED, BLUE_HALF, Falloff::Linear);
    assert_eq!(p.take_recording().len(), 1);

    for sigma in [f32::NAN, f32::INFINITY, -3.0, 0.0] {
        let scene = scene_of(|p| p.shadow(R, 4.0, sigma, RED));
        assert_eq!(under(&scene, LayerId::Base)[0].params, [4.0, 0.0, 0.0, 0.0]);
    }
}

#[test]
fn the_falloff_codes() {
    let code = |f: Falloff| {
        let scene = scene_of(|p| p.radial([50.0, 50.0], [10.0, 10.0], RED, BLUE_HALF, f));
        under(&scene, LayerId::Base)[0].params[1]
    };
    assert_eq!(code(Falloff::Linear), 0.0);
    assert_eq!(code(Falloff::Smooth), 1.0);
    assert_eq!(code(Falloff::Gaussian), 2.0);
}

#[test]
fn a_plain_fill_keeps_its_zero_params_and_under() {
    let scene = scene_of(|p| {
        p.fill(R, 4.0, RED);
        all(p);
    });
    let fill = under(&scene, LayerId::Base)[0];
    assert_eq!(fill.params, [0.0; 4]);
    assert_eq!(fill.under, [0.0; 4]);
}
