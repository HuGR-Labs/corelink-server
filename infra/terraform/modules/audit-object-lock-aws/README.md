# `audit-object-lock-aws` — inactive AWS S3 Compliance archive template

This module creates a **new** S3 bucket with Object Lock enabled at creation,
versioning, a Compliance-mode default retention rule, public-access blocks,
and a separate writer role scoped to `audit/*`.

It is intentionally not referenced by an active environment root. Applying it
requires the approvals and authenticated evidence in
[`docs/operator/aws-s3-object-lock-archive-provisioning.md`](../../../../docs/operator/aws-s3-object-lock-archive-provisioning.md).
No credentials, account identifiers, bucket names, or retention values belong
in this module or its documentation examples.

The writer role has no `s3:DeleteObject` permission. A separately controlled
probe identity performs the required pre-expiry delete-denial proof, and the
resulting redacted receipt is required before the application can expose the
`ObjectLockArchiveAdapter` write surface.
