//! Per-contour outline growth, with the push orientation as a parameter.
//!
//! Copied from swash 0.2.10, `src/scale/outline.rs` (`LayerMut::embolden`, the private `embolden`
//! and `compute_winding`), (c) 2020 Chad Brokaw, MIT OR Apache-2.0 (`licenses/swash-LICENSE-MIT`).
//! swash's `embolden` is a port of FreeType's `FT_Outline_EmboldenXY` (FreeType licence,
//! `licenses/FreeType-FTL.txt`). What changed: swash computes one winding for the whole layer and
//! keeps it private; here the winding is an argument of [`grow_layer`], and [`grow`] takes it from
//! an [`Orient`]. The arithmetic, its order and the contour split are swash's, bit for bit.
//!
//! The orientation of a layer is FreeType's rule (`FT_Outline_Get_Orientation`): the sum of its
//! contours' own areas ([`summed_winding`]). swash instead takes one sign over the contours
//! joined end to end, which the bridges between far-apart contours can flip against every
//! contour's own direction, and then the growth pushes inward and the glyph thins.

use swash::scale::outline::Outline;
use swash::zeno::{Point, Verb};

/// Which way [`grow`] pushes each layer's contours out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Orient {
    /// The layer's summed contour areas decide ([`summed_winding`]): FreeType's rule.
    Summed,
    /// The opposite of [`Orient::Summed`], the second try when the first thins the glyph.
    Opposite,
}

/// swash's `compute_winding`: the sign of one shoelace sum over the whole point list (every
/// contour of the layer concatenated), `> 0.0` giving 1, anything else 0. What swash's own
/// `Outline::embolden` pushes by; the tests compare against it.
#[cfg(test)]
pub(super) fn concat_winding(points: &[Point]) -> u8 {
    if contour_area(points) > 0. { 1 } else { 0 }
}

/// One contour's term of swash's shoelace sum, wrapping from its last point to its first.
fn contour_area(points: &[Point]) -> f32 {
    if points.is_empty() {
        return 0.;
    }
    let mut area = 0.;
    let last = points.len() - 1;
    let mut prev = points[last];
    for cur in points[0..=last].iter() {
        area += (cur.y - prev.y) * (cur.x + prev.x);
        prev = *cur;
    }
    area
}

/// FreeType's orientation of one layer: swash's per-contour shoelace term summed over the layer's
/// contours (split on `MoveTo` and `Close` as [`grow_layer`] splits them, summed in contour
/// order), `> 0.0` giving 1, anything else 0; no contours give 0.
pub(super) fn summed_winding(points: &[Point], verbs: &[Verb]) -> u8 {
    let mut total = 0.;
    let mut add = |range: std::ops::Range<usize>| {
        if let Some(contour) = points.get(range) {
            total += contour_area(contour);
        }
    };
    let mut point_start = 0;
    let mut pos = 0;
    for verb in verbs {
        match verb {
            Verb::MoveTo | Verb::Close => {
                add(point_start..pos);
                point_start = pos;
                if *verb == Verb::MoveTo {
                    pos += 1;
                }
            }
            Verb::LineTo => pos += 1,
            Verb::QuadTo => pos += 2,
            Verb::CurveTo => pos += 3,
        }
    }
    if pos > point_start {
        add(point_start..pos);
    }
    if total > 0. { 1 } else { 0 }
}

/// swash's `LayerMut::embolden` with the winding passed in: grows every contour of one layer by
/// `dx`, `dy`, the contours split on `MoveTo` and `Close` exactly as swash splits them (an
/// out-of-range slice stops the work, as there).
pub(super) fn grow_layer(points: &mut [Point], verbs: &[Verb], winding: u8, dx: f32, dy: f32) {
    let mut point_start = 0;
    let mut pos = 0;
    for verb in verbs {
        match verb {
            Verb::MoveTo | Verb::Close => {
                if let Some(points) = points.get_mut(point_start..pos) {
                    if !points.is_empty() {
                        embolden(points, winding, dx, dy);
                    }
                    point_start = pos;
                    if *verb == Verb::MoveTo {
                        pos += 1;
                    }
                } else {
                    return;
                }
            }
            Verb::LineTo => pos += 1,
            Verb::QuadTo => pos += 2,
            Verb::CurveTo => pos += 3,
        }
    }
    if pos > point_start {
        if let Some(points) = points.get_mut(point_start..pos) {
            embolden(points, winding, dx, dy);
        }
    }
}

