//! Every platform but the browser: there is no textarea and no DOM, so the bridge is inert. The
//! window's own events reach the toolkit as winit gives them (an input method is winit's here).

use std::marker::PhantomData;

use lune_ui_core::input::Keymap;
use winit::event::{Ime, WindowEvent};
use winit::window::Window;

/// The keyboard conventions of the platform this was built for, as `lune_ui_winit::keymap()`
/// answers off the web: `Keymap::MAC` on macOS, `Keymap::PC` elsewhere.
pub(crate) fn keymap() -> Keymap {
    if cfg!(target_os = "macos") {
        Keymap::MAC
    } else {
        Keymap::PC
    }
}

/// Inert: `take` is always empty, the setters do nothing, every window event passes. `!Send`, as
/// the browser one is.
#[derive(Debug)]
pub(crate) struct TextInput {
    _not_send: PhantomData<*const ()>,
}

impl TextInput {
    pub(crate) fn attach(_window: &Window, _on_event: impl Fn() + 'static) -> Self {
        Self::inert()
    }

    pub(crate) fn inert() -> Self {
        Self {
            _not_send: PhantomData,
        }
    }

    pub(crate) fn set_allowed(&mut self, _allowed: bool) {}

    pub(crate) fn set_area(&mut self, _area: (f32, f32, f32, f32)) {}

    pub(crate) fn take(&mut self) -> Vec<Ime> {
        Vec::new()
    }

    pub(crate) fn passes(&mut self, _event: &WindowEvent) -> bool {
        true
    }

    pub(crate) fn take_focus(&mut self) -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use winit::event::{Ime, WindowEvent};

    use super::TextInput;

    #[test]
    fn off_the_web_nothing_is_taken_and_every_event_passes() {
        let mut input = TextInput::inert();
        input.set_allowed(true);
        input.set_area((1.0, 2.0, 3.0, 4.0));
        assert!(input.take().is_empty());
        assert_eq!(input.take_focus(), None);
        assert!(input.passes(&WindowEvent::Focused(true)));
        assert!(input.passes(&WindowEvent::Focused(false)));
        assert!(input.passes(&WindowEvent::Ime(Ime::Enabled)));
        input.set_allowed(false);
        assert!(input.take().is_empty());
    }
}
