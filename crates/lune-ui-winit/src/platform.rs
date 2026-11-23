//! Per-platform window attributes (the glass look and the popover window of each platform) and keyboard conventions. The bodies are in
//! `platform/`, one file per platform, selected by `cfg`.

#[cfg(any(target_os = "ios", target_os = "android"))]
mod clipboard_none;
#[cfg(not(any(target_os = "ios", target_os = "android", target_family = "wasm")))]
mod clipboard_os;
#[cfg(target_family = "wasm")]
mod clipboard_web;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(any(target_os = "ios", target_os = "android"))]
pub(crate) use clipboard_none::source as clipboard_source;
#[cfg(not(any(target_os = "ios", target_os = "android", target_family = "wasm")))]
pub(crate) use clipboard_os::source as clipboard_source;
#[cfg(target_family = "wasm")]
pub(crate) use clipboard_web::source as clipboard_source;
#[cfg(target_os = "macos")]
pub(crate) use macos::{glass, keymap, panel, popover};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) use other::{glass, keymap, panel, popover};
#[cfg(target_os = "windows")]
pub(crate) use windows::{glass, keymap, panel, popover};
