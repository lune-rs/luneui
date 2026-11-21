//! Every platform but iOS: there are no scenes, so nothing is attached, and no safe area is reported
//! (a desktop window's title bar is not one).

use std::marker::PhantomData;

use lune_ui_core::layout::Insets;
use winit::window::Window;

use crate::mobile::SceneAttach;

pub(crate) fn attach_to_scene(_window: &Window) -> SceneAttach {
    SceneAttach::NotNeeded
}

pub(crate) fn safe_area(_window: &Window) -> Insets {
    Insets::ZERO
}

/// Inert: no soft keyboard is observed, so the inset is always zero and `on_change` never runs.
/// `!Send`, as the iOS one is.
#[derive(Debug)]
pub(crate) struct KeyboardInset {
    _not_send: PhantomData<*const ()>,
}

impl KeyboardInset {
    pub(crate) fn new(_window: &Window, _on_change: impl Fn() + 'static) -> Self {
        Self {
            _not_send: PhantomData,
        }
    }

    pub(crate) fn inset(&self) -> Insets {
        Insets::ZERO
    }
}
