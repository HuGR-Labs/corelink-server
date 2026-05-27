# W35-P2-TELEMETRY — Absorption of 5 sub-crates into `corelink-telemetry`

**Status:** SEALED
**Date:** 2026-05-26
**Branch:** `w35-p2-telemetry`
**Parent spec:** `specs/_audits/sealed/2026-05-26-wave-33-34-closure-followups.md` §4
**Charter mandate:** "Pure refactor. No behaviour drift. Preserve `#[non_exhaustive]`, `#![forbid(unsafe_code)]`, no `unwrap`/`expect`/`panic` outside tests, no `use tokio` in src/, INV-AUDIT + CTRL-CRED-001."

Tags: `wave-35`, `phase-2`, `absorption`, `telemetry`, `corelink-telemetry`, `seal`.

---

## §1 Scope

Physically absorb the 5 telemetry sub-crates that were carried as
Stage-0 Option-A aggregator re-exports of `corelink-telemetry`, into
that crate as inline `pub mod` modules. This finally collapses the
"two-path" surface (`corelink_canary::X` + `corelink_telemetry::canary::X`)
into a single canonical path because the original sub-crates had only
one external consumer left — the umbrella shim itself — making
physical absorption a no-consumer-migration refactor.

The 2 remaining aggregator tenants (`corelink-tracing`, `corelink-slo`)
are NOT touched here — they still have live external consumers and
their absorption belongs to a future Stream-C consumer-migration
sweep.

## §2 Crates removed (5)

| # | Removed | LOC moved | Tests moved | New canonical surface |
|---|---|---:|---:|---|
| 1 | `crates/corelink-canary` | 2,290 | 96 | `corelink_telemetry::canary` |
| 2 | `crates/corelink-lighthouse-tracker` | 1,057 | 36 | `corelink_telemetry::lighthouse` |
| 3 | `crates/corelink-logpush` | 2,842 | 78 | `corelink_telemetry::logpush` |
| 4 | `crates/corelink-otel-export` | 1,652 | 30 | `corelink_telemetry::otel` |
| 5 | `crates/corelink-synthetic-pager` | 1,170 | 12 | `corelink_telemetry::synthetic_pager` |
| | **Total** | **9,011** | **252** | |

`workspace.members` went 149 → 144 (5 crates dropped).

`workspace.dependencies`: dropped 5 entries
(`corelink-canary`, `corelink-logpush`, `corelink-otel-export`,
`corelink-synthetic-pager`, `corelink-lighthouse-tracker`).

## §3 Mechanics

Per-crate procedure:

1. `git mv crates/<abs>/src/lib.rs → crates/corelink-telemetry/src/<mod>.rs`
   (named for the existing umbrella shim slot; previous shim file was
   `git rm`-ed first).
2. `git mv crates/<abs>/src/*.rs → crates/corelink-telemetry/src/<mod>/`
   for every sibling source file.
3. `git mv crates/<abs>/tests/*.rs → crates/corelink-telemetry/tests/`
   (no filename collisions — confirmed pre-flight).
