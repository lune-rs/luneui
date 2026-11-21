//! The demo's binary: builds the event loop and hands it to [`demo::run`]. On iOS `run_app` never
//! returns, so nothing may follow it that the platform needs.

#![deny(unsafe_code)]

use std::process::ExitCode;

use demo::Options;
use winit::event_loop::EventLoop;

const USAGE: &str =
    "usage: demo [--probe] [--frames <n>] [--no-scene-attach] [--draw-while-suspended]";

/// The options the command line asks for, or `None` for an argument that is not understood.
fn parse_args(args: impl Iterator<Item = String>) -> Option<Options> {
    let mut options = Options::default();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--probe" => options.probe = true,
            "--frames" => options.frames = Some(args.next()?.parse().ok()?),
            "--no-scene-attach" => options.scene_attach = false,
            "--draw-while-suspended" => options.pause = false,
            _ => return None,
        }
    }
    Some(options)
}

fn main() -> ExitCode {
    let Some(options) = parse_args(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
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
