//! Chip 0 build. Links libghostty-vt at the pinned revision only.
//!
//! ADR 0002 D7: upstream declares this API unstable. Building against whatever
//! `main` happened to be is not reproducible, so this fails closed rather than
//! linking an archive of unknown provenance.

use std::env;
use std::path::PathBuf;

fn read_pin(pin_file: &PathBuf) -> String {
    let text = std::fs::read_to_string(pin_file).unwrap_or_else(|e| {
        panic!(
            "missing pin file {} ({e}). ADR 0002 D7 requires a pinned libghostty-vt.",
            pin_file.display()
        )
    });
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("sha") {
            let rest = rest.trim_start();
            if let Some(sha) = rest.strip_prefix('=') {
                let sha = sha.trim().to_string();
                assert_eq!(
                    sha.len(),
                    40,
                    "pin sha must be a full 40-char commit id, got {sha:?}"
                );
                return sha;
            }
        }
    }
    panic!("no `sha = ...` line in {}", pin_file.display());
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest"));
    let root = manifest.join("../..");
    let pin_file = root.join("third_party/ghostty.pin");
    let pin_sha = read_pin(&pin_file);

    let ghostty = env::var("RILL_GHOSTTY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("third_party/ghostty"));
    let include = ghostty.join("include");
    let libdir = ghostty.join("zig-out/lib");
    let archive = libdir.join("libghostty-vt.a");
    let stamp = ghostty.join(".rill-built-sha");

    if !archive.exists() {
        panic!(
            "libghostty-vt.a missing at {}. Run: sh scripts/fetch-libghostty-vt.sh",
            archive.display()
        );
    }

    // Provenance. An archive we cannot attribute to the pin is not linked.
    let built = std::fs::read_to_string(&stamp).unwrap_or_else(|_| {
        panic!(
            "no provenance stamp at {}. The archive was not produced by \
             scripts/fetch-libghostty-vt.sh. Delete {} and re-run it.",
            stamp.display(),
            ghostty.display()
        )
    });
    let built = built.trim();
    if built != pin_sha {
        panic!(
            "libghostty-vt provenance mismatch.\n  pin:   {pin_sha}\n  built: {built}\n\
             Moving the pin is its own PR with the gate suite re-run (ADR 0002 D7).\n\
             To rebuild at the pin: sh scripts/fetch-libghostty-vt.sh"
        );
    }

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let staged = out.join("libghostty-vt.a");
    std::fs::copy(&archive, &staged).expect("stage libghostty-vt.a");

    println!("cargo:rerun-if-changed=src/adapter/rill_chip0_vt.c");
    println!("cargo:rerun-if-changed=src/adapter/rill_chip0_vt.h");
    println!("cargo:rerun-if-changed={}", pin_file.display());
    println!("cargo:rerun-if-changed={}", stamp.display());
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rustc-env=RILL_GHOSTTY_SHA={pin_sha}");

    let mut build = cc::Build::new();
    build
        .file("src/adapter/rill_chip0_vt.c")
        .include(&include)
        .define("GHOSTTY_STATIC", None)
        .warnings(true)
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wextra")
        .flag_if_supported("-Werror=array-bounds")
        .flag_if_supported("-Werror=stack-protector")
        .flag_if_supported("-fstack-protector-strong");

    // The gates run this under ASan over fixtures/bytes/ (SPEC-CHIP0 §5).
    if env::var("RILL_ASAN").is_ok() {
        build.flag_if_supported("-fsanitize=address");
        println!("cargo:rustc-link-arg=-fsanitize=address");
    }

    build.compile("rill_chip0_vt");
}
