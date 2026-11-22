//! The demo's binary: builds the event loop and hands it to [`demo::run`]. On iOS `run_app` never
//! returns, so nothing may follow it that the platform needs.

#![deny(unsafe_code)]

use std::process::ExitCode;

use std::path::PathBuf;

use demo::Options;
use winit::event_loop::EventLoop;

const USAGE: &str = "usage: demo [--probe] [--frames <n>] [--no-scene-attach] [--draw-while-suspended] [--focus-field] [--probe-clipboard]\n       demo --dump <dir>   (headless: home, detail, sheet, cards and settings as PNGs)";

/// What the command line asks for.
#[derive(Debug)]
enum Mode {
    /// Open the window with these options.
    Window(Options),
    /// Write the screens' PNGs into a directory.
    Dump(PathBuf),
}

/// What the command line asks for, or `None` for an argument that is not understood.
fn parse_args(args: impl Iterator<Item = String>) -> Option<Mode> {
    let mut options = Options::default();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dump" => {
                let dir = PathBuf::from(args.next()?);
                return args.next().is_none().then_some(Mode::Dump(dir));
            }
            "--probe" => options.probe = true,
            "--frames" => options.frames = Some(args.next()?.parse().ok()?),
            "--no-scene-attach" => options.scene_attach = false,
            "--draw-while-suspended" => options.pause = false,
            "--focus-field" => options.focus_field = true,
            "--probe-clipboard" => options.probe_clipboard = true,
            _ => return None,
        }
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
        Mode::Dump(dir) => {
            return match demo::dump::write_all(&dir) {
                Ok(()) => ExitCode::SUCCESS,
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
