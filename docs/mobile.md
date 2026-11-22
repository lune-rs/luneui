# Mobile targets

How the toolkit builds for the phone targets, how the demo runs on an iOS simulator, and what winit and wgpu report there. M4, M7 and M8 extend this page. Source of every reading below: `apps/demo --probe` (macOS host and the iOS simulator), run on 2026-11-20.

## Building

The phone targets are a rustup target each: `rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android`.

- The five library crates and `apps/demo` pass `cargo clippy -p lune-ui-core -p lune-ui-wgpu -p lune-ui-winit -p lune-ui-platform -p lune-ui -p demo --lib --bins --locked --target <target> -- -D warnings` for `aarch64-apple-ios`, `aarch64-apple-ios-sim` and `aarch64-linux-android`; `scripts/check-targets.sh` runs that line as its `ios`, `ios-sim` and `android` halves (D340).
- Android: that clippy passes with no NDK installed (winit's `android-native-activity` compiles no C, and the activity feature is the demo's: `[target.'cfg(target_os = "android")'.dependencies]` in `apps/demo/Cargo.toml`). The demo's `android_main` (`apps/demo/src/lib.rs`) is the one exported symbol android-activity looks up. `scripts/android-emu.sh --build-only` links the `cdylib` with the NDK's clang and packages the APK (the next section); it needs the NDK, which clippy does not.
- `arboard` is compiled only off iOS and Android (`lune-ui-winit/src/platform/clipboard_os.rs`); on both phones `clipboard_none.rs` answers every read with `ClipboardUnavailable` and drops every write; `mobile::Pasteboard` is the real clipboard on both (M4: `UIPasteboard`; M8: `ClipboardManager` through JNI).
- `lune-ui-wgpu` asks the device for wgpu's default limits lowered to the adapter's (D337): the simulator's Metal offers 15 inter-stage shader variables, the default asks 16.
- `lune-ui-winit --features accesskit` also passes clippy for `aarch64-apple-ios-sim` (`accesskit_ios` 0.2.0 is pulled in); input for M4's accessibility bridge.
- An iOS binary is built with `IPHONEOS_DEPLOYMENT_TARGET=17.0` (the simulator script sets it). Nothing here signs for a real device.

## Wiring an iOS app

What an app adds to the toolkit's iOS layer, with the demo's code as the reference: `docs/consuming.md`, "0.7.2 to 0.7.3: iOS platform layer" (D353 to D360). The readings below are the demo's, on the simulator.

## `scripts/ios-sim.sh`

Builds `apps/demo` for `aarch64-apple-ios-sim`, assembles `target/ios-sim/Demo.app` (the binary and `apps/demo/ios/Info.plist`, ad-hoc signed), runs it on one simulator device of its own and checks that a frame was presented.

- Device: `luneui-demo-iphone`, type `iPhone-16-Pro`, runtime `iOS-18-0` (override with `LUNEUI_IOS_DEVICE_TYPE`, `LUNEUI_IOS_RUNTIME`; any other runtime gets a device of its own, `luneui-demo-iphone-<runtime>`, as `luneui-demo-iphone-iOS-27-2`); found by exact name, created when missing, shut down at the end (`--keep-booted` leaves it booted, `--delete-device` deletes it).
- Default mode: waits up to 60 s for `presented 1` in `target/ios-sim/stdout.log`, takes one screenshot (`shot-1.png`), exits 0 on `ios-sim: passed`. Exit 1 a check failed, 2 setup (tool, rustup target or runtime missing).
- `--probe`: launches the demo with `--probe`, screenshot `shot-1.png` 2 s after `presented 1`, launches the Settings app (`com.apple.Preferences`, by `simctl launch`) to send the demo to the background, launches the demo again (no `--terminate-running-process`), screenshot `shot-2.png`, and writes the `probe`/`presented` lines of both launches to `target/ios-sim/probe.txt`. Two screenshots in all.
- `--keyboard`: launches the demo with `--probe --focus-field`, requires `probe keyboard_inset 0,0,0,<b>` with `<b>` above 200, takes one screenshot 2 s later (`shot-1.png`; `shot-nokeyboard.png` when no keyboard showed) and stops (no Settings round trip). In a `--smoke` run the same launch is the last step and its screenshot `shot-2.png` is the run's second one (the resume screenshot is not taken). `LUNEUI_IOS_DEVICE_NAME` picks a device name of its own.
- `--smoke`: `--probe`, and also requires the clipboard probe to read back its write and `xcrun simctl pbpaste <udid>` to print the same text, `probe scene attached` as the demo's third `probe scene` line (frame 30) and, in launch 1's output, `probe lifecycle resumed` at launch, then `suspended` and `resumed` across the Settings round trip. Two screenshots in all. It also requires every `probe safe_area_event` line (frames 1 and 30) to be `0,62,0,34`, the iOS 18.0 portrait safe area (printed and not judged with `--landscape`), and no `presented` line between `probe lifecycle suspended` and the next `probe lifecycle resumed`, with one after it (a `--probe` run prints a `presented` line for every frame; `--demo-arg --draw-while-suspended` is the control run that must make this check fail).
- `--demo` (D418; exclusive with `--probe`, `--smoke` and `--keyboard`, else usage and exit 2): launches the demo with `--demo --touches` (plus any `--demo-arg`), waits for `presented 1`, then follows `stdout.log`: a `demo mark <name> FAIL` line fails the run at once (exit 1, the line quoted); `shot-1.png`, `shot-2.png` and `shot-3.png` are taken when `demo mark sheet-half ok`, `demo mark page-2 ok` and `demo mark settings ok` print (three in all); `demo end <n>/<n>` with 16 `ok` marks is required within 60 s plus 60 s; the `demo` lines go to `target/ios-sim/demo.txt`. No Settings round trip.
- Control flags, which edit only the bundle copy `target/ios-sim/Demo.app/Info.plist` (with `/usr/libexec/PlistBuddy`), never `apps/demo/ios/Info.plist`: `--no-manifest` deletes `UIApplicationSceneManifest` (the app without the scene life cycle); `--landscape` sets `UISupportedInterfaceOrientations` to `UIInterfaceOrientationLandscapeLeft`. `--demo-arg <arg>` passes one more argument to the demo (repeatable), as `--no-scene-attach`, which skips `mobile::attach_to_scene`.
- Never: touches another device, uses `booted` as a device argument, opens Simulator.app or drives the UI, records video, writes outside `target/ios-sim/`.

