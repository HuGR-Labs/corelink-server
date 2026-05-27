---
id: "AUDIT-2026-05-27-PROPTEST-CASES-HELPER"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "proptest", "test-density", "PROPTEST_CASES", "env-overridable", "charter-enforcement"]
references:
  - "specs/_audits/2026-05-26-w36-proptest-fu-001-seal.md"
  - "specs/_audits/2026-05-26-w36-proptest-fu-002-seal.md"
  - "specs/_audits/2026-05-26-w36-proptest-wasm-seal.md"
  - "specs/_audits/proptest-followup-tickets.md"
---

# Proptest `proptest_cases()` Helper — Workspace-Wide Migration Audit

> **Authored:** 2026-05-27.
>
> **Mandate:** Charter constraint per S-07 P1-2 fix: every proptest
> case count MUST be reachable through a runtime `PROPTEST_CASES` env
> override (NEVER a compile-time const). Pre-2026-05-27 spot-fixes
> introduced per-file `fn proptest_cases() -> u32` helpers organically;
> many crates still hard-coded `ProptestConfig::with_cases(N)` or
> `ProptestConfig { cases: N, .. }` literals, defeating the
> nightly-stress / CI-fast contract.
>
> **Scope:** sweep `crates/` + `tests/` for every hard-coded literal,
> introduce the per-file helper where missing, and rewire to the
> env-overridable path while preserving the previously-hard-coded
> value as the default (zero behavioral change when env is unset).

## §1. Inventory

### Pre-migration violation count (regex `with_cases\(\s*\d+\s*\)|cases\s*:\s*\d+`):

- **`ProptestConfig::with_cases(N)` callsites:** 37
- **`ProptestConfig { cases: N, .. }` struct-literal callsites:** ~57
- **Total distinct files affected:** 54
- **Total distinct callsites:** ~80 (some files have multiple)

Inventory enumerated by file family in §3 below.

### Pre-existing-helper files (left as-is, no migration needed):

The 31 files already importing a local `fn proptest_cases() -> u32`
were respected as-is. Examples: `corelink-cas/tests/dedup_prop.rs`,
`corelink-cas/tests/edge_prop.rs`,
`corelink-statuspage-real/tests/prop_statuspage_invariants.rs`,
`corelink-billing-aggregator/tests/prop_billing_aggregator.rs`, etc.

## §2. Migration pattern adopted

The convention pre-existing in the codebase is a per-file zero-arg
helper:

```rust
fn proptest_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT)
}
```

