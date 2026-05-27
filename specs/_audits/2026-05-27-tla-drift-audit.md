# TLA+ Spec Drift Audit — Post Wave 33–36 Reorg

**Date:** 2026-05-27
**Auditor:** TLA-DRIFT-AUDIT agent (read-only)
**Scope:** Verify path/symbol references inside TLA+ specs still resolve to
existing source files after Wave 33 architectural reorg + Wave 35 Phase 2
crate absorptions (107 → 68 crates).
**Mandate:** Surface drift only. **NO `.tla` / `.cfg` files were modified.**
Re-checking invariants requires the TLA+ Toolbox (TLC) and is out of scope
for this catalog pass.

---

## 1. Corpus surveyed

| Location | `.tla` | `.cfg` |
|---|---:|---:|
| `specs/tla/` | 47 | 92 |
| `specs/03_architecture/tla+/runbooks/` | 4 | 4 |
| **Total** | **51** | **96** |

Total comment/anchor references to `crates/corelink-*` paths inside specs:
**30 unique `<spec-file, crate-path>` pairs**.

---

## 2. Stale crate-directory references (9 crates absorbed)

The following crates are referenced by TLA+ specs but no longer exist as
top-level workspace members after Wave 35 P2 absorption. Likely new homes
were located by symbol search in the surviving crates and are listed for
orchestrator review (NOT applied).

| Absorbed crate (referenced) | Spec(s) referencing it | Likely new home | Confidence |
|---|---|---|---|
| `crates/corelink-backup-verify` | `backup_fresh.tla`, `backup_restore_ephemeral.tla` | `crates/corelink-ops/src/dr.rs` (+ tests `dr_backup_verify_*`) | High |
| `crates/corelink-chunker` | `multipart_determinism.tla` | `crates/corelink-cas/src/chunker/fastcdc.rs` | High |
| `crates/corelink-multipart` | `multipart_determinism.tla` | `crates/corelink-r2-multipart/src/adapter.rs` (or `crates/corelink-cas/src/r2_multipart.rs`) | Medium — two candidates |
| `crates/corelink-obs` | `audit_no_raw_pii.tla` | `crates/corelink-tracing/` and/or `crates/corelink-telemetry/` (no surviving `tracing_filter.rs`) | Medium — symbol may have been renamed |
| `crates/corelink-otel-export` | `obs_no_pii.tla` | `crates/corelink-telemetry/src/otel/` | Medium |
| `crates/corelink-privacy-consent-ledger` | `consent_proof_verifiable.tla` | `crates/corelink-privacy/src/consent.rs` (+ `src/consent/`) | High |
| `crates/corelink-privacy-sub-processors` | `sub_processor_audit_fail_closed.tla` | `crates/corelink-privacy/src/sub_processor.rs` (+ `src/sub_processor/`) | High |
| `crates/corelink-rollout-controller` | `rollout_cosign_gate.tla` | `crates/corelink-ops/src/deploy.rs` (cosign verifier present) | Medium |
| `crates/corelink-tenant-offboarding` | `offboarding_audit_complete.tla` | `crates/corelink-ops/src/tenant_offboarding.rs` (+ `src/tenant_offboarding/`) | High |

**Total absorbed-crate references:** 11 hits across **9 spec files**.

---

## 3. Stale `.rs` file references (under still-existing crates)

These point at files that **moved during Wave 33 module reorganisation** even
though the parent crate still exists:

| Referenced path | Spec | Likely new location |
|---|---|---|
| `crates/corelink-ac/src/merkle.rs` | `ac_integrity.tla` | `crates/corelink-ac/src/ac_core/merkle.rs` |
| `crates/corelink-ac/src/sig.rs` | `ac_integrity.tla` | `crates/corelink-ac/src/ac_core/sig.rs` |
| `crates/corelink-audit/src/pseudonymize.rs` | `auth_audit_pseudonymization.tla` | `crates/corelink-auth/src/schema/pseudonymize.rs` **or** `crates/corelink-privacy/src/pseudonymize.rs` (two candidates — disambiguate via spec context) |
| `crates/corelink-audit/src/redactor.rs` | `audit_no_raw_pii.tla` | Symbol `redactor` not found by filename; check `crates/corelink-audit/src/redact.rs` (renamed `redact.rs`?) |
| `crates/corelink-auth/src/orm.rs` | `auth_schema_rls_default_on.tla` | `corelink-auth/src/schema/` (no `orm.rs`; likely renamed to `schema.rs` / split under `schema/`) |
| `crates/corelink-cas/src/store.rs` | `cas_immutability.tla` | `crates/corelink-cas/src/dedup.rs` or `crates/corelink-cas/src/manifest.rs` (no `store.rs` in cas now) |
| `crates/corelink-worker/src/r2_put.rs` | `digest_verification.tla` | `crates/corelink-worker/src/storage/` (no `r2_put.rs`; storage module reshuffled) |
| `src/routes/signup.rs` | `signup_token_idempotent.tla` | `crates/corelink-signup/src/orchestrator.rs` + `request.rs` + `idempotency.rs` (legacy monolith path; pre-workspace layout) |

