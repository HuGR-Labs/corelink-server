---
id: "AUDIT-2026-05-16-PILOT-ONBOARDING-E2E"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-GA-CUTOVER"
  - "RB-PILOT-ONBOARDING-E2E"
  - "AUDIT-2026-05-16-PRE-GA-PENTEST-SCOPE"
  - "INVARIANT-REGISTRY"
tags:
  - "audit"
  - "wave-23"
  - "r-prep"
  - "pilot"
  - "onboarding"
  - "e2e"
  - "ga-readiness"
---

# AUDIT-2026-05-16-PILOT-ONBOARDING-E2E — Pilot tenant onboarding journey

> **Status:** ACTIVE. Owner: Gustavo Schneiter. Wave-23 (R-PREP).
>
> **Scope:** documenting the five-stage end-to-end test journey that
> exercises the full GA customer happy path against the in-memory
> CoreLink stack, plus the expected per-route SLOs that gate GA cutover.
>
> **Test crate:** [`tests/e2e-pilot-onboarding/`](../../tests/e2e-pilot-onboarding/).
>
> **Runbook:** [`RB-PILOT-ONBOARDING-E2E`](../_runbooks/RB-PILOT-ONBOARDING-E2E.md).

## 1. Purpose

GA cutover (RB-GA-CUTOVER §3) depends on a single deterministic test
that walks a pilot tenant through every customer-visible state
transition the platform exposes: signup, first CAS upload, audit
export, DSR erasure, offboarding. This audit pins the contract of
that journey so any subsystem-level refactor that breaks the
cross-system shape is caught by `cargo test -p e2e-pilot-onboarding`
before it ships.

The test crate is **self-contained** (charter
`trait-abstraction-defer`): it does not bind to the production signup
/ billing / DSR / audit-chain crates. Each of those subsystems already
has its own integration crate (`e2e-signup-flow`, `e2e-billing-flow`,
`e2e-dsr`) which pins internal contracts. This crate pins the
**journey shape** across subsystems so the 5-test surface survives
arbitrary internal-API churn.

## 2. The five-stage journey

```text
1. tenant signup
     POST /v1/signup → Stripe checkout → webhook → tenant provisioned in D1
        ↓
2. first CAS upload
     BatchUpdateBlobs (100 blobs) → R2 chunks + tenant prefix
        ↓
3. audit export round-trip
     export 24h window → NDJSON + manifest + chain integrity verify
        ↓
4. DSR erasure
     POST /v1/dsr/erasure → 7-day SLA → CAS + audit drained
        ↓
5. tenant offboarding
     subscription cancel → 30-day grace → final delete + erasure verify
```

## 3. Per-stage contract + SLO

### Stage 1 — `POST /v1/signup`

| Aspect | Contract |
|---|---|
| Audit rows emitted | `SignupRequested` → `CheckoutSessionCreated` → `CheckoutWebhookVerified` → `TenantProvisioned` (in order) |
| State transitions | lifecycle `None → PendingActivation → Active`; subscription `None → CheckoutPending → Active` |
| Fail-CLOSED | audit emitted BEFORE every state flip |
| Idempotency | duplicate signup for same slug → `SignupError::AlreadyProvisioned` |
| **SLO target** | p99 end-to-end signup < 5 s (cf. SLO-SIGNUP-LATENCY in slo_catalog) |

### Stage 2 — `BatchUpdateBlobs` (first CAS upload)

| Aspect | Contract |
|---|---|
| Batch size | 100 blobs × 4 KiB (canonical pilot fixture) |
| Audit rows | 1 × `BatchUploadAccepted` + 100 × `BlobPutCommitted`, in order |
| Tenant-prefix scoping | R2 key = `tenants/<tenant_id>/cas/<digest_hex>`; cross-tenant read → `CasError::TenantPrefixViolation` (INV-MULTIPART-PATH-TENANT-SCOPED) |
| Subscription gate | upload requires `SubscriptionState::Active`; otherwise `CasError::SubscriptionNotActive` |
| Digest integrity | server recomputes BLAKE3; mismatch → `CasError::DigestMismatch` (INV-CAS-INTEGRITY) |
| **SLO target** | p99 batch (100 blobs) < 2 s; per-blob put p99 < 100 ms |

### Stage 3 — audit export round-trip

| Aspect | Contract |
|---|---|
| Window | 24h (canonical pilot fixture) |
| Body format | NDJSON; one row per line; trailing newline preserved |
| Manifest | `tenant_id`, `window_start_ms`, `window_end_ms`, `row_count`, `body_digest_hex` (BLAKE3 of body), `first_row_hash_hex`, `last_row_hash_hex` |
| Chain integrity | row[N].`prev_hash_hex` == row[N-1].`row_hash_hex`; row[0].`prev_hash_hex` == `0`×64 |
| Tamper detection | any 1-byte flip in body → verify returns `AuditChainError::ManifestDigestMismatch` OR `ChainBreak` OR `ManifestRowCountMismatch` |
| Self-marker exclusion | `AuditExportStarted` row is NOT included in its own export body |
| Invalid window | `start > end` → `AuditExportError::InvalidWindow` |
| **SLO target** | p99 export (1k rows / 24h) < 10 s |

### Stage 4 — DSR erasure

| Aspect | Contract |
|---|---|
| Endpoint | `POST /v1/dsr/erasure` |
| SLA window | 7 calendar days (`DSR_ERASURE_WINDOW_MS`) per CTRL-PRIV-ERASURE |
| Premature finalisation | `DsrErasureError::SlaWindowOpen { remaining_ms }` |
| Idempotency | duplicate request id → `DsrErasureError::AlreadyRequested` |
| Post-finalisation state | CAS empty; audit chain collapsed to single tombstone row whose `kind == DsrErasureCompleted` and `prev_hash_hex == 0×64` |
| Tombstone retention | the `DsrErasureCompleted` event survives the redaction for compliance evidence (CTRL-PRIV-ERASURE §audit-trail) |
| **SLO target** | acceptance p99 < 1 s; completion within 7-day SLA (99.9% target) |