`cargo run -p demo -- --probe --frames 60` does the same probe on the desktop: a phone-shaped window (402 x 874 points) for about a second, exit 0 after 60 presented frames. `--frames` is ignored on iOS and Android, where the loop cannot return.

## Android

M8 (D420 to D423). `scripts/android-emu.sh` builds `apps/demo` as an APK with only the installed SDK, runs it on an emulator of its own and checks it. Nothing here needs Gradle, Java code or cargo-apk.

- Build (`scripts/android-emu.sh --build-only`, no emulator, exit 0 or 1; the local link check, since CI has no Android job): `cargo rustc -p demo --lib --crate-type cdylib --release --target aarch64-linux-android --locked` with `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER` set to the NDK's `aarch64-linux-android26-clang` (found under `ANDROID_NDK_HOME` or the newest `$SDK/ndk/*`; no machine path is committed); the library stripped with `llvm-strip` and checked to export `ANativeActivity_onCreate`; `aapt2 link --debug-mode` of `apps/demo/android/AndroidManifest.xml` (no Java: `hasCode="false"`); the library added with `zip` as `lib/arm64-v8a/libdemo.so`; `zipalign -p 4`; `apksigner` with a throwaway debug key `keytool` makes once at `target/android-emu/debug.keystore`. Result: `target/android-emu/demo.apk`, 4,813,216 bytes with a 9,750,136-byte stripped `libdemo.so` (0.7.5 plus M8). The SDK comes from `ANDROID_HOME`, `ANDROID_SDK_ROOT` or `~/Library/Android/sdk`; JDK 17 (`apksigner`, `keytool`, `avdmanager`) from `LUNEUI_ANDROID_JAVA_HOME`, `JAVA_HOME`, `java_home -v 17` or Homebrew's `openjdk@17`.
- Manifest keys (`apps/demo/android/AndroidManifest.xml`, D3): package `rs.lune.luneui.demo` (the iOS bundle id); `minSdkVersion` 26 (the API level of the linker, `aarch64-linux-android26-clang`) and `targetSdkVersion` 34; `hasCode="false"` and the platform's `android.app.NativeActivity` with `android.app.lib_name` `demo` (android-activity's native-activity glue; no Java class); `extractNativeLibs="true"`; `launchMode="singleTask"` (a second activity instance would call `android_main` again in one process, and winit's event loop cannot be built twice); `configChanges` for orientation, size, density, keyboard and the rest, so rotation and the keyboard never recreate the activity; `windowSoftInputMode="adjustResize|stateHidden"` (the content rect shrinks with the keyboard, which `mobile::KeyboardInset` reads, D8); `theme="@android:style/Theme.DeviceDefault.NoActionBar"` (with no theme the framework's default has an action bar, which NativeActivity never draws but which the content rect leaves room for: the first reading had a top of 283 px, the status bar's 136 plus 147 px of blank band, 56 dp; with the theme it is 136 px). `--debug-mode` makes the APK debuggable, which `run-as` needs.
- Launch arguments (D4): the script writes the flags, one per line, to the app's `files/launch-args` with `adb shell run-as rs.lune.luneui.demo` before `am start`; `android_main` reads `internal_data_path()/launch-args`, deletes it and parses it with `demo::parse_window_args`, the parser `main` uses. A launch from the home screen has no flags. A file that does not parse prints `demo: bad launch arguments` and the usage line, and the app returns.
- Logs (D5): the demo's `println!` goes to logcat's `RustStdoutStderr` tag (android-activity forwards stdout); the script follows `adb logcat -v raw -s RustStdoutStderr:I` into `target/android-emu/stdout.log` (`stdout-2.log` for a smoke run's second launch) and keeps a bounded `logcat -d` in `logcat.txt`.
- The emulator (D11): its own AVD `luneui-demo-pixel7` (`pixel_7`, `system-images;android-34;google_apis;arm64-v8a`, `hw.keyboard=no` so the soft keyboard shows; `LUNEUI_ANDROID_AVD` picks another name), created when missing, booted `-no-window -no-audio -no-boot-anim -no-snapshot -port 5580 -gpu swiftshader_indirect` (`LUNEUI_ANDROID_GPU`), every adb call `-s emulator-5580`. A serial on that port that is not this AVD is exit 2. `--keep-booted` leaves it up (a later run reuses it and shuts it down unless it keeps too), `--delete-device` deletes only this AVD. Never touches another AVD or serial, never runs `adb kill-server`, records no video, never drives the emulator's window.
- Modes (exit 0 passed, 1 a check failed, 2 usage or setup): default (a frame presented, `shot-1.png`); `--probe` (the `probe` lines to `probe.txt`, with `wm size` and `wm density`); `--smoke`, `--keyboard`, `--demo` as the M8 readings below say, each its own mode (`--demo` also excludes `--probe`); `--landscape` with any mode; `--build-only`; `--demo-arg <arg>` (repeatable); `--keep-booted`; `--delete-device`.

## M1 reading (iOS 18.0 simulator, iPhone 16 Pro, Xcode SDK 27.2; macOS host (Darwin 27.2), Apple M1 Pro)

winit 0.30.13, wgpu 30.0.1. Physical pixels unless a unit is named.

| key | macOS window | iOS, before background | iOS, after resume |
| --- | --- | --- | --- |
| `inner_size()` | 804x1748 | 1206x2334 | 1206x2334 |
| `outer_size()` | 804x1812 | 1206x2622 | 1206x2622 |
| `inner_position()` | 1326,220 | 0,186 | 0,186 |
| `outer_position()` | 1326,156 | 0,0 | 0,0 |
| `scale_factor()` | 2 | 3 | 3 |
| derived insets, logical points (left, top, right, bottom) | 0, 32, 0, 0 (the title bar) | 0, 62, 0, 34 | 0, 62, 0, 34 |
| `Resized` payload | 804x1748 | 1206x2622 (twice, before the first frame) | none after resume |
| `ScaleFactorChanged` | 2, once at launch | 3, once at launch | none |
| `current_monitor()` refresh | 120000 mHz | 60000 mHz | 60000 mHz |
| surface size at creation | 804x1748 | 1206x2334 (from `inner_size()`) | |
| surface size after the `Resized` | 804x1748 | 1206x2622 | 1206x2622 |
| surface format, alpha mode | `Bgra8Unorm`, `Opaque` | `Bgra8Unorm`, `Opaque` | same |
| adapter | Apple M1 Pro, Metal, IntegratedGpu | Apple iOS simulator GPU, Metal, DiscreteGpu | same |

What the values say (P3):

- On iOS `inner_size()` is the safe area (the screen minus 62 points on top and 34 on the bottom here: 186 and 102 physical) and `outer_size()` the screen, as winit's source says (`ios/window.rs:186-200, 672-684`). Left and right insets are 0 in portrait.
- The `Resized` payload carries the screen size (1206x2622), not the safe-area size `inner_size()` returns at the same time (1206x2334). A surface sized from `inner_size()` at creation is therefore resized to the full screen before the first frame, and the drawn surface covers the screen: in the screenshots the four edge markers (top red, right green, bottom blue, left yellow) sit on the screen's outer edges, under the status bar and the clock, around the Dynamic Island area and under the home indicator. Nothing is inset by the safe area; an app that wants to stay clear of it reads the derived insets and does it itself (full screen versus safe area is M4's choice, C3(b)).
- The derived-inset formula (`inner_position - outer_position` for left and top, the rest from the sizes) works on iOS; on macOS the "top" inset is the title bar.
- A value that could not be read: the adapter driver string is empty on both hosts (wgpu reports none for Metal). Nothing else read as an error: both position calls returned `Ok`.

Life cycle, observed order of `probe lifecycle` lines (`target/ios-sim/probe.txt`):

- iOS, launch: `resumed`, window and surface created, `ScaleFactorChanged` 3, `Resized` 1206x2622 twice, first frame. macOS, launch: `resumed`, then the same events with scale 2, no `suspended` ever.
- iOS, Settings launched over the demo: `suspended` (sizes unchanged), then the next frame. A second `simctl launch` of the demo: `resumed` (sizes unchanged), then a frame. No `Resized` and no `ScaleFactorChanged` in the cycle. No `memory_warning` appeared.
- `suspended` and `resumed` are the only life-cycle callbacks winit 0.30.13 gives (no `UIScene` callbacks, see below).
- The second launch is the same process (same pid on both launches). Its new `--stdout` file stays empty: the output of the running process keeps going to the first launch's file, so `stdout-2.log` is empty and `probe.txt`'s launch 1 section holds the whole sequence.
- The screenshot after the resume (`shot-2.png`) shows the same frame as before, with the status bar's "Settings" back link at the top left (iOS puts it there after a switch from another app); the frame is redrawn after `resumed` (`presented 3`).

Redraws on iOS: a `request_redraw()` made inside the `RedrawRequested` handler was not delivered while the app was idle (only `presented 1` appeared in 8 s; frames 2 and 3 appeared only at `suspended` and `resumed`). With `request_redraw()` also called from `about_to_wait`, frames 2 and 3 came at once. winit's `request_redraw` on iOS queues the window when the view is itself Metal-backed and calls `setNeedsDisplay` otherwise (`ios/window.rs:138-150`); which branch this view takes was not checked, and why the request from inside the handler is lost is not established. Only the observation above was run. M4 must drive redraws from `about_to_wait` (or a display link) on iOS. (M4 step 2 did: see "M4 reading: safe area and life cycle".)

`simctl launch --terminate-running-process` worked in every script run and in one manual retry with the app running (new pid, new output file). It was refused (SBMainWorkspace) twice in earlier manual retries after a previous run; not reproduced, cause unknown.

## M4 reading: the scene life cycle

Run 2026-11-20 and 2026-11-21 on `iPhone-16-Pro` devices: iOS 18.0 (`luneui-demo-iphone`) and iOS 27.2 (24B8117, `luneui-demo-iphone-iOS-27-2`); Xcode SDK 27.2, winit 0.30.13. Each run's `stdout.log`, `stderr.log` and screenshots are kept under `target/ios-sim/runs/<run>/`.

The route (D354): `apps/demo/ios/Info.plist` declares the scene manifest, with no scene delegate class and no storyboard:

```xml
<key>UIApplicationSceneManifest</key>
<dict>
	<key>UIApplicationSupportsMultipleScenes</key>
	<false/>
	<key>UISceneConfigurations</key>
	<dict>
		<key>UIWindowSceneSessionRoleApplication</key>
		<array>
			<dict>
				<key>UISceneConfigurationName</key>
				<string>Default Configuration</string>
			</dict>
		</array>
	</dict>
</dict>
```

and the app calls `lune_ui::mobile::attach_to_scene(&window)` right after `create_window`, which hands winit's `UIWindow` to the connected `UIWindowScene` (or to the next one that connects) and re-seats the root view controller. The demo prints `probe scene <attached|waiting|not-needed|skipped>` at window creation, at frame 1 and, under `--probe`, at frame 30.

| run | flags | what it printed | screenshot |
| --- | --- | --- | --- |
| R1, iOS 18.0 (`r1-smoke-reseat`) | `--smoke` | `probe scene attached` three times; `presented 1 1206x2622`; `ios-sim: passed` | the frame full screen, the four edge markers on the screen edges, as in M1 |
| R1, iOS 27.2 (`ios27-r1-smoke`) | `--smoke` | the same; life cycle `resumed suspended resumed`; `ios-sim: passed` | 1206x2622, both shots |
| R2, iOS 18.0 and 27.2 | (R1's output) | `inner_size` 1206x2334 at `inner_position` 0,186; `outer_size` 1206x2622 at 0,0; scale 3; derived insets 0, 62, 0, 34; last `Resized` 1206x2622: M1's values | |
| R3, iOS 18.0 and 27.2 | (R1's output) | `probe lifecycle resumed` at launch, then `suspended` and `resumed` across the Settings round trip | |
| R4, control A, iOS 18.0 (`r4-no-manifest`) | `--no-manifest` | M1's behaviour: frames, `Resized` 1206x2622 only; `probe scene attached` (UIKit gives a manifest-less app an implicit scene and puts winit's window in it) | the frame |
| R4, control A, iOS 27.2 (`ios27-r4-no-manifest`) | `--no-manifest` | nothing: the process stops at launch with `SIGTRAP` (`EXC_BREAKPOINT`) in UIKit's `_UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`, called from `-[UIApplication workspace:didCreateScene:…]` under `UIApplicationMain` (the crash report, `crash.ips`); empty stdout and stderr, so `resumed` never arrives | none |
| R5, control B, iOS 18.0 (`r5-no-attach`) | `--demo-arg --no-scene-attach` | `probe scene skipped`; every size 0x0, safe area 0, the surface 1x1, no `presented` line | black (status bar and home indicator only): the screenshot tells an attached window from one that is not |
| R6, iOS 18.0 (`r6-landscape`, before the re-seat) | `--landscape` | `inner_size` 2622x918 at 0,186, insets 0, 62, 0, 34: the portrait safe area in a landscape window (wrong) | |
| R6, iOS 18.0 (`r6-landscape-reseat`) | `--landscape` | `inner_size` 2250x1143 at 186,0, insets 62, 0, 62, 21, the same as the landscape run without the manifest (`r6-landscape-no-manifest`) | |

What the values say:

- The manifest with no scene delegate class is enough: with the attach the window shows and draws on 18.0 and 27.2, and iOS 27.2 refuses an app built with the iOS 27 SDK that has no manifest (R4), which is the refusal TN3187 announced. So `launches on iOS 27`: held on the 27.2 simulator; a real device is unverified here.
- Under the manifest the attach is what shows the window (R5): without it the window has no scene, every size reads 0 and nothing is presented.
- The order matters for the safe area: winit sets its root view controller before the window has a scene, so the controller keeps the portrait orientation (R6, first run). `attach_to_scene` sets the root view controller to none and back after setting the scene, and the landscape insets come out right; portrait is unchanged (R1). The re-seat makes UIKit print `Unbalanced calls to begin/end appearance transitions for <WinitUIViewController …>` twice on stderr; nothing else follows from it in these runs.
- With the manifest a first `Resized` 0x0 arrives before the 1206x2622 one (without it only 1206x2622 arrives): an app skips a 0 size (the demo does; `WindowSurface::resize` would clamp it to 1x1).
- winit's `resumed` and `suspended` keep arriving under the manifest, at launch and across the background round trip (R3), so the life cycle stays winit's (D356's branch: no `mobile::Lifecycle`).
- Not driven here: a scene the OS discards and reconnects (`attach_to_scene` registers its one observer on every path, so the window is handed to a scene that connects while it has none; the simulator gives no way to force a discard, so the reconnect itself was not seen: the observer registration on the attach-at-once path is by construction, not observed) and rotation at run time.

## M4 reading: safe area and life cycle

Run 2026-11-21, iOS 18.0 (`luneui-demo-iphone`), iPhone-16-Pro; `scripts/ios-sim.sh --smoke`.

Safe area (D355): `mobile::safe_area(&window)` derives the insets from winit's `inner_position`, `outer_position`, `inner_size`, `outer_size` and `scale_factor` (live on iOS, from `safeAreaInsets`; the formula is `platform/mobile.rs`, `insets_from_geometry`, unit-tested with the portrait reading) and is `Insets::ZERO` on every other OS (on macOS the same formula gives the title bar, so it must not run there). The demo pushes `Event::SafeArea(mobile::safe_area(window))` after every `Translator::take` and draws an outline inset by `ui.safe_area()`. `probe safe_area_event` is the value `ui.safe_area()` returned: `0,62,0,34` at frame 1 and at frame 30 on iOS 18.0; the screenshot's outline sits 62 points below the top edge and 34 above the bottom, the four edge markers on the screen edges.

Life cycle (D356) and redraws: the demo keeps a `paused` flag, set by `suspended` and cleared by `resumed`; while it is set `render` returns before acquiring a texture, so nothing is drawn or presented. The `WindowSurface` is kept (on iOS the CAMetalLayer survives the background; a platform that destroys the surface on suspend drops it and rebuilds it against the same `Gpu` on `resumed`, which the demo does on Android: M8 reading below). Every frame the demo asks for is requested from `about_to_wait` (a `want_frame` flag the frame sets when its repaint wants one, and any window event other than `RedrawRequested` sets), never inside `RedrawRequested`. The order, for an app on any phone target:

1. on `suspended`: stop frames (acquire and present nothing); keep the `Gpu` and, on iOS, the surface;
2. on `resumed`: clear the stop, and ask for a frame;
3. a frame is requested outside `RedrawRequested`, and a `StartCause::ResumeTimeReached` wake-up (a `Repaint::At` that came due) is turned into one.

Reading: `probe lifecycle` lines `resumed suspended resumed`; frames presented between `suspended` and the next `resumed`: 0; after it: 2 (`ios-sim: frames around the suspend: during=0 after=2`); the same smoke passed on the iOS 27.2 simulator (`LUNEUI_IOS_RUNTIME=iOS-27-2`, same readings). The control run `--demo-arg --draw-while-suspended` (the demo asks for a frame on `suspended` and does not pause) presented 3 frames between them (`during=3 after=1`) and the smoke failed, so the check can see a frame drawn while suspended. A frame the core asks for while idle arrives: the demo's `--probe` run asks for frame 31 with `Repaint::At` one second after frame 30 (`request_repaint_at`), and it arrived at once while the app was idle (`presented 31`); with the demo's `new_events` handler disabled (the `ControlFlow::WaitUntil` wake-up must be turned into a frame by the app: `StartCause::ResumeTimeReached` sets the flag) the smoke failed with no `presented 31`, so the check sees a lost scheduled frame. `Repaint::Now` is the same flag (`control_flow`'s second value); no demo frame asks for it, so only the flag path was exercised, the first three frames and a probe's first 30 going through it.

## M4 reading: the soft keyboard

Run 2026-11-21, `scripts/ios-sim.sh --keyboard` and `--smoke`; the demo's `--focus-field` focuses its text field on frame 1, `OutputState::apply` then calls `Window::set_ime_allowed(true)`, which winit's iOS backend answers with `becomeFirstResponder` (its view conforms to `UIKeyInput`), and UIKit raises the keyboard. No Simulator.app runs and no Simulator preference is touched; the script sets one key inside its own device (below).

`mobile::KeyboardInset` (D357) observes `UIKeyboardWillChangeFrameNotification` and `UIKeyboardWillHideNotification`, converts the end frame from screen space (`UIScreen.coordinateSpace`) into the window's view, and keeps the overlap of its top edge with the view's height as a bottom inset in points, clamped to the view. The demo pushes it as `Event::KeyboardInset` after every `take`, lays the field out 12 points above the keyboard (the shapes and frost move up to clear it) and prints `probe keyboard_inset l,t,r,b` when it changes.

| Device | Reading |
| --- | --- |
| iOS 18.0, `luneui-demo-iphone` | end frame 583 + 291 points: `probe keyboard_inset 0,0,0,291`; the keyboard's top sits at 583 points, the field's bottom 12 points above it |
| iOS 27.2, `luneui-demo-iphone-iOS-27-2` | end frame 573 + 301 points: `probe keyboard_inset 0,0,0,301`; the same layout (a one-time system sheet about sliding to type covered the keys on the first run of the fresh device) |
| iOS 18.0, any device, keyboard minimisation on (the control) | no keyboard: UIKit posts one change with the end frame below the screen (874 points, 75 high, overlap 0) and a hide; no inset line, the screenshot shows the focused field and no keyboard |

What the readings say:

- Why a device showed the keyboard once and then never (F4, 2026-11-21): a headless simulator always has its hardware keyboard attached (backboardd logs `Hardware keyboard attached` at every boot, from the simulator's `ExternalKeyboardService`), and the device's keyboard setting `AutomaticMinimizationEnabled` in `com.apple.keyboard.preferences` is true after the system writes it during a boot. With both, UIKit shows only the minimised 75-point bar below the screen. On a fresh device the key is sometimes not yet in effect on its first boot, so the keyboard rose there and on later launches of that boot only. Proof on one device (`luneui-demo-f4-b`): the key false gave the keyboard on three runs across reboots, the key true (the control) gave none, false again gave it; written while booted it takes effect at the next launch, with no reboot. `scripts/ios-sim.sh --keyboard` and `--smoke` therefore write it false in their own device (`simctl spawn <udid> defaults write com.apple.keyboard.preferences AutomaticMinimizationEnabled -bool false`) before the launches; nothing else on the device and nothing of the Simulator's own preferences changes. `--smoke` then passed twice in a row on `luneui-demo-iphone` (the device that had failed) and on `luneui-demo-iphone-iOS-27-2`.
- The inset arrives through the observer and reaches the frame while the app is idle: the demo's observer sets a flag and asks for a redraw, and `about_to_wait` turns the flag into the frame (a request made inside `RedrawRequested` is lost while iOS is idle, and `becomeFirstResponder` is called inside a frame).
- The keyboard's height is 291 to 301 points on the iPhone 16 Pro (it includes the home-indicator area, so the window's bottom safe area is not added to it).
- Typed text through a live keyboard cannot be injected here: unverified.

## M4 reading: the clipboard

Run 2026-11-21, `scripts/ios-sim.sh --smoke` (the demo's `--probe-clipboard`). `mobile::Pasteboard` (D358) is the `ClipboardBackend` over `UIPasteboard.generalPasteboard`, main thread only; the demo builds `Clipboard::with_backend(Box::new(Pasteboard::new()))` on iOS and `Clipboard::new()` elsewhere. On frame 2 the demo calls `ui.clipboard_write("luneui-probe-<pid>")` (the frame's own request, the path a text edit's copy takes), on frame 4 `ui.clipboard_read()`, and prints the reply event it receives on the next frame: `probe clipboard luneui-probe-<pid>`. The text carries the process id, so a stale pasteboard cannot pass for the run's write.

| Device | Reading |
| --- | --- |
| iOS 18.0 | written, read back and `xcrun simctl pbpaste <udid>` all `luneui-probe-24141` |
| iOS 27.2 | all three `luneui-probe-26244` |

`simctl pbpaste` reads the device's pasteboard, never the Mac's. Unverified here: the iOS "Allow Paste" prompt for text another app put on the pasteboard (the probe reads its own text), and pasting by a person.

## M4 reading: accessibility and the desktop-only modules on a phone

Accessibility (D359): the demo's `access` cargo feature turns on the facade's `accesskit`; `scripts/ios-sim.sh --smoke` builds with it. `setup` creates the window hidden, makes the `AccessBridge` (a failure, or a panic caught with `catch_unwind`, is printed as `probe access err <why>` or `panic`, and the demo runs on with the null bridge, which is what every call does without the feature), then `mobile::attach_to_scene`, then shows the window (an attach shows only a window that is not hidden). Reading, iOS 18.0: `probe access ok`, `probe scene attached`, `presented 1` and the frame as before. On iOS `Window::is_visible()` answers `None`, so `AccessBridge::new`'s "window must be hidden" check (`lune-ui-winit/src/access.rs:651-653`) cannot fire there; the hidden start is for the desktop (the same demo code). The error path was not run on a phone (nothing makes the bridge fail there). Unverified here: VoiceOver (no agent can drive it), and a screen reader reading the tree.

The answers of every desktop-only entry point on iOS and Android (both use the `other` files; the line numbers are the code that decides). Nothing in this table changed except the popover:

| Entry point | Answer on a phone | Where |
| --- | --- | --- |
| `popover::PopoverWindow::wrap` | `Err("popover windows do not exist on a phone")` (changed, D359: a second winit window is never attached to a scene, so it would be invisible while `wrap` said `Ok`); macOS, Windows and Linux unchanged | `platform/other.rs:18-24` (`cfg` on `ios`, `android`), `popover.rs` `wrap` |
| `tray::Tray::new` | `Err(TrayError::Unsupported)` | `platform/other.rs:50-52` |
| `Tray::set_image`, `set_tooltip`, `rect`, `bar_is_dark`; `tray::set_event_handler` | unreachable (no `Tray` exists); `set_event_handler` is a no-op | `platform/other.rs:54-71` |
| `popover::frontmost_app` (internal) | `None` | `platform/other.rs:41` |
| `desktop::monitors`, `window_frame`, `units_per_logical` | winit's physical monitor and frame values (`work` equals `bounds`); not `Unsupported` | `platform/desktop/other.rs:13, 34, 56` |
| `desktop::pointer`, `desktop::window_at` | `None` | `platform/desktop/other.rs:30, 64` |
| `desktop::display_depth` | `None` | `platform/desktop/other.rs:60` |
| `desktop::set_window_origin`, `move_windows` | `winit`'s `set_outer_position`, which iOS ignores; `move_windows` answers `Ok` | `platform/desktop/other.rs:45, 49` |
| `desktop::set_window_shape` | `Ok(ShapeMode::Unsupported)` (`SHAPE_MODE`) | `platform/desktop/other.rs:69, 71` |
| `group::owned_by`, `WindowGroup::add`, `set_docked`, `raise`, `minimise`, `restore` | `Ok` or no-ops: a group only records what the app asks | `platform/group/other.rs:5-21` |
| `material::apply`, `apply_rounded` | `Err("no OS glass on this platform; use the opaque material")` for a glass material, `Ok` for the opaque one; `BEST_SUPPORTED` is `Opaque` | `material/other.rs:6-23` |

Windows on a phone: the toolkit's own glass (`Painter::acrylic`, `frost`, `backdrop`) is drawn by the renderer and works on the phone (the demo's frost panel); only the OS material is absent.

## M7 reading: demo mode

`apps/demo` plays its showcase tour hands-free with `--demo` (`docs/demo-mode.md`; D415 to D417): 16 marks, each a check of the app's state, printed as `demo mark <name> ok` or `demo mark <name> FAIL <reason>`, then `demo end <ok>/<total>`. The scripted touches enter only through `RawInput.events`. `scripts/ios-sim.sh --demo` (D418) ran it on the simulators, 2026-11-22 (macOS host Darwin 27.2, Apple M1 Pro; iPhone 16 Pro, 1206x2622 at scale 3, safe area 0,62,0,34):

| run | result |
| --- | --- |
| `scripts/ios-sim.sh --demo`, iOS 18.0 | exit 0, `demo end 16/16`, three screenshots |
| `LUNEUI_IOS_RUNTIME=iOS-27-2 scripts/ios-sim.sh --demo`, iOS 27.2 | exit 0, `demo end 16/16`, three screenshots |
| the same two with `--demo-arg --probe` (a `presented` line for every frame) | 18.0: 41 s from `presented 1` to `demo end`, 965 presented lines; 27.2: 42 s, 959 |
| `scripts/ios-sim.sh --smoke`, iOS 18.0 and 27.2 | exit 0 on both: the new three-tab screen holds every M4 check (`probe scene attached`, `probe access ok`, clipboard round trip, `probe safe_area_event 0,62,0,34`, no frame while suspended, `presented 31` while idle, `probe keyboard_inset` 291 on 18.0 and 301 on 27.2) |
| `cargo run -p demo -- --demo --exit-after-demo`, desktop | unverified here: the window was created and no frame was presented in 150 s (the session's windows are occluded; `IOConsoleLocked` read false), so it hung until `timeout` killed it; the headless `apps/demo/tests/showcase.rs` runs the same marks |

The frame rate: the simulator reports 60000 mHz, and the demo draws only when something changes (it asks for a frame at the player's next cue), so the 41 s tour presented about 23 frames a second on average (965 over 41 s), the frames dense while a gesture moves and sparse in the waits between marks. Every mark passed on the first run on both runtimes. Unverified here: 120 Hz (a real device), a live finger (an agent cannot drive the simulator's touch input).

## M8 reading: first frame and GPU

Run 2026-11-22, `scripts/android-emu.sh --probe` (macOS host Darwin 27.2, Apple M1 Pro; emulator 37.3.10, AVD `luneui-demo-pixel7`, Android 14 API 34 google_apis arm64-v8a, pixel_7).

- The `cdylib` links first time and exports `ANativeActivity_onCreate` and `android_main` (`llvm-readelf --dyn-syms`). The AVD booted cold in 17 to 26 s (`-no-snapshot`, time to `sys.boot_completed`).
- `-gpu swiftshader_indirect` presented on the first try: wgpu backend Vulkan, adapter `SwiftShader Device (LLVM 10.0.0)`, type `Cpu`, driver `SwiftShader driver`, surface format `Rgba8Unorm`, alpha mode `Inherit`. `presented 1 1080x2400` and `shot-1.png` shows the Home screen. `-gpu host` was tried once with the demo tour: the adapter was Vulkan `Apple M1 Pro` (KosmicKrisp), and 5 frames were presented, then none for 180 s (a stall, not investigated), so `swiftshader_indirect` is the default.
- The window: `inner_size` 1080x2400, scale factor 2.625 (density 420 over 160; `wm size` 1080x2400, `wm density` 420), `refresh_mhz none`, `inner_position` and `outer_position` `NotSupported` (so `insets_from_geometry` cannot apply; M8 reads the content rect instead).
- Frame rate: the software renderer draws a frame in about 60 ms. Measured from the logcat timestamps of a `--demo --probe` run: 165 frame gaps with a median of 61 ms, a 90th percentile of 252 ms (the long gaps include the app idling until its next scheduled frame, and the first gap was 2.4 s). That is what made the tour's `top` mark fail until the nudge below.

## M8 reading: safe area and life cycle

`mobile::safe_area` (D7, D422) reads `WindowExtAndroid::content_rect()` and the native window's size and scale: left and top are the rect's, right and bottom what is left of the window past it, in logical pixels, each clamped to at least 0 (`insets_from_content_rect`, unit-tested with this reading). winit sends no event for it (its `ContentRectChanged` only logs), so the demo reads it every frame, as on iOS.

| window | content rect (physical px) | `probe safe_area_event` (logical) |
| --- | --- | --- |
| portrait 1080x2400 at rest | `0,136,1080,2337` | `0,51.809525,0,24` (136/2.625, 63/2.625) |
| landscape 2264x1080 at rest (`--landscape`) | `0,74,2264,1017` | `0,28.190475,0,24`; left and right are 0: the 136 px display cutout is outside the window, so this AVD shows no side inset, the formula holds |

`scripts/android-emu.sh --smoke` requires every `probe safe_area_event` line to equal what the preceding `probe content_rect` line gives (computed in the script, `checked=2 bad=none`), portrait and `--landscape`. To turn the display the script puts `accelerometer_rotation 0` and `user_rotation 1` and requires `mDisplayRotation=ROTATION_90`: the system turns the display for the app on top and not while the launcher is (the first put left it at `ROTATION_0`), so the script puts them again after `presented 1`; both settings are put back to the values read (`1` and `0`) on every exit path.

Life cycle (D6, D421): the native window is released after `suspended` returns and a new one comes with `resumed`. Reading before the fix (step 0, `input keyevent KEYCODE_HOME`, then `am start` again): the demo kept presenting (`presented` lines, no error, no crash, same pid) into a screen that was black (a 22 KB screenshot against 250 KB), because its surface was over the released window. Since the fix the demo drops its `WindowSurface` (and the wgpu `Surface`) in `suspended` on Android (`DROP_SURFACE_ON_SUSPEND`) and makes a new one from the same `Arc<Window>` and `Gpu` in `resumed` at the window's current size (a failure prints `surface: <error>` and the app waits for the next `resumed`); `render` with no surface does nothing; iOS keeps its surface. Smoke reading: `probe lifecycle` lines `resumed suspended resumed`, `probe surface_size none` while suspended, frames between `suspended` and `resumed`: 0, after it: 4 (`during=0 after=4`), and the screenshot after the resume draws the app again. The control `--smoke --demo-arg --draw-while-suspended` also read `during=0 after=4`: on Android it cannot show frames while suspended, because the surface is gone; the verdict logic was checked on a known input instead (a `presented` line between the two gives `during=1`).

## M8 reading: the soft keyboard and text

`NativeActivity` raises the soft keyboard: `show_soft_input` (winit's `set_ime_allowed(true)`, from the demo's focused field) gave `mInputShown=true` in `dumpsys input_method` within the 10 s the script allows, so GameActivity stays a follow-up (D2) and not a need. With `adjustResize` the content rect's bottom shrinks while the native window keeps its size (portrait: `0,136,1080,2337` to `0,136,1080,1517`; landscape: `0,74,2264,1017` to `0,74,2264,394`). That is D8's branch (a): `mobile::KeyboardInset::inset()` reports the current bottom inset as the keyboard's bottom when it exceeds the smallest bottom inset seen at that window size (the baseline, the system bars' alone) by more than 1 logical pixel, else zero, and `safe_area` keeps reporting the baseline as its bottom (`0,51.809525,0,24` held while the keyboard was up). Readings: portrait `probe keyboard_inset 0,0,0,336.38095` (883 px over 2.625), landscape `0,0,0,261.33334`. `on_change` never runs (no event); the app sees it on its next frame, and `safe_area` must be called before `inset`. A keyboard already up when the window changes size makes the baseline too big until it hides (pinned by a test).

Text: `adb shell input text luneui` into the focused search field gave `probe field "luneui"` (winit maps the key events to `event.text`), portrait and landscape; `--keyboard` and `--smoke` require it. Unverified here: typing on the keyboard's own keys (the IME commit and composition path: the keys' layout is read from a screenshot, and winit 0.30.13 surfaces no IME events on Android), hardware keyboards and other IMEs.

## M8 reading: the clipboard

`mobile::Pasteboard` on Android (D9, D422) is `ClipboardManager` through JNI: the VM and the Application from `ndk_context::android_context()` (android-activity fills both before `android_main`), `getSystemService("clipboard")`, `getPrimaryClip()` item 0 `coerceToText(application).toString()` for a read, `ClipData.newPlainText` and `setPrimaryClip` for a write; every call runs in a local frame, and a Java exception, a null or a missing context answers `Err(())`. `unsafe` is in `platform/mobile/android.rs` only, two blocks with `SAFETY` comments. Reading (`--smoke`, `--probe-clipboard`): `probe clipboard_write luneui-probe-<pid>` on frame 2 and `probe clipboard luneui-probe-<pid>` on frame 4, equal, the same lines as the iOS run. Unverified here: a read when the app lacks input focus (Android 10 and later answer null, which is `Err(())`), other API levels, and the exception path (nothing made it throw).

## M8 reading: live touch

Android takes real OS input from an agent (`adb shell input tap|swipe|text`, MotionEvents and KeyEvents), which the iOS simulator could not (D12). The demo prints where its controls are under `--probe` (physical pixels): `probe tab_item <i> <x>,<y>` for each tab bar item's centre, `probe pager <x0>,<y>,<x1>` (a swipe line across the Cards card, right to left), and `probe tab`, `probe page`, `probe field` when they change. `--smoke` reads: `input tap` on the Cards item gives `probe tab 1`; `input swipe` along `probe pager` (300 ms) gives `probe page 1`; `input tap` on the Home item gives `probe tab 0` (portrait: 540,2273; 889,1272 to 191; 180,2273. Landscape: 1132,953; 1895,581 to 369; 377,953). So the winit Android touch mapping, the reduction to the toolkit's touch events and the widgets are read once through the OS path. One finding: a Play services dialog (`com.google.android.gms/.nearby.sharing.SettingsReviewActivity`) takes input focus a few seconds after every boot of this AVD, and the first tap went to it; the script checks `dumpsys window | grep mCurrentFocus` before each input and, while the demo is not focused, sends Back (the demo ignores Back) and `am start`, with three tries per input. Back itself does nothing in the demo (step 0 reading: no life-cycle line, the process stays, focus stays; winit's Android backend reports every key but the volume keys as handled, `platform_impl/android/mod.rs:374-440`); mapping it to the navigation stack is an app's choice, not made here.

## M8 reading: demo mode

`scripts/android-emu.sh --demo` (launch arguments `--demo --touches`, the log followed, three screenshots at `sheet-half`, `page-2` and `settings`, 16 marks `ok` and `demo end <n>/<n>` required within 60 s plus 120 s): exit 0, `demo end 16/16` after 43 s, three screenshots written (`shot-3.png`, read, shows Settings with the volume at 80 %; `shot-1.png` and `shot-2.png` were not read).

The first run failed: `demo mark top FAIL list offset is 31.7, want 0`. The tour was written against 60 and 120 Hz frames; the emulator draws about every 61 ms. The pointer reads a release's speed from the frames' positions and takes a finger that was still for the last 40 ms (`VELOCITY_STILL`, `crates/lune-ui-core/src/ui/pointer.rs`) as still, so with frames more than 40 ms apart the only sample inside that window is the last one and every swipe's fling reads zero. The tour's swipe back to the top then left the list 31 to 36 points short (a headless run with fixed frame periods of 45, 60 and 100 ms reproduced it on all three layouts; 16, 30, 36 and 40 ms passed). The tour now finishes with a short drag down (`NUDGE`, 48 points, `apps/demo/src/showcase.rs`) after the swipe: where the swipe did not fling it covers the rest, where it already reached the top it is a pull under the refresh threshold (64) that springs back, so both end at 0. `apps/demo/tests/showcase.rs` gained steady schedules of 8, 33, 45, 60 and 100 ms and a replay of the emulator's measured frame gaps (each at most 250 ms), on the two phone layouts and the emulator's, beside M7's four. The library's fling at frames over 40 ms is a limit of the pointer, not changed here (a follow-up: it affects any app whose frames are slower than 25 per second). `scripts/ios-sim.sh --demo` still passes 16/16 (43 s). Not verified: other emulator renderers (`-gpu host` stalled) and a real phone.

## M8 reading: accessibility

Not wired on Android (D10): the demo's default build has no `access` feature and the null bridge stands (`probe access off`). `accesskit_android` would be a new package in the lock with `NativeActivity` support unknown, and TalkBack cannot be driven here. A follow-up would check the tree with `adb shell uiautomator dump`.

## Unverified here

- A real device: 120 Hz (the simulator reports 60000 mHz), the GPU's cost (glass blur), thermal behaviour, the real safe area of other models, landscape and the Dynamic Island's insets.
- A live finger on the iOS simulator: not driven by an agent (Android's is, M8 reading above) (the demo's scripted touches, `--demo`, enter through `RawInput.events`, not through the simulator's input), typed text through the soft keyboard and IME, rotation, the home indicator behaviour.
- iOS 27 on a real device: winit 0.30.13 has no `UIScene` life cycle, and the toolkit adopts it outside winit (`mobile::attach_to_scene` and the manifest, M4 reading above), which launches on the iOS 27.2 simulator; nothing ran on a device. A scene the OS discards and reconnects, and rotation at run time, were not driven.
- Android (M8, an emulator only): a real phone (GPU other than SwiftShader's CPU rendering, 120 Hz, other API levels, 16 KB pages: the `.so` is not linked for them, `-z max-page-size=16384`), typing on the soft keyboard's own keys and IME composition (the keys' layout is read from a screenshot; winit surfaces no IME events), a clipboard read without input focus, the clipboard's Java exception path, accessibility (D10: not wired), a landscape display with a side cutout inside the window, Back mapped to the navigation stack, and `-gpu host`. The fling at frame periods over 40 ms (a limit of the pointer, M8 demo mode reading).
