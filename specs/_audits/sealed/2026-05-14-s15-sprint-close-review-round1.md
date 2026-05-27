---
id: "AUDIT-S15-SPRINT-CLOSE-R1"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Review (Sonnet)"
tags: ["audit", "sprint-close", "s15", "adversarial"]
---

# S-15 Sprint-Close Adversarial Review — Round 1

Sprint: S-15 (CLI + SDK + Bazel/Buck2 + FFI + CI templates). WIs reviewed: 001–005. WI-S15-006 explicitly excluded per orchestrator instruction.

## 1. Score

**4.5 / 10 — FAIL (do NOT SEAL until P0s resolved).**

The sprint shipped substantial scope (7-subcommand CLI, FFI for 3 languages, CI templates for 3 providers, Bazel/Buck2 starters, telemetry opt-in with LINDDUN review, well-structured ADR-0016 at canonical path), but the merged main branch is currently **non-building under sprint-defined lint gates** and contains **unresolved Git merge-conflict markers in 4 starter-project files** that a Bazel runner would reject on first parse. This is the canonical "merge claimed SEAL but conflict resolution was never finished" failure pattern. Additionally, spec frontmatter is internally inconsistent (WI-S15-002 still `DRAFT`/`READY`; WI-S15-004 uses an invalid `work_status: SEALED`; ADR-0016 has a duplicate at a non-canonical path with `doc_status: ACCEPTED` which is not in the enum). The CLI/FFI implementations themselves are mostly clean; failure modes are concentrated at the integration seams (Git merges, frontmatter consistency, CI hygiene).

## 2. P0 (must-fix before SEAL)

1. **`cargo clippy --workspace --tests -- -D warnings` FAILS** — sprint quality gate `Quality Standards 14.s15.x` not met.
   - `crates/corelink-cli/src/commands/mod.rs:1` — `mod_module_files` violation: file disallowed by `[lints.clippy] mod_module_files = "deny"` in the crate's own Cargo.toml. Move to `crates/corelink-cli/src/commands.rs` + flatten.
   - `crates/corelink-cli/src/commands/bench.rs:204` — `let mut data = vec![...]` triggers `unused_mut` under `-D warnings`. Remove `mut`.
   - Even `cargo clippy --workspace` (without `--tests`) fails on the same `mod.rs` error.

2. **Unresolved Git merge-conflict markers in `examples/bazel-starter/`** (4 files) — Bazel will refuse to parse these; the bazel-starter-ci.yml workflow cannot pass; "5-min cache hit" DoD impossible. These markers landed in commit `7f78740` ("resolve corelink-cli + S-12 spec-contract merge conflicts") which apparently did NOT resolve them all.
   - `examples/bazel-starter/WORKSPACE:2` / `:21` / `:24`
   - `examples/bazel-starter/.bazelrc:1` / `:38` / `:58`
   - `examples/bazel-starter/BUILD.bazel:1` / `:18` / `:26`
   - `examples/bazel-starter/.bazel/corelink-credential-helper.sh:2` / `:29` / `:47`

3. **`python3 scripts/validate_specs.py` FAILS for S-15 docs** (DoD spec validation gate):
   - `specs/04_sprints/S15/work_items/WI-S15-004-ffi-wrappers-python-go-js-adr-0016.md` — `work_status: "SEALED"` is not in the allowed enum (`PROPOSED / READY / DOING / REVIEWING / BLOCKED / DONE / CANCELED / ROLLED_BACK`). Must be `DONE` (mirror WI-001/003/005 which correctly use `work_status: DONE` + `doc_status: SEALED`).
   - `specs/_decisions/ADR-0016-ffi-vs-native-http.md` — `doc_status: "ACCEPTED"` not in allowed enum; `reviewers` entries are strings, schema expects objects.

4. **Duplicate ADR-0016 at two paths** — schema-violating, source-of-truth ambiguity.
   - Canonical (per sprint contract): `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md` — `doc_status: DRAFT`, version `0.1.0`, "stub".
   - Non-canonical (and schema-invalid): `specs/_decisions/ADR-0016-ffi-vs-native-http.md` — `doc_status: ACCEPTED`, version `1.0.0`, full content.
   - The "real" ADR content lives at the wrong path while the canonical path holds a stub. Either promote the canonical one (move content, fix frontmatter to allowed `SEALED` or `FROZEN`) and delete the duplicate, or `supersedes`/`superseded_by` one from the other.

5. **WI-S15-002 frontmatter still `DRAFT` / `READY`** — sprint contract says it was merged + SEALED; the work item itself disagrees.
   - `specs/04_sprints/S15/work_items/WI-S15-002-bazel-integration-starter-ci-test.md:4-5` — `doc_status: "DRAFT"`, `work_status: "READY"`. Must update to `SEALED` / `DONE` like its peers.

