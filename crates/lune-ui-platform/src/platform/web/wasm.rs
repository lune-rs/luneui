//! The browser: one hidden `<textarea>` the toolkit's text goes through (see `crate::web` for what
//! it does and why). No `unsafe`; every browser call is here.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{
    CompositionEvent, Event, EventTarget, FocusEvent, FocusOptions, HtmlCanvasElement, HtmlElement,
    HtmlTextAreaElement, InputEvent, KeyboardEvent, KeyboardEventInit,
};
use winit::event::{Ime, WindowEvent};
use winit::platform::web::WindowExtWebSys;
use winit::window::Window;

use lune_ui_core::input::Keymap;

use super::keys::{
    FocusTracker, KeyRoute, composition_end, composition_update, edit_key, input_text,
    keymap_for_platform, releases_at_once, route_keydown,
};

/// The keyboard conventions for the platform the browser reports: `navigator.platform`, else the
/// user agent string (a page with no `window` gets the PC map).
pub(crate) fn keymap() -> Keymap {
    let Some(navigator) = web_sys::window().map(|w| w.navigator()) else {
        return Keymap::PC;
    };
    let platform = navigator.platform().unwrap_or_default();
    if platform.is_empty() {
        keymap_for_platform(&navigator.user_agent().unwrap_or_default())
    } else {
        keymap_for_platform(&platform)
    }
}

/// What the listeners and the owner share.
#[derive(Debug, Default)]
struct Inner {
    /// The `Ime` values not yet taken.
    events: Vec<Ime>,
    /// The toolkit wants text (`set_allowed`).
    allowed: bool,
    /// Between `compositionstart` and `compositionend`.
    composing: bool,
    /// The `code`s of keys whose `keydown` was forwarded and whose `keyup` is owed.
    forwarded: HashSet<String>,
    focus: FocusTracker,
}

/// One registered listener, removed on drop.
struct Listener {
    target: EventTarget,
    name: &'static str,
    callback: Closure<dyn FnMut(Event)>,
}

impl Listener {
    fn add(
        target: &EventTarget,
        name: &'static str,
        handler: impl FnMut(Event) + 'static,
    ) -> Option<Listener> {
        let callback = Closure::wrap(Box::new(handler) as Box<dyn FnMut(Event)>);
        target
            .add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
            .ok()?;
        Some(Listener {
            target: target.clone(),
            name,
            callback,
        })
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(self.name, self.callback.as_ref().unchecked_ref());
    }
}

impl fmt::Debug for Listener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Listener")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The element and the listeners; a window with no canvas has none.
struct Live {
    inner: Rc<RefCell<Inner>>,
    canvas: HtmlCanvasElement,
    area: HtmlTextAreaElement,
    wake: Rc<dyn Fn()>,
    _listeners: Vec<Listener>,
}

impl fmt::Debug for Live {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Live").finish_non_exhaustive()
    }
}

/// The bridge; `!Send` through the `Rc`.
#[derive(Debug)]
pub(crate) struct TextInput {
    live: Option<Live>,
}

/// Gives the textarea the focus without scrolling the page to it. Never called with `inner`
/// borrowed: the focus events it causes run listeners, and winit's, which can reach the app.
fn focus_text(inner: &RefCell<Inner>, area: &HtmlTextAreaElement) {
    inner.borrow_mut().focus.text_focusing();
    let options = FocusOptions::new();
    options.set_prevent_scroll(true);
    let _ = area.focus_with_options(&options);
}

/// Dispatches a synthetic key event on the canvas, for winit to map as it does a real one. `from`
/// is the textarea's own event (its modifiers and repeat are kept); `None` is a bare key.
fn dispatch_key(
    canvas: &HtmlCanvasElement,
    kind: &str,
    key: &str,
    code: &str,
    from: Option<&KeyboardEvent>,
) {
    let init = KeyboardEventInit::new();
    init.set_key(key);
    init.set_code(code);
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_composed(true);
    if let Some(e) = from {
        init.set_location(e.location());
        init.set_repeat(e.repeat());
        init.set_ctrl_key(e.ctrl_key());
        init.set_shift_key(e.shift_key());
        init.set_alt_key(e.alt_key());
        init.set_meta_key(e.meta_key());
    }
    if let Ok(event) = KeyboardEvent::new_with_keyboard_event_init_dict(kind, &init) {
        let _ = canvas.dispatch_event(&event);
    }
}

impl TextInput {
    pub(crate) fn attach(window: &Window, on_event: impl Fn() + 'static) -> Self {
        Self {
            live: Self::build(window, Rc::new(on_event)),
        }
    }