4. Rewrite intra-crate paths in sibling source files:
   `\bcrate::X` → `crate::<mod>::X` (preserves visibility, only
   updates the absolute path prefix; the old `crate::` rooted at the
   absorbed crate's root, the new `crate::<mod>::` roots at the
   `corelink-telemetry` crate's root with a `<mod>::` hop).
5. Inside `#[cfg(test)] mod tests` blocks, restore `use super::*;`
   (preserves the original semantic — the test's parent is the same
   file's module, regardless of where the file lives in the workspace).
6. Add `pub const fn module_path_marker() -> &'static str` to each
   absorbed `<mod>.rs` so `corelink-telemetry`'s pre-existing
   crate-level smoke test continues to type-check the 7-submodule
   resolve graph.
7. Adjust `include_str!("../../../migrations/d1/0016_log_schema.sql")`
   in `logpush.rs` — relative path from `crates/corelink-telemetry/src/`
   needs 3 `..` segments, not 2 (absorbed crate was at the same depth
   as the umbrella shim file, so the original path travelled the same
   number of `..` segments by coincidence).
8. Rewrite integration test imports:
   `corelink_canary::X` → `corelink_telemetry::canary::X`
   (and analogously for the other 4 absorbed crates).
9. Rewrite the lone doctest in `otel/secret.rs`:
   `use corelink_otel_export::constant_time_secret_eq;` →
   `use corelink_telemetry::otel::constant_time_secret_eq;`.
10. Update `corelink-telemetry/Cargo.toml` — drop the 5
    `corelink-<abs>` workspace deps, add the union of their direct
    deps (`thiserror`, `serde`, `serde_json`, `uuid` v4+v7+serde,
    `subtle`, `tracing`, `corelink-analytics`), move test harness
    declarations (6 `[[test]]` blocks) into the umbrella crate's
    manifest. Update `[package].description` to record the W35-P2
    absorption.
11. Update root `Cargo.toml` — remove the 5
    `crates/corelink-<abs>` `workspace.members` entries and the 5
    matching `workspace.dependencies` path entries.

All file moves done via `git mv` to preserve blame-history.

### §3.1 mod-file naming

The original placement of the absorbed code as `crates/corelink-telemetry/src/<mod>/mod.rs` would have tripped the workspace-wide `mod_module_files = "deny"` clippy lint. Each absorbed `mod.rs` was therefore renamed (via `git mv`) to `crates/corelink-telemetry/src/<mod>.rs` (the modern Rust 2018+ flat layout). The sibling files remained inside `crates/corelink-telemetry/src/<mod>/`.

## §4 Preserved invariants

* `#![forbid(unsafe_code)]` — preserved at crate root + module-level
  in every absorbed module's top-of-file inner attributes.
* `#![deny(missing_docs)]` + `#![deny(missing_debug_implementations)]`
  — preserved at crate root + module-level.
* No `unwrap` / `expect` / `panic` outside `#[cfg(test)]` blocks —
  clippy `unwrap_used = "deny"` etc. enforces this; clippy GREEN.
* No `use tokio` in `src/` — none of the absorbed crates used tokio
  in `src/`; confirmed via `grep -rn 'use tokio' crates/corelink-telemetry/src`.
* `#[non_exhaustive]` enums and structs preserved verbatim (file
  moves are byte-identical; the only character-level edits were
  path rewrites + appended `module_path_marker` fns).
* INV-AUDIT (audit-emit-BEFORE-mutation fail-CLOSED envelope) — not
  edited; only path-prefixes in `use` statements were touched.
* CTRL-CRED-001 (constant-time secret comparison) — `otel::secret`
  module preserved verbatim; the only edit was the doctest
  `use` path.

## §5 Verification (per §4 of mandate)

```bash
$ cargo build -p corelink-telemetry
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.26s

$ cargo clippy -p corelink-telemetry --tests -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.04s

$ cargo test -p corelink-telemetry           # totals across all binaries
TOTAL: 253                                   # 196 lib + 10 + 1 + 20 + 15 + 4 + 6 + 1 doctest

$ for c in corelink-canary corelink-lighthouse-tracker corelink-logpush \
           corelink-otel-export corelink-synthetic-pager; do
    test -d "crates/$c" && echo "FAIL: $c" || echo "OK: $c removed"
  done
OK: corelink-canary removed
OK: corelink-lighthouse-tracker removed
OK: corelink-logpush removed
OK: corelink-otel-export removed
OK: corelink-synthetic-pager removed

$ grep -rEn "<<<<<<<|>>>>>>>" --include='*.toml' --include='*.rs' --include='*.md' \
       crates/corelink-telemetry Cargo.toml
                                                # no output — no merge markers
```

Pre-absorption test count baseline (run on stashed HEAD across the 5
absorbed crates only):

```
$ cargo test -p corelink-canary -p corelink-lighthouse-tracker \
             -p corelink-logpush -p corelink-otel-export \
             -p corelink-synthetic-pager
ORIG TOTAL: 252
```

Post-absorption corelink-telemetry test count: **253** = 252 absorbed
tests + 1 pre-existing `module_path_marker` umbrella smoke test
preserved unchanged. Zero regression; the count is conserved exactly.

## §6 Parallel-safety surface

Touched files:

* `Cargo.toml` (root) — `[workspace.members]` + `[workspace.dependencies]` (5 entries each removed)
* `Cargo.lock` — re-generated by cargo build
* `crates/corelink-telemetry/Cargo.toml` — manifest
* `crates/corelink-telemetry/src/**` — all absorbed code lives here now
* `crates/corelink-telemetry/tests/**` — 6 integration test files (5 moved + 1 was already empty dir, so created)
* `specs/_audits/sealed/2026-05-26-w35-p2-telemetry-absorption.md` — this file

NOT touched:

* `crates/corelink-cas*` — owned by W35-P2-CAS
* `crates/corelink-adapter-*` — owned by W35-P2-ADAPTER-HOST
* Anything under `corelink-billing/*`, `corelink-ops/*`, `corelink-auth/*` — W36 Stage 2.C zone
* `corelink-tracing`, `corelink-slo` — remaining aggregator tenants, deferred

Conflict surface against sibling agents: only the root `Cargo.toml` (5
distinct line ranges per agent; orchestrator union-merges).

## §7 Definition-of-Done

- [x] 5 absorbed dirs gone from `crates/`
- [x] 5 `workspace.members` entries gone from root `Cargo.toml`
- [x] 5 `workspace.dependencies` entries gone from root `Cargo.toml`
- [x] `corelink-telemetry/Cargo.toml` re-targeted at union of absorbed deps + 6 test harness declarations
- [x] All absorbed source files preserved verbatim except for path-prefix rewrites + appended `module_path_marker`
- [x] All absorbed tests preserved verbatim except for `corelink_<abs>::X` → `corelink_telemetry::<mod>::X` rewrites
- [x] `cargo build -p corelink-telemetry` GREEN
- [x] `cargo clippy -p corelink-telemetry --tests -- -D warnings` GREEN
- [x] `cargo test -p corelink-telemetry` GREEN (253 tests pass, zero regression vs. 252 originals + 1 pre-existing smoke)
- [x] Audit (this file) written
- [x] Commit drafted with `seal(w35-p2-telemetry): …` subject line

## §8 SEAL

```
SEAL — W35-P2-TELEMETRY
Branch: w35-p2-telemetry
Crates removed (5): canary, lighthouse-tracker, logpush, otel-export, synthetic-pager
LOC moved: 9,011
Tests: 252 → 253 (+1 pre-existing smoke preserved)
Build/clippy/tests: GREEN
```