6. **GitHub Action workflows for S-15 not SHA-pinned (CTRL-SUPPLY-001 violation).** Bazel/Buck2 starter CI workflows are properly pinned, but two S-15 workflows are not:
   - `.github/workflows/release-cli.yml:64` `actions/checkout@v4` (floating tag)
   - `.github/workflows/release-cli.yml:80` `dtolnay/rust-toolchain@master` (mutable branch — worse than a floating tag)
   - `.github/workflows/release-cli.yml:164` `softprops/action-gh-release@v2`
   - `.github/workflows/ffi-matrix-ci.yml:40,50,86,89,94,99,164,167,172,177,214,217,222,228,281,284,290,295,300,305` — all `@v4`/`@v5`/`@stable` floating, in a workflow whose entire purpose is FFI security boundary verification.

## 3. P1 (should-fix before SEAL)

1. **WASM bundle-size profile not set** — `crates/corelink-wasm/Cargo.toml` has no `[profile.release]` override. Workspace default is `opt-level = 3`; WASM needs `opt-level = "z"` (size). The 1 MiB CI gate documented at `crates/corelink-wasm/src/lib.rs:22` may still pass for an empty stub, but ships with the wrong profile by design. Add a crate-local `[profile.release]` with `opt-level = "z"` (and confirm `lto = "fat"`, `strip = true` inherit, `panic = "abort"` is set if/when the workspace allows per-crate panic strategy).

2. **Spec contract `S15/_spec_contract.md` is missing a changelog section** — review checklist item 18 requires "rows for 001..005"; no `## Changelog` exists. The contract's `doc_status` is also still `DRAFT` (`specs/04_sprints/S15/_spec_contract.md:4`) despite 5/6 WIs claiming SEAL.

3. **Heavy use of blanket `#[allow(clippy::uninlined_format_args, clippy::format_in_format_args, ...)]`** across `corelink-cli` test modules (12 occurrences) — silences rust 1.91 strict lints rather than fixing the test code (review checklist 21). Examples:
   - `crates/corelink-cli/src/auth.rs:132`
   - `crates/corelink-cli/src/client.rs:183`
   - `crates/corelink-cli/src/doctor.rs:385`
   - `crates/corelink-cli/src/output.rs:88`
   - 8× under `crates/corelink-cli/src/commands/*.rs`

4. **Telemetry "non-blocking" guarantee is fragile** — `crates/corelink-cli/src/telemetry.rs:104` uses `tokio::spawn` for fire-and-forget, but `main.rs:171` awaits `run()` and then immediately falls through to `std::process::exit(1)` on error (`main.rs:189`). A detached `tokio::spawn` whose future has not been polled before `exit()` is dropped silently — meaning telemetry is in practice "emit only when the CLI happens to live long enough for the runtime to schedule it once". That is acceptable for an opt-in, best-effort path, but the docs at `telemetry.rs:88-91` claim "always completes regardless of telemetry endpoint health" — which is true for blocking, but the wording oversells. Either await with a hard `tokio::time::timeout(1s, ...)` before exit, or rewrite the comment to clarify "may be skipped on early process exit; this is intentional".

5. **`AuthConfig::redacted_pat` panics-safe but slicing logic is brittle** — `crates/corelink-cli/src/config.rs:58-67`: `&p[..idx.min(20)]` slices by byte index into a `String` that may contain multi-byte UTF-8 (PATs are usually ASCII, but the type is `String` without an ASCII invariant). If a non-ASCII PAT ever lands, this panics. Use `char_indices` or guard with `is_char_boundary`. P1 not P0 because PATs are ASCII-by-convention.

6. **Telemetry endpoint hardcoded with no override** — `crates/corelink-cli/src/telemetry.rs:24`: `const TELEMETRY_ENDPOINT: &str = "https://telemetry.corelink.humangr.com/v1/events";` Privacy review (LINDDUN at `specs/_audits/2026-05-14-linddun-cli-telemetry.md`) implies customers might want to direct telemetry to their own collector. No env var override (`CORELINK_TELEMETRY_ENDPOINT`) and no config key. Pre-GA constraint, but worth noting.

7. **CTRL-CRED-001 rejection happens before clap parse but does not cover `--pat=<value>` in unicode-fold attacks or `-p` short flag.** `crates/corelink-cli/src/main.rs:161-168` checks `a == "--pat" || a.starts_with("--pat=")`. If anyone ever adds `-p` as a short alias for any future flag, the security rejection is bypassed. Defensive: also reject `-p` and any arg that contains the bytes `pat=`. P1, not P0, because no such flag exists today.

## 4. P2 (post-SEAL nice-to-have)

