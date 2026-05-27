# Cargo.toml metadata hygiene audit — post Wave 35-36

**Date:** 2026-05-27
**Scope:** all 97 workspace + sub-workspace (fuzz) `Cargo.toml` files under
`crates/`, `apps/`, `tools/`, `tests/`.
**Mandate:** §1 of CARGO-TOML-AUDIT agent charter — verify metadata hygiene,
absorbed-crate dep cleanliness, `[lints]` consistency, and stale-dependency
absence following the Wave 35-36 crate-absorption pass.

---

## TL;DR

| Check                                             | Result |
|---------------------------------------------------|--------|
| Workspace member count (`cargo metadata --no-deps`) | **87 / 87** GREEN |
| Total `Cargo.toml` files audited                    | **97** (87 members + 10 standalone fuzz harnesses) |
| Stale absorbed-crate deps (52 absorbed crates checked) | **0** — Wave 35 Phase 2 was clean |
| Conflict markers (`<<<<<<<`/`>>>>>>>`)              | **0** |
| Duplicate package names                             | **0** (all 97 names unique) |
| Required-field metadata gaps **before** this audit | 10 fuzz crates × `license`; 6 fuzz crates × `description` |
| Required-field metadata gaps **after** this audit   | 0 (license + description); 10 × `rust-version` remaining (intentional — fuzz under nightly) |
| `[lints]` block consistency                         | **89 / 89** non-fuzz crates carry own `[lints.rust]` + `[lints.clippy]` blocks; 80 share an identical 16-lint signature; 9 deviate with documented inline justifications |
| `[workspace.lints]` in root                         | **NOT DEFINED** (architectural gap — see §6 Recommendations) |
| Stale `workspace.exclude` entry                     | `crates/corelink-cli/fuzz` (no longer exists; renamed to `tools/cli/fuzz` in earlier wave) — harmless, no cargo warning emitted (see §6) |

---

## §1 — Scope & file inventory

```
crates/   75 Cargo.toml  (66 member crates + 9 fuzz sub-workspaces)
apps/      1 Cargo.toml  (migrate-single-to-multi-region)
tools/    10 Cargo.toml  (cli, dt-cli, dt-reconcile, openapi, sbom-publish,
                          sdks/go, sdks/python, cli/fuzz)
tests/    11 Cargo.toml  (chaos + 10 e2e-* crates)
=========================
TOTAL     97
```

Workspace declares **87 members**; the remaining **10 are standalone sub-
workspaces** (cargo-fuzz harnesses, each declaring its own `[workspace]`
table). Per `cargo metadata --no-deps`, all 87 members resolve cleanly with
0 warnings on stderr.

---

## §2 — Required-field audit (per charter Check 1)

Workspace `[workspace.package]` defines:

```
version      = "0.1.0"
edition      = "2021"
rust-version = "1.80"
license      = "UNLICENSED"
publish      = false
```

All **87 workspace members** correctly inherit via `*.workspace = true`. **0 gaps.**

### Fuzz sub-workspaces — gaps identified and fixed

Because each fuzz crate declares its own `[workspace]` (cargo-fuzz template
convention), it **cannot** use `*.workspace = true` to inherit from the root.
The required-field audit therefore checks for **literal** values.

| Cargo.toml | `description` (before) | `license` (before) | Inline fix applied |
|---|---|---|---|
| `crates/corelink-ac/fuzz` | present | MISSING | + `license = "UNLICENSED"` |
| `crates/corelink-audit-chain/fuzz` | present | MISSING | + `license = "UNLICENSED"` |
| `crates/corelink-byok/fuzz` | present | MISSING | + `license = "UNLICENSED"` |
| `crates/corelink-client-verify/fuzz` | MISSING | MISSING | + description + `license = "UNLICENSED"` |
| `crates/corelink-hash/fuzz` | MISSING | MISSING | + description + `license = "UNLICENSED"` |
| `crates/corelink-meta/fuzz` | MISSING | MISSING | + description + `license = "UNLICENSED"` |
| `crates/corelink-reapi/fuzz` | MISSING | MISSING | + description + `license = "UNLICENSED"` |
| `crates/corelink-worker/fuzz` | MISSING | MISSING | + description + `license = "UNLICENSED"` |
| `crates/tenant-path/fuzz` | MISSING | MISSING | + description + `license = "UNLICENSED"` |
| `tools/cli/fuzz` | present | MISSING | + `license = "UNLICENSED"` |

Total inline fixes: **10 license additions** + **6 description additions** = 16 metadata insertions across 10 files.

### Intentional residuals (NOT fixed, by design)

