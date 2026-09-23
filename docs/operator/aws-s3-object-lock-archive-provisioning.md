# AWS S3 Object-Lock archive: proposed path and provisioning gate

## Decision

AWS S3 Object Lock is the proposed backend family for the Compliance archive
path. This is an evidence-backed provider capability choice. A campaign-
authorized, synthetic-only target was provisioned in a redacted AWS account,
region `us-east-1`, with one-day Compliance retention. The account identifier
is redacted from the repository receipt. This is not production approval of an
account, bucket, residency mapping, retention term, legal-hold policy, or
runtime credential. No tenant data was written and no archive route was
enabled. The present Cloudflare R2 archive remains non-WORM and is not changed
by this document or the inactive Terraform module.

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
or override retention. Its legal-hold permission is condition-bound to
setting a hold `ON`; it cannot release a hold. S3 applies the bucket's
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
4. An externally durable audit source that records S3 object data events for
   the target, such as an approved CloudTrail trail or CloudTrail Lake store,
   and the retention/access policy for its receipt.
5. A redacted, authenticated live-probe record for that account and bucket.

The synthetic probe proves provider controls only for that isolated target;
it does not approve production residency, retention, legal-hold operations,
credentials, or route wiring. Its redacted receipt is
[`B-046/aws-s3-object-lock-probe.json`](../../evidence/owner-actions/B-046/aws-s3-object-lock-probe.json).
Consequently, `VerifiedObjectLockArchive::connect` must not be wired into an
archive route. This is a production-approval blocker, not a reason to represent
R2, lifecycle rules, conditional writes, or an in-memory adapter as WORM.

## Required live probe

The campaign-authorized synthetic probe used a temporary, narrowly scoped
delete-probe role and a CloudTrail trail whose S3 data selector covered only
the synthetic target prefix. Account, bucket, and object identifiers are
redacted from the repository receipt. CloudTrail log-file validation is
enabled; the hourly digest containing the probe events was not yet available
when the receipt was captured, so digest-chain validation remains pending. No
production route may rely on this probe as its durable audit source.

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

It must construct `VerifiedObjectLockArchive` before its first put, send the
requested compliance retention and legal-hold settings on the object write,
read both back, verify the expected residency, and attach a durable audit
receipt. It must not mount an immutable archive route or perform a fallback
write if any of those steps fails.

The generated SDK `PutObject` output used here exposes a provider request ID
but no modeled write timestamp. The receipt's S3 request ID is the
provider-issued correlation value. Its `observed_at_unix_ms` field is the
adapter's local clock reading immediately after the successful response; it is
not an S3 event timestamp. Use the provider's durable CloudTrail event and
digest as the external audit record.

Capability negotiation also requires a separate probe client and the exact
key/version of an already locked synthetic object, plus a non-secret reference
to current evidence that the configured probe identity is allowed to call
`s3:DeleteObjectVersion` for that exact object. The adapter rejects a missing
reference or evidence that names a different identity, key, or version. It
then performs the versioned delete and accepts only a structured S3
`AccessDenied` response for that exact version; a missing probe client,
missing version, successful delete, or any other service error fails closed.
Because S3 also uses `AccessDenied` for authorization failures, the caller must
revalidate the referenced permission evidence before constructing the adapter.
Without current effective permission for the same probe identity, bucket, and
object resource, the result is inconclusive and negotiation must remain
closed. The adapter requires an explicit evidence reference but does not
query IAM policy state itself. The archive writer must never receive delete
permission.

The template writer policy grants no delete or retention-override authority.
It permits setting legal hold only to `ON`. The runtime adapter stays
feature-gated and is not wired to a production route until Security,
Compliance, and Legal approve the production target and policy.

## Sources

- [Amazon S3 Object Lock overview](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock-overview.html)
- [Configuring S3 Object Lock](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock-configure.html)
- [Managing Object Lock retention and legal holds](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock-managing.html)
- [ADR-0100 R2 capability gate](../knowledge/adr/adr-0100-r2-object-lock-capability-gate.md)
- [ADR-0101 archive adapter contract](../knowledge/adr/adr-0101-object-lock-archive-adapter-contract.md)
