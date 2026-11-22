//! Android: the safe area and the soft keyboard's inset from the activity's content rect, and the
//! system clipboard through JNI. There is no scene to attach to.
//!
//! winit reports no event for either inset on Android (its `ContentRectChanged` only logs), so
//! both are read per frame from `WindowExtAndroid::content_rect` (D7, D8). The keyboard's inset is
//! the same rect split against a baseline: the smallest bottom inset seen at the window's size, the
//! bottom the system bars alone cover (`super::split_bottom`).
//!
//! `unsafe` is here and nowhere else in the crate's mobile code: the JNI handles `ndk_context`
//! holds (D9).

use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Mutex;

use jni::errors::Error;
use jni::objects::{JObject, JString};
use jni::{JValue, JavaVM, jni_sig, jni_str};
use lune_ui_core::layout::Insets;
use winit::platform::android::WindowExtAndroid;
use winit::window::Window;

use super::{BottomRecord, insets_from_content_rect, split_bottom};
use crate::mobile::SceneAttach;

pub(crate) fn attach_to_scene(_window: &Window) -> SceneAttach {
    SceneAttach::NotNeeded
}

/// The baseline record and the keyboard bottom the last `safe_area` call computed, in logical
/// pixels. `safe_area` writes it and `KeyboardInset::inset` reads it, so an app calls `safe_area`
/// before `inset` each frame. A poisoned lock answers zero keyboard and the content rect's own
/// bottom.
static STATE: Mutex<Option<(BottomRecord, f32)>> = Mutex::new(None);

pub(crate) fn safe_area(window: &Window) -> Insets {
    let rect = window.content_rect();
    let size = window.inner_size();
    let raw = insets_from_content_rect(
        (rect.left, rect.top, rect.right, rect.bottom),
        (size.width, size.height),
        window.scale_factor(),
    );
    let Ok(mut state) = STATE.lock() else {
        return raw;
    };
    let (record, safe_bottom, keyboard) = split_bottom(
        state.map(|(record, _)| record),
        (size.width, size.height),
        raw.bottom,
    );
    *state = Some((record, keyboard));
    Insets::new(raw.left, raw.top, raw.right, safe_bottom)
}

/// Reads the keyboard's overlap `safe_area` last computed; no observer exists on Android, so
/// `on_change` never runs and the app sees a change on its next frame. `!Send`, as the others are.
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
        match STATE.lock() {
            Ok(state) => state.map_or(Insets::ZERO, |(_, keyboard)| {
                Insets::new(0.0, 0.0, 0.0, keyboard)
            }),
            Err(_) => Insets::ZERO,
        }
    }
}

/// The system clipboard (`ClipboardManager`) through JNI (D9), with the Application context
/// `android-activity` hands to `ndk_context`. `!Send`, as the others are.
#[derive(Debug, Default)]
pub(crate) struct Pasteboard {
    _not_send: PhantomData<*const ()>,
}

/// Runs `f` with the JNI environment of the current thread (attached when it is not) and the
/// Application object, in a local frame that releases every local reference at its end. Any error,
/// a Java exception (the attach wrapper catches and clears it) or a panic is `Err(())`.
fn with_application<T>(
    f: impl FnOnce(&mut jni::Env, &JObject) -> Result<T, Error>,
) -> Result<T, ()> {
    let context = ndk_context::android_context();
    if context.vm().is_null() || context.context().is_null() {
        return Err(());
    }
    let run = || {
        // SAFETY: `android-activity` stores the process's `JavaVM*` here before `android_main`
        // (init.rs, `initialize_android_context`); the VM lives for the process and the pointer
        // was checked non-null above. `from_raw` only wraps it.
        let vm = unsafe { JavaVM::from_raw(context.vm().cast()) };
        vm.attach_current_thread(|env| {
            // SAFETY: `ndk_context` holds a global reference to the Application (made by
            // `android-activity` and never deleted, so valid for the process and on every thread).
            // `JObject` has no `Drop`: this wrapper never deletes the reference it borrows, and it
            // does not outlive this callback.
            let application = unsafe { JObject::from_raw(env, context.context().cast()) };
            f(env, &application)
        })
    };
    catch_unwind(AssertUnwindSafe(run))
        .map_err(|_| ())?
        .map_err(|_| ())
}

/// The `ClipboardManager` of `application`, or an error when there is none.
fn clipboard_manager<'local>(
    env: &mut jni::Env<'local>,
    application: &JObject,
) -> Result<JObject<'local>, Error> {
    let name = env.new_string("clipboard")?;
    let manager = env
        .call_method(
            application,
            jni_str!("getSystemService"),
            jni_sig!((name: java.lang.String) -> java.lang.Object),
            &[JValue::Object(&name)],
        )?
        .l()?;
    if manager.is_null() {
        return Err(Error::NullPtr("clipboard service"));
    }
    Ok(manager)
}

impl Pasteboard {
    pub(crate) fn read(&mut self) -> Result<String, ()> {
        with_application(|env, application| {
            let manager = clipboard_manager(env, application)?;
            let clip = env
                .call_method(
                    &manager,
                    jni_str!("getPrimaryClip"),
                    jni_sig!(() -> android.content.ClipData),
                    &[],
                )?
                .l()?;
            if clip.is_null() {
                return Err(Error::NullPtr("primary clip"));
            }
            let count = env
                .call_method(&clip, jni_str!("getItemCount"), jni_sig!(() -> int), &[])?
                .i()?;
            if count < 1 {
                return Err(Error::NullPtr("clip item"));
            }
            let item = env
                .call_method(
                    &clip,
                    jni_str!("getItemAt"),
                    jni_sig!((index: int) -> android.content.ClipData::Item),
                    &[JValue::Int(0)],
                )?
                .l()?;
            let text = env
                .call_method(
                    &item,
                    jni_str!("coerceToText"),
                    jni_sig!((context: android.content.Context) -> java.lang.CharSequence),
                    &[JValue::Object(application)],
                )?
                .l()?;
            if text.is_null() {
                return Err(Error::NullPtr("clip text"));
            }
            let string = env
                .call_method(
                    &text,
                    jni_str!("toString"),
                    jni_sig!(() -> java.lang.String),
                    &[],
                )?
                .l()?;
            let string = env.cast_local::<JString>(string)?;
            string.try_to_string(env)
        })
    }

    pub(crate) fn write(&mut self, text: &str) -> Result<(), ()> {
        with_application(|env, application| {
            let manager = clipboard_manager(env, application)?;
            let label = env.new_string("text")?;
            let content = env.new_string(text)?;
            let clip = env
                .call_static_method(
                    jni_str!("android/content/ClipData"),
                    jni_str!("newPlainText"),
                    jni_sig!(
                        (label: java.lang.CharSequence, text: java.lang.CharSequence)
                            -> android.content.ClipData
                    ),
                    &[JValue::Object(&label), JValue::Object(&content)],
                )?
                .l()?;
            if clip.is_null() {
                return Err(Error::NullPtr("new clip"));
            }
            env.call_method(
                &manager,
                jni_str!("setPrimaryClip"),
                jni_sig!((clip: android.content.ClipData) -> void),
                &[JValue::Object(&clip)],
            )?
            .v()
        })
    }
}