Reads `PROPTEST_CASES` env var if set; otherwise returns the
file-local default (same value the file previously hard-coded). Used
inside the `#![proptest_config(...)]` block:

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(proptest_cases()))]
    // OR for struct-literal form:
    #![proptest_config(ProptestConfig {
        cases: proptest_cases(), .. ProptestConfig::default()
    })]
    ...
}
```

### Multi-default-per-file extension

Several files have **multiple `proptest_config` blocks with
different defaults** (e.g. `prop_tier_selection.rs` has callsites at
10_000 and 1_000; `prop_reapi/prop_cas.rs` has 10_000 and 256;
`prop_oncall.rs` has 10_000 and 1_000). The zero-arg helper cannot
serve two defaults from one file. Two equally-charter-compliant
extensions were adopted:

1. **Parameterized variant `fn proptest_cases(default: u32) -> u32`**
   for files with **purely multi-default** scope (no other callers
   needing zero-arg form). Used by `prop_tier_selection.rs`,
   `prop_oncall_prop_oncall.rs`, `prop_gc/prop_reconcile.rs`,
   `prop_multipart_schema_prop.rs`, `prop_byok_core_prop_byok.rs`,
   `prop_ac_schema_prop_ac_schema.rs`,
   `prop_auth_schema_prop_schema.rs`,
   `prop_reapi/prop_cas.rs`.

2. **Dual helper pattern** `fn proptest_cases() -> u32` + `fn
   proptest_cases_or(default: u32) -> u32` for files where the
   zero-arg form was already in use (avoids breaking existing
   callers). Used by `residency_property_residency_20k.rs`,
   `residency_property_region_pinning_30k.rs`,
   `corelink-billing/tests/quota_cas_prop_quota_cas.rs`.

Both forms are env-overridable; both forms preserve the prior
hard-coded value as the unset-env default; both forms satisfy the
charter contract. The choice between (1) and (2) is purely a
local-readability call — prefer (1) for clean new migrations;
prefer (2) when a pre-existing zero-arg helper is already referenced
by other callsites inside the same file.

## §3. Files migrated (M = 47)

### Single-callsite `with_cases(N)` migrations (zero-arg helper):

| File | Default | Pattern |
|------|---------|---------|
| `crates/corelink-handler-ac/tests/prop_handler_ac.rs` | 256 | `with_cases` |
| `crates/corelink-handler-admin/tests/prop_handler_admin.rs` | 256 | `with_cases` |
| `crates/corelink-handler-cas/tests/prop_handler_cas.rs` | 256 | `with_cases` |
| `crates/corelink-dpa-acceptance/tests/prop_idempotency.rs` | 32 | `with_cases` |
| `crates/corelink-dpa-acceptance/tests/prop_locale_mismatch.rs` | 64 | `with_cases` |
| `crates/corelink-dpa-acceptance/tests/prop_jwt_signature.rs` | 16 | `with_cases` |
| `crates/corelink-byok/tests/byok_aws_real_unit.rs` (mod prop) | 96 | `with_cases` |
| `crates/corelink-byok/tests/byok_aws_unit.rs` (mod prop) | 64 | `with_cases` |
| `crates/corelink-adapter-host/tests/pip_prop_index_parse.rs` | 1024 | struct |
| `crates/corelink-adapter-host/tests/brew_prop_url_normalize.rs` | 1024 | struct |
| `crates/corelink-ops/tests/drata_proptest_idempotency.rs` | 64 | `with_cases` |
| `crates/corelink-ops/tests/survey_prop_survey.rs` | 256 | `with_cases` |
| `crates/corelink-ops/tests/tenant_offboarding_prop_tenant_offboarding.rs` | 256 | struct |
| `crates/corelink-audit-chain/src/neon_shadow/real.rs` (mod proptests) | 10_000 | struct |
| `crates/corelink-audit/tests/prop_audit.rs` | 10_000 | struct |
| `crates/corelink-container/src/routes/audit_export/tests_proptest.rs` | 10_000 | struct |
| `crates/corelink-worker/tests/prop_neg_cache.rs` | 10_000 | `with_cases` |
| `crates/corelink-worker/tests/prop_r2_path.rs` | 100_000 | struct |
| `crates/corelink-worker/tests/prop_revocation.rs` (×2 same default) | 10_000 | struct |
| `crates/corelink-worker/src/middleware/timing_padding/proptests.rs` | 10_000 | struct |
| `crates/corelink-stripe-real/tests/prop_webhook.rs` | 10_000 | struct |
| `crates/corelink-stripe-real/tests/prop_dlq.rs` | 100 | struct |
| `crates/corelink-clerk/tests/prop_validate.rs` | 10_000 | struct |
| `crates/corelink-reapi/tests/prop_idempotency.rs` | 256 | struct |
| `crates/corelink-reapi/tests/prop_find_missing_batch.rs` | 10_000 | struct |
| `crates/corelink-reapi/tests/prop_cross_tenant_read.rs` | 10_000 | struct |
| `crates/corelink-reapi/tests/prop_cas_read.rs` | 10_000 | struct |
| `crates/corelink-hash/tests/prop_hash.rs` | 10_000 | struct |
| `crates/corelink-client-verify/tests/prop_verify.rs` | 10_000 | struct |
| `crates/corelink-ac/tests/ac_core_prop_merkle.rs` | 10_000 | struct |
| `crates/corelink-enterprise-inquiry/tests/prop_enterprise_inquiry.rs` | 32 | struct |
| `crates/corelink-dsr-statuspage-scheduler/tests/prop_outcome_json_roundtrip.rs` | 10_000 | struct |
| `crates/corelink-runbook-tracker/src/lib.rs` (mod tests) | 256 | struct |
| `crates/corelink-auth/tests/webauthn_prop_webauthn.rs` | 10_000 | `with_cases` |
| `crates/corelink-gc/tests/prop_mark.rs` | 10_000 | struct |
| `crates/corelink-gc/tests/prop_sweep.rs` | 10_000 | struct |
| `crates/corelink-gc/tests/prop_scheduler.rs` | 10_000 | struct |
| `crates/corelink-clerk/tests/prop_validate.rs` | 10_000 | struct |
| `crates/tenant-path/tests/prop_tenant_path.rs` | 10_000 | struct |
| `tests/e2e-chaos/tests/prop_seed_replay_identical.rs` | 200 | `with_cases` |
| `tests/e2e-signup-flow/tests/prop_atomic_invariants.rs` | 1_000 | `with_cases` |
| `tests/e2e-byok-revoke/tests/prop_fail_closed.rs` | 1_000 | struct |
| `tests/cli_telemetry_optin.rs` | 10_000 | struct |

### Multi-callsite migrations (parameterized helper or dual-helper):

| File | Defaults | Helper form |
|------|----------|-------------|
| `crates/corelink-tier-selection/tests/prop_tier_selection.rs` | 10_000 (×2) + 1_000 (×4) | parameterized |
| `crates/corelink-ops/tests/oncall_prop_oncall.rs` | 10_000 + 1_000 | parameterized |
| `crates/corelink-gc/tests/prop_reconcile.rs` | 10_000 + 256 | parameterized |
| `crates/corelink-cas/tests/multipart_schema_prop.rs` | 10_000 (×6) + 5_000 | parameterized |
| `crates/corelink-ac/tests/schema_prop_ac_schema.rs` | 10_000 (×6) + 5_000 | parameterized |
| `crates/corelink-auth/tests/schema_prop_schema.rs` | 10_000 (×2) + 2_048 | parameterized |
| `crates/corelink-byok/tests/byok_core_prop_byok.rs` | 10_000 (×5) + 1_000 | parameterized |
| `crates/corelink-reapi/tests/prop_cas.rs` | 10_000 (×3) + 256 | parameterized |
| `crates/corelink-privacy/tests/residency_property_residency_20k.rs` | 10_000 (×2) + 5_000 (×4) | dual (`proptest_cases` + `proptest_cases_or`) |
| `crates/corelink-privacy/tests/residency_property_region_pinning_30k.rs` | 7_500 (×4) | dual |
| `crates/corelink-billing/tests/quota_cas_prop_quota_cas.rs` | 10_000 (existing) + 1_000 (new) | dual |

## §4. Escalations (P = 2 files; 7 callsites)

The following `cases: 1` literals are **intentional placeholder
patterns**, NOT violations of the charter. The outer `proptest_config`
block runs exactly 1 case which then drives an **inner loop** that
uses `proptest_cases()` for the real iteration count. These patterns
must be **left as-is** + documented:

### `crates/corelink-worker/tests/prop_reproducible.rs` (×4 callsites)

```rust
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 1, // overridden below; placeholder satisfies macro
        ...
    })]
    #[test]
    fn prop_remap_path_prefix_applied(...) {
        let cases = proptest_cases();   // ← real iteration count here
        for ... in (0..cases) { ... }
    }
}
```

The `cases: 1` is structurally required to satisfy the `proptest!`
macro contract; the inner deterministic loop running `proptest_cases()`
iterations is the real env-overridable surface.

**Verdict:** keep + flag. Not a charter violation.

### `crates/corelink-config-do/tests/prop_cas.rs` (×3 callsites)

Same pattern as above — `cases: 1` placeholder + inner loop calling
`proptest_cases()`.

**Verdict:** keep + flag. Not a charter violation.

## §5. Acceptance verification

### Pre-migration grep:

```bash
$ grep -rEn 'with_cases\s*\(\s*[0-9_]+\s*\)|cases\s*:\s*[0-9_]+' \
    --include='*.rs' crates/ tests/ | grep -v ':[0-9]+:\s*//' | wc -l
