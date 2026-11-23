# Web target

How the toolkit builds for a browser, how `apps/demo` runs there, what degrades, and what is not verified. D425 to D430. Every reading below is from headless Chromium 159 (the separate `~/chromium`) on an Apple-silicon Mac, run on 2026-11-23, through `scripts/web-run.sh`; a claim with no such run is under "Unverified".

The toolkit compiles to `wasm32-unknown-unknown` and draws on one `<canvas>` with its own renderer (WebGPU, with a WebGL2 fallback). There is no DOM UI, no CSS styling of the UI and no JS UI logic: around the canvas an app has the generated wasm-bindgen glue, one static `index.html`, and the one DOM element the library owns, the hidden `<textarea>` of the text bridge (D425, D1).

## Building

`rustup target add wasm32-unknown-unknown`.

- The five library crates and `apps/demo` pass `cargo clippy -p lune-ui-core -p lune-ui-wgpu -p lune-ui-winit -p lune-ui-platform -p lune-ui -p demo --lib --bins --locked --target wasm32-unknown-unknown -- -D warnings`; `scripts/check-targets.sh` runs that line as its `web` half and CI as its `web` job (D425, D426).
- `scripts/web-build.sh <package>` (D426): `cargo build --release --locked --target wasm32-unknown-unknown -p <package> --bin <package>`, then the `wasm-bindgen` CLI of exactly the version `Cargo.lock` holds for the crate (`--target web --no-typescript`; the one on `PATH` when its version matches, else `target/web-tools/bin`, installed there with `cargo install --locked wasm-bindgen-cli --version <v> --root target/web-tools`), then `apps/<package>/web/*` is copied next to the glue. The output, `target/web/<package>/`, is a static directory. There is no bundler and no `wasm-opt` (not installed): the sizes below are the raw and gzipped `.wasm` as wasm-bindgen wrote it.
- `lune-ui-wgpu` turns on wgpu's `webgl` feature for `target_family = "wasm"` only; `webgpu` is already a default feature. `arboard` is not in the wasm build. `lune-ui-winit` takes `web_time::Instant` on wasm (the native signature of `control_flow` is unchanged). A native build is unchanged and its release size too: every new dependency sits under a wasm target table.
- AccessKit compiles on web and does nothing there (it has no web adapter, so the accessibility gap of D46 stays).

Reading, `apps/demo` (release profile: fat LTO, `strip`, `panic = "abort"`): `demo_bg.wasm` 9,056,957 bytes, 3,666,278 gzipped; `demo.js` 152 KB.

## Running and checking: `scripts/web-run.sh`

`scripts/web-run.sh <package> [--demo] [--gl] [--smoke] [--type-field] [--phone] [--query <q>] [--no-build] [--keep-serving]` is the web counterpart of `ios-sim.sh` (D427; the header of the script is the reference for every flag and exit code). It builds, serves `target/web/<package>/` on `127.0.0.1` (an ephemeral port, loopback only), opens it in the separate Chromium (headless, a throwaway profile under `target/web-run/`, never Chrome or Firefox), waits for `presented 1` in the console, requires `probe adapter_backend BrowserWebGpu` (or `Gl` with `--gl`), takes `page.screenshot()` shots of the canvas and kills the browser on every exit path. Not in the gate. Needs node and pnpm (`scripts/web/`, one exact-pinned `playwright-core`).

