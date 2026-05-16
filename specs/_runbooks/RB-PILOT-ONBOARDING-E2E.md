---
id: "RB-PILOT-ONBOARDING-E2E"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "R-PREP-WAVE-23"
parent_wi: "WI-R-PREP-PILOT-ONBOARDING-E2E"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-GA-CUTOVER"
  - "AUDIT-2026-05-16-PILOT-ONBOARDING-E2E"
tags:
  - "runbook"
  - "wave-23"
  - "r-prep"
  - "pilot"
  - "onboarding"
  - "e2e"
  - "ga-readiness"
---

# RB-PILOT-ONBOARDING-E2E — Pilot tenant onboarding E2E runbook

> **Status:** DRAFT. Owner: Gustavo Schneiter.
>
> **Scope:** how to run the five-stage pilot tenant onboarding journey
> test (`cargo test -p e2e-pilot-onboarding`) and what to do when each
> of the 5 test binaries fails. Companion to
> [`AUDIT-2026-05-16-PILOT-ONBOARDING-E2E`](../_audits/2026-05-16-pilot-onboarding-e2e.md).

## 1. When to run

| Trigger | Action |
|---|---|
| Every PR that touches `corelink-signup`, `corelink-billing-stripe`, `corelink-dsr`, `corelink-audit-chain`, `corelink-r2-multipart`, `corelink-tenant-path` | Run full suite (CI runs it automatically) |
| Every GA cutover dress-rehearsal (RB-GA-CUTOVER §3) | Run full suite + cross-reference SLOs in AUDIT-2026-05-16-PILOT-ONBOARDING-E2E §3 |
| Suspected regression in tenant lifecycle | Run only the failing stage (see §3 below for per-test debug) |

## 2. Standard invocation

```bash
# Build the workspace first to surface any compile error.
cargo build --workspace

# Full pilot suite (5 test binaries, 14 tests + 3 in-crate units).
cargo test -p e2e-pilot-onboarding --no-fail-fast

# A single stage:
cargo test -p e2e-pilot-onboarding --test test_03_audit_export_roundtrip
```

Expected: 17 passing tests, full suite under 1 s on Apple M-class
silicon. The deliverable budget is < 60 s per test / < 5 min for the
whole suite.

## 3. Per-test debug playbook

### 3.1 `test_01_tenant_signup`

| Assertion | If it fails, suspect |
|---|---|
| 4 audit rows in canonical order | The fail-CLOSED ordering in `complete_signup` was reordered; check audit emission is still BEFORE every state mutation |
| `prev_hash` chain integrity | BLAKE3 canonical-payload composition changed; check `Self::emit_audit` |
| `lifecycle == Active` post-signup | `TenantProvisioned` audit row firing but state flip skipped; check the second-half of `complete_signup` |
| Duplicate rejected | The idempotency check at the top of `complete_signup` got short-circuited |

### 3.2 `test_02_first_cas_upload`

| Assertion | If it fails, suspect |
|---|---|
| `blobs_uploaded == 100` | The `payloads.len() != CANONICAL_BLOB_COUNT`; check `canonical_blob_payloads` |
| Tenant-prefix string | `BatchUploadReceipt.tenant_prefix` formatter drifted from canonical `tenants/<tenant_id>/cas/` |
| GET roundtrip byte-identical | CAS storage mutation or digest mis-computation |
| Cross-tenant read rejected | `read_blob` no longer walks the inner map to detect prefix violations (INV-MULTIPART-PATH-TENANT-SCOPED) |
| 105 audit rows post-upload | Either signup ran twice, an audit emission was dropped, or extra rows are being emitted |

### 3.3 `test_03_audit_export_roundtrip`

| Assertion | If it fails, suspect |
|---|---|
| `manifest.row_count == 105` | Cutoff filter in `export_audit_window` (the `seq < cutoff_seq` clause) regressed |
| `body_digest_hex == BLAKE3(body)` | NDJSON line composition changed (extra/missing newline, key ordering) |
| Tamper detection fires | One of the three verify clauses (`ManifestDigestMismatch`, `ChainBreak`, `ManifestRowCountMismatch`) was loosened |
| `InvalidWindow` rejected | The `start > end` guard at the top of `export_audit_window` was removed |

### 3.4 `test_04_dsr_erasure`

