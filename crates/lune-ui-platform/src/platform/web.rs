//! The OS code of the web text bridge, one file per platform; the safe API is `crate::web`. The
//! pure routing rules (`keys`) compile for the tests on every host.

#[cfg(any(target_family = "wasm", test))]
mod keys;
#[cfg(not(target_family = "wasm"))]
mod other;
#[cfg(target_family = "wasm")]
mod wasm;

#[cfg(not(target_family = "wasm"))]
pub(crate) use other::{TextInput, keymap};
#[cfg(target_family = "wasm")]
pub(crate) use wasm::{TextInput, keymap};