- `corelink-py` and `corelink-wasm` `get()` are stubs returning the empty-blob digest (`crates/corelink-py/src/lib.rs:113` "stub: production impl calls server"). Spec contract §5.3 R-S15-9 said "FFI wrappers with client verify default-on" — stubs satisfy the verify path but not the cache path. Acceptable for S-15 if WI-S15-006 wires the HTTP backend.
- `corelink-cli/Cargo.toml`'s `[lints.clippy]` does not include `print_stdout`/`print_stderr` deny at crate level (it's allow'd in `main.rs`). Fine for a binary, but the inline comment in Cargo.toml is inconsistent with the actual config.
- `.github/workflows/ffi-matrix-ci.yml:18` references `corelink-go/**` (a non-existent root path) in `paths:` — silent dead path filter.
- No SHA-pin on `bazelisk` binary itself in `examples/bazel-starter/scripts/` (assuming it downloads transitively).
- Telemetry payload's `subcommand` field type is `String` but always populated from `&'static str` literals — could be `&'static str` to save an alloc per emit. Microscopic.

## 5. Positives

- **FFI ownership model is clean.** `crates/corelink-go/src/go_bridge.rs` documents Safety invariants per `unsafe` block, opaque-handle pattern is consistent with `corelink-client-verify/ffi.rs`, free-null is a documented no-op (`go_bridge.rs:288`), and tests exercise the null-handle path (`go_bridge.rs:365-378`). No UAF / double-free risk detected.
- **Telemetry privacy contract is enforced at the type level.** `TelemetryEvent` (`telemetry.rs:33-46`) makes it structurally impossible to emit `tenant_id`/`pat`/`digest` because those fields don't exist on the struct, and the test at `telemetry.rs:144-162` belt-and-suspenders this by string-matching the serialized JSON.
- **CTRL-CRED-001 rejection is pre-clap.** Inspecting `std::env::args()` before `Cli::try_parse()` (`main.rs:160-168`) means even malformed inputs cannot bypass the PAT check via parser quirks.
- **Atomic write + chmod 600 + permission check on load.** `config.rs:285-308` does temp-file-rename atomically and warns on load if perms drift (`InsecurePermissions` error variant on Unix).
- **`#[non_exhaustive]` consistently applied** to all public CLI enums/structs (`Commands`, `ConfigAction`, `CorelinkConfig`, `AuthConfig`, `DefaultsConfig`, `TelemetryConfig`).
- **CI template `corelink-cache.yml` (GitHub Actions) uses a credential helper script that reads `CORELINK_PAT` from env and emits JSON on stdout** — correctly avoiding the `ps aux` argv leak (`templates/ci/github-actions/corelink-cache.yml:55-66`). The Bazel CTRL-CRED-001 model is honored.
- **Bazel/Buck2 starter CI workflows are SHA-pinned** with version comments. This pattern should propagate to `release-cli.yml` and `ffi-matrix-ci.yml`.

## 6. Per-WI sub-scores

| WI | Score | Notes |
|---|---|---|
| **WI-S15-001 (CLI)** | **5.5/10** | Clippy `mod_module_files` + `unused_mut` block `-D warnings`. CTRL-CRED-001 pre-parse rejection good; config atomic-write good; non-exhaustive consistent; tokio runtime fine for binary. Test code uses blanket `#[allow]` rather than fixing format args (12 sites). |
| **WI-S15-002 (Bazel)** | **2/10** | Unresolved merge-conflict markers in WORKSPACE, .bazelrc, BUILD.bazel, credential-helper.sh — CI definitionally cannot pass. Spec frontmatter still `DRAFT`/`READY` despite "SEALED" claim. |
| **WI-S15-003 (Buck2)** | **7.5/10** | Clean BUCK file, parity with bazel-starter, CI workflow SHA-pinned. No merge markers. Spec frontmatter correctly SEALED/DONE. |
| **WI-S15-004 (FFI)** | **6/10** | FFI ownership clean (no UAF/double-free), client-verify default-on enforced, ADR-0016 well-written but at wrong path with invalid frontmatter (duplicate). WASM crate missing size profile. Schema validator rejects `work_status: SEALED`. |
| **WI-S15-005 (CI templates + telemetry)** | **7/10** | Telemetry type-level privacy invariant strong, LINDDUN review attached, atomic config write, opt-in default-off correct. CI templates honour credential-helper pattern. Telemetry "non-blocking" doc claim slightly oversold (detached spawn vs process exit race). |

## 7. Verdict

**DO NOT SEAL.** Three independent hard gates fail right now: clippy, spec validator, Bazel-starter parseability. None of these are debatable — they are mechanical. After P0-1..P0-5 fix, re-run `cargo clippy --workspace --tests -- -D warnings`, `python3 scripts/validate_specs.py`, and `bazel build //...` from `examples/bazel-starter/` (locally or in CI). Then a round-2 review can rescore. Estimated remediation effort: 3–5 hours, mostly mechanical (resolve 4 conflict files, rename one `mod.rs`, fix one `mut`, update 2 frontmatters, add changelog rows).
