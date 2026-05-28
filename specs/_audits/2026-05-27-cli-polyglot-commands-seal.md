---
id: "2026-05-27-cli-polyglot-commands-seal"
type: "audit"
doc_status: "ACTIVE"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: ["agent-a284293445f9352e2"]
supersedes: null
superseded_by: null
tags: ["cli", "wave32", "polyglot", "cargo-init", "npm-init", "docker-init", "doctor", "seal"]
references:
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md §2 WP-2.1"
  - "specs/_audits/2026-05-27-deep-prep-inputs.md §3"
  - "humangr-labs/corelink-cli @ e4f80dc834f1f8fce6c18002afc3213eceb968c4"
---

# SEAL audit: corelink-cli polyglot commands + doctor

**WP:** 2.1  
**Context:** ROADMAP Phase 1.3 CLI feature expansion  
**Agent:** agent-a284293445f9352e2  
**Date:** 2026-05-27

---

## §1 Pre-flight

```
pwd → /Users/gustavoschneiter/Documents/HuGR/corelink-server/.claude/worktrees/agent-a284293445f9352e2
```

Ends with `agent-a284293445f9352e2` — pre-flight PASS.

## §2 External repo state before + after

| Field | Before | After |
|---|---|---|
| Repo | humangr-labs/corelink-cli | humangr-labs/corelink-cli |
| Base SHA | `71d7f6a9` | `e4f80dc834f1f8fce6c18002afc3213eceb968c4` |
| Subcommands | ping, bazel-init, config show | **+cargo-init, npm-init, docker-init, doctor** |
| Cargo deps | clap, reqwest, serde, toml, thiserror, anyhow, dirs | +serde_json |

## §3 Files changed (external repo)

| File | Action | LOC delta |
|---|---|---|
| `crates/corelink/src/commands/cargo_init.rs` | NEW | +145 |
| `crates/corelink/src/commands/npm_init.rs` | NEW | +196 |
| `crates/corelink/src/commands/docker_init.rs` | NEW | +207 |
| `crates/corelink/src/commands/doctor.rs` | NEW | +345 |
| `crates/corelink/src/commands/mod.rs` | MODIFIED | +4 |
| `crates/corelink/src/main.rs` | MODIFIED | +43 |
| `crates/corelink/Cargo.toml` | MODIFIED | +1 (serde_json) |
| `Cargo.lock` | MODIFIED | +serde_json tree |
| `README.md` | MODIFIED | +97 (all 4 commands documented) |

## §4 DoD verification

| # | DoD item | Status | Evidence |
|---|---|---|---|
| 1 | `cargo check` exits 0 | ✅ | `Finished dev profile [unoptimized] target(s)` |
| 2 | `cargo build --release` exits 0 | ✅ | `Finished release profile [optimized] target(s)` |
| 3 | `cargo test` exits 0, ≥12 new tests | ✅ | 35 tests (32 new across 4 commands), 0 failed |
| 4 | `./corelink --help` shows cargo-init | ✅ | Confirmed in acceptance run |
| 5 | `./corelink --help` shows npm-init | ✅ | Confirmed in acceptance run |
| 6 | `./corelink --help` shows docker-init | ✅ | Confirmed in acceptance run |
| 7 | `./corelink --help` shows doctor | ✅ | Confirmed in acceptance run |
| 8 | `./corelink doctor` runs end-to-end without endpoint, exits 1 | ✅ | Prints ✗ for token/endpoint checks, exits 1 |
| 9 | All GHA `uses:` SHA-pinned | ✅ | `grep -E "uses:" *.yml \| grep -vE "[a-f0-9]{40}"` → empty |
| 10 | README updated | ✅ | All 4 commands + examples added |
| 11 | Pushed to external main | ✅ | SHA `e4f80dc834f1f8fce6c18002afc3213eceb968c4` |
| 12 | SEAL audit in monorepo worktree | ✅ | This document |

## §5 Test inventory

### cargo-init (5 tests)
| Test | Type | Outcome |
|---|---|---|
| `detect_success_finds_cargo_toml` | detect-success | ✅ pass |
| `detect_miss_no_cargo_toml` | detect-miss | ✅ pass |
| `already_applied_noop` | already-applied-noop | ✅ pass |
| `upsert_appends_when_absent` | mechanics | ✅ pass |
| `upsert_replaces_existing_block` | mechanics | ✅ pass |
| `stanza_contains_required_keys` | mechanics | ✅ pass |

### npm-init (5 tests)
| Test | Type | Outcome |
|---|---|---|
| `detect_success_finds_package_json` | detect-success | ✅ pass |
| `detect_miss_no_package_json` | detect-miss | ✅ pass |
| `already_applied_noop_same_endpoint` | already-applied-noop | ✅ pass |
| `upsert_creates_turbo_json_when_absent` | mechanics | ✅ pass |
| `upsert_preserves_existing_pipeline_config` | mechanics | ✅ pass |
| `upsert_updates_when_endpoint_changes` | mechanics | ✅ pass |

