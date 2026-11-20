//! Every platform but iOS: there are no scenes, so nothing is attached.

use winit::window::Window;

use crate::mobile::SceneAttach;

pub(crate) fn attach_to_scene(_window: &Window) -> SceneAttach {
    SceneAttach::NotNeeded
}
