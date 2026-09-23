# AWS S3 Object-Lock archive: proposed path and provisioning gate

## Decision

AWS S3 Object Lock is the proposed backend family for the Compliance archive
path. This is an evidence-backed provider capability choice, not approval of an
AWS account, bucket, residency mapping, retention term, legal-hold policy, or
runtime credential. The present Cloudflare R2 archive remains non-WORM and is
not changed by this document or the inactive Terraform module.

Amazon S3 requires Object Lock to be enabled when a bucket is created. Its
Compliance retention mode prevents deletion or overwrite before the retention
date, and S3 provides object-retention and legal-hold read APIs. Those facts
make S3 suitable to evaluate against `ObjectLockArchiveAdapter`; they do not
prove any CoreLink target has the contract.

## Repository-owned implementation

`infra/terraform/modules/audit-object-lock-aws` is the inactive provisioning
template. It creates a new S3 bucket with Object Lock enabled, versioning,
Compliance default retention, public-access blocks, server-side encryption,
and a dedicated writer role. The writer role can read Object Lock state and
write objects under `audit/*`; it cannot read object payloads, delete objects,
override retention, or set/release legal holds. S3 applies the bucket's
Compliance default retention to writer puts.

The module is not called from an environment root. It must stay inactive until
the prerequisites below are approved. Terraform apply remains manual and
subject to the repository's existing plan and dual-approval procedure.

## External blocker

Security, Compliance, residency, and procurement must approve all of the
following for one exact target before an immutable archive route can be
enabled:

1. The AWS account, S3 region, jurisdiction-to-region mapping, and archive
   bucket name.
2. The Compliance retention term, legal-hold process and owners, including
   the conditions under which a hold can be released.
3. Separate writer, provisioning, break-glass, and live-probe identities. The
   writer has no delete permission; no principal used by the application may
   bypass the retention control.
4. An externally durable audit source that records S3 **data events** for the
   target, such as an approved CloudTrail Lake event-data store and a delivered
   CloudTrail trail with digest validation. Management-event history is not a
   substitute for S3 data-event evidence.
5. A redacted, authenticated live-probe record for that account and bucket.

The repository has no approved target, credentials, or durable audit receipt
source today. Consequently it cannot truthfully report any complete
`ObjectLockArchiveAdapter` capability set, and
`VerifiedObjectLockArchive::connect` must not be wired into an archive route.
This is a provisioning blocker, not a reason to fall back to R2, lifecycle
rules, conditional writes, or an in-memory adapter.

## Required live probe

Run the following operations against the exact new target with an approved
short-lived probe identity. Preserve only redacted output and immutable
request/event identifiers in the evidence record.

1. Read the bucket Object Lock configuration and verify the configured default
   mode is `COMPLIANCE` and retention is the approved value.
2. Put a unique test object with `COMPLIANCE` retention and legal hold enabled.
3. Read back the object's retain-until timestamp and legal-hold state; both
   must exactly match the request.
4. Attempt deletion before expiry using the separately approved probe identity.
   Record S3's retention or legal-hold denial. A successful deletion fails the
   gate.
5. Obtain the corresponding durable provider audit record and bind its event
   identifier to the object key and S3 request/version identifiers.
6. Record the bucket region and approved jurisdiction mapping, then verify it
   matches the adapter request's `ArchiveResidency` value.

Any missing configuration, authorization failure, incomplete readback,
unavailable audit receipt, or deletion success is a failed negotiation before
production archival. Do not write production archive data while the probe is
incomplete.

## Runtime handoff after approval

The eventual AWS S3 implementation must receive configuration only from the
approved deployment secret/configuration system: the exact bucket, AWS region,
jurisdiction, location class, approved retention/hold policy identifier, and
redacted evidence reference. It must use a short-lived workload identity for
the dedicated writer role; do not commit access keys, session tokens, account
IDs, bucket names, or live receipt values.

It must construct `VerifiedObjectLockArchive` before its first put, append and
verify durable audit intent before the irreversible put, then record the exact
success or failure outcome. It must send requested Compliance retention and
legal-hold settings on the object write; read both back against the exact S3
version; verify the expected tenant, bucket, account, and residency; and bind
the durable receipt to that version. It must not mount an immutable archive
route or perform a fallback write if any of those steps fails.

The inactive writer policy permits per-object retention and legal-hold headers
only with IAM conditions for `COMPLIANCE`, an active legal hold, and at least
the approved retention term. It has no delete, bypass, or hold-release
authority. Do not widen it directly; the live probe must validate the approved
probe/cleanup policy before the route is enabled.

## Migration and cleanup boundary

`migrations/d1/0144_cas_retention_compliance_metadata.sql` rebuilds the D1/
SQLite `cas_retention` table because the deployed inline mode check cannot be
widened in place. Existing Governance rows, primary key, and tenant indexes
are preserved. Governance remains the R2-only reversible legal-hold path. A
Compliance row includes provider target, exact version, and evidence metadata;
it cannot be selected by a Governance release or drain. The migration runner
ledger applies the numbered rebuild once. Replayed Governance writes retain
their original `INSERT OR IGNORE` behavior.

Rollback after a Compliance row exists is one-way: do not drop its metadata,
shorten retention, delete the S3 version, or route it to R2. Keep the widened
schema and apply a forward repair.

`.github/workflows/aws-s3-object-lock-live-proof.yml` is manual-only and runs
only from protected canonical `main` in the approved environment. It accepts
no target or role inputs. It uses GitHub OIDC, a new synthetic bucket, an STS
identity bound to an effective delete-permission observation, CloudTrail Lake
S3 data events, and CloudTrail digest validation. Cleanup derives the exact
synthetic bucket from the source run, waits for retention expiry, turns the
legal hold off only then, and removes only that version and bucket. A failed or
delayed cleanup remains an owned obligation; no bypass or retention shortening
is permitted.

## Sources

- [Amazon S3 Object Lock overview](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock-overview.html)
- [Configuring S3 Object Lock](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock-configure.html)
- [Managing Object Lock retention and legal holds](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock-managing.html)
- [ADR-0100 R2 capability gate](../knowledge/adr/adr-0100-r2-object-lock-capability-gate.md)
- [ADR-0101 archive adapter contract](../knowledge/adr/adr-0101-object-lock-archive-adapter-contract.md)
