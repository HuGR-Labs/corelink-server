# SEAL Audit — corelink-cli Bootstrap (ROADMAP Phase 1.1)

**Date:** 2026-05-27
**Agent task ID:** agent-a816e36f94b7dd648
**Repo:** https://github.com/HumanGuardrail/corelink-cli
**HEAD SHA:** `71d7f6a930a58092fa7deddee04ad5229cd88445`

---

## Result

**MVP bootstrapped and pushed to `HumanGuardrail/corelink-cli` main.**

---

## Repo State

`already-existed-and-pushed-to-main`

The repo existed with a prior commit (`61e71d1`) that had skeleton code
but several spec violations. This run fixed all violations and pushed
a single new commit.

---

## Files Created / Modified

### In `HumanGuardrail/corelink-cli` (cloned to `/tmp/corelink-cli-bootstrap-agent-a816e36f94b7dd648/`)

| File | Action | Notes |
|---|---|---|
| `Cargo.toml` | Modified | MSRV `1.91` → `1.85`; license `MIT` → `Apache-2.0` |
| `LICENSE` | Replaced | MIT body replaced with full Apache-2.0 text |
| `README.md` | Modified | License section updated; ping description updated with POST + override info |
| `crates/corelink/Cargo.toml` | Modified | Added `"env"` feature to clap |
| `crates/corelink/src/main.rs` | Modified | `Ping` subcommand now carries `PingArgs` struct |
| `crates/corelink/src/commands/ping.rs` | Replaced | GET → POST; `PingArgs` struct with `--endpoint`/`--token` + `CORELINK_ENDPOINT`/`CORELINK_TOKEN` env support; prints status + latency |
| `crates/corelink/src/config.rs` | Modified | Added `load_with_overrides(endpoint, token)` for env/CLI override chain |
| `.github/workflows/ci.yml` | Replaced | All `uses:` SHA-pinned (40-char SHAs) |
| `.github/workflows/release.yml` | Replaced | All `uses:` SHA-pinned; 5-target matrix aligned with `install.ts` asset naming |

### In monorepo worktree (this file):

| File | Action |
|---|---|
| `specs/_audits/2026-05-27-corelink-cli-bootstrap-seal.md` | Created |

---

## Definition of Done Checklist

| # | Criterion | Status |
|---|---|---|
| 1 | `cargo check` exits 0 | PASS — verified locally |
| 2 | `cargo build --release` succeeds | PASS — verified locally |
| 3 | `./target/release/corelink ping --help` prints reasonable help | PASS — shows endpoint/token flags |
| 4 | All `uses:` SHA-pinned (40-char SHA) | PASS — grep check empty |
| 5 | README has Install + Usage + License sections | PASS |
| 6 | Repo pushed to `HumanGuardrail/corelink-cli` main | PASS — `71d7f6a` |
| 7 | This SEAL audit doc | PASS |
| 8 | Worktree commit pushed | PENDING (after this doc commit) |

---

## SHA-Pin Reference Table

| Action | Pinned SHA | Tag |
|---|---|---|
| `actions/checkout` | `34e114876b0b11c390a56381ad16ebd13914f8d5` | v4 |
| `dtolnay/rust-toolchain` | `29eef336d9b2848a0b548edc03f92a220660cdb8` | stable branch |
| `Swatinem/rust-cache` | `e18b497796c12c097a38f9edb9d0641fb99eee32` | v2 |
| `actions/upload-artifact` | `ea165f8d65b6e75b540449e92b4886f43607fa02` | v4 |
| `actions/download-artifact` | `d3f86a106a0bac45b974a628896c90dbdf5c8093` | v4 |
| `softprops/action-gh-release` | `3bb12739c298aeb8a4eeaf626c5b8d85266b0e65` | v2 |

---

## Release Workflow — 5 Platform Matrix

| Target | OS Runner | Asset Name | Notes |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | ubuntu-latest | `corelink-linux-x86_64` | glibc, cargo native |
| `x86_64-unknown-linux-musl` | ubuntu-latest | `corelink-linux-x86_64-musl` | static/Alpine, cross |
| `x86_64-apple-darwin` | macos-latest | `corelink-darwin-x86_64` | Intel Mac |
| `aarch64-apple-darwin` | macos-latest | `corelink-darwin-aarch64` | Apple Silicon |
| `x86_64-pc-windows-msvc` | windows-latest | `corelink-windows-x86_64.exe` | Windows |

Asset naming follows `install.ts` contract: `corelink-${uname -s | lowercase}-${uname -m}`
(raw binaries, no `.tar.gz` wrapper — the installer `chmod +x`s directly).

**Note:** `install.ts` is a POSIX sh script and only handles Unix targets.
The Windows `.exe` is available via direct download from the releases page.

---

## Deviations from Agent Contract

| Item | Contract | Actual | Reason |
|---|---|---|---|
| Asset format | "tar.gz the bin" | Raw binary | `install.ts` downloads `corelink-${OS}-${ARCH}` with `chmod +x` — no extraction step. Contract says "read install.ts to confirm exact naming"; install.ts is ground truth. |
| `src/commands/` layout | Flat `src/commands/ping.rs` etc. | `crates/corelink/src/commands/ping.rs` | Repo pre-existed with workspace layout; no functional difference. |
| MSRV in contract | 1.85 | Fixed from 1.91 → 1.85 | Pre-existing code used 1.91; corrected. |

---

## Test Results (local)

```
test commands::bazel_init::tests::upsert_appends_when_absent ... ok
test commands::bazel_init::tests::upsert_replaces_existing_block ... ok
test commands::bazel_init::tests::upsert_is_idempotent ... ok
test config::tests::empty_token_rejected ... ok
test config::tests::parse_full ... ok
test config::tests::roundtrip ... ok
test config::tests::parse_minimal ... ok
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured
```

---

## Follow-Ups

1. **musl target on CI**: `x86_64-unknown-linux-musl` uses `cross` in the release
   workflow; the CI workflow only runs `cargo test` on native `ubuntu-latest` (glibc).
   Add a musl CI job if needed.
2. **`corelink login` command**: `config.rs` references `corelink login` in error
   messages but the subcommand is not yet implemented. Add in a follow-up sprint.
3. **Wave 32 Phase B unblock**: `corelink ping` currently fails with a 404 on
   `https://corelink-api.humangr.com/v1/ping` (control-plane not yet live).
   Unblocked by Wave 32 production deploy.
4. **Install script naming for musl**: `install.ts` does not currently distinguish
   glibc vs musl. A future `--musl` flag or auto-detection could select the right asset.
5. **Workflow dry-run**: First real release can be triggered by pushing tag `v0.1.0`
   to `HumanGuardrail/corelink-cli`. No tag has been pushed yet (MVP is source-only).

---

## Blockers

NONE
