//! Running as a phone app. On iOS the toolkit's window must belong to a `UIWindowScene`: since the
//! iOS 27 SDK, UIKit refuses to launch an app that does not adopt the scene life cycle, and winit
//! 0.30 creates its `UIWindow` without a scene. An iOS app declares the scene manifest in its
//! `Info.plist` (`docs/mobile.md` has the keys) and calls [`attach_to_scene`] once its window
//! exists.
//!
//! Every item exists on every target; off iOS each one does nothing and says so.

use winit::window::Window;

use crate::platform::mobile as os;

/// What [`attach_to_scene`] found or did.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneAttach {
    /// The window belongs to a window scene: it was attached now or before.
    Attached,
    /// No window scene to attach to yet. On iOS the window is attached when one connects (an
    /// observer waits for it); this is also the answer off the main thread, or when the window
    /// has no `UIWindow` behind it, and then nothing was done.
    Waiting,
    /// This platform has no scenes (every target but iOS); nothing was done.
    NotNeeded,
}

/// Puts `window` in the app's window scene, so UIKit shows it under the scene life cycle (iOS).
///
/// On iOS, called on the main thread (the event loop's): when the window already has a scene it
/// answers [`SceneAttach::Attached`]; otherwise it attaches the window to the first connected
/// `UIWindowScene`, or registers one observer for the life of the process that attaches it to
/// every scene that connects later (a scene the OS discards in the background reconnects as a
/// new one), and answers [`SceneAttach::Waiting`]. A later call never registers a second observer,
/// so calling again is how an app asks for the state. An attach makes the window key and visible
/// only when it is not hidden: an app that keeps it hidden (to create its accessibility bridge
/// first) shows it itself.
///
/// Off iOS it does nothing and answers [`SceneAttach::NotNeeded`].
pub fn attach_to_scene(window: &Window) -> SceneAttach {
    os::attach_to_scene(window)
}
