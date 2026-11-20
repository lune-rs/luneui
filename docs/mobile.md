# Mobile targets

How the toolkit builds for the phone targets, how the demo runs on an iOS simulator, and what winit and wgpu report there. M4 and M7 extend this page. Source of every reading below: `apps/demo --probe` (macOS host and the iOS simulator), run on 2026-11-20.

## Building

The phone targets are a rustup target each: `rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android`.

- The five library crates and `apps/demo` pass `cargo clippy -p lune-ui-core -p lune-ui-wgpu -p lune-ui-winit -p lune-ui-platform -p lune-ui -p demo --lib --bins --locked --target <target> -- -D warnings` for `aarch64-apple-ios`, `aarch64-apple-ios-sim` and `aarch64-linux-android`; `scripts/check-targets.sh` runs that line as its `ios`, `ios-sim` and `android` halves (D340).
- Android: that clippy passes with no NDK installed (winit's `android-native-activity` compiles no C, and the activity feature is the demo's: `[target.'cfg(target_os = "android")'.dependencies]` in `apps/demo/Cargo.toml`). The demo's `android_main` (`apps/demo/src/lib.rs`) is the one exported symbol android-activity looks up; a `cdylib` and an APK are M8's, nothing has run on Android.
- `arboard` is compiled only off iOS and Android (`lune-ui-winit/src/platform/clipboard_os.rs`); on both phones `clipboard_none.rs` answers every read with `ClipboardUnavailable` and drops every write (M4 gives iOS a real one).
- `lune-ui-wgpu` asks the device for wgpu's default limits lowered to the adapter's (D337): the simulator's Metal offers 15 inter-stage shader variables, the default asks 16.
- `lune-ui-winit --features accesskit` also passes clippy for `aarch64-apple-ios-sim` (`accesskit_ios` 0.2.0 is pulled in); input for M4's accessibility bridge.
- An iOS binary is built with `IPHONEOS_DEPLOYMENT_TARGET=17.0` (the simulator script sets it). Nothing here signs for a real device.

## `scripts/ios-sim.sh`

Builds `apps/demo` for `aarch64-apple-ios-sim`, assembles `target/ios-sim/Demo.app` (the binary and `apps/demo/ios/Info.plist`, ad-hoc signed), runs it on one simulator device of its own and checks that a frame was presented.

- Device: `luneui-demo-iphone`, type `iPhone-16-Pro`, runtime `iOS-18-0` (override with `LUNEUI_IOS_DEVICE_TYPE`, `LUNEUI_IOS_RUNTIME`; any other runtime gets a device of its own, `luneui-demo-iphone-<runtime>`, as `luneui-demo-iphone-iOS-27-2`); found by exact name, created when missing, shut down at the end (`--keep-booted` leaves it booted, `--delete-device` deletes it).
- Default mode: waits up to 60 s for `presented 1` in `target/ios-sim/stdout.log`, takes one screenshot (`shot-1.png`), exits 0 on `ios-sim: passed`. Exit 1 a check failed, 2 setup (tool, rustup target or runtime missing).
- `--probe`: launches the demo with `--probe`, screenshot `shot-1.png` 2 s after `presented 1`, launches the Settings app (`com.apple.Preferences`, by `simctl launch`) to send the demo to the background, launches the demo again (no `--terminate-running-process`), screenshot `shot-2.png`, and writes the `probe`/`presented` lines of both launches to `target/ios-sim/probe.txt`. Two screenshots in all.
- `--smoke`: `--probe`, and also requires `probe scene attached` as the demo's third `probe scene` line (frame 30) and, in launch 1's output, `probe lifecycle resumed` at launch, then `suspended` and `resumed` across the Settings round trip. Two screenshots in all.
- Control flags, which edit only the bundle copy `target/ios-sim/Demo.app/Info.plist` (with `/usr/libexec/PlistBuddy`), never `apps/demo/ios/Info.plist`: `--no-manifest` deletes `UIApplicationSceneManifest` (the app without the scene life cycle); `--landscape` sets `UISupportedInterfaceOrientations` to `UIInterfaceOrientationLandscapeLeft`. `--demo-arg <arg>` passes one more argument to the demo (repeatable), as `--no-scene-attach`, which skips `mobile::attach_to_scene`.
- Never: touches another device, uses `booted` as a device argument, opens Simulator.app or drives the UI, records video, writes outside `target/ios-sim/`.

`cargo run -p demo -- --probe --frames 60` does the same probe on the desktop: a phone-shaped window (402 x 874 points) for about a second, exit 0 after 60 presented frames. `--frames` is ignored on iOS and Android, where the loop cannot return.

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

Redraws on iOS: a `request_redraw()` made inside the `RedrawRequested` handler was not delivered while the app was idle (only `presented 1` appeared in 8 s; frames 2 and 3 appeared only at `suspended` and `resumed`). With `request_redraw()` also called from `about_to_wait`, frames 2 and 3 came at once. winit's `request_redraw` on iOS queues the window when the view is itself Metal-backed and calls `setNeedsDisplay` otherwise (`ios/window.rs:138-150`); which branch this view takes was not checked, and why the request from inside the handler is lost is not established. Only the observation above was run. M4 must drive redraws from `about_to_wait` (or a display link) on iOS.

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
- Not driven here: a scene the OS discards and reconnects (the observer attaches the window to each scene that connects, by design; the simulator gives no way to force it) and rotation at run time.

## Unverified here

- A real device: 120 Hz (the simulator reports 60000 mHz), the GPU's cost (glass blur), thermal behaviour, the real safe area of other models, landscape and the Dynamic Island's insets.
- Touch (the demo's button is drawn but taps are not translated; `Translator` drops `WindowEvent::Touch`), the soft keyboard and IME, rotation, the home indicator behaviour.
- iOS 27 on a real device: winit 0.30.13 has no `UIScene` life cycle, and the toolkit adopts it outside winit (`mobile::attach_to_scene` and the manifest, M4 reading above), which launches on the iOS 27.2 simulator; nothing ran on a device. A scene the OS discards and reconnects, and rotation at run time, were not driven.
- Android: nothing was run (`aarch64-linux-android` is compile-checked only).
