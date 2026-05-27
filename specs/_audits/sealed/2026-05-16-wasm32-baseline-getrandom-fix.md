# Wave-26 — wasm32-unknown-unknown baseline fix (`getrandom 0.4.2` transitive)

- **Date**: 2026-05-16
- **Wave**: 26 (R-prep)
- **Branch**: `wt/r-prep-wasm32-baseline-getrandom`
- **Base**: `main 2a4e00c`
- **Driver**: pre-existing baseline failure flagged in wave-25 audit §8 (commit `7e1a694` first surfaced the diagnosis; the wave-25 audit `2026-05-16-tenant-config-cf-prod-wire.md` §8 explicitly defers the fix to a future wave).
- **Scope**: workspace-only build-configuration change. No runtime behaviour change. No spec-document change. No new INV/CTRL/PAT/FM coverage.

---

## 1. Root cause

`uuid 1.23.1` declares two distinct RNG-backed feature paths:

1. `rng` — pulls `dep:getrandom` directly (only on **non**-wasm32 targets per uuid's `[target.'cfg(not(all(target_arch = "wasm32", ...)))'.dependencies.getrandom]` gate).
2. `rng-getrandom` — pulls `uuid-rng-internal-lib/getrandom`, which is the optional `getrandom 0.4` dep declared inside the `uuid-rng-internal` companion crate. This path is target-unconditional.

Seven workspace crates enable `rng-getrandom` directly:

- `corelink-audit`
- `corelink-pat`
- `corelink-privacy-erasure-worker` (indirectly, see its Cargo.toml comment)
- `corelink-reapi` (gated under `host-server`)
- `corelink-survey`
- `corelink-tenant-path`
- `corelink-webauthn`

`corelink-tenant-path` flows into `corelink-worker → corelink-cf-bindings → {corelink-clerk-cf, corelink-dsr-statuspage-scheduler, corelink-privacy-erasure-worker, corelink-statuspage-real}` — the four CF-worker crates that target `wasm32-unknown-unknown`. Cargo feature-unification therefore pulls `getrandom 0.4.2` into the wasm32 build of all four.

`getrandom 0.4.x` introduced a backend-selection design where the wasm32-unknown-unknown target compiles an **empty** `backends` module unless BOTH of the following are set:

1. **cargo feature** `wasm_js` — pulls `wasm-bindgen` + `js-sys` deps used by the wasm backend module.
2. **rustc cfg** `--cfg=getrandom_backend="wasm_js"` — selects the wasm_js backend module via `cfg_if` inside `getrandom::backends`.

Without both, the build fails with E0425:

```
error[E0425]: cannot find function `fill_inner` in module `backends`
  --> getrandom-0.4.2/src/lib.rs:120:19
error[E0425]: cannot find function `inner_u32` in module `backends`
  --> getrandom-0.4.2/src/lib.rs:144:15
error[E0425]: cannot find function `inner_u64` in module `backends`
  --> getrandom-0.4.2/src/lib.rs:158:15
```

(`getrandom 0.2` does not have this requirement — it uses a different feature-gate scheme — which is why `uuid 1.x` with the `js` feature works for `getrandom 0.2` but not `0.4`.)

---

## 2. Fix mechanism

Two coordinated changes that together activate the `wasm_js` backend in the wasm32 build:

### 2a. `.cargo/config.toml` (workspace root) — new file

```toml
[target.wasm32-unknown-unknown]
rustflags = ['--cfg=getrandom_backend="wasm_js"']
```

The cfg only applies to the wasm32-unknown-unknown target. It is inert for native builds (no other crate keys off `getrandom_backend`).

### 2b. `crates/corelink-cf-bindings/Cargo.toml` — new wasm32-targeted dep

```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
getrandom_v04 = { package = "getrandom", version = "0.4", default-features = false, features = ["wasm_js"] }
```

`corelink-cf-bindings` is chosen as the home for this dep because every CF worker crate that targets wasm32 already depends on it (it is the canonical CF binding adapter layer per `R2-10`). The renamed `getrandom_v04` alias avoids any future collision with the `getrandom = "0.2"` workspace dep on the BYOK code path.

The `default-features = false` keeps the `std` feature off (CF Workers run no_std-friendly).

### Why both halves are needed

- The cfg alone leaves the wasm backend module referenced but compiles it without `wasm-bindgen` / `js-sys` (linker / type errors).
- The feature alone pulls the deps in but the backend module is never selected (E0425 persists).

This dual-knob design is intentional in `getrandom 0.4` — the cfg distinguishes between "user explicitly wants wasm_js backend" (browser/Cloudflare Workers) vs. "user wants the `unsupported` no-op backend" (WASI-snapshot targets that have their own randomness source). See the [getrandom 0.4 backend matrix](https://github.com/rust-random/getrandom/blob/master/src/lib.rs) header docs.

---

## 3. Why NOT `[patch.crates-io]`

The wave-25 §8 caveat suggested a `[patch.crates-io]` block pinning `getrandom = { version = "0.4", features = ["js"] }`. This was investigated and rejected:

1. Cargo's `[patch.crates-io]` does **not** allow specifying features — it only allows replacing the source of the crate (path/git/registry). Feature flags are resolved at the consumer's `[dependencies]` declaration. A `[patch]` entry with a features key is silently ignored.
2. The feature name in `getrandom 0.4.2` is `wasm_js`, not `js`. (`js` is the uuid 1.x feature name; uuid's `js` feature only enables `wasm-bindgen` / `js-sys` on uuid itself, not on the transitive getrandom.)
3. Even with a path-patch to a fork that hard-coded the feature, the missing rustc cfg would still cause E0425.

The chosen mechanism (`.cargo/config.toml` + direct wasm32-only feature-bearing dep) is the upstream-recommended pattern documented in `getrandom`'s own README for downstream wasm32 consumers.

---

## 4. Verification matrix

| Gate                                                                                       | Result    | Notes                                                                         |
| ------------------------------------------------------------------------------------------ | --------- | ----------------------------------------------------------------------------- |
| `cargo build --workspace` (default features, native)                                       | **GREEN** | 2m 35s on M3; no new warnings.                                                |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`                         | **GREEN** | Was broken on base `2a4e00c`; now compiles in 40s cold.                       |
| `cargo build --target wasm32-unknown-unknown -p corelink-stripe-real`                      | **GREEN** | Preserves wave-19 wasm32 closure.                                             |
| `cargo build --target wasm32-unknown-unknown -p corelink-billing-stripe-materializer`      | **GREEN** | Preserves wave-22 wasm32 closure.                                             |
| `cargo test -p corelink-clerk-cf`                                                          | **GREEN** | 6 unit tests + 2 doc-tests (ignored).                                         |
| `cargo clippy --workspace --all-targets -- -D warnings`                                    | **CLEAN** | No new warnings introduced.                                                   |
| `python3 scripts/validate_specs.py`                                                        | **GREEN** | 446 with full schema, 9 YAML-only (455 total).                                |
| `python3 scripts/validate_references.py`                                                   | **GREEN** | No dangling refs.                                                             |

Reproduction (pre-fix) on `main 2a4e00c`:

```
$ cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf
error[E0425]: cannot find function `fill_inner` in module `backends`
   --> /Users/.../getrandom-0.4.2/src/lib.rs:120:19
[... 3 more E0425 errors ...]
error: could not compile `getrandom` (lib) due to 4 previous errors
```

Post-fix:

```
$ cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 40.11s
```

---

## 5. Charter compliance

- `#![forbid(unsafe_code)]` preserved on `corelink-cf-bindings` (the only crate whose `Cargo.toml` was edited carries the lint already at the file level).
- No `unwrap` / `expect` / `panic` / `unreachable` / `todo` in src — pure build-configuration change, no src delta.
- DCO sign-off + Co-Authored-By on the commit (see git log).
- No `unsafe`, no FFI, no new public API surface, no schema change, no migration.

---

## 6. Spec / INV / CTRL coverage

None. This is a build-system fix. No invariant or control is added or weakened. The closure flips wave-25 audit §8 first bullet to `CLOSED-WAVE-26` and adds this audit doc as the canonical fix reference.

---

## 7. Caveats / follow-ons

- `getrandom 0.2` is still present in the workspace dep tree (transitive via `rand` 0.8 and `ring`'s legacy paths) — it has no wasm32-unknown-unknown issue, and we keep the existing `getrandom = "0.2"` workspace dep for the BYOK code path. The wasm32 backend selection is independent across the two majors.
- The `.cargo/config.toml` file is workspace-scoped via cargo's nearest-ancestor resolution. CI invocations from the workspace root pick it up automatically. Tooling that runs from a subdirectory inside the workspace (e.g. `cargo build` from `crates/corelink-clerk-cf`) also picks it up because cargo searches upward.
- No CI matrix change is required — the existing wasm32 build job (per `R2-10` policy) already executes from the workspace root.

---

## 8. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