**Total stale `.rs` references:** 8 unique paths across **7 spec files**
(some specs hit by both §2 and §3 — counted independently).

Path **`crates/corelink-worker/src/middleware/auth.rs`** referenced by
`auth_constant_time_cold_pad.tla` was initially flagged but **re-verified to
exist** — not drift.

---

## 4. Other notable references (info, not drift)

- `corelink_time::` Rust module reference inside `billing_atomicity.tla` —
  no `corelink-time` crate exists. Time utilities now live in
  `crates/corelink-core/src/time.rs`. Spec comment is the only mention.
- `corelink_pat::` inside `auth_constant_time_cold_pad.tla` resolves
  cleanly to `crates/corelink-pat/` (still present).

---

## 5. Per-spec drift summary

Spec files with stale references (orchestrator review recommended):

| Spec file | Issues |
|---|---|
| `specs/tla/ac_integrity.tla` | 2 moved `.rs` paths (§3) |
| `specs/tla/audit_no_raw_pii.tla` | 1 absorbed crate + 1 moved `.rs` |
| `specs/tla/auth_audit_pseudonymization.tla` | 1 moved `.rs` (ambiguous target) |
| `specs/tla/auth_schema_rls_default_on.tla` | 1 moved `.rs` |
| `specs/tla/backup_fresh.tla` | 1 absorbed crate |
| `specs/tla/backup_restore_ephemeral.tla` | 1 absorbed crate |
| `specs/tla/cas_immutability.tla` | 1 moved `.rs` |
| `specs/tla/consent_proof_verifiable.tla` | 1 absorbed crate |
| `specs/tla/digest_verification.tla` | 1 moved `.rs` |
| `specs/tla/multipart_determinism.tla` | 2 absorbed crates |
| `specs/tla/obs_no_pii.tla` | 1 absorbed crate (otel-export) |
| `specs/tla/offboarding_audit_complete.tla` | 1 absorbed crate |
| `specs/tla/rollout_cosign_gate.tla` | 1 absorbed crate |
| `specs/tla/signup_token_idempotent.tla` | 1 legacy monolith path |
| `specs/tla/sub_processor_audit_fail_closed.tla` | 1 absorbed crate |

**16 spec files** out of 51 carry at least one stale anchor reference.

`.cfg` files surveyed contained no `crates/…` path references — they hold
TLC config (constants, properties, invariants) only.

---

## 6. Recommendation (NOT applied by this audit)

1. Update the listed spec-file comments to point at the new module paths
   so future readers can cross-reference code without grep gymnastics.
2. After path comments are corrected, run `tlc` against each affected
   spec/`.cfg` pair to confirm that **only path comments changed** and
   no PlusCal/TLA structures need rework. The state-space and operators
   should be unaffected — these are documentation anchors, not module
   imports, so no invariant proof should be invalidated by the rename.
3. Two cases need careful disambiguation before edits:
   - `audit/pseudonymize.rs` — two surviving candidates
     (`corelink-auth/src/schema/pseudonymize.rs` vs
     `corelink-privacy/src/pseudonymize.rs`). Pick by spec semantics.
   - `audit/redactor.rs` vs `corelink-audit/src/redact.rs` — confirm
     whether this is a rename or a different module entirely.

## 7. Audit guarantees

- **Zero `.tla` files modified.**
- **Zero `.cfg` files modified.**
- **No `tlc` runs claimed.** This catalog does NOT certify invariants as
  proved or broken — only flags out-of-date code-path comments.
- All stale paths were verified by `[ ! -d ]` / `[ ! -f ]` on the current
  worktree (branch `worktree-agent-a19d35c122ff333a9`, head
  `6e488fb3`).
