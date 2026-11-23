//! The demo's binary: a window on a desktop, `--dump` headless, and in a browser the page's entry.

#![deny(unsafe_code)]

#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;
#[cfg(not(target_family = "wasm"))]
use std::process::ExitCode;

#[cfg(not(target_family = "wasm"))]
use web_demo::dump::{self, Request};
#[cfg(not(target_family = "wasm"))]
use web_demo::shell::WINDOW_USAGE;
#[cfg(not(target_family = "wasm"))]
use winit::event_loop::EventLoop;

#[cfg(not(target_family = "wasm"))]
const USAGE_DUMP: &str = "       web-demo --dump <dir>                      every scene at 1440x900 @2 and 390x844 @3\n       web-demo --dump <dir> --only <scene> [--window <W>x<H>] [--scale <S>] [--at <secs>] [--to <scene> --progress <0-100>] [--motion full|reduced|off] [--name <file>]";

/// What the command line asks for.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug)]
enum Mode {
    Window(web_demo::Options),
    Dump {
        dir: PathBuf,
        request: Option<Request>,
    },
}

/// What the command line asks for, or `None` for an argument that is not understood.
#[cfg(not(target_family = "wasm"))]
fn parse(args: impl Iterator<Item = String>) -> Option<Mode> {
    let mut dir: Option<PathBuf> = None;
    let mut dump_args = Vec::new();
    let mut rest = Vec::new();
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dump" => dir = Some(PathBuf::from(args.next()?)),
            "--only" | "--window" | "--scale" | "--at" | "--to" | "--progress" | "--name" => {
                dump_args.push(arg);
                dump_args.push(args.next()?);
            }
            // A flag both kinds of run read: the dump takes it too.
            "--motion" => {
                let word = args.next()?;
                dump_args.push(arg.clone());
                dump_args.push(word.clone());
                rest.push(arg);
                rest.push(word);
            }
            _ => rest.push(arg),
        }
    }
    match dir {
        Some(dir) => {
            let request = dump::parse_request(dump_args.into_iter())?;
            if request.is_none() && !rest.is_empty() {
                return None;
            }
            Some(Mode::Dump { dir, request })
        }
        None if !dump_args.is_empty() => None,
        None => web_demo::parse_args(rest.into_iter()).map(Mode::Window),
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() -> ExitCode {
    let Some(mode) = parse(std::env::args().skip(1)) else {
        eprintln!("{WINDOW_USAGE}\n{USAGE_DUMP}");
        return ExitCode::from(2);
    };
    match mode {
        Mode::Dump { dir, request } => {
            let done = match request {
                None => dump::write_all(&dir),
                Some(r) => dump::write_one(&dir, &r),
            };
            match done {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("dump: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Mode::Window(options) => {
            let event_loop = match EventLoop::new() {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("event loop: {e}");
                    return ExitCode::from(1);
                }
            };
            match web_demo::run(event_loop, options) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
    }
}

/// The browser build: the page's query string carries the flags ([`web_demo::web::main`]).
#[cfg(target_family = "wasm")]
fn main() {
    web_demo::web::main();
}
