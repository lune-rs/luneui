//! The OS code of the phone functions, one file per platform; the safe API is `crate::mobile`.

#[cfg(target_os = "ios")]
mod ios;
#[cfg(not(target_os = "ios"))]
mod other;

#[cfg(target_os = "ios")]
pub(crate) use ios::{attach_to_scene, safe_area};
#[cfg(not(target_os = "ios"))]
pub(crate) use other::{attach_to_scene, safe_area};

/// The safe-area insets, in logical pixels, from a window's geometry in physical pixels: the offset
/// of the inner rectangle inside the outer one on the left and top, the rest on the right and
/// bottom, each clamped to at least 0. A scale that is not a positive finite number gives zero.
#[cfg(any(target_os = "ios", test))]
pub(crate) fn insets_from_geometry(
    inner_pos: (i32, i32),
    outer_pos: (i32, i32),
    inner_size: (u32, u32),
    outer_size: (u32, u32),
    scale: f64,
) -> lune_ui_core::layout::Insets {
    use lune_ui_core::layout::Insets;
    if !(scale.is_finite() && scale > 0.0) {
        return Insets::ZERO;
    }
    let left = f64::from(inner_pos.0) - f64::from(outer_pos.0);
    let top = f64::from(inner_pos.1) - f64::from(outer_pos.1);
    let right = f64::from(outer_size.0) - f64::from(inner_size.0) - left;
    let bottom = f64::from(outer_size.1) - f64::from(inner_size.1) - top;
    let logical = |v: f64| {
        let l = (v / scale).max(0.0);
        if l.is_finite() { l as f32 } else { 0.0 }
    };
    Insets::new(logical(left), logical(top), logical(right), logical(bottom))
}

#[cfg(test)]
mod tests {
    use lune_ui_core::layout::Insets;

    use super::insets_from_geometry;

    #[test]
    fn the_ios_portrait_reading_gives_zero_62_zero_34() {
        // iOS 18.0, iPhone 16 Pro (docs/mobile.md, M1 reading): inner 1206x2334 at 0,186, outer
        // 1206x2622 at 0,0, scale 3.
        let got = insets_from_geometry((0, 186), (0, 0), (1206, 2334), (1206, 2622), 3.0);
        assert_eq!(got, Insets::new(0.0, 62.0, 0.0, 34.0));
    }

    #[test]
    fn a_landscape_reading_has_left_and_right_and_no_top() {
        // A made-up landscape geometry (59 points each side), not a reading.
        let got = insets_from_geometry((177, 0), (0, 0), (2268, 1206), (2622, 1206), 3.0);
        assert_eq!(got, Insets::new(59.0, 0.0, 59.0, 0.0));
    }

    #[test]
    fn a_negative_value_is_clamped_to_zero() {
        let got = insets_from_geometry((-3, -3), (0, 0), (110, 110), (100, 100), 1.0);
        assert_eq!(got, Insets::ZERO);
    }

    #[test]
    fn a_scale_that_is_not_positive_and_finite_gives_zero() {
        for scale in [0.0, -2.0, f64::NAN, f64::INFINITY] {
            let got = insets_from_geometry((0, 186), (0, 0), (1206, 2334), (1206, 2622), scale);
            assert_eq!(got, Insets::ZERO, "scale {scale}");
        }
    }
}