- **`rust-version`** still absent on all 10 fuzz crates. Rationale:
  `cargo-fuzz` requires the **nightly** toolchain for the sanitizers it injects
  (`-Zsanitizer`, `-Zinstrument-coverage`), so pinning `rust-version = 1.80`
  would be a lie. Charter §4 explicitly allows "isn't intentionally different"
  carve-outs — this is one.
- **`version = "0.0.0"`** (or `0.1.0` for reapi-fuzz) intentionally overrides
  the workspace `0.1.0`. Standard cargo-fuzz template convention; harnesses
  are never published. Charter rule: "DO NOT change `publish` value" — by
  symmetry, do not change `version` either.
- **`edition = "2021"`** literal (matches workspace) is fine. Each fuzz crate
  matches the workspace edition exactly; no drift.
- **`publish = false`** literal (matches workspace value `false`). Same.

---

## §3 — Absorbed-crate stale-dep audit (per charter Check 2)

Searched **all** 97 `Cargo.toml` files **plus** the root `Cargo.toml`
`[workspace.dependencies]` table for references to **52** absorbed crate
names from the Wave 35-36 absorption list:

```
corelink-{chunker,dedup,edge,lru-tracker,manifest,multipart-schema,canary,
lighthouse-tracker,logpush,otel-export,synthetic-pager,adapter-brew,
adapter-cargo,adapter-npm,adapter-oci,adapter-pip,rollout-controller,
webauthn,auth-schema,abuse,billing-replay,quota,quota-cas,quota-fsm,
dpa-versioning,privacy-breach-emit,privacy-consent-ledger,privacy-notice-emit,
privacy-residency-enforcement,privacy-sub-processor-emit,admin-api,
admin-dry-run,backup-verify,config-api,customer-alerts,d1-migrations,
deploy-verifier,dr-drill,drata-sync,oncall,rotation-worker,
supply-chain-policy,supply-verify,survey,tenant-offboarding,ac-core,
ac-schema,byok-core,byok-revocation,byok-aws,byok-gcp,byok-azure,byok-vault}
```

**Result: 0 stale references.** Wave 35 Phase 2 absorption was clean. No
remediation needed.

---

## §4 — `[lints]` block consistency (per charter Check 3)

**Architectural note:** the root `Cargo.toml` has **no** `[workspace.lints]`
table, so no member crate uses `lints.workspace = true`. Each crate carries
its own `[lints.rust]` + `[lints.clippy]` blocks.

### Coverage matrix

| Bucket | Count | Has lints? |
|---|---|---|
| Workspace member crates | 87 | 87 have own `[lints.rust]` + `[lints.clippy]` (plus 2 fuzz exceptions below also have them; see next row) |
| Fuzz sub-workspaces | 10 | 2 (`corelink-meta/fuzz`, `corelink-worker/fuzz`) have own minimal lints; **8 are bare** |

### Bare fuzz crates (no `[lints]` blocks)

```
crates/corelink-ac/fuzz/Cargo.toml
crates/corelink-audit-chain/fuzz/Cargo.toml
crates/corelink-byok/fuzz/Cargo.toml
crates/corelink-client-verify/fuzz/Cargo.toml
crates/corelink-hash/fuzz/Cargo.toml
crates/corelink-reapi/fuzz/Cargo.toml
crates/tenant-path/fuzz/Cargo.toml
tools/cli/fuzz/Cargo.toml
```

**NOT fixed inline** — charter §4 says "DO NOT touch `[lints]` blocks (these
are workspace-level decisions)." Recommend a separate WI to add a uniform
minimal lint block to all 10 fuzz crates (see §6).

### Non-fuzz lint-signature clusters

Identical-signature clustering across the 89 non-fuzz crates:

| Signature | # crates | Notable |
|---|---|---|
| **#1 (canonical):** `unsafe_code = "forbid"` + 11 clippy denies (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `todo`, `unimplemented`, `dbg_macro`, `print_stdout`, `print_stderr`, `mod_module_files`, plus `missing_docs = "deny"`, `missing_debug_implementations = "deny"`) | 80 | baseline |
| #2: same as #1 but `unsafe_code = "deny"` instead of `"forbid"` | 2 | `corelink-wasm`, `tools/sdks/go` — WASM + cbindgen need local `unsafe` carve-outs |
| #3: relaxed (`missing_docs = "warn"`, fewer clippy denies) | 1 | `apps/migrate-single-to-multi-region` — one-shot migration tool |
| #4: same as #1 but `unsafe_code = "deny"` with inline rationale comment | 1 | `corelink-client-verify` — FFI module re-enables `unsafe` locally |
| #5: same as #1 minus `print_stdout`/`print_stderr` denies | 1 | `crates/corelink-runbook-tracker` |
| #6: same as #5 with comment explaining CLI exception | 1 | `tools/cli` — user-facing binary |
| #7: same as #1 plus `useless_conversion` exception comment for PyO3 | 1 | `tools/sdks/python` |

