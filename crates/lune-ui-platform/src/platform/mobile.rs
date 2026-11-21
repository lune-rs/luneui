//! The OS code of the phone functions, one file per platform; the safe API is `crate::mobile`.

#[cfg(target_os = "ios")]
mod ios;
#[cfg(not(target_os = "ios"))]
mod other;

#[cfg(target_os = "ios")]
pub(crate) use ios::{KeyboardInset, attach_to_scene, safe_area};
#[cfg(not(target_os = "ios"))]
pub(crate) use other::{KeyboardInset, attach_to_scene, safe_area};

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

/// How far the keyboard's top edge reaches into a view of height `view_height`, in points: the
/// view's height less the keyboard's top (`keyboard_top`, in the view's own space), clamped to
/// `0..=view_height`. A value that is not finite gives 0.
#[cfg(any(target_os = "ios", test))]
pub(crate) fn keyboard_overlap(view_height: f64, keyboard_top: f64) -> f32 {
    let overlap = view_height - keyboard_top;
    if !overlap.is_finite() || !view_height.is_finite() {
        return 0.0;
    }
    overlap.clamp(0.0, view_height.max(0.0)) as f32
}

#[cfg(test)]
mod tests {
    use lune_ui_core::layout::Insets;

    use super::{insets_from_geometry, keyboard_overlap};

    #[test]
    fn a_keyboard_in_the_lower_part_of_the_view_overlaps_it_by_its_height() {
        // 874 points high view, keyboard top at 538 (the iPhone 16 Pro's portrait keyboard).
        assert_eq!(keyboard_overlap(874.0, 538.0), 336.0);
    }

    #[test]
    fn a_keyboard_below_the_view_or_above_its_top_is_clamped() {
        assert_eq!(keyboard_overlap(874.0, 874.0), 0.0);
        assert_eq!(keyboard_overlap(874.0, 900.0), 0.0);
        assert_eq!(keyboard_overlap(874.0, -50.0), 874.0);
    }

    #[test]
    fn a_keyboard_frame_that_is_not_finite_gives_zero() {
        assert_eq!(keyboard_overlap(874.0, f64::NAN), 0.0);
        assert_eq!(keyboard_overlap(874.0, f64::INFINITY), 0.0);
        assert_eq!(keyboard_overlap(874.0, f64::NEG_INFINITY), 0.0);
        assert_eq!(keyboard_overlap(f64::NAN, 538.0), 0.0);
    }

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