| Assertion | If it fails, suspect |
|---|---|
| `sla_deadline_ms == FIXED_NOW_MS + 7 days` | `DSR_ERASURE_WINDOW_MS` constant drifted from 7 × `ONE_DAY_MS` |
| Premature finalisation rejected | The SLA-window check in `finalise_dsr_erasure` regressed |
| Post-erasure CAS empty + 1 audit row | The drain logic dropped, OR the tombstone-anchor logic regressed (chain MUST be `0×64 → kind=DsrErasureCompleted`) |
| Duplicate request rejected | The `erasure_requests.contains` check at the top of `request_dsr_erasure` regressed |

### 3.5 `test_05_tenant_offboarding`

| Assertion | If it fails, suspect |
|---|---|
| `CancelledInGrace { grace_ends_at_ms }` | The lifecycle transition in `cancel_subscription` was changed |
| `grace_ends_at_ms == FIXED_NOW_MS + 30 days` | `OFFBOARDING_GRACE_MS` constant drifted |
| Premature offboarding rejected | The grace-window check in `complete_offboarding` regressed |
| `residual_cas_blobs == 0 && residual_audit_rows == 0` | The wipe-then-measure ordering in `complete_offboarding` regressed (must drain BEFORE counting residual) |
| Post-DSR offboarding flow | Either erasure left state-only residual, or offboarding tripped over an empty audit chain |

## 4. Cross-test invariants

These hold across every stage and are asserted in multiple test
binaries — a regression in any of them is structural, not stage-
local.

1. **INV-DATA-AUDIT-CHAIN** — every audit row's `prev_hash_hex`
   equals the predecessor row's `row_hash_hex`; genesis row's
   `prev_hash_hex` is `0×64`. (Cf. `INV-AUDIT-CHAIN-HASH-DETERMINISTIC`,
   `INV-AUDIT-APPEND-ONLY`.)
2. **INV-CAS-INTEGRITY** — blob bytes round-trip via BLAKE3; any
   tamper triggers `CasError::DigestMismatch`.
3. **INV-MULTIPART-PATH-TENANT-SCOPED** — a tenant cannot read a
   digest stored under another tenant's prefix.
4. **INV-DATA-ERASURE-COMPLETE** — post-offboarding (and post-DSR-
   erasure) residual CAS + audit data is exactly 0.
5. **Fail-CLOSED audit ordering** — every state-mutating method
   emits its audit row BEFORE flipping the corresponding state field.

If any cross-test invariant breaks, the failure surfaces in at least
two of the 5 test binaries. Treat that as a structural regression
rather than a stage-local bug.

## 5. Updating the harness

The harness lives in
[`tests/e2e-pilot-onboarding/src/harness.rs`](../../tests/e2e-pilot-onboarding/src/harness.rs).
To add a new stage:

1. Add a new variant to `AuditEventKind` (preserve canonical ordering
   — append at the end).
2. Add a new method on `PilotHarness` that follows the
   `with_tenant` → audit-then-mutate pattern.
3. Add a new test binary under `tests/` with at least one happy-path
   case + one adversarial case + one idempotency case.
4. Register the test binary in the crate's `Cargo.toml` `[[test]]`
   section.
5. Update [`AUDIT-2026-05-16-PILOT-ONBOARDING-E2E`](../_audits/2026-05-16-pilot-onboarding-e2e.md)
   §3 with the new stage's contract + SLO.

## 6. Charter constraints

- `#![forbid(unsafe_code)]` at the crate root.
- `#[deny(missing_docs)]`, `#[deny(missing_debug_implementations)]`.
- All non-test code returns typed `PilotHarnessError` (no `unwrap` /
  `expect` / `panic` / `indexing_slicing`).
- No `tokio` in `src/`; no network IO under any branch.

## 7. Related runbooks

- [`RB-GA-CUTOVER`](RB-GA-CUTOVER.md) — the cutover sequence that this
  test journey gates.
- [`RB-AUDIT-EXPORT-INTEGRITY`](RB-AUDIT-EXPORT-INTEGRITY.md) — what
  to do when a customer-reported audit-export verify fails (the
  production-side counterpart to stage 3 of this journey).
- [`RB-DSR-GDPR`](RB-DSR-GDPR.md) — GDPR DSR ticket triage (the
  production-side counterpart to stage 4 of this journey).
