//! Every platform but iOS: there are no scenes, so nothing is attached, and no safe area is reported
//! (a desktop window's title bar is not one).

use lune_ui_core::layout::Insets;
use winit::window::Window;

use crate::mobile::SceneAttach;

pub(crate) fn attach_to_scene(_window: &Window) -> SceneAttach {
    SceneAttach::NotNeeded
}

pub(crate) fn safe_area(_window: &Window) -> Insets {
    Insets::ZERO
}
