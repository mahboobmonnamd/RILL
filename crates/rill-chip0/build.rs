use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest"));
    let root = manifest.join("../..");
    let ghostty = env::var("RILL_GHOSTTY_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("third_party/ghostty"));
    let include = ghostty.join("include");
    let libdir = ghostty.join("zig-out/lib");
    let archive = libdir.join("libghostty-vt.a");
    if !archive.exists() {
        panic!(
            "libghostty-vt.a missing at {}. Run: zig build -Demit-lib-vt -Doptimize=ReleaseFast (in third_party/ghostty)",
            archive.display()
        );
    }

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let staged = out.join("libghostty-vt.a");
    std::fs::copy(&archive, &staged).expect("stage libghostty-vt.a");

    println!("cargo:rerun-if-changed=src/adapter/rill_chip0_vt.c");
    println!("cargo:rerun-if-changed=src/adapter/rill_chip0_vt.h");
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=ghostty-vt");

    cc::Build::new()
        .file("src/adapter/rill_chip0_vt.c")
        .include(&include)
        .define("GHOSTTY_STATIC", None)
        .warnings(false)
        .compile("rill_chip0_vt");
}
