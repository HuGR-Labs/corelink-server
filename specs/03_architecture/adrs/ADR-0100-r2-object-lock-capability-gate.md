---
id: "ADR-0100"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-09-05"
updated: "2026-09-05"
owner: "tl"
final_approver: "pending"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "retention", "object-lock", "r2", "legal-hold", "deferred"]
---

# ADR-0100 — Object-Lock capability gate for Compliance retention

## Status

**DEFERRED / BLOCKED.** R2 does not currently implement the S3 Object-Lock
operations required for storage-enforced Compliance retention. This ADR records
the gate and the evidence procedure; it does not claim that a backend is live.

## Context

B-009 ships Governance-mode legal-hold CAS retention. That path is deliberately
code-reversible: it severs the subject linkage in `cas_retention` and lets an
authorized drain release bytes after the hold. Compliance mode has a stronger
meaning: a storage administrator must be unable to delete an object before its
retention date. Lifecycle policy, `If-None-Match`, a D1 row, a Rust trait, and an
in-memory fake do not provide that property.

The last R2 probe returned `NotImplemented` for both bucket-level Object-Lock
creation and per-object Compliance retention. A permission, credential,
endpoint, or transport failure would not prove the same thing.

## Decision

1. Keep B-046 open and keep `cas_retention.mode` limited to `governance`.
2. Require two successful operations against the same provisioned backend and
   account before recording external capability evidence:
   `CreateBucket(ObjectLockEnabledForBucket=true)` and
   `PutObject(ObjectLockMode=COMPLIANCE, RetainUntilDate=...)`.
3. Treat explicit `NotImplemented` as `BLOCKED`; treat every other failure as
   `INDETERMINATE`. Neither closes B-046.
4. Mount no Compliance retention route and do not send Object-Lock setters to
   R2 while the capability gate is blocked. Legal-hold erasure remains
   fail-closed through the existing tenant-bound DSR legitimacy and
   `R2_TDK_HEX` checks.
5. If an Object-Lock-capable provider is approved later, add a provider-bound
   adapter, tenant/residency routing, migration, destructive-path tests, and an
   owner-approved rollout record as a new implementation decision. This ADR is
   not that implementation.

## Consequences

The strongest shipped retention guarantee remains Governance mode, and customer
or legal material must not call it WORM or storage-enforced immutable. A future
provider can be evaluated without changing the truth of the current R2 path.
Unknown probe failures stay visible instead of silently turning into either a
false block or a false Compliance capability.

## Evidence

- `scripts/verify_b046_object_lock_probe.py` — redacted two-operation probe and
  fail-closed classifier.
- `docs/knowledge/ops/r2-object-lock-probe.md` — operator procedure and truth
  table.
- `BACKLOG.md` B-046 — open status and dated R2 blocker.
- `crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs` and
  `migrations/d1/0102_cas_retention.sql` — current Governance/legal-hold path.
