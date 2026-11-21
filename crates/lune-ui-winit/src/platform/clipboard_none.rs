//! iOS and Android: no OS clipboard backend in this crate (an iOS app gives the clipboard
//! `lune_ui_platform::mobile::Pasteboard`, which calls UIKit directly, with `Clipboard::with_backend`). A read answers `Err` (the service turns it into
//! `ClipboardUnavailable`) and a write is dropped.

use lune_ui_core::input::ClipboardBackend;

/// No backend: every read and write fails.
#[derive(Default)]
pub(crate) struct Os(());

impl ClipboardBackend for Os {
    fn read(&mut self) -> Result<String, ()> {
        Err(())
    }

    fn write(&mut self, _text: &str) -> Result<(), ()> {
        Err(())
    }
}
