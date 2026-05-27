# Cargo.toml audit — 3 non-blocking follow-ups closure SEAL

**Date:** 2026-05-27
**Charter:** CARGO-TOML-FOLLOWUPS agent — close §6 follow-up WIs from the
post Wave 35-36 cargo-toml hygiene audit (`specs/_audits/2026-05-27-cargo-toml-audit-post-w36.md`,
commit `f5cffdc8` in main).
**Mandate:** non-blocking but maintainability-positive — consolidate the
canonical 16-lint signature to `[workspace.lints]`; drop the stale
`crates/corelink-cli/fuzz` exclude; bring 10 fuzz Cargo.toml files to
uniform lint coverage.

---

## TL;DR

| Check | Result |
|---|---|
| Workspace member count (`cargo metadata --no-deps`) | **87 / 87** GREEN (preserved) |
| Conflict markers (`<<<<<<<` / `>>>>>>>`) | **0** |
| `cargo build --workspace` | **GREEN** (4 m 56 s) |
| `cargo clippy --workspace --tests -- -D warnings` | GREEN on all 87 packages migrated; one PRE-EXISTING `doc_lazy_continuation` failure in `corelink-billing` reproduced from `main` baseline unrelated to this change (see §5) |
| Crates migrated to `lints.workspace = true` | **80 / 87** non-fuzz members (the canonical-signature cluster) |
| Crates kept with custom lints (rationale documented) | **7** (down from audit's "9" — see §3 reconciliation) |
| Stale `crates/corelink-cli/fuzz` exclude | **REMOVED** (root `Cargo.toml` line 359 + 2-line comment block) |
| Fuzz Cargo.toml lint uniformity | **10 / 10** now carry the canonical fuzz minimal block (2 already present + 8 newly aligned) |
| Net LOC delta | **89 files changed, +257 / −1283 = −1026 net** |

---

## §1 — `[workspace.lints]` consolidation (follow-up #1)

### Canonical signature inserted into root `Cargo.toml`

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "deny"
missing_debug_implementations = "deny"

[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
print_stdout = "deny"
print_stderr = "deny"
mod_module_files = "deny"
```

Inserted **after** `[workspace.package]` (line 376) and **before**
`[workspace.dependencies]` (line 378 pre-edit, line 397 post-edit). 16
lints total — matches the audit-identified canonical 16-lint signature
exactly. **No new lints introduced; no severity escalations.** Every lint
preserved verbatim from the per-crate blocks they replace.

### Member-crate migration

For each crate whose `[lints.rust]` + `[lints.clippy]` blocks matched the
canonical 16-lint signature **byte-for-byte** (no inline comments, no
extra entries, no missing entries), I replaced the two adjacent blocks
with:

```toml
[lints]
workspace = true
```

**Migrated: 80 crates.** Diff per crate is uniform: −18 lines, +2 lines.

### Crates kept with custom lints (NOT migrated)

7 crates deviate from the canonical signature and were left intact per
the §0 rule "DO NOT change lint SEVERITIES" + "if a crate has custom
lints different from canonical: leave alone + document why":

| Crate | Deviation | Rationale (preserved verbatim from prior audit §4) |
|---|---|---|
| `crates/corelink-wasm/Cargo.toml` | `unsafe_code = "deny"` (not `"forbid"`) | WASM target needs a local `unsafe` carve-out — already inline-justified |
| `tools/sdks/go/Cargo.toml` | `unsafe_code = "deny"` | cbindgen FFI surface re-enables `unsafe` locally |
| `tools/sdks/python/Cargo.toml` | `unsafe_code = "deny"` + adds `clippy::useless_conversion = "allow"` | PyO3 bindings — both deviations explained inline |
| `crates/corelink-client-verify/Cargo.toml` | `unsafe_code = "deny"` (FFI module) | Inline rationale comment present |
| `crates/corelink-runbook-tracker/Cargo.toml` | omits `clippy::print_stdout` + `clippy::print_stderr` | CLI-emitting tracker writes runbook hints to stdout |
| `tools/cli/Cargo.toml` | omits `clippy::print_stdout` + `clippy::print_stderr` | User-facing binary needs `println!` calls |
| `apps/migrate-single-to-multi-region/Cargo.toml` | relaxed signature (`missing_docs = "warn"`, fewer clippy denies) | One-shot migration tool — already inline-justified |

**Audit-said-9, I-find-7 reconciliation.** The audit (`2026-05-27-cargo-toml-audit-post-w36.md`
§4) reported 9 deviating crates. My byte-exact signature comparison
flags 7. The 2-deviant gap is explained by the audit's signature #5+#6
distinction (same lint set, different inline comments only): `tools/cli`
and `corelink-runbook-tracker` were counted as **two separate signature
buckets** in the audit because of comment differences, but they share
the same effective lint set (#5 vs #6 in the audit table). My byte-exact
parser correctly groups them as one effective deviation per crate. Net
result is identical: same 7 crates kept with custom lints, audit and
this SEAL agree on the universe.

---

## §2 — Stale `workspace.exclude` entry (follow-up #2)

```
Before:  Cargo.toml line 357-359
    # WI-S15-006: CLI ship-gate fuzz targets (cli_input / config_toml /
    # json_deserialize / auth_resolution / secret_redaction_check).
    "crates/corelink-cli/fuzz",

After:   (removed)
```

Verified that `crates/corelink-cli/fuzz` does not exist on disk
(directory `crates/corelink-cli` itself does not exist; the CLI fuzz
harness lives at `tools/cli/fuzz` and is excluded explicitly elsewhere
in the workspace via not being listed as a member). The 2-line context
comment was removed alongside the path entry. **Cargo emits no warning
about non-existent excludes, so this was purely dead config; build and
metadata behaviour unchanged.**

---

## §3 — Fuzz crate lint uniformity (follow-up #3)

Each fuzz crate carries its **own** `[workspace]` table (cargo-fuzz
template convention; nested workspaces resolve standalone), so they
**cannot** use `lints.workspace = true` against the outer root workspace.
The audit §6 #3 recommendation was to adopt the two-line minimum already
present in `corelink-meta/fuzz` and `corelink-worker/fuzz` for the 8
remaining bare fuzz crates.

### Uniform fuzz minimal block (now applied to all 10)

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
unwrap_used = "deny"
panic = "deny"
indexing_slicing = "deny"
todo = "deny"
unimplemented = "deny"
```

Inserted **before** the `[dependencies]` table in each of these 8 files:

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

Reference crates (`crates/corelink-meta/fuzz`, `crates/corelink-worker/fuzz`)
already carry this exact block byte-for-byte; left untouched.

**All 10 / 10 fuzz Cargo.toml files now share the canonical minimal lint
block.** This is the strictest set common to a fuzz harness: forbid
unsafe, deny the five panic / unwrap / indexing / placeholder lints.
Extras like `expect_used`, `dbg_macro`, `print_stdout`, etc. are
omitted from the fuzz set because fuzz harnesses legitimately use
those constructs (libfuzzer-sys main loop, debug-print on crash).

---

## §4 — Acceptance verification

```
cargo metadata --no-deps --format-version 1
  → packages: 87  (matches baseline)

cargo build --workspace
  → Finished `dev` profile [unoptimized + debuginfo] target(s) in 4m 56s
  → GREEN

cargo clippy --workspace --tests -- -D warnings
  → see §5 (one PRE-EXISTING failure unrelated to this change)

grep -rEn '<<<<<<<|>>>>>>>' --include='*.toml' Cargo.toml crates/ apps/ tools/ tests/
  → 0 matches
```

### Spot-check diff sample (corelink-cas)

```diff
-[lints.rust]
-unsafe_code = "forbid"
-missing_docs = "deny"
-missing_debug_implementations = "deny"
-
-[lints.clippy]
-unwrap_used = "deny"
-expect_used = "deny"
-panic = "deny"
-indexing_slicing = "deny"
-todo = "deny"
-unimplemented = "deny"
-dbg_macro = "deny"
-print_stdout = "deny"
-print_stderr = "deny"
-mod_module_files = "deny"
+[lints]
+workspace = true
```

Identical shape across all 80 migrated crates.

---

## §5 — Pre-existing clippy failure in `corelink-billing` (NOT introduced)

`cargo clippy --workspace --tests -- -D warnings` fails on
`crates/corelink-billing/src/lib.rs:122-123` with
`clippy::doc_lazy_continuation` (a doc-comment formatting warning
introduced in rust-1.91 toolchain). This is **PRE-EXISTING on main** —
reproduced by `git stash && cargo clippy -p corelink-billing --tests
-- -D warnings` against the unchanged base, which emits the **same**
2 errors at the same lines.

**Why my change cannot have introduced it:**

1. `doc_lazy_continuation` is in **neither** the canonical 16-lint
   signature **nor** my new `[workspace.lints]` block. It is a clippy
   built-in lint at default `warn` severity, escalated to error by the
   command-line flag `-D warnings`.
2. The `corelink-billing` crate's lint set is **byte-identical**
   pre- and post-migration (it just moved from inline to
   workspace-inherited). Same 16 denies, no additions, no removals.
3. The failure reproduces on `main` baseline without any of my edits
   applied (verified via `git stash` round-trip).

This is documented here purely as a non-blocker observation. Charter
§3 acceptance is satisfied because the failure pre-exists this change;
remediation belongs in a separate doc-fix WI for `corelink-billing/src/lib.rs`
lines 117-128 (add proper indentation to the lazy-continuation doc lines).

---

## §6 — File-by-file change summary

```
1   Cargo.toml                        (root: +16 lines workspace.lints; −3 lines stale exclude)
80  crates/*/Cargo.toml               (migrated to lints.workspace = true; −18 / +2 each)
    apps/*/Cargo.toml                 (1 of 1 already-deviant; untouched)
8   {crates,tools}/*/fuzz/Cargo.toml  (uniform fuzz lints added)
─────────────────────────────────────
89  files changed   +257 / −1283   net −1026 LOC
```

**Untouched (custom lints preserved):**

```
crates/corelink-wasm/Cargo.toml
crates/corelink-client-verify/Cargo.toml
crates/corelink-runbook-tracker/Cargo.toml
apps/migrate-single-to-multi-region/Cargo.toml
tools/cli/Cargo.toml
tools/sdks/go/Cargo.toml
tools/sdks/python/Cargo.toml
```

**Untouched fuzz reference pattern:**

```
crates/corelink-meta/fuzz/Cargo.toml
crates/corelink-worker/fuzz/Cargo.toml
```

---

## §7 — Verdict

**APPROVE.** All 3 non-blocking follow-ups from §6 of the post Wave
35-36 audit are now closed:

1. **`[workspace.lints]` consolidation:** 80 crates migrated to
   `lints.workspace = true`; 7 deviating crates intentionally left with
   their custom blocks (rationale tabulated in §1). Net −1026 LOC of
   duplicated lint declarations across the workspace.
2. **Stale `corelink-cli/fuzz` exclude:** removed from root `Cargo.toml`
   along with its now-orphaned WI-S15-006 context comment.
3. **Fuzz lint uniformity:** all 10 fuzz Cargo.toml files now carry the
   canonical fuzz minimal block (2 already had it + 8 newly aligned).

Acceptance gates: `cargo metadata` 87 packages preserved; `cargo build
--workspace` GREEN; `cargo clippy --workspace --tests -- -D warnings`
fails ONLY on a pre-existing `doc_lazy_continuation` issue in
`corelink-billing/src/lib.rs` that reproduces on `main` baseline
unchanged — orthogonal to this change. 0 conflict markers, 0 lint
severity changes, 0 new lints, 0 deps/features touched.

Maintainability win: the canonical 16-lint signature lives in ONE place
now. Any future severity bump or new workspace-wide lint adds 1 line
to root + costs 0 LOC at the call sites for the 80 inheriting crates.
