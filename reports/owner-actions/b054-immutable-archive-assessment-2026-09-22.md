# B-054 immutable archive assessment

**Issue:** [#1795](https://github.com/HuGR-dev/corelink-server/issues/1795)
**Assessment date:** 2026-09-22
**Assessed tree:** `origin/main` at `f9f6eccace9765b9061e71d8d975eee08bffe8fa`

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

**B-046/provider assessment evidence is the sole remainder for #1795:** no
current authenticated, redacted live readback binds an approved provider,
account, and archive target to the data and manifest configuration and proves
conditional create, exact byte readback, ordinary overwrite/delete denial,
retention, and listing/fetch/loss behavior. R2's official capability page
marks S3 Object Locking unsupported, and the AWS path is an inactive template
with no approved target. Until that evidence exists, #1795 must remain open;
the repository must not claim an immutable or WORM archive.

## Source pointers

- `crates/corelink-container/src/routes/audit_archive.rs`: archive bucket, conditional write, manifest key, and exact authentication boundary.
- `.github/workflows/audit-chain-daily-verify.yml`: list/fetch/checkpoint workflow and retained configuration note.
- `BACKLOG.md`: B-015 historical archive prose, B-046 provider capability blocker, and B-054 keyed-epoch evidence boundary.
- `evidence/owner-actions/B-054/keyed-audit-epoch-rollout.json`: retained B-054 dependency statuses.
- [Cloudflare R2 S3 API compatibility](https://developers.cloudflare.com/r2/api/s3/api/): current Object Locking capability statement.
