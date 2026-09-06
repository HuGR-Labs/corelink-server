---
type: "OperationalControl"
title: "R2 Object-Lock capability probe"
description: "The fail-closed evidence boundary for storage-enforced Compliance retention; R2 remains blocked until an external provider probe succeeds."
source_files:
  - "scripts/verify_b046_object_lock_probe.py"
  - "BACKLOG.md"
  - "specs/03_architecture/adrs/ADR-0100-r2-object-lock-capability-gate.md"
  - "crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs"
  - "migrations/d1/0102_cas_retention.sql"
checkpoint_sha: "5e655c7d2bdc5447d76717216d2244aa400bd82d"
provenance: "AUTHORED"
deferred: true
tags: ["r2", "object-lock", "retention", "legal-hold", "compliance", "fail-closed"]
timestamp: "2026-09-05T00:00:00Z"

---
# R2 Object-Lock capability probe

B-046 is a capability boundary, not a promise that can be manufactured in the
repository. Cloudflare R2 returned `NotImplemented` for both required S3
operations in the last recorded probe (2026-08-25): creating a bucket with
Object Lock enabled and putting an object with `COMPLIANCE` retention. Until
both operations succeed against a provisioned, owner-approved backend, CoreLink
does not expose a Compliance mode and does not claim storage-enforced
immutability.

## Operational procedure

`scripts/verify_b046_object_lock_probe.py` has a read-only contract mode and an
explicit `--probe` mode. The latter requires `B046_PROBE_ALLOW_MUTATION=1`, a
probe bucket named `corelink-b046-probe-*`, HTTPS S3 endpoint, and write-only
credentials in the environment (`R2_S3_ACCESS_KEY_ID`,
`R2_S3_SECRET_ACCESS_KEY`, and optional `R2_S3_SESSION_TOKEN`). The verifier
passes these as `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and optional
`AWS_SESSION_TOKEN` to the child AWS CLI only; they are never CLI arguments or
unredacted output. It performs exactly two capability operations:

1. `CreateBucket` with `--object-lock-enabled-for-bucket`.
2. `PutObject` with `--object-lock-mode COMPLIANCE` and a future
   `--object-lock-retain-until-date`.

The output is `SUPPORTED` only when both calls succeed. An explicit provider
`NotImplemented` is `BLOCKED`; permission, credentials, network, endpoint, or
any unknown error is `INDETERMINATE`. No unknown error is converted into a
negative platform result, and no local fake is evidence of provider support.

The probe bucket is intentionally not auto-deleted: deletion could itself
invalidate the evidence or violate retention. The operator must preserve the
redacted output, bucket identity, provider/account, date, and cleanup decision
as external evidence. A successful probe is still not a product guarantee until
the owner provisions the backend and approves its tenant/data-residency use.

## Tenant and legal-hold safety

The shipped B-009 path remains Governance-mode CAS legal hold. It stores a
subject-free `cas_retention` row and is code-reversible after an authorized hold
release; it is not Object-Lock WORM. A B-046 probe must never change that path,
weaken the DSR legitimacy gate, delete held bytes, or imply that an in-memory
test double protects a tenant. `migrations/d1/0102_cas_retention.sql` therefore
continues to admit only `governance`; a future `compliance` value requires a
separate migration and an independently verified Object-Lock backend.

## Truth table

| CreateBucket Object Lock | PutObject Compliance retention | Repository posture |
|---|---|---|
| `NotImplemented` | `NotImplemented` | `BLOCKED`; Governance is the maximum shipped mode |
| unknown/error | any | `INDETERMINATE`; do not close or make a claim |
| success | unknown/error | `INDETERMINATE`; do not close or make a claim |
| success | success | external capability evidence only; owner provisioning and contract approval remain |

## Citations

- `scripts/verify_b046_object_lock_probe.py:50-76` — conservative operation
  classification and aggregate capability verdict.
- `scripts/verify_b046_object_lock_probe.py:100-176` — explicit opt-in,
  safety-prefixed bucket, and the two S3 operations.
- `BACKLOG.md:642-680` — current R2 blocker, Governance ceiling, and no-claim
  boundary.
- `specs/03_architecture/adrs/ADR-0100-r2-object-lock-capability-gate.md:36-54` —
  the deferred decision and fail-closed provider rule.
- `crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs:11-32` —
  tenant legal-hold behavior and explicit distinction from Object Lock.
- `migrations/d1/0102_cas_retention.sql:22-35` — Governance-only schema and
  future table-rebuild requirement for a Compliance mode.
