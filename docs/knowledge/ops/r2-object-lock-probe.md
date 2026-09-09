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
Object-Lock operations in the last valid probe (2026-08-25). Cloudflare now
documents a different native **Bucket Lock** feature. It prevents overwrite and
deletion while a rule is configured, but the same configuration authority can
remove that rule through dashboard, Wrangler, or API. It therefore does not
satisfy this backlog item's S3 Compliance/adversarial-administrator property.

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

### Re-probe record (2026-09-08)

An operator opted into the probe against the production-account R2 S3 endpoint
using the local AWS credential profile. `CreateBucket` was rejected with the
provider response `InvalidArgument` (the supplied access-key shape was not a
valid R2 key); the per-object operation was therefore not attempted. The
aggregate result is **INDETERMINATE**, not `BLOCKED`: this was a credential
boundary failure, not an Object-Lock capability response. No probe bucket or
object was created, and no production retention state was touched. The
2026-08-25 `NotImplemented` result remains the latest valid capability
evidence; B-046 stays parked.

### Provider/configuration reassessment (2026-09-09)

Cloudflare's current provider documentation was re-read directly. Native
Bucket Locks support age/date/indefinite rules, apply to existing and new
objects, and take precedence over lifecycle deletion. The same documentation
also gives explicit rule-removal procedures. Consequently:

- native Bucket Lock is real retention protection, but is not S3 Object Lock
  Compliance mode and does not resist the bucket-configuration administrator;
- native Bucket Lock is therefore administrator-removable, not WORM;
- the repository's named `corelink-audit-7y-retention` / `220924800` values are
  desired configuration, not live provider metadata;
- the `audit-witness-production` environment endpoint returned HTTP 404 under
  the current GitHub principal, repository secret APIs exposed names only, and
  the inspected local AWS profiles did not yield a validated R2 key. These
  observations do not prove that no local credential or hidden environment
  exists. They do mean no current bucket-lock rule, account separation, or
  provider retention state was verified in this pass;
- B-046 remains `BLOCKED` for Compliance capability and `INDETERMINATE` for the
  current native lock configuration of `corelink-audit-weur`.

Primary provider sources (observed 2026-09-09):

- https://developers.cloudflare.com/r2/buckets/bucket-locks/
- https://developers.cloudflare.com/r2/buckets/object-lifecycles/
- https://developers.cloudflare.com/r2/reference/consistency/

When a read-only Cloudflare token is available, capture the native rule without
printing the token. This is metadata inspection, not the S3 mutation probe:

```bash
curl --fail-with-body --silent --show-error \
  -H "Authorization: Bearer $CF_API_TOKEN" \
  "https://api.cloudflare.com/client/v4/accounts/$CF_ACCOUNT_ID/r2/buckets/corelink-audit-weur/lock" \
  | jq '{success,errors,result:{rules:[.result.rules[] | {id,enabled,prefix,condition}]}}'
```

Require exactly one enabled rule covering the archive prefix (or the whole
bucket), id `corelink-audit-7y-retention`, age `220924800` seconds or a stricter
date/indefinite condition. Preserve the redacted response and observation time.
An absent/ambiguous rule, HTTP error, or unavailable token is `INDETERMINATE`.
Even a matching response proves only the current native rule, not
administrator-resistant Compliance/WORM.

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
