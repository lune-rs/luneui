//! The demo's binary: builds the event loop and hands it to [`demo::run`]. On iOS `run_app` never
//! returns, so nothing may follow it that the platform needs.

#![deny(unsafe_code)]

use std::process::ExitCode;

use std::path::PathBuf;

use demo::Options;
use winit::event_loop::EventLoop;

const USAGE: &str = "usage: demo [--probe] [--frames <n>] [--no-scene-attach] [--draw-while-suspended] [--focus-field] [--probe-clipboard]\n            [--demo [--loop | --exit-after-demo] [--touches]]\n       demo [--demo [--touches]] --dump <dir>   (headless: home, detail, sheet, cards and settings as PNGs; with --demo one PNG per mark of the tour)\n  --demo              play the showcase tour hands-free (demo mark <name> ok|FAIL, demo end <ok>/<total>)\n  --loop              with --demo: play it again when it ends\n  --touches           with --demo: draw a disc under each scripted touch\n  --exit-after-demo   with --demo, desktop: exit after the tour, 1 when a mark failed";

/// What the command line asks for.
#[derive(Debug)]
enum Mode {
    /// Open the window with these options.
    Window(Options),
    /// Write PNGs into a directory: the five states, or with `tour` one per mark of the tour
    /// (`touches` draws the touch discs).
    Dump {
        dir: PathBuf,
        tour: bool,
        touches: bool,
    },
}

/// What the command line asks for, or `None` for an argument that is not understood or a
/// combination that makes no sense.
fn parse_args(args: impl Iterator<Item = String>) -> Option<Mode> {
    let mut options = Options::default();
    let mut dump: Option<PathBuf> = None;
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dump" => dump = Some(PathBuf::from(args.next()?)),
            "--probe" => options.probe = true,
            "--frames" => options.frames = Some(args.next()?.parse().ok()?),
            "--no-scene-attach" => options.scene_attach = false,
            "--draw-while-suspended" => options.pause = false,
            "--focus-field" => options.focus_field = true,
            "--probe-clipboard" => options.probe_clipboard = true,
            "--demo" => options.demo = true,
            "--loop" => options.repeat = true,
            "--touches" => options.touches = true,
            "--exit-after-demo" => options.exit_after_demo = true,
            _ => return None,
        }
    }
    // The tour's own flags need the tour; a loop never ends, so it cannot exit after it.
    if (options.repeat || options.exit_after_demo) && !options.demo {
        return None;
    }
    if options.repeat && options.exit_after_demo {
        return None;
    }
    if let Some(dir) = dump {
        // A dump has no window: only the tour's flags that shape it go with it.
        let plain = Options {
            demo: options.demo,
            touches: options.touches,
            ..Options::default()
        };
        if options != plain || (options.touches && !options.demo) {
            return None;
        }
        return Some(Mode::Dump {
            dir,
            tour: options.demo,
            touches: options.touches,
        });
    }
    Some(Mode::Window(options))
}

fn main() -> ExitCode {
    let Some(mode) = parse_args(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let options = match mode {
        Mode::Window(options) => options,
        Mode::Dump {
            dir, tour: false, ..
        } => {
            return match demo::dump::write_all(&dir) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("dump: {e}");
                    ExitCode::from(1)
                }
            };
        }
        Mode::Dump {
            dir,
            tour: true,
            touches,
        } => {
            return match demo::dump::write_tour(&dir, touches) {
                Ok(r) if r.ok == r.total => ExitCode::SUCCESS,
                Ok(_) => ExitCode::from(1),
                Err(e) => {
                    eprintln!("dump: {e}");
                    ExitCode::from(1)
                }
            };
        }
    };
    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("event loop: {e}");
            return ExitCode::from(1);
        }
    };
    match demo::run(event_loop, options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