- `--demo`: the page's query is `?demo&touches`; the demo's 16-mark tour plays and the run requires `demo end 16/16`.
- `--gl`: `?backend=gl`, WebGL2 only; the run requires `probe adapter_backend Gl`.
- `--smoke`: the demo prints its `probe` lines; the run also checks the timed frame (`presented 31`, a `Repaint::At` frame through the browser's timer), the clipboard round trip (`probe clipboard` equals `probe clipboard_write`, and the page's own `navigator.clipboard.readText()` returns it), the text bridge (the next section's list), and the keymap: the first page reports `navigator.platform` `Win32` (`probe keymap PC`, Ctrl+A is its select all) and a second page reports `MacIntel` (`probe keymap Mac`, Cmd+A then Backspace empties the field).

Readings:

| check | result |
| --- | --- |
| `web-run.sh demo --demo` (WebGPU) | exit 0, 5 consecutive runs; 16/16 marks ok; a tour takes about 45 s |
| `web-run.sh demo --demo --gl` (WebGL2) | exit 0, 5 consecutive runs; 16/16 marks ok |
| `web-run.sh demo --smoke` and `--smoke --gl` | exit 0 on both (steps 3 and 4: clipboard round trip, field `hello` then `hell`, preedit `にほん`, commit `hell日本`, Ctrl+A and Backspace empty it, typing after Tab or a click outside the canvas changes nothing, at most one `probe focused` line per click) |
| `--smoke` after step 5 (keymap page added) | 5 consecutive clean WebGPU runs and 3 WebGL2; `probe keymap PC` on the `Win32` page, `probe keymap Mac` and Cmd+A then Backspace emptying the field on the `MacIntel` page; `--demo` and `--demo --gl` once more, exit 0 |
| renderer on both backends | no change needed: no uniform-size, colour-space or blur-pass failure; the WebGPU adapter reports an empty name and type `Other`, the WebGL2 path is ANGLE |
| shots | six viewed (three marks, two backends): the same look to the eye |

## A web app

`apps/demo` is the model (`apps/demo/src/web.rs`, `apps/demo/web/index.html`); the app's duties are in `docs/consuming.md`, "Web apps". The shell, in order:

1. The page has `<canvas id="luneui" tabindex="0">` with CSS that sizes it, and a module script that imports the glue and calls its init after checking `navigator.gpu` or a `webgl2` context.
2. `main` installs a panic hook that writes to `console.error`, reads its flags from `location.search` (`?demo&touches` is `--demo --touches`, `k=v` is `--k v`) and hands the app to `EventLoopExtWebSys::spawn_app`: the browser owns the loop, so `main` returns at once.
3. The window is the page's canvas: `WindowAttributesExtWebSys::with_canvas(Some(canvas)).with_append(false)`.
4. The GPU opens on the page's task queue (`wasm_bindgen_futures::spawn_local`): `wgpu::util::new_instance_with_webgpu_detection` over backends `BROWSER_WEBGPU | GL` (`GL` alone under `?backend=gl`), then the surface and `Gpu::new` as on a desktop. No frame is drawn before it is there.
5. A frame waits for a non-zero window size (winit reports a canvas size only once its resize observer fires; the first frame otherwise drew at 1x1) and the surface follows `window.inner_size()`.
6. Time is `web_time::Instant`; waking the app from a browser callback goes through the event-loop proxy or `Window::request_redraw`.
7. `Clipboard::new().with_wake(..)`, `TextInput::attach(&window, wake)` and `web::keymap()` (below).
8. Fonts are embedded bytes (D10): the canvas draws text from font bytes, so the app brings its TTFs as the native apps do. No CJK font is bundled: composed CJK text is stored right and draws as missing glyphs.

## What works and what degrades

| area | on the web | basis |
| --- | --- | --- |
| drawing, glass in the canvas (frost, backdrop blur), motion, text, every widget | as on a desktop, on WebGPU and on WebGL2 | `--demo` 16/16 on both |
| window material (`material::apply`) | best supported kind is `Opaque`; a glass request answers `Err` (the in-canvas frost is the toolkit's own and works) | `material/other.rs`, read; not run in a browser |
| popover window (`PopoverWindow::wrap`) | `Err("popover windows do not exist in a browser")`, the window is left as it was (D430): winit would add a second canvas, which is no floating window | `platform/other.rs`; the wasm clippy half compiles it; no browser run (no popover is made by the demo) |
| tray item (`TrayItem::new`) | `Err(Unsupported)` | `platform/other.rs`, read |
| window shape (`desktop::set_window_shape`) | `SHAPE_MODE = Unsupported` | `desktop/other.rs`, read |
| desktop queries | `monitors` is empty (winit's web `available_monitors` is empty), `pointer`, `window_at` and `display_depth` are `None`; `window_frame` is the canvas's position and size; `set_window_origin` and `move_windows` answer `Ok` and move the canvas element by CSS, which is no window move | `desktop/other.rs` and winit 0.30.13 `platform_impl/web/window.rs`, read; not run |
| window groups and docking (`group`) | record only: every call answers `Ok` and nothing is raised, minimised or kept on top | `group/other.rs`, read |
| multi-window | not supported: winit would make another canvas | decision D6 of the task, not run |
| clipboard | the page's asynchronous `navigator.clipboard`: replies one per read, in request order, under the 32 MiB cap; a late reply is queued by the next `service` and the app is woken through `Clipboard::with_wake`; no `navigator.clipboard` (an insecure context), a rejected promise or a non-text value answers `ClipboardUnavailable`; a failed write is silent | D428; smoke on both backends |
| typing, IME, phone keyboards | `web::TextInput`, a hidden `<textarea>`: printable keys reach a field exactly once, named keys and Ctrl or Cmd chords are re-dispatched on the canvas, composition becomes `Ime::Preedit` and `Ime::Commit`, at most one focus change per click, dropping it removes the element and every listener | D429; smoke on both backends, desktop Chromium only |
| keyboard conventions | `web::keymap()` reads the platform name the browser reports: Mac chords on macOS, iOS and iPadOS, PC chords elsewhere (D430) | smoke, second page with `navigator.platform` overridden |
| accessibility | AccessKit compiles and does nothing: no screen reader sees the canvas | `--features accesskit` wasm clippy; gap recorded |

### Keyboard

`lune_ui_winit::keymap()` is a compile-time choice and answers `Keymap::PC` for a wasm build, which would make select all, copy and paste Ctrl-based in a browser on a Mac. A web app passes `lune_ui::web::keymap()` as `FrameConfig::keymap` instead (the demo does, `Demo::set_keymap`): it reads `navigator.platform` (else the user agent string) and answers `Keymap::MAC` when the name holds `mac`, `iphone`, `ipad`, `ipod` or `ios`, `Keymap::PC` otherwise. Off the web it answers what `lune_ui_winit::keymap()` does, so one call serves every target. The name is the browser's claim, not the keyboard: a Mac keyboard on a PC, or a spoofed name, gets the other map. The driver overrides `navigator.platform` before the page's scripts run, because this Mac's own name is `MacIntel` and would make every page the Mac keymap: `Win32` on the first page (PC, Ctrl+A), `MacIntel` on the second (Mac, Cmd+A); the page prints `probe keymap PC` or `probe keymap Mac`.

## Unverified

- Safari (no native WebKit on this Mac, and Playwright's WebKit is never run natively here), Firefox (Playwright's Firefox is cached but was not tried) and any browser but Chromium 159.
- Real phones: the soft keyboard (headless Chromium has none: the `beforeinput` mapping of Backspace, Delete and Enter, and iOS's rule that a keyboard opens only on a focus inside a gesture, are by design only), touch on a device, and the safe area of a phone browser.
- A promise of the browser clipboard that never settles leaves a field's paste pending (no timeout; no engine showed it).
- A WebGPU adapter that is not Apple's Metal one (the readings are one GPU), and frame times: headless Chromium's timing is noise, so none is reported.
- Anything under "What works" marked "read" or "not run": the OS-level fallbacks were checked by reading the code and by the wasm clippy, not by a browser run.
- AccessKit on web: there is no adapter, so nothing exposes the canvas to assistive technology.
- Hosting: nothing is published; the output is a local static directory (D11 of the task).
