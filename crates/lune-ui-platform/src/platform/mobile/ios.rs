//! iOS: the window scene attach, through UIKit's typed bindings (`objc2-ui-kit`). winit 0.30
//! creates its `UIWindow` without a scene; under a scene manifest UIKit shows only windows that
//! belong to a `UIWindowScene`, so the window is handed to the connected one, or to the next one
//! that connects.
//!
//! Every UIKit call runs on the main thread: each entry checks it with `MainThreadMarker::new` and
//! does nothing elsewhere.

use std::cell::RefCell;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::{Retained, Weak};
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{NSNotification, NSNotificationCenter};
use objc2_ui_kit::{
    UIApplication, UISceneWillConnectNotification, UIView, UIWindow, UIWindowScene,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use lune_ui_core::layout::Insets;

use super::insets_from_geometry;
use crate::mobile::SceneAttach;

/// The scene observer's state, on the main thread only.
#[derive(Default)]
struct Observer {
    /// The window to attach to a scene that connects: the last one `attach_to_scene` was given
    /// without a scene. Weak, so the observer never keeps a window winit dropped alive.
    window: Option<Weak<UIWindow>>,
    /// The registration's token, held for the life of the process: the observer is never
    /// removed, because a scene the OS discards in the background reconnects as a new one.
    token: Option<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
}

thread_local! {
    static OBSERVER: RefCell<Observer> = RefCell::new(Observer::default());
}

/// The `UIWindow` behind `window`'s view, or `None` when there is no UIKit handle or no window.
fn ui_window(window: &Window, _mtm: MainThreadMarker) -> Option<Retained<UIWindow>> {
    let RawWindowHandle::UiKit(handle) = window.window_handle().ok()?.as_raw() else {
        return None;
    };
    // SAFETY: winit guarantees `ui_view` is a live UIView while the window exists, and the
    // marker proves this runs on the main thread, where UIKit objects may be used.
    let view: &UIView = unsafe { handle.ui_view.cast().as_ref() };
    view.window()
}

/// The first connected window scene, if any.
fn first_window_scene(mtm: MainThreadMarker) -> Option<Retained<UIWindowScene>> {
    UIApplication::sharedApplication(mtm)
        .connectedScenes()
        .iter()
        .find_map(|scene| scene.downcast::<UIWindowScene>().ok())
}

/// Puts `window` in `scene`, and makes it key and visible unless it is hidden.
///
/// winit set the root view controller before the window had a scene, so the controller keeps the
/// portrait interface orientation and the portrait safe area (docs/mobile.md, M4 reading, R6). It
/// is re-seated after the scene is set, so it takes the scene's orientation.
fn attach(window: &UIWindow, scene: &UIWindowScene) {
    let hidden = window.isHidden();
    window.setWindowScene(Some(scene));
    if let Some(root) = window.rootViewController() {
        window.setRootViewController(None);
        window.setRootViewController(Some(&root));
    }
    if !hidden {
        window.makeKeyAndVisible();
    }
}

/// Called by the notification centre on the posting thread when a scene connects.
fn scene_connected(note: NonNull<NSNotification>) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: the notification centre passes a live NSNotification for the block's duration.
    let note = unsafe { note.as_ref() };
    let Some(scene) = note
        .object()
        .and_then(|o| o.downcast::<UIWindowScene>().ok())
    else {
        return;
    };
    let window = OBSERVER.with(|o| o.borrow().window.as_ref().and_then(Weak::load));
    if let Some(window) = window {
        attach(&window, &scene);
    }
}

/// Remembers `window` for the observer, and registers the observer once per process.
fn wait_for_scene(window: &Retained<UIWindow>) {
    OBSERVER.with(|o| {
        let mut o = o.borrow_mut();
        o.window = Some(Weak::from_retained(window));
        if o.token.is_some() {
            return;
        }
        let block = RcBlock::new(scene_connected);
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: the name is UIKit's own extern static, always initialised once UIKit is loaded;
        // `obj` is `None` (any sender); `queue` is `None`, so the block runs on the posting
        // thread, which for UIKit's scene notifications is the main thread, and `scene_connected`
        // checks it again. The block captures nothing, so it is safe to send.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(UISceneWillConnectNotification),
                None,
                None,
                &block,
            )
        };
        o.token = Some(token);
    });
}

pub(crate) fn attach_to_scene(window: &Window) -> SceneAttach {
    let Some(mtm) = MainThreadMarker::new() else {
        return SceneAttach::Waiting;
    };
    let Some(ui_window) = ui_window(window, mtm) else {
        return SceneAttach::Waiting;
    };
    if ui_window.windowScene().is_some() {
        return SceneAttach::Attached;
    }
    if let Some(scene) = first_window_scene(mtm) {
        attach(&ui_window, &scene);
        return SceneAttach::Attached;
    }
    wait_for_scene(&ui_window);
    SceneAttach::Waiting
}

/// The safe-area insets from winit's live queries (on iOS `inner_*` is the safe area and `outer_*`
/// the screen).
pub(crate) fn safe_area(window: &Window) -> Insets {
    let (Ok(inner_pos), Ok(outer_pos)) = (window.inner_position(), window.outer_position()) else {
        return Insets::ZERO;
    };
    let (inner, outer) = (window.inner_size(), window.outer_size());
    insets_from_geometry(
        (inner_pos.x, inner_pos.y),
        (outer_pos.x, outer_pos.y),
        (inner.width, inner.height),
        (outer.width, outer.height),
        window.scale_factor(),
    )
}
