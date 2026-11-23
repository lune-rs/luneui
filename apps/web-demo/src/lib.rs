//! The web demo: a fully animated showcase on the toolkit. A glass frame (a side rail of six scenes
//! and a stage, or on a phone a title bar and a bottom tab bar) floats over a slowly moving
//! background, and the scenes change with a push. It runs in a browser on a `<canvas>` (WebGPU, or
//! WebGL2 where there is none: [`web`]) and natively in a window, and `--dump` writes it headless.
//!
//! App code only (D9): it uses the facade and changes nothing under `crates/`. The screens and their
//! state are in [`stage`] and [`scenes`], shared by the window shell ([`shell`]), the headless dump
//! ([`dump`]) and the tests; every colour comes from the theme.

#![deny(unsafe_code)]

#[cfg(not(target_family = "wasm"))]
pub mod dump;
pub mod scenes;
pub mod shell;
pub mod stage;
#[cfg(target_family = "wasm")]
pub mod web;

pub use shell::{Options, parse_args, run};

use lune_ui::scene::FrameParams;
use lune_ui::text::TextCache;

const UI_FONT: &str = "RznSans";
const MONO_FONT: &str = "RobotoMono Nerd Font Mono";

/// The backdrop blur's sigma (physical pixels) and saturation: the gallery's values.
const BLUR_SIGMA: f32 = 8.0;
const BLUR_SATURATION: f32 = 1.8;

/// The app's fonts: the gallery's own files (D10), loaded in a fixed order.
pub fn text_cache() -> TextCache {
    TextCache::new(
        &[
            include_bytes!("../../gallery/assets/RobotoMonoNerdFontMono-Regular.ttf").as_slice(),
            include_bytes!("../../gallery/assets/RznSans-Regular.ttf"),
            include_bytes!("../../gallery/assets/RznSans-Medium.ttf"),
        ],
        UI_FONT,
        MONO_FONT,
    )
}

/// The frame parameters for a target of `size` physical pixels at `scale`.
pub(crate) fn frame_params(size: (u32, u32), scale: f32) -> FrameParams {
    FrameParams {
        size,
        scale,
        blur_sigma: BLUR_SIGMA,
        saturation: BLUR_SATURATION,
        ..FrameParams::default()
    }
}