/// swash's `Outline::embolden` with the orientation chosen per layer: every layer grown by `dx`,
/// `dy`, pushed the way its [`summed_winding`] says (`Summed`) or the other way (`Opposite`).
pub(super) fn grow(outline: &mut Outline, orient: Orient, dx: f32, dy: f32) {
    for i in 0..outline.len() {
        if let Some(mut layer) = outline.get_mut(i) {
            let verbs = layer.verbs();
            let points = layer.points_mut();
            let summed = summed_winding(points, verbs);
            let winding = match orient {
                Orient::Summed => summed,
                Orient::Opposite => 1 - summed,
            };
            grow_layer(points, verbs, winding, dx, dy);
        }
    }
}

// swash's per-contour `embolden`, copied as is: a rewrite that reorders float operations breaks
// the bit-exact test below (`copy_at_the_concatenated_sign_is_swashs_embolden`).
fn embolden(points: &mut [Point], winding: u8, x_strength: f32, y_strength: f32) {
    if points.is_empty() {
        return;
    }
    let last = points.len() - 1;
    let mut i = last;
    let mut j = 0;
    let mut k = !0;
    let mut out_len;
    let mut in_len = 0.;
    let mut anchor_len = 0.;
    let mut anchor = Point::ZERO;
    let mut out;
    let mut in_ = Point::ZERO;
    while j != i && i != k {
        if j != k {
            out = points[j] - points[i];
            out_len = out.length();
            if out_len == 0. {
                j = if j < last { j + 1 } else { 0 };
                continue;
            } else {
                let s = 1. / out_len;
                out.x *= s;
                out.y *= s;
            }
        } else {
            out = anchor;
            out_len = anchor_len;
        }
        if in_len != 0. {
            if k == !0 {
                k = i;
                anchor = in_;
                anchor_len = in_len;
            }
            let mut d = (in_.x * out.x) + (in_.y * out.y);
            let shift = if d > -0.9396 {
                d += 1.;
                let mut sx = in_.y + out.y;
                let mut sy = in_.x + out.x;
                if winding == 0 {
                    sx = -sx;
                } else {
                    sy = -sy;
                }
                let mut q = (out.x * in_.y) - (out.y * in_.x);
                if winding == 0 {
                    q = -q;
                }
                let l = in_len.min(out_len);
                if x_strength * q <= l * d {
                    sx = sx * x_strength / d;
                } else {
                    sx = sx * l / q;
                }
                if y_strength * q <= l * d {
                    sy = sy * y_strength / d;
                } else {
                    sy = sy * l / q;
                }
                Point::new(sx, sy)
            } else {
                Point::ZERO
            };

            while i != j {
                points[i].x += x_strength + shift.x;
                points[i].y += y_strength + shift.y;
                i = if i < last { i + 1 } else { 0 };
            }
        } else {
            i = j;
        }
        in_ = out;
        in_len = out_len;
        j = if j < last { j + 1 } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lune_ui_core::text::TextRendering;
    use swash::FontRef;
    use swash::scale::ScaleContext;

    const SANS: &[u8] = include_bytes!("../../../../apps/gallery/assets/RznSans-Regular.ttf");
    const MONO: &[u8] =
        include_bytes!("../../../../apps/gallery/assets/RobotoMonoNerdFontMono-Regular.ttf");

    fn bits(outline: &Outline) -> Vec<(u32, u32)> {
        outline
            .points()
            .iter()
            .map(|p| (p.x.to_bits(), p.y.to_bits()))
            .collect()
    }

    /// swash's own orientation: [`grow`] with each layer's concatenated sign.
    fn grow_concat(outline: &mut Outline, dx: f32, dy: f32) {
        for i in 0..outline.len() {
            if let Some(mut layer) = outline.get_mut(i) {
                let verbs = layer.verbs();
                let points = layer.points_mut();
                let winding = concat_winding(points);
                grow_layer(points, verbs, winding, dx, dy);
            }
        }
    }

    /// What one scan of a font found.
    #[derive(Default)]
    struct Scan {
        /// Glyphs with an outline.
        compared: usize,
        /// Glyphs where `Orient::Summed` was compared with swash (every layer's summed and
        /// concatenated orientations agree).
        same_orientation: usize,
        /// Glyph ids with a layer whose summed and concatenated orientations differ.
        mismatched: Vec<u16>,
    }

    /// Every glyph of `data` at `size` (hinting as given), grown by SMOOTH's dilation at `size`
    /// and compared bit for bit with swash's `Outline::embolden`: through the concatenated sign
    /// always (E1), through `Orient::Summed` where the two orientations agree (E4). `stride`
    /// takes every `stride`-th glyph id.
    fn scan(data: &[u8], size: f32, hint: bool, stride: usize) -> Scan {
        let font = FontRef::from_index(data, 0).expect("the font parses");
        let [dx, dy] = TextRendering::SMOOTH.glyph_params(size).dilation();
        assert!(dx > 0.0 && dy > 0.0, "SMOOTH dilates at {size}");
        let mut ctx = ScaleContext::new();
        let mut scaler = ctx.builder(font).size(size).hint(hint).build();
        let mut found = Scan::default();
        for id in (0..font.glyph_metrics(&[]).glyph_count()).step_by(stride) {
            let Some(outline) = scaler.scale_outline(id) else {
                continue;
            };
            if outline.points().is_empty() {
                continue;
            }
            let mut want = outline.clone();
            want.embolden(dx, dy);
            let mut concat = outline.clone();
            grow_concat(&mut concat, dx, dy);
            assert_eq!(bits(&concat), bits(&want), "E1: glyph {id} at {size} px");
            found.compared += 1;
            let agree = (0..outline.len()).all(|i| {
                let layer = outline.get(i).expect("the layer exists");
                summed_winding(layer.points(), layer.verbs()) == concat_winding(layer.points())
            });
            if agree {
                let mut got = outline;
                grow(&mut got, Orient::Summed, dx, dy);
                assert_eq!(bits(&got), bits(&want), "E4: glyph {id} at {size} px");
                found.same_orientation += 1;
            } else {
                found.mismatched.push(id);
            }
        }
        found
    }

    /// E1: the copy at swash's concatenated sign matches swash bit for bit on every glyph of
    /// both gallery fonts.
    #[test]
    fn copy_at_the_concatenated_sign_is_swashs_embolden() {
        let mut total = 0;
        for (name, data, min) in [("RznSans", SANS, 400), ("RobotoMono", MONO, 5000)] {
            for (size, hint) in [(13.0, false), (26.0, false), (13.0, true)] {
                let n = scan(data, size, hint, 1).compared;
                println!("E1 {name} {size} px hinted={hint}: {n} glyphs compared");
                assert!(n >= min, "{name} {size} px: only {n} glyphs compared");
                total += n;
            }
        }
        println!("E1: {total} glyph comparisons");
    }

    /// E4: where a glyph's summed orientation equals the concatenated one, `Orient::Summed` is
    /// swash's embolden bit for bit (0.5.1's bytes); RznSans's only mismatches are the vulgar
    /// fractions.
    #[test]
    fn summed_orientation_is_swashs_where_they_agree() {
        let sans = FontRef::from_index(SANS, 0).expect("the font parses");
        let fractions: Vec<u16> = ['\u{2153}', '\u{215d}']
            .iter()
            .map(|&c| sans.charmap().map(c))
            .collect();
        for (name, data, min) in [("RznSans", SANS, 400), ("RobotoMono", MONO, 5000)] {
            for size in [13.0, 26.0] {
                let found = scan(data, size, false, 1);
                println!(
                    "E4 {name} {size} px: {} compared, {} skipped as mismatched",
                    found.same_orientation,
                    found.mismatched.len()
                );
                assert!(
                    found.same_orientation >= min,
                    "{name} {size} px: only {} compared",
                    found.same_orientation
                );
                if name == "RznSans" {
                    assert_eq!(found.mismatched, fractions, "RznSans {size} px");
                }
            }
        }
    }

    const VERBS: [Verb; 10] = [
        Verb::MoveTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::Close,
        Verb::MoveTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::LineTo,
        Verb::Close,
    ];

    fn pts(raw: [(f32, f32); 8]) -> Vec<Point> {
        raw.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    /// Two counter-clockwise squares: swash's concatenated sign is 0 (it thins), though the
    /// contours' own areas sum to a positive number.
    fn trigger() -> Vec<Point> {
        pts([
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (110.0, 10.0),
            (100.0, 10.0),
            (100.0, 0.0),
            (110.0, 0.0),
        ])
    }

    /// The trigger mirrored about x = 60, each contour kept counter-clockwise.
    fn mirrored() -> Vec<Point> {
        pts([
            (120.0, 10.0),
            (110.0, 10.0),
            (110.0, 0.0),
            (120.0, 0.0),
            (20.0, 10.0),
            (10.0, 10.0),
            (10.0, 0.0),
            (20.0, 0.0),
        ])
    }

    /// Standard signed area (positive counter-clockwise, y up) of each of the two squares.
    fn areas(points: &[Point]) -> [f64; 2] {
        let one = |sq: &[Point]| {
            let mut s = 0.0;
            for (i, a) in sq.iter().enumerate() {
                let b = sq[(i + 1) % sq.len()];
                s += f64::from(a.x) * f64::from(b.y) - f64::from(b.x) * f64::from(a.y);
            }
            s / 2.0
        };
        [one(&points[..4]), one(&points[4..])]
    }

    /// The side lengths (width, height) of each square's bounding box.
    fn boxes(points: &[Point]) -> [(f32, f32); 2] {
        let one = |sq: &[Point]| {
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in sq {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            (x1 - x0, y1 - y0)
        };
        [one(&points[..4]), one(&points[4..])]
    }

    fn grown(points: &[Point], winding: u8) -> Vec<Point> {
        let mut p = points.to_vec();
        grow_layer(&mut p, &VERBS, winding, 0.3, 0.3);
        p
    }

    /// E2r: on the trigger the concatenated sign is 0 and shrinks both squares, though the
    /// summed orientation is 1; `Orient::Summed` grows each to 10.6 x 10.6.
    #[test]
    fn synthetic_trigger_grows_at_the_summed_orientation() {
        let t = trigger();
        assert_eq!(concat_winding(&t), 0);
        assert_eq!(summed_winding(&t, &VERBS), 1);
        let [a, b] = areas(&t);
        assert!(
            a + b > 0.0,
            "the contours' own areas sum positive: {}",
            a + b
        );
        let [a1, b1] = areas(&grown(&t, concat_winding(&t)));
        assert!(
            a1 + b1 < a + b,
            "concatenated shrinks: {} -> {}",
            a + b,
            a1 + b1
        );
        let mut outline_like = t.clone();
        let summed = summed_winding(&outline_like, &VERBS);
        grow_layer(&mut outline_like, &VERBS, summed, 0.3, 0.3);
        let [a2, b2] = areas(&outline_like);
        assert!(a2 + b2 > a + b, "summed grows: {} -> {}", a + b, a2 + b2);
        for (w, h) in boxes(&outline_like) {
            assert!(
                (w - 10.6).abs() < 1e-3 && (h - 10.6).abs() < 1e-3,
                "{w} x {h}"
            );
        }
    }

    /// E3r: the mirrored pair has summed and concatenated orientation 1; grown at the summed one
    /// its area equals the trigger's grown at the summed one.
    #[test]
    fn mirrored_pair_grows_like_the_trigger() {
        let m = mirrored();
        assert_eq!(summed_winding(&m, &VERBS), 1);
        assert_eq!(concat_winding(&m), 1);
        let [ma, mb] = areas(&grown(&m, summed_winding(&m, &VERBS)));
        let t = trigger();
        let [ta, tb] = areas(&grown(&t, summed_winding(&t, &VERBS)));
        let (got, want) = (ma + mb, ta + tb);
        assert!(((got - want) / want).abs() < 1e-5, "{got} vs {want}");
    }
}
