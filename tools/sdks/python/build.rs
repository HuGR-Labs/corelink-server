//! Build script for `corelink-py`.
//!
//! # Why this exists
//!
//! When the `extension-module` cargo feature is enabled, PyO3 deliberately does
//! *not* emit `cargo:rustc-link-lib=python3.x` because the resulting `cdylib`
//! is loaded *by* a Python interpreter at runtime — the interpreter itself
//! provides all `Py*` symbols. This is the canonical extension-module model.
//!
//! Historically the Rust compiler defaulted to passing
//! `-undefined dynamic_lookup` to the macOS linker for `cdylib` crates, which
//! allowed the unresolved `Py*` symbols to be deferred to load time. That
//! default was removed (see rust-lang/rust#93970 and #99016) and is no longer
//! emitted by modern toolchains. As a result, on macOS hosts a plain
//! `cargo build -p corelink-py --features extension-module` (and therefore
//! also `cargo build --workspace --all-features`) fails with hundreds of
//! "Undefined symbols for architecture …" linker errors for every `Py*` /
//! `_Py*` symbol referenced by `pyo3` / `pyo3-ffi`.
//!
//! Upstream PyO3's recommended fix for downstream crates is to either:
//!   1. build via `maturin` (which sets the rustflag), or
//!   2. add `-undefined dynamic_lookup` to `RUSTFLAGS` / `.cargo/config.toml`.
//!
//! Option (2) at workspace scope would leak the rustflag to every crate in the
//! repository, which is undesirable. This build script is the per-crate
//! equivalent: it conditionally emits the link-arg **only** when targeting an
//! Apple platform **and** the `extension-module` feature is active, so the
//! workspace-wide `cargo build --all-features` succeeds without affecting any
//! other crate.
//!
//! Without this fix, the `corelink-py` crate is unbuildable on macOS under
//! `--all-features`, which is a P1 GA blocker per the wave-30 audit
//! (`specs/_audits/sealed/2026-05-16-corelink-py-linker-fix.md`).

fn main() {
    // Only act when the consumer actually enables the PyO3 extension-module
    // feature. The bare `cargo check` / `cargo build` path (no features) still
    // succeeds upstream and must not be perturbed.
    let extension_module = std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some();
    if !extension_module {
        return;
    }

    // `CARGO_CFG_TARGET_VENDOR=apple` covers x86_64-apple-darwin,
    // aarch64-apple-darwin, and the iOS / tvOS / watchOS Apple targets — all of
    // which share the same dyld-based dynamic-lookup model.
    let is_apple = std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple");
    if !is_apple {
        return;
    }

    // Emit the linker flag in the two-argument form expected by ld64 / lld.
    // Using `cargo:rustc-link-arg-cdylib` (instead of the generic
    // `cargo:rustc-link-arg`) confines the flag to the cdylib output and
    // avoids accidentally affecting integration test binaries.
    println!("cargo:rustc-link-arg-cdylib=-undefined");
    println!("cargo:rustc-link-arg-cdylib=dynamic_lookup");

    // Re-run only when the feature set or target changes.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_EXTENSION_MODULE");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_VENDOR");
}
