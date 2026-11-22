//! The demo is a consumer of the toolkit's public API (C4(b)): it depends on the facade and the
//! platform crates only, and names no core or test-support path.

use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED: [&str; 4] = ["lune-ui", "winit", "wgpu", "pollster"];

/// The dependency names in every `[...dependencies]` table of the manifest.
fn dependency_names(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_deps = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_deps = line
                .trim_matches(|c| c == '[' || c == ']')
                .ends_with("dependencies");
            continue;
        }
        if in_deps
            && !line.starts_with('#')
            && let Some((name, _)) = line.split_once('=')
        {
            names.push(name.trim().to_owned());
        }
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
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("manifest");
    let names = dependency_names(&manifest);
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