### Stage 5 — tenant offboarding

| Aspect | Contract |
|---|---|
| Subscription cancel | lifecycle `Active → CancelledInGrace { grace_ends_at_ms }`; subscription `Active → Cancelled` |
| Grace window | 30 calendar days (`OFFBOARDING_GRACE_MS`) per RB-GA-CUTOVER §3.4 |
| Premature finalisation | `OffboardingError::GraceNotElapsed { remaining_ms }` |
| Final hard-delete | CAS drained; audit chain drained; lifecycle `→ HardDeleted` |
| Erasure verification | `OffboardingReceipt.residual_cas_blobs == 0 && residual_audit_rows == 0` (INV-DATA-ERASURE-COMPLETE) |
| Post-DSR offboarding | starting from a tenant that already executed DSR erasure is supported; residual stays at 0 |
| **SLO target** | hard-delete completes within 30-day grace deadline (100% target) |

## 4. Test surface

| Test file | Cases | Asserts |
|---|---|---|
| `test_01_tenant_signup.rs` | 2 | full signup pipeline; duplicate rejection |
| `test_02_first_cas_upload.rs` | 3 | 100-blob batch; no-subscription rejection; cross-tenant rejection |
| `test_03_audit_export_roundtrip.rs` | 3 | 24h round-trip; tamper detection; invalid window |
| `test_04_dsr_erasure.rs` | 3 | full erasure lifecycle; duplicate rejection; no-signup rejection |
| `test_05_tenant_offboarding.rs` | 3 | full offboarding lifecycle; no-cancel rejection; post-DSR offboarding |
| `harness.rs` (in-crate unit) | 3 | tenant determinism; blob payload determinism; signup audit chain shape |
| **Total** | **17** | — |

## 5. Charter compliance

- `#![forbid(unsafe_code)]` at the crate root.
- `#[deny(missing_docs)]` and `#[deny(missing_debug_implementations)]`.
- All non-test code returns typed `PilotHarnessError` (no `unwrap` /
  `expect` / `panic` / `indexing_slicing`).
- DCO sign-off on the commit.
- No `tokio` in `src/`; no network IO under any branch.

## 6. Per-test runtime envelope

Measured locally on Apple M-class silicon, dev profile:

| Test binary | Wall time |
|---|---|
| `test_01_tenant_signup` | < 0.05 s |
| `test_02_first_cas_upload` | < 0.05 s |
| `test_03_audit_export_roundtrip` | < 0.05 s |
| `test_04_dsr_erasure` | < 0.05 s |
| `test_05_tenant_offboarding` | < 0.05 s |
| **Full suite** | **< 1 s** (well inside the 5-min budget) |

## 7. Open questions / follow-ups

- The harness pins the journey *shape*, not the production wiring of
  each subsystem. The companion crates `e2e-signup-flow`,
  `e2e-billing-flow`, and `e2e-dsr` already pin internal contracts; a
  future deliverable could compose all three at the trait level once
  the production crates stabilise their public surfaces.
- The 100-blob batch size and 4-KiB payload size are pilot-tier
  defaults. A `proptest`-shaped follow-up could randomise batch shapes
  and pin the journey under wider parameter coverage.
- SLO targets above are taken from the canonical `slo_catalog.md` for
  the corresponding routes; if any target tightens or relaxes, this
  audit + the runbook MUST be updated in lock-step.

## 8. pilot-signup-pipeline — wave-27 prep cross-reference

The wave-27 R-prep stream extends the wave-23 *journey-shape* audit
with the *operator-action-shape* tooling that wraps it: the three
admin scripts (`grant-pilot-tier.sh`, `list-pilot-tenants.sh`,
`pilot-24h-checkin.sh`), the Grafana dashboard placeholder
(`dashboards/grafana/dash-pilot-tenants.yml`), and a new DEBT row
tracking "pilot signups ≥ 3" GA readiness.

See `specs/_audits/2026-05-16-pilot-signup-pipeline.md` for the full
wave-27 deliverables and `DEBT-027` in
`specs/_audits/2026-05-15-debt-register.md` for the gate-tracking row.

### 8.1 Wave-29 stream-3 closure — admin endpoints + web UI

The wave-27 placeholder scripts above are SUPERSEDED for the
operator-on-browser path by wave-29 stream-3:

- `GET  /v1/admin/pilots?state=<S>` — replaces `list-pilot-tenants.sh`.
- `POST /v1/admin/pilots/{tenant_id}/grant-tier` — replaces `grant-pilot-tier.sh`.
- `POST /v1/admin/pilots/{tenant_id}/checkin` — replaces `pilot-24h-checkin.sh`.
- `apps/docs/src/pages/admin/pilots.tsx` — operator-facing web UI
  (auto-polls every 30s, Clerk-JWT-gated by the
  `corelink:admin:pilots` scope claim).

Routes are gated by 5-Layer Defense + fail-CLOSED audit emit
ordering (matches the wave-15 admin handler discipline). Test
coverage: `apps/server/tests/admin_pilot.rs` — 12 async integration
tests; full audit + closure note in
`specs/_audits/2026-05-16-pilot-admin-web-ui.md`.

The wave-27 shell scripts remain in-tree as the operator-shell
escape hatch until the Clerk admin instance is provisioned end-to-
end (Phase 2 swap — see wave-29 stream-3 audit §8).