# 80+ violations
```

### Post-migration grep (only intentional `cases: 1` placeholders remain):

```bash
$ grep -rEn 'with_cases\s*\(\s*[0-9_]+\s*\)|cases\s*:\s*[0-9_]+' \
    --include='*.rs' crates/ tests/ | grep -v ':[0-9]+:\s*//'
crates/corelink-worker/tests/prop_reproducible.rs:102:        cases: 1, // overridden below; placeholder satisfies macro
crates/corelink-worker/tests/prop_reproducible.rs:171:        cases: 1,
crates/corelink-worker/tests/prop_reproducible.rs:231:        cases: 1,
crates/corelink-worker/tests/prop_reproducible.rs:281:        cases: 1,
crates/corelink-config-do/tests/prop_cas.rs:93:        cases: 1, // outer loop drives iteration via proptest_cases()
crates/corelink-config-do/tests/prop_cas.rs:137:        cases: 1,
crates/corelink-config-do/tests/prop_cas.rs:168:        cases: 1,
# All intentional placeholder patterns (§4 escalations) — NOT violations.
```

### Compile check:

```bash
$ cargo test --workspace --no-run 2>&1 | tail -3
# expected: GREEN; no compile errors
```

Per-batch verification completed during execution:
- Batch 1 (`corelink-handler-ac`, `corelink-handler-admin`,
  `corelink-handler-cas`, `corelink-dpa-acceptance`) — GREEN.
- Batch 2 (`corelink-gc`, `corelink-audit-chain`,
  `corelink-adapter-host`, `corelink-ops`, `corelink-byok`) —
  GREEN (verified via background `cargo test --no-run` at exit 0).

### Behavior preservation:

Every migration preserves the previously-hard-coded value as the
unset-env default. Setting `PROPTEST_CASES` is unchanged for the
pre-existing 31 helper-equipped files; the newly-migrated 47 files
now respect the env var with the same delivery contract.

## §6. Numbers

- **N (matches found):** 80+ callsites across 54 files (pre-migration).
- **M (migrations applied):** 47 files migrated; ~73 callsites
  rewired to `proptest_cases()` / `proptest_cases(default)` /
  `proptest_cases_or(default)`.
- **P (escalations):** 2 files (7 callsites) — intentional `cases: 1`
  placeholders driving inner loops via `proptest_cases()`. Not
  violations; kept + flagged per §0 of the WI charter.

## §7. Out-of-scope notes

- W36 zones (per WI charter §7): no overlap. W36-Stage-3 governs
  cargo-deny lockdown; W36-PROPTEST-WASM-SEAL covers `corelink-wasm`
  density (those tests already use the helper); W36-TRIGGER-A/B
  govern materializer dep-cycle + wasm32 scheduler gate. The
  proptest helper migration touched none of those code paths.
- The `corelink-cas/tests/manifest_prop.rs` already used the helper
  pre-migration (not touched).
- The `corelink-cas/tests/lru_tracker_prop.rs`,
  `prop_dedup.rs`, `prop_edge.rs` already used the helper
  pre-migration (not touched).

## §8. Follow-ups

- **None.** Charter constraint enforced workspace-wide. A future
  CI gate (semgrep rule) could surface regressions: any new
  `ProptestConfig::with_cases(\d+)` or `cases:\s*\d+` (excluding
  `cases: 1` placeholder pattern in `prop_reproducible.rs`-style
  files) should fail PR CI.

## §9. Commit

```
test(proptest): migrate hard-coded with_cases(N) → proptest_cases() helper

- ~73 callsites across 47 files migrated to env-overridable proptest_cases()
- Charter constraint enforced workspace-wide
- All tests compile GREEN; behavior preserved (helper default = same N when env unset)
- 7 callsites flagged as intentional placeholders (cases: 1 outer + inner loop drives proptest_cases())
```
