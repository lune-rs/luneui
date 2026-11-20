//! iOS and Android: no OS clipboard backend yet. A read answers `Err` (the service turns it into
//! `ClipboardUnavailable`) and a write is dropped.

use crate::clipboard::Backend;

/// No backend: every read and write fails.
#[derive(Default)]
pub(crate) struct Os(());

impl Backend for Os {
    fn read(&mut self) -> Result<String, ()> {
        Err(())
    }

    fn write(&mut self, _text: &str) -> Result<(), ()> {
        Err(())
    }
}
