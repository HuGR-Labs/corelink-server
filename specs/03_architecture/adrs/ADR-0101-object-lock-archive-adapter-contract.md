---
id: "ADR-0101"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-09-22"
updated: "2026-09-22"
owner: "tl"
final_approver: "pending"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "retention", "object-lock", "archive", "capability-gate", "r2"]
---

# ADR-0101 — Provider-neutral Object-Lock archive adapter contract

## Status

**ACTIVE contract; provider selection deferred.** This ADR implements the
software boundary required by ADR-0100. It does not approve an archive
provider, account, region, retention policy, legal-hold policy, or any
credentials.

## Context

Compliance retention requires storage-enforced immutability: a storage
administrator cannot delete an object before the retention term ends. The
current R2 archive cannot supply that property. Cloudflare's S3 compatibility
table marks bucket Object Lock enablement, `GetObjectLockConfiguration`, and
`PutObjectLockConfiguration` unsupported. The existing R2 Bucket Lock rule,
conditional writes, application records, and hash-chain verification remain
useful controls but are not WORM because an authorized administrator can
change the rule or delete objects.

ADR-0100 already keeps the Compliance mode blocked. Without a single archive
interface, a future implementation could accidentally convert a failed
Object-Lock request into an ordinary archive put or a lifecycle policy, then
describe the result as immutable.

## Decision

Adopt `corelink_audit_chain::ObjectLockArchiveAdapter` as the portable archive
port. The interface is deliberately limited to the evidence a Compliance
archive needs:

1. Capability negotiation for immutable put, retain-until readback, legal-hold
   readback, delete denial, residency metadata, and a durable audit receipt.
2. Immutable put followed by exact provider readback of retain-until,
   legal-hold, and residency.
3. An explicit delete-denial result; a provider that allows deletion is a hard
   contract failure.
4. A non-secret backend identity and evidence reference that binds a successful
   negotiation to one archive target.

`VerifiedObjectLockArchive` is the consumer surface. It refuses construction
when negotiation is incomplete and exposes no fallback backend. Every immutable
write validates the returned audit receipt and performs post-write readback.
The supplied `R2ObjectLockUnavailable` adapter makes the present R2 outcome
explicit: negotiation fails before an immutable put can run.

The in-memory adapter is a CI conformance fixture only. It proves the contract
mechanics and includes a mutable negative fixture that fails delete-denial
verification. It is not provider evidence and never authorizes a WORM claim.

## Consequences

- Current R2 audit archival stays non-WORM and no route or data migration
  changes in this ADR.
- An S3 or GCS implementation may be selected later, but only after the
  separate child issue approves the provider/account/residency design and
  supplies redacted live evidence for the exact target.
- Provider adapters must pass the contract conformance tests and run through
  GitHub Actions. Credentials, provisioning, and migration work are out of
  scope for this decision.
- A capability, receipt, retention, legal-hold, residency, or delete-denial
  failure prevents the immutable archive operation from succeeding. It may not
  use R2, a lifecycle rule, or a plain put as a substitute.

## Follow-up

- #1877 — provider selection, provisioning, live evidence, and a concrete
  adapter implementation. It remains `status:needs-evidence` until security,
  compliance, residency, and procurement approve the provider target.

## Evidence

- ADR-0100 — existing R2 Object-Lock capability gate.
- Cloudflare R2 S3 compatibility table:
  https://developers.cloudflare.com/r2/api/s3/api/
- `crates/corelink-audit-chain/src/object_lock_archive.rs` — portable contract,
  R2 failure adapter, and CI conformance fake.
