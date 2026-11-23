//! The demo is a consumer of the toolkit's public API (C4(b)): it depends on the facade and the
//! platform crates only, and names no core or test-support path.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const ALLOWED: [&str; 8] = [
    "lune-ui",
    "winit",
    "wgpu",
    "pollster",
    // The browser build (D426): the clock, and the glue to the page and its console.
    "web-time",
    "wasm-bindgen",
    "wasm-bindgen-futures",
    "web-sys",
];

/// The names of the package's dependencies of every kind and target, as cargo resolves them
/// (`cargo metadata --no-deps`, offline), so no manifest spelling (a `[dependencies.<name>]`
/// table, a target table, a trailing comment, a rename) can hide one.
fn dependency_names(root: &Path) -> Vec<String> {
    let out = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(root)
        .output()
        .expect("run cargo metadata");
    assert!(out.status.success(), "cargo metadata failed");
    let json = String::from_utf8(out.stdout).expect("utf-8 metadata");
    names_in_metadata(&json, "demo")
}

/// Every dependency `name` of the package called `package` in `cargo metadata` output. The
/// output is scanned token by token (strings with their escapes, brackets by depth) because the
/// test may add no JSON crate; it panics when the package or its list is not found.
fn names_in_metadata(json: &str, package: &str) -> Vec<String> {
    let head = format!("\"name\":\"{package}\",\"version\"");
    let start = json.find(&head).expect("the demo package in the metadata");
    let rest = &json[start..];
    let key = "\"dependencies\":[";
    let list = rest.find(key).expect("the dependencies list") + key.len();
    let bytes = rest.as_bytes();
    let (mut i, mut depth) = (list, 1usize);
    let mut names = Vec::new();
    while depth > 0 {
        match bytes[i] {
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth -= 1,
            b'"' => {
                let from = i + 1;
                i = from;
                while bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                let text = &rest[from..i];
                // A key at the depth of a dependency object's own fields is followed by `:`.
                if depth == 2 && text == "name" && bytes[i + 1] == b':' {
                    let value = rest[i + 2..].strip_prefix('"').expect("a string name");
                    names.push(value[..value.find('"').expect("closing quote")].to_owned());
                }
            }
            _ => {}
        }
        i += 1;
    }
    names
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_manifest_names_only_the_facade_and_the_platform_crates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let names = dependency_names(root);
    assert!(names.iter().any(|n| n == "lune-ui"), "{names:?}");
    for name in &names {
        assert!(ALLOWED.contains(&name.as_str()), "dependency `{name}`");
    }
}

#[test]
fn no_source_names_a_core_or_test_support_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Built by concatenation so this file does not match itself.
    let needles = [
        ["lune_ui", "::testing"].concat(),
        ["lune_ui", "_core"].concat(),
    ];
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    rust_files(&root.join("tests"), &mut files);
    assert!(files.len() >= 4, "{files:?}");
    for file in files {
        let text = fs::read_to_string(&file).expect("read");
        for needle in &needles {
            assert!(
                !text.contains(needle.as_str()),
                "{} names `{needle}`",
                file.display()
            );
        }
    }
}

#[test]
fn the_metadata_scan_sees_every_kind_and_a_table_form() {
    let json = concat!(
        r#"{"packages":[{"name":"other","version":"1","dependencies":[{"name":"x"}]},"#,
        r#"{"name":"demo","version":"0.1.0","description":"a [b] \"c\"","dependencies":["#,
        r#"{"name":"lune-ui","req":"*","features":["a","name"]},"#,
        r#"{"name":"evil","kind":"dev","target":"cfg(target_os = \"ios\")"},"#,
        r#"{"name":"build-one","kind":"build"}],"targets":[{"name":"demo"}]}]}"#
    );
    assert_eq!(
        names_in_metadata(json, "demo"),
        ["lune-ui", "evil", "build-one"]
    );
}