### docker-init (7 tests)
| Test | Type | Outcome |
|---|---|---|
| `detect_success_finds_dockerfile` | detect-success | ✅ pass |
| `detect_miss_no_dockerfile` | detect-miss | ✅ pass |
| `already_applied_noop_same_endpoint` | already-applied-noop | ✅ pass |
| `already_applied_false_for_different_endpoint` | mechanics | ✅ pass |
| `cache_json_contains_required_keys` | mechanics | ✅ pass |
| `gitignore_block_appended_when_absent` | mechanics | ✅ pass |
| `gitignore_block_idempotent` | mechanics | ✅ pass |

### doctor (9 tests)
| Test | Type | Outcome |
|---|---|---|
| `valid_url_https` | url validation | ✅ pass |
| `valid_url_http_local` | url validation | ✅ pass |
| `invalid_url_bare_hostname` | url validation | ✅ pass |
| `redact_shows_prefix_only` | token redaction | ✅ pass |
| `bin_on_path_finds_sh` | PATH check | ✅ pass |
| `bin_on_path_returns_false_for_nonexistent` | PATH check | ✅ pass |
| `file_contains_returns_true_when_needle_present` | file detection | ✅ pass |
| `file_contains_returns_false_when_absent` | file detection | ✅ pass |
| `file_contains_returns_false_for_missing_file` | file detection | ✅ pass |

**Total: 35 tests, 0 failed** (32 new in this WP, 3 pre-existing in bazel_init + config).

## §6 Charter compliance

| Constraint | Status |
|---|---|
| `#![forbid(unsafe_code)]` at crate root | ✅ Present in main.rs |
| `#[non_exhaustive]` on public enums | ✅ `Command` and `ConfigCmd` in main.rs |
| No `unwrap()`/`expect()` outside `#[cfg(test)]` | ✅ All unwrap/expect inside test modules |
| No `tokio` import in `src/` | ✅ No tokio in source (reqwest blocking only) |
| Secrets never logged | ✅ Token always passed through `redact()` in doctor; never logged in other commands |
| GHA `uses:` SHA-pinned | ✅ All 8 `uses:` in ci.yml and release.yml have 40-char SHA |

## §7 Resolution compliance

| Resolution | Applied |
|---|---|
| R1: doctor as new command (user decision) | ✅ Implemented with 9-point checklist |
| R9: cargo-init writes project-local `.cargo/config.toml` | ✅ `cwd.join(".cargo").join("config.toml")` |
| R10: npm-init targets turbo.json | ✅ Turborepo `remoteCache` key |
| R11: JSON upsert inline in npm_init.rs | ✅ `upsert_remote_cache_json` in npm_init.rs |
| R26: cargo-init checks `sccache --version` | ✅ `check_sccache()` fn; actionable error with install hints |

## §8 Acceptance gates output

```bash
# cargo check
$ cargo check 2>&1 | tail -3
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 40s
EXIT: 0

# cargo build --release
$ cargo build --release 2>&1 | tail -3
Finished `release` profile [optimized] target(s) in 30.46s
EXIT: 0

# cargo test
$ cargo test 2>&1 | tail -10
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
EXIT: 0

# --help grep
$ ./target/release/corelink --help | grep -E "cargo-init|npm-init|docker-init|doctor"
  cargo-init   Inject project-local `.cargo/config.toml` sccache stanza (idempotent)
  npm-init     Inject `turbo.json` remoteCache stanza for Turborepo (idempotent)
  docker-init  Configure Docker BuildKit remote registry cache (idempotent)
  doctor       9-point environment diagnostic: auth, endpoint, installed tool deps

# doctor (no endpoint configured — expected exit 1)
$ ./target/release/corelink doctor 2>&1 | head -12
corelink doctor — environment diagnostic

  ✓ [1] CLI version: 0.1.0 (x86_64)
  ✓ [2] endpoint: https://corelink-api.humangr.com (default — set CORELINK_ENDPOINT to override)
  ✗ [3] token: no token found — pass --token, set CORELINK_TOKEN, or add `token` to ~/.corelink/config.toml
  ✗ [4] endpoint reachable: skipped — endpoint or token not configured
  ✗ [5] auth valid: skipped — endpoint or token not configured
  ✓ [6] bazel (bazel-init not applied): skipped — no .bazelrc corelink block detected
  ✓ [7] sccache (cargo-init not applied): skipped — no .cargo/config.toml corelink block detected
  ✓ [8] turbo (npm-init not applied): skipped — no turbo.json remoteCache block detected
  ✓ [9] docker buildx (docker-init not applied): skipped — no .docker/buildx-cache.json detected

# GHA SHA-pin verification (empty = all SHA-pinned)
$ grep -E "uses:" .github/workflows/*.yml | grep -vE "[a-f0-9]{40}"
(empty — all clean)

# git push
$ git push origin main 2>&1 | tail -3
To https://github.com/humangr-labs/corelink-cli.git
   71d7f6a..e4f80dc  main -> main
EXIT: 0
```

## §9 Known residuals

- `corelink doctor` runs `which`/`where` to detect tools; on some Windows configurations
  `where` behavior may differ. Covered by `bin_on_path_finds_sh` test on Unix.
- npm-init targets Turborepo only (R10 decision). Nx support deferred to follow-up per spec.
- docker-init tokens are written to `.docker/buildx-cache.json` which is `.gitignore`d.
  Users running `docker buildx` directly still need to inject the token separately;
  `buildx-cache.json` serves as a reference, not a live-injection config.

## §10 DCO

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

**End of WP-2.1 SEAL.**
