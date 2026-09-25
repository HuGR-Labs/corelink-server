# B-054 immutable archive assessment

**Issue:** [#1795](https://github.com/HuGR-dev/corelink-server/issues/1795)
**Original assessment date:** 2026-09-22
**Original assessed tree:** `origin/main` at `f9f6eccace9765b9061e71d8d975eee08bffe8fa`

## Reconciliation update — 2026-09-25

This update supersedes the original assessment below wherever it says that no
current provider receipt exists. The latest redacted [B-046 provider receipt](https://github.com/HuGR-dev/corelink-server/issues/1646#issuecomment-5827683238)
records a bounded AWS S3 observation on 2026-09-25 at 06:02:53Z: account suffix
`8582`, region `us-east-1`, and a dedicated non-production synthetic-only
Terraform-managed bucket (bucket SHA-256
`13778af13b943e855dc62c5f36b904cbd9fcf08a02361038810cb0f08284ca73`). Readbacks
reported versioning and Object Lock enabled, default one-day Compliance
retention, AES256 encryption, and public-access blocks enabled. The receipt says
the bucket has no lifecycle deletion policy or bucket policy. It records one
synthetic no-customer-data object under `audit/`, with object-key SHA-256
`45c606b96cfffd5c4b8d0b8833c58e2c352444eaffee4d559b173c721e6bb5d3`, version
SHA-256 `9dcc19961c719f04d81118e9262d038c6fc3db5f87b86fadef781a06fc1abab5`,
Compliance retention read back through `2026-09-26T06:02:42Z`, and legal hold
`ON`. IAM simulation for the exact CLI principal/resource/action returned
`allowed`; the same-version pre-expiry delete request was denied by S3 Object
Lock. The linked trail digest validation exited successfully and reported
provider/digest delivery times.

This is observed provider behavior for one synthetic object, not an approved
CoreLink archive deployment or an accepted protected workflow receipt. The
receipt does not establish the approved residency/jurisdiction mapping, the
complete data and manifest writer/reader inventory, content-addressed
conditional-create behavior, byte-for-byte fetch/readback, list/fetch and
overwrite denial, loss/recovery behavior, signed-manifest retention/legal-hold
behavior, or exact CloudTrail data-event identifiers. The receipt itself says
the fresh S3 data-event records were absent after one bounded delivery check;
therefore it claims no event identifier. No provider object was deleted and no
retention was shortened.

The #2237 [protected OIDC action packet](https://github.com/HuGR-dev/corelink-server/issues/2237#issuecomment-5828074197)
records that the `s3-object-lock-live-proof` environment is absent and the
canonical workflow has zero runs. Security/Compliance must approve and configure
the fixed account, region/jurisdiction, OIDC role, bucket prefix, active trail
and CloudTrail Lake Event Data Store, approval reference, cost ceiling, and
named cost/cleanup owners. Only then can one protected `main` probe bind the
provider observations and both data-event IDs/digest validation to the exact
workflow run and object versions. Keep the protected version intact; cleanup
is expiry-safe and owner-operated. Reconcile any accepted receipts with
[B-046/#1646](https://github.com/HuGR-dev/corelink-server/issues/1646),
[#1877](https://github.com/HuGR-dev/corelink-server/issues/1877), and parent
[#1647](https://github.com/HuGR-dev/corelink-server/issues/1647).

Disposition remains **OPEN / INDETERMINATE** for #1795. The existing receipt
improves the provider-behavior record, but it does not establish completeness
or immutability for CoreLink's archive objects and signed manifests.

## Decision

This is a source and retained-record assessment. It is not a provider probe,
an approval receipt, or proof that an archive target is immutable. The source
defines a Cloudflare R2 path and fail-closed archive checks, but the repository
does not contain current authenticated, redacted evidence binding an approved
provider account and target to the required object behavior.

The B-054 invariant therefore remains **INDETERMINATE**: a missing, stale,
overwritten, unreadable, or unverifiable object or signed manifest cannot be
treated as archive completeness. D1 index rows and a provider or bucket name
are not substitutes for the object and manifest evidence.

## Criterion map

| #1795 requirement | What the assessed tree defines or records | Evidence status |
| --- | --- | --- |
| Provider, bucket, account, prefixes | The implementation and daily verifier name Cloudflare R2 through its S3/API-v4 paths and default bucket `corelink-audit-weur`. Chunk keys are shaped as `audit/<YYYY>/<MM>/<DD>/<tenant>/<region>/...`; signed manifests use `audit/manifests/<tenant>/<region>/epoch-<8-digits>/<manifest-hash>.jcs.json`; verifier checkpoints use `audit-checkpoints/<YYYY>/<MM>/<DD>/partitions.ndjson`. The account identifier is supplied through protected configuration and is intentionally absent here. | **Source configuration only.** No retained authenticated readback proves the exact approved account, bucket ownership, or live prefixes. |
| Retention configuration | Source comments record a 2026-09-13 observation of a native R2 Bucket Lock rule named `corelink-audit-7y-retention` with `2557` days (`220924800` seconds). | **Historical/configuration observation only.** This is administrator-removable native Bucket Lock/lifecycle configuration. It is not S3 Object Lock Compliance/WORM, and it is not a current live receipt. |
| Writer and readers for data and manifests | The archive route writes objects with a conditional create and performs exact readback. The daily verifier lists and fetches objects through the Cloudflare API-v4 path. D1 indexes manifest metadata; the signed manifest object remains the authority. Checkpoint writes use a separate write-only token and `If-None-Match: *`. | **Code/configuration only.** No current redacted custody or access receipt identifies the approved production writer and reader identities. |
| Content-addressed conditional create | The route derives manifest object keys from tenant, region, epoch, and manifest hash; chunk and checkpoint paths use conditional creation. | **Repository control present; live behavior unproved.** No retained authenticated probe records the successful create and duplicate-create response for the approved target. |
| Byte-for-byte readback | The route and verifier contain exact object readback and signed-manifest authentication paths. | **Repository control present; live behavior unproved.** No current object and manifest bytes, hashes, and readback receipt are retained. |
| Ordinary overwrite and delete denial | The source documents the conditional write boundary and the R2 limitation. Cloudflare's current API capability documentation says bucket-level and object-level S3 Object Locking are unsupported. | **Not proven.** There is no current authenticated overwrite/delete denial probe for the target; native R2 Bucket Lock is removable by an authorized administrator. |
| Listing, fetch, overwrite, retention, and loss behavior | The daily verifier specifies paginated listing and per-object GET. The historical B-015 record describes ten chunks read back on 2026-08-24, but it does not cover the complete current object/manifest, overwrite/delete, retention, and loss matrix. | **Incomplete and stale for closure.** A workflow definition or historical prose cannot stand in for a current retained run artifact. |
| B-046 linkage | [B-046/#1646](https://github.com/HuGR-dev/corelink-server/issues/1646) remains parked pending a current provider capability probe and approved Compliance/Object-Lock evidence. The direct R2 probe recorded there returned `NotImplemented`; the issue explicitly prohibits a Compliance claim. | **Blocked.** [#1877](https://github.com/HuGR-dev/corelink-server/issues/1877) still requires provider, account, residency, procurement, IAM, and live conformance evidence. |
| F-001 / #1647 linkage | The retained B-054 keyed-epoch record marks the external witness, D1 binding, archive proof, signing-key custody, and rotation as blocked or not executed. It records contract checks only; it contains no provider archive receipt. | **Blocked.** The local contract gate is not production archive evidence. |

## Evidence boundary and rejected paths

- [PR #1883](https://github.com/HuGR-dev/corelink-server/pull/1883) merged the provider-neutral Object Lock adapter seam. It selected and provisioned no provider.
- [PR #1895](https://github.com/HuGR-dev/corelink-server/pull/1895) merged an inactive AWS S3 Object Lock Terraform template. It explicitly has no approved account, target, deployment, runtime wiring, or live receipt.
- The current repository contract checks prove fail-closed code and configuration boundaries. They do not contact a provider or establish production retention, custody, or loss behavior.
- The historical B-015 wording that calls the R2 retention rule an “Object Lock rule” must not be read as an S3 Object Lock or WORM claim. The current B-046 record and the source comments identify it as native, administrator-removable R2 Bucket Lock/lifecycle configuration.

## Exact blocker

**The remaining #1795 gate is the approved protected archive proof:** the
2026-09-25 synthetic AWS receipt proves only the bounded behavior listed in the
reconciliation update. It is not bound to a configured protected OIDC run and
does not cover CoreLink's data and signed-manifest operations. The configured
R2 probe remains `INDETERMINATE` and Cloudflare's authority page still marks
the required R2 Object Lock APIs unsupported. Keep the AWS adapter inactive,
production routing unwired, and #1795 **OPEN / INDETERMINATE** until the named
approvers configure the protected target, the canonical run supplies the full
target-bound receipt, and the same accepted evidence is reconciled across
B-046/#1646, #1877, and #1647. Do not claim CoreLink's archive is immutable or
WORM on the basis of the one-object AWS receipt.

## Source pointers

- `crates/corelink-container/src/routes/audit_archive.rs`: archive bucket, conditional write, manifest key, and exact authentication boundary.
- `.github/workflows/audit-chain-daily-verify.yml`: list/fetch/checkpoint workflow and retained configuration note.
- `BACKLOG.md`: B-015 historical archive prose, B-046 provider capability blocker, and B-054 keyed-epoch evidence boundary.
- `evidence/owner-actions/B-054/keyed-audit-epoch-rollout.json`: retained B-054 dependency statuses.
- [Cloudflare R2 S3 API compatibility](https://developers.cloudflare.com/r2/api/s3/api/): current Object Locking capability statement.
