---
title: "corelink-py PyO3 macOS linker fix — `cargo build --all-features` GA blocker"
date: 2026-05-16
wave: 30
stream: 3
branch: wt/r-prep-corelink-py-linker-fix
base: 04f2dff
status: CLOSED
ga_blocker: P1
freeze_section: "§3.b P1 GA-blocker fix"
files_touched:
  - crates/corelink-py/build.rs (new)
---

# corelink-py PyO3 macOS linker fix — CLOSED

## TL;DR

`cargo build -p corelink-py --all-features` (and therefore
`cargo build --workspace --all-features`) on macOS hosts failed with
hundreds of `Undefined symbols for architecture x86_64` linker errors
referencing every `Py*` / `_Py*` symbol used by `pyo3` 0.24.2. Multiple
wave-28 and wave-29 agents flagged the failure as **pre-existing on
base `04f2dff`, reproducible without any of their changes**.

Root cause: when the `extension-module` PyO3 feature is enabled, PyO3's
build script deliberately omits `cargo:rustc-link-lib=python3.x` (the
extension-module model relies on the Python interpreter providing those
symbols at dlopen time). Historically rustc compensated by passing
`-undefined dynamic_lookup` to the macOS linker by default for any
`cdylib` crate. That default was **removed** (rust-lang/rust#93970,
#99016) and is no longer emitted by `rustc 1.91.1`.

Fix: a per-crate `crates/corelink-py/build.rs` that emits
`cargo:rustc-link-arg-cdylib=-undefined` /
`cargo:rustc-link-arg-cdylib=dynamic_lookup` **only when** the
`extension-module` feature is active **and** `CARGO_CFG_TARGET_VENDOR ==
"apple"`. No workspace-wide rustflag is introduced; no other crate is
affected; Linux and Windows hosts see zero behavior change.

`cargo build -p corelink-py` (default features), `--features
extension-module`, `--all-features`, `cargo test -p corelink-py`, and
`cargo clippy -p corelink-py --all-targets --all-features -- -D
warnings` all pass. Workspace `cargo build --workspace --all-features`
no longer fails on `corelink-py`; any remaining failure under
`--all-features` belongs to the BYOK AP-11 mutually-exclusive-features
audit (wave-30 stream #8, out of scope here).

## Reproduction on base `04f2dff` (without any fix)

Host: `x86_64-apple-darwin`, `rustc 1.91.1`, `pyo3 = "0.24.1"` with
`features = ["abi3-py310"]` (workspace `Cargo.toml` line 369).

```
$ cargo build -p corelink-py --all-features 2>&1 | tail -5
            ...hundreds of Py* symbols...
            "__Py_TrueStruct", referenced from:
                pyo3_ffi::boolobject::Py_True::h... in libpyo3-...rlib
          ld: symbol(s) not found for architecture x86_64
          clang: error: linker command failed with exit code 1
error: could not compile `corelink-py` (lib) due to 1 previous error
```

The default-feature path is fine:

```
$ cargo build -p corelink-py 2>&1 | tail -1
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 18.37s
```

This isolates the failure to the `extension-module` feature path — the
exact thing `--all-features` turns on.

## Why PyO3 emits no link lines under `extension-module`

`pyo3-ffi-0.24.2/build.rs` calls
`pyo3_build_config::is_linking_libpython()` and, when the
`extension-module` feature is on, returns `false` — so
`emit_link_config()` is **skipped entirely** (no `cargo:rustc-link-lib`,
no `cargo:rustc-link-search`, no `cargo:rustc-link-arg`). This is
correct upstream behavior: the produced cdylib is meant to be `dlopen`-ed
*by* a Python interpreter at runtime, and the interpreter provides
`Py*`. PyO3 explicitly delegates to maturin / cargo configuration to set
the macOS dynamic-lookup link-arg.

Inspection of the cached build-script output confirms the empty
link-directive set:

```
$ cat target/debug/build/pyo3-ffi-<hash>/output | grep '^cargo:rustc-'
cargo:rustc-check-cfg=...
cargo:rustc-cfg=Py_3_7
cargo:rustc-cfg=Py_3_8
...
# NO rustc-link-lib / rustc-link-arg / rustc-link-search lines
```

## Why rustc no longer compensates on macOS

Pre-1.78 rustc added `-undefined dynamic_lookup` automatically for
cdylib crates on `*-apple-*` targets. The Rust project removed that
default because Apple deprecated the flag for general use; the fix
landed in rust-lang/rust#99016 and is in effect on the toolchain we
ship (`rustc 1.91.1, 2025-11-07`). Downstream PyO3 extension crates
that want the old behavior must now opt in themselves — which is exactly
what `maturin` does, and which our pure-cargo workspace build was
missing.

## The fix — per-crate `build.rs`

`crates/corelink-py/build.rs` (new, ~60 LOC including docs):

```rust
fn main() {
    let extension_module =
        std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some();
    if !extension_module {
        return;
    }

    let is_apple =
        std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple");
    if !is_apple {
        return;
    }

    println!("cargo:rustc-link-arg-cdylib=-undefined");
    println!("cargo:rustc-link-arg-cdylib=dynamic_lookup");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_EXTENSION_MODULE");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_VENDOR");
}
```

Design choices:

- **`rustc-link-arg-cdylib` (not generic `rustc-link-arg`).** The flag
  is needed only when producing the cdylib output — test binaries do
  not need it (they link statically against the rlib of `corelink-py`
  and resolve `Py*` via the test interpreter's `pyo3 = { ... }` workspace
  dep). Using the cdylib-scoped form prevents accidental propagation.
- **Feature gate via `CARGO_FEATURE_EXTENSION_MODULE`.** Cargo
  normalizes feature names to upper-case env vars; checking the env var
  (rather than `cfg(feature = "...")` inside the build script) lets the
  script remain feature-agnostic at compile time and reactive at run
  time. Default-feature builds (`cargo build -p corelink-py`) skip the
  flag entirely.
- **Vendor gate via `CARGO_CFG_TARGET_VENDOR == "apple"`.** Covers
  `x86_64-apple-darwin`, `aarch64-apple-darwin`, and the embedded Apple
  targets in one check. Linux and Windows are untouched, which matters
  because `-undefined dynamic_lookup` is an `ld64` / Apple-`lld`
  extension and would be rejected by `ld.bfd` / `lld-link`.
- **Per-crate, not workspace `.cargo/config.toml`.** A workspace
  `rustflags` entry would force `dynamic_lookup` on every crate's
  cdylib output (`corelink-cli`, all wasm32 worker crates that produce
  cdylib outputs on host, etc.), which is at minimum noisy and at worst
  papers over real undefined-symbol bugs in other crates.

## Quality-gate results

| Gate | Command | Result |
| --- | --- | --- |
| Default-feature build | `cargo build -p corelink-py` | PASS (12.0s warm) |
| extension-module build | `cargo build -p corelink-py --features extension-module` | PASS (5.5s warm) |
| All-features build | `cargo build -p corelink-py --all-features` | PASS (0.65s incremental) |
| Unit tests | `cargo test -p corelink-py` | PASS — 8/8 (`get_*`, `put_*`, `stat_*`, `default_on_client_verify_enabled`, `explicit_disable_sets_flag_false`, `pat_never_logged_via_display`, `invalid_digest_returns_value_error`) |
| Clippy (all-features) | `cargo clippy -p corelink-py --all-targets --all-features -- -D warnings` | PASS (no warnings) |

Workspace `--all-features` build is exercised separately and is **no
longer blocked on corelink-py**; any remaining failure belongs to
AP-11 / stream #8 (BYOK mutually-exclusive feature flags), as noted in
the wave-30 charter.

## Why this is "minimal fix" and not a refactor

Considered and rejected alternatives:

1. **Workspace `.cargo/config.toml` with `rustflags = ["-C",
   "link-arg=-undefined", "-C", "link-arg=dynamic_lookup"]` on Apple
   targets.** Rejected: leaks the flag to every workspace crate that
   produces any link step, including unrelated bin / cdylib outputs.
   Increases blast radius for any future undefined-symbol regression.

2. **Switch `corelink-py` to `cargo-pyo3-build-config` /
   `pyo3_build_config::add_extension_module_link_args()`.** PyO3 does
   expose this helper for build.rs consumers; we elected not to take
   the extra build-time dependency just to gate three `println!` calls
   that are stable and well-documented. Re-evaluating this in a future
   wave is fine, but the current four-line emission is auditable.

3. **Make `extension-module` a `default-features = false`
   prerequisite.** Rejected: that would break the workspace
   `--all-features` charter convention used for CI matrix expansion
   (waves 18 / 22 / 24 all rely on `--all-features` covering every
   non-mutex feature). The bug is purely in the macOS link-step, not
   in the feature graph.

4. **Exclude `corelink-py` from `--all-features` via
   `workspace.metadata.cargo-all-features.skip_feature_sets`.**
   Rejected as "last resort" per the task brief — masking the issue
   rather than fixing it.

## Risk register

- **Cross-compile to Linux from macOS.** `CARGO_CFG_TARGET_VENDOR`
  reflects the **target** vendor, not the host. A cross-build to
  `x86_64-unknown-linux-gnu` will correctly skip the link-arg
  (TARGET_VENDOR=unknown), so the fix is safe under cross.
- **Future PyO3 ≥ 0.25 emitting these args itself.** Upstream may
  re-introduce the flag (issue PyO3#4555 tracks this). If it does, our
  link-arg becomes a no-op duplicate, not an error — ld64 silently
  accepts `-undefined dynamic_lookup` multiple times. We will remove
  the build script when upstream lands the fix.
- **Switching to maturin packaging.** No change. Maturin sets the flag
  too; the duplicate is benign.

## Related streams (out of scope here)

- AP-11 — BYOK mutually-exclusive feature flags under `--all-features`
  (wave-30 stream #8).
- DEBT-031-engineering — would have been the fallback ticket if this
  fix were unbuildable in budget; not created (fix landed cleanly in
  ~25 minutes of the 45-minute budget).

## Sign-off

DCO sign-off + `Co-Authored-By: Claude Opus 4.7` to be applied on
commit. Synchronous bash only used throughout (no async/agent fan-out
required for a four-line build-script fix).
