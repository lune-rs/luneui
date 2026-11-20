//! The demo's binary: builds the event loop and hands it to [`demo::run`]. On iOS `run_app` never
//! returns, so nothing may follow it that the platform needs.

#![deny(unsafe_code)]

use std::process::ExitCode;

use demo::Options;
use winit::event_loop::EventLoop;

fn main() -> ExitCode {
    let event_loop = match EventLoop::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("event loop: {e}");
            return ExitCode::from(1);
        }
    };
    match demo::run(event_loop, Options::default()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