    fn build(window: &Window, wake: Rc<dyn Fn()>) -> Option<Live> {
        let canvas = window.canvas()?;
        let document = web_sys::window()?.document()?;
        let area: HtmlTextAreaElement =
            document.create_element("textarea").ok()?.dyn_into().ok()?;
        for (name, value) in [
            ("data-luneui", "text-input"),
            ("autocapitalize", "off"),
            ("autocomplete", "off"),
            ("autocorrect", "off"),
            ("spellcheck", "false"),
            ("aria-hidden", "true"),
            ("tabindex", "-1"),
            ("rows", "1"),
        ] {
            area.set_attribute(name, value).ok()?;
        }
        // Invisible and out of the way, but real: a 1 px element with a 16 px font (iOS zooms the
        // page on focusing anything smaller), no pointer events, no caret.
        area.set_attribute(
            "style",
            "position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;border:0;padding:0;\
             margin:0;resize:none;overflow:hidden;font-size:16px;pointer-events:none;\
             caret-color:transparent;background:transparent;color:transparent;outline:none",
        )
        .ok()?;
        document.body()?.append_child(&area).ok()?;

        let inner = Rc::new(RefCell::new(Inner::default()));
        let target: &EventTarget = area.as_ref();
        let canvas_target: &EventTarget = canvas.as_ref();
        let mut listeners = Vec::new();
        let mut add = |target: &EventTarget, name: &'static str, f: Box<dyn FnMut(Event)>| {
            if let Some(l) = Listener::add(target, name, f) {
                listeners.push(l);
            }
        };

        // A key that is not text goes to the canvas, where winit maps it.
        add(target, "keydown", {
            let (inner, canvas) = (inner.clone(), canvas.clone());
            Box::new(move |event: Event| {
                let Ok(e) = event.dyn_into::<KeyboardEvent>() else {
                    return;
                };
                let key = e.key();
                let route = route_keydown(
                    &key,
                    e.ctrl_key(),
                    e.meta_key(),
                    e.get_modifier_state("AltGraph"),
                    e.is_composing(),
                    e.key_code(),
                );
                if route != KeyRoute::Forward {
                    return;
                }
                e.prevent_default();
                let code = e.code();
                dispatch_key(&canvas, "keydown", &key, &code, Some(&e));
                if releases_at_once(&key, e.meta_key()) {
                    dispatch_key(&canvas, "keyup", &key, &code, Some(&e));
                } else {
                    inner.borrow_mut().forwarded.insert(code);
                }
            })
        });
        add(target, "keyup", {
            let (inner, canvas) = (inner.clone(), canvas.clone());
            Box::new(move |event: Event| {
                let Ok(e) = event.dyn_into::<KeyboardEvent>() else {
                    return;
                };
                let code = e.code();
                let owed = inner.borrow_mut().forwarded.remove(&code);
                if owed {
                    e.prevent_default();
                    dispatch_key(&canvas, "keyup", &e.key(), &code, Some(&e));
                }
            })
        });

        // The input method's composition.
        add(target, "compositionstart", {
            let inner = inner.clone();
            Box::new(move |_| inner.borrow_mut().composing = true)
        });
        add(target, "compositionupdate", {
            let (inner, wake) = (inner.clone(), wake.clone());
            Box::new(move |event: Event| {
                let Ok(e) = event.dyn_into::<CompositionEvent>() else {
                    return;
                };
                inner
                    .borrow_mut()
                    .events
                    .extend(composition_update(&e.data().unwrap_or_default()));
                wake();
            })
        });
        add(target, "compositionend", {
            let (inner, wake, area) = (inner.clone(), wake.clone(), area.clone());
            Box::new(move |event: Event| {
                let Ok(e) = event.dyn_into::<CompositionEvent>() else {
                    return;
                };
                {
                    let mut i = inner.borrow_mut();
                    i.composing = false;
                    i.events
                        .extend(composition_end(&e.data().unwrap_or_default()));
                }
                area.set_value("");
                wake();
            })
        });

        // A soft keyboard's Backspace, Delete and Enter, which may come with no usable `keydown`.
        add(target, "beforeinput", {
            let (inner, canvas, wake) = (inner.clone(), canvas.clone(), wake.clone());
            Box::new(move |event: Event| {
                let Ok(e) = event.dyn_into::<InputEvent>() else {
                    return;
                };
                if inner.borrow().composing || e.is_composing() {
                    return;
                }
                if let Some(key) = edit_key(&e.input_type()) {
                    e.prevent_default();
                    dispatch_key(&canvas, "keydown", key, key, None);
                    dispatch_key(&canvas, "keyup", key, key, None);
                    wake();
                }
            })
        });
        // Typed text: the one route a printable key takes to the field.
        add(target, "input", {
            let (inner, wake, area) = (inner.clone(), wake.clone(), area.clone());
            Box::new(move |event: Event| {
                let Ok(e) = event.dyn_into::<InputEvent>() else {
                    return;
                };
                let composing = inner.borrow().composing || e.is_composing();
                let events = input_text(e.data().as_deref(), &e.input_type(), composing);
                if !composing {
                    area.set_value("");
                }
                if !events.is_empty() {
                    inner.borrow_mut().events.extend(events);
                    wake();
                }
            })
        });