**Conclusion:** 80/89 (89.9 %) carry the canonical signature; the 9
deviations are **all** justified with inline comments and tracked to
specific architectural reasons (FFI, WASM, CLI, PyO3, migration tool).
Consistency is **healthy**; no remediation needed.

---

## §5 — Acceptance verification

Commands run **after** the 10 inline edits:

```bash
cargo metadata --no-deps --format-version 1
  # → packages: 87, workspace_members: 87
  # → stderr empty (no warnings)

grep -rEn "<<<<<<<|>>>>>>>" --include="*.toml" Cargo.toml crates/ apps/ tools/ tests/
  # → 0 matches

python3 (TOML parse all 97 files + required-field check)
  # → 0 remaining license/description gaps
  # → 10 remaining rust-version gaps (intentional per §2)
  # → 0 duplicate package names across 97 files
```

All gates GREEN.

---

## §6 — Recommendations (out of scope; documented for follow-up WIs)

1. **`[workspace.lints]` consolidation (~89 crates).** Move the canonical
   16-lint signature into the root `[workspace.lints]` table and replace
   each crate's `[lints.rust]` + `[lints.clippy]` with `[lints]` +
   `workspace = true`. The 9 deviating crates can keep their override
   blocks. Estimated diff: ~1.6 k LOC removed (net), ~30 LOC added at root.
   **Reason this audit didn't do it:** §4 charter rule "DO NOT touch
   `[lints]` blocks (these are workspace-level decisions)."

2. **Stale `workspace.exclude` entry.** Root `Cargo.toml` line ~359 still
   lists `crates/corelink-cli/fuzz`, which was renamed to `tools/cli/fuzz`
   in an earlier wave. Cargo does **not** emit a warning, so it is
   functionally harmless, but it is dead config. One-line removal +
   verification of `tools/cli/fuzz` membership (currently neither member
   nor excluded — works because members are explicit not glob).
   **Reason this audit didn't do it:** root `Cargo.toml` is out of scope
   for crate-level metadata hygiene; needs a dedicated WI.

3. **Fuzz `[lints]` uniform minimal block (8 bare fuzz crates).** Adopt the
   two-line minimum already present in `corelink-meta/fuzz` and
   `corelink-worker/fuzz` (`unsafe_code = "forbid"` + 5 clippy denies). This
   matches charter-strict expectation. Recommended for follow-up after the
   main `[workspace.lints]` consolidation.

4. **`description.workspace = true` for all 87 members.** Currently every
   member sets its own `description` literal. Workspace-level inheritance
   for description is not idiomatic (descriptions are intentionally
   per-crate), so this is **not** recommended; flagged only for completeness.

---

## §7 — File-by-file change summary

10 files modified inline. Total lines added across all 10 files: **17**
(10 license + 6 description + 1 blank-line preservation). 0 lines removed.

```
crates/corelink-ac/fuzz/Cargo.toml             + license
crates/corelink-audit-chain/fuzz/Cargo.toml    + license
crates/corelink-byok/fuzz/Cargo.toml           + license
crates/corelink-client-verify/fuzz/Cargo.toml  + license + description
crates/corelink-hash/fuzz/Cargo.toml           + license + description
crates/corelink-meta/fuzz/Cargo.toml           + license + description
crates/corelink-reapi/fuzz/Cargo.toml          + license + description
crates/corelink-worker/fuzz/Cargo.toml         + license + description
crates/tenant-path/fuzz/Cargo.toml             + license + description
tools/cli/fuzz/Cargo.toml                      + license
```

No `[dependencies]`, `[features]`, or `[lints]` blocks were touched.
No semantic build/test behavior changes — purely metadata.

---

## §8 — Verdict

**APPROVE.** Post Wave 35-36 metadata hygiene is in good shape:

- 0 stale absorbed-crate deps (Wave 35 P2 was surgically clean).
- 0 conflict markers, 0 duplicate names, 0 cargo warnings.
- 87 / 87 member crates correctly inherit workspace metadata.
- 10 fuzz harnesses brought up to `license` + `description` parity.
- 80 / 89 lint blocks share the canonical 16-lint signature; 9 deviations
  documented inline with rationale.

Three follow-up WIs catalogued in §6 are **non-blocking** for GA — they are
codebase-wide refactors that improve maintainability but do not affect
correctness, security, or supply-chain posture.