        // Focus: the textarea's own, for the window's focus as a whole.
        add(target, "focus", {
            let (inner, wake) = (inner.clone(), wake.clone());
            Box::new(move |_| {
                inner.borrow_mut().focus.text_gained();
                wake();
            })
        });
        add(target, "blur", {
            let (inner, wake, canvas) = (inner.clone(), wake.clone(), canvas.clone());
            Box::new(move |event: Event| {
                let to_canvas = event
                    .dyn_ref::<FocusEvent>()
                    .and_then(FocusEvent::related_target)
                    .is_some_and(|t| same(&t, &canvas));
                {
                    let mut i = inner.borrow_mut();
                    i.focus.text_lost(to_canvas);
                    i.forwarded.clear();
                }
                wake();
            })
        });
        // winit gives the canvas the focus on every pointer press; its release gives it back to the
        // textarea while the toolkit is editing text, inside the gesture a phone needs to show its
        // keyboard.
        add(canvas_target, "pointerup", {
            let (inner, area) = (inner.clone(), area.clone());
            Box::new(move |_| {
                let wanted = inner.borrow().allowed;
                if wanted && !is_active(&area) {
                    focus_text(&inner, &area);
                }
            })
        });

        Some(Live {
            inner,
            canvas,
            area,
            wake,
            _listeners: listeners,
        })
    }

    pub(crate) fn set_allowed(&mut self, allowed: bool) {
        let Some(live) = &self.live else {
            return;
        };
        {
            let mut i = live.inner.borrow_mut();
            if i.allowed == allowed {
                return;
            }
            i.allowed = allowed;
            i.events
                .push(if allowed { Ime::Enabled } else { Ime::Disabled });
            if !allowed {
                i.composing = false;
            }
        }
        if allowed {
            focus_text(&live.inner, &live.area);
        } else {
            live.area.set_value("");
            // Only when the textarea has it: a focus that moved elsewhere stays there.
            if is_active(&live.area) {
                let _ = HtmlElement::focus(live.canvas.as_ref());
            }
        }
        (live.wake)();
    }

    pub(crate) fn set_area(&mut self, (x, y, w, h): (f32, f32, f32, f32)) {
        let Some(live) = &self.live else {
            return;
        };
        let Some(window) = web_sys::window() else {
            return;
        };
        let dpr = window.device_pixel_ratio();
        if !(dpr.is_finite() && dpr > 0.0) {
            return;
        }
        let origin = live.canvas.get_bounding_client_rect();
        let css = |v: f32| f64::from(v) / dpr;
        let style = live.area.style();
        let px = |v: f64| format!("{}px", if v.is_finite() { v } else { 0.0 });
        let _ = style.set_property("left", &px(origin.x() + css(x)));
        let _ = style.set_property("top", &px(origin.y() + css(y)));
        let _ = style.set_property("width", &px(css(w).max(1.0)));
        let _ = style.set_property("height", &px(css(h).max(1.0)));
    }

    pub(crate) fn take(&mut self) -> Vec<Ime> {
        match &self.live {
            Some(live) => std::mem::take(&mut live.inner.borrow_mut().events),
            None => Vec::new(),
        }
    }

    pub(crate) fn passes(&mut self, event: &WindowEvent) -> bool {
        let (Some(live), WindowEvent::Focused(on)) = (&self.live, event) else {
            return true;
        };
        live.inner.borrow_mut().focus.canvas_focus(*on);
        false
    }

    pub(crate) fn take_focus(&mut self) -> Option<bool> {
        self.live.as_ref()?.inner.borrow_mut().focus.take()
    }
}

/// Whether two DOM objects are the same object.
fn same(a: &impl AsRef<wasm_bindgen::JsValue>, b: &impl AsRef<wasm_bindgen::JsValue>) -> bool {
    a.as_ref() == b.as_ref()
}

/// Whether the textarea is the page's focused element.
fn is_active(area: &HtmlTextAreaElement) -> bool {
    area.owner_document()
        .and_then(|d| d.active_element())
        .is_some_and(|a| same(&a, area))
}

impl Drop for TextInput {
    fn drop(&mut self) {
        if let Some(live) = &self.live {
            // The listeners go with `live`; the element is the page's, so it is removed here.
            live.area.remove();
        }
    }
}
