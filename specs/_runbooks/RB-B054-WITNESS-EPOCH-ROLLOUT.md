---
id: "RB-B054-WITNESS-EPOCH-ROLLOUT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-09-09"
updated: "2026-09-22"
owner: "Security + SRE"
final_approver: "Security Lead"
reviewers: []
supersedes: null
superseded_by: null
tags: ["b-054", "audit-chain", "witness", "epoch", "rollout"]
---

# B-054 independent witness and epoch rollout

This runbook is the operator handoff for the post-merge B-054 disposable
rollout. It does not authorize production expansion. The governing decision is
[ADR-0073](../03_architecture/adrs/ADR-0073-independent-audit-head-witness.md):
the witness must run in a Security-administered Cloudflare account distinct
from the account holding CoreLink production D1.

Never place a private seed, append token, link key, API token, or internal-auth
value in a command transcript, issue, pull request, artifact, D1, R2, or this
document. Commands below name shell variables only. Use a no-echo protected
shell or the protected GitHub environment for their values.

## 1. Stop conditions

Stop fail-closed and open an incident if any of the following is true:

- the reviewed SHA is not the current `main` SHA;
- the witness and CoreLink Cloudflare account ids are equal;
- the custom domain is not a lowercase DNS hostname owned by the independent
  Security account, or a `workers.dev` fallback is proposed;
- a public-key registry is malformed, incomplete, or not authorized through
  the documented out-of-band ceremony;
- the disposable partition is not exclusively disposable, has an unexpected
  head, epoch, sealed row, lease, registry, or witness latest;
- any endpoint returns `indeterminate`, any challenged latest differs from D1,
  or an expected idempotent retry creates a second witness successor;
- `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME` is enabled during bootstrap, transition,
  restart, archive, or recovery evidence;
- an operator proposes deleting/rewinding witness, ledger, epoch, registry, or
  sealed-row state as rollback.

Rollback after E1 is forward-only: create a newly witnessed successor epoch.

## 2. Exact witness deployment inputs

The canonical deployment path is
`.github/workflows/audit-witness-deploy.yml`, manually dispatched against
`main`. Its protected GitHub environment is `audit-witness-production`.

| Input/binding | Location | Required contract |
|---|---|---|
| `expected_sha` | workflow dispatch input | Full reviewed SHA; must equal the checked-out and fetched `main` SHA. |
| `AUDIT_WITNESS_CF_ACCOUNT_ID` | protected environment secret | Security-owned account id; must differ from `CF_ACCOUNT_ID`. |
| `CF_ACCOUNT_ID` | protected environment secret | CoreLink production account id, used only by the inequality gate. |
| `AUDIT_WITNESS_CF_API_TOKEN` | protected environment secret | Least-privilege deploy credential scoped only to the witness account. |
| `AUDIT_WITNESS_DOMAIN` | protected environment variable | Lowercase DNS hostname; no scheme, path, port, or trailing slash. |
| `WITNESS_APPEND_TOKEN` | witness Worker secret | At least 32 characters; mirrored to the drain as `AUDIT_WITNESS_APPEND_TOKEN`. |
| `WITNESS_RECEIPT_SIGNING_SEED_HEX` | witness Worker secret | Exactly 32 bytes encoded as 64 lowercase hex characters; never mirrored to CoreLink. |
| `WITNESS_ID` | witness Worker var | Immutable canonical identity. The reviewed config currently declares `corelink-security-witness-1`. |
| `WITNESS_RECEIPT_KEY_ID` | witness Worker var | Positive monotonic decimal id. Increment atomically with receipt-key rotation. |

Provision the two witness secrets from a protected shell in the Security
account before dispatch. These commands write secret values to Wrangler via
stdin and do not deploy:

```bash
cd apps/audit-witness-worker
printf '%s' "$WITNESS_APPEND_TOKEN" | \
  CLOUDFLARE_ACCOUNT_ID="$AUDIT_WITNESS_CF_ACCOUNT_ID" \
  CLOUDFLARE_API_TOKEN="$AUDIT_WITNESS_CF_API_TOKEN" \
  pnpm exec wrangler secret put WITNESS_APPEND_TOKEN --config wrangler.toml
printf '%s' "$WITNESS_RECEIPT_SIGNING_SEED_HEX" | \
  CLOUDFLARE_ACCOUNT_ID="$AUDIT_WITNESS_CF_ACCOUNT_ID" \
  CLOUDFLARE_API_TOKEN="$AUDIT_WITNESS_CF_API_TOKEN" \
  pnpm exec wrangler secret put WITNESS_RECEIPT_SIGNING_SEED_HEX --config wrangler.toml
```

Review without exposing values:

```bash
cd apps/audit-witness-worker
CLOUDFLARE_ACCOUNT_ID="$AUDIT_WITNESS_CF_ACCOUNT_ID" \
CLOUDFLARE_API_TOKEN="$AUDIT_WITNESS_CF_API_TOKEN" \
pnpm exec wrangler secret list --config wrangler.toml | jq -r '.[].name' | sort
```

The expected names are exactly `WITNESS_APPEND_TOKEN` and
`WITNESS_RECEIPT_SIGNING_SEED_HEX`. Dispatch the deploy only after the reviewed
commit is on `main`:

```bash
REVIEWED_MAIN_SHA="$(git rev-parse origin/main)"
test "$REVIEWED_MAIN_SHA" = "$(git rev-parse main)"
gh workflow run audit-witness-deploy.yml \
  --ref main \
  -f expected_sha="$REVIEWED_MAIN_SHA"
```

The workflow runs typecheck, unit tests, real-workerd concurrency tests, the
independent-account gate, the secret-name gate, and finally the exact deploy:

```bash
pnpm --filter @corelink/audit-witness-worker exec wrangler deploy \
  --config wrangler.toml \
  --domain "$AUDIT_WITNESS_DOMAIN"
```

The last command is documentary: do not run it outside the protected workflow.
Record the workflow URL, run id, reviewed SHA, deployment id, account-id hash or
approved redaction, custom domain, `WITNESS_ID`, and receipt key id. Do not
record either secret value.

## 3. CoreLink-side pinned inputs

Before starting a new disposable container, bind the following on the main
Worker/container path. Existing containers must be deliberately recycled after
bindings change because the Durable Object forwards them only at container
start.

| Binding | Contract |
|---|---|
| `AUDIT_WITNESS_URL` | Exact `https://host` origin matching `AUDIT_WITNESS_DOMAIN`; no path, query, userinfo, IP literal, redirect, proxy, non-default port, or trailing slash. |
| `AUDIT_WITNESS_APPEND_TOKEN` | Secret byte-for-byte equal to `WITNESS_APPEND_TOKEN`. |
| `AUDIT_WITNESS_ID` | Exact immutable `WITNESS_ID`. |
| `AUDIT_WITNESS_PUBLIC_KEYS_JSON` | Compact JSON object from positive decimal witness receipt-key ids to canonical padded-base64 32-byte Ed25519 public keys. Retain historical keys for the evidence-retention period. |
| `AUDIT_CHAIN_TRUST_ROOT_PUBLIC_KEYS_JSON` | Compact JSON object from immutable root ids to canonical padded-base64 32-byte Ed25519 public keys. Public material only. |
| `AUDIT_CHAIN_SIGNING_SEED_HEX` / `AUDIT_CHAIN_SIGNING_KEY_ID` | Current head-signing seed and matching positive id, or the documented erasure-attestation fallback. Never let a dedicated empty value mask the fallback. |
| `AUDIT_CHAIN_LINK_KEYS_JSON` | Secret JSON object from positive decimal link-key ids to 64-lowercase-hex 32-byte keys. Historical keys required by retention remain present. |
| `AUDIT_DRAIN_LEASE_ENABLED` | Exactly `1`/`true`/`TRUE`; B-054 bootstrap and transition otherwise reject. |
| `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME` | Unset. It is prohibited for this rollout. |
| `CORELINK_ERASE_AUTH_KEY` | Dedicated internal authorization value, at least 32 characters; it gates drain, epoch-admin, and archive. |
| `CORELINK_ADMIN_APPROVER_AUTH_KEY` | Dedicated Security-approver value, at least 32 characters and byte-distinct from `CORELINK_ERASE_AUTH_KEY`; epoch-admin is unavailable without it. |

The authoritative acquisition, custody, and rotation rules are rows 164–165,
182, 245–257 of `docs/internal/secrets-checklist.md`. Confirm only binding names
and non-secret public values in evidence.

## 4. Trust-root and registry artifacts

### 4.1 Signing-key registry prepared offline

Security constructs exact RFC-8785/JCS bytes with no unknown fields:

```text
{"algorithm":"ed25519-v1","key_type":"audit-chain-head-signing","public_key_b64":"<PADDED_BASE64_32_BYTE_SIGNING_PUBLIC_KEY>","registered_at_ms":<CEREMONY_TIMESTAMP_MS>,"registry_type":"audit-chain-signing-key-registry","registry_version":1,"signing_key_id":<POSITIVE_SIGNING_KEY_ID>,"trust_root_key_id":"<OOB_ROOT_ID>"}
```

Angle-bracket fields are inputs, not fabricated rollout values. Use a
ceremony-approved non-negative timestamp and positive signing-key id.
The selected offline trust-root private key signs the exact JCS bytes directly.
The resulting signature is canonical padded base64 of 64 Ed25519 bytes. The
private root never enters either Cloudflare account.

Required ceremony outputs:

- exact JCS bytes and their SHA-256 or BLAKE3 evidence digest;
- `registry_jcs_b64`, the canonical padded base64 of those exact bytes;
- `registry_signature_b64`;
- the root id and public key included in
  `AUDIT_CHAIN_TRUST_ROOT_PUBLIC_KEYS_JSON`;
- independent confirmation that the registry public key is derived from the
  currently configured audit-chain signing seed and id.

### 4.2 Link-key registry prepared by the runtime

The operator does not send the link key or a signed link artifact to the API.
The runtime selects `link_key_id` from `AUDIT_CHAIN_LINK_KEYS_JSON`, computes
the domain-separated commitment, constructs exact JCS, signs it with the
current audit-chain signing seed, writes it append-only, and reads it back.
The request supplies only a positive unused id and the ceremony timestamp.

The resulting public registry shape is:

```text
{"algorithm_id":1,"key_commitment_hex":"<64_LOWERCASE_HEX_COMMITMENT>","key_type":"audit-chain-link","link_key_id":<POSITIVE_LINK_KEY_ID>,"registered_at_ms":<CEREMONY_TIMESTAMP_MS>,"registry_type":"audit-chain-link-key-registry","registry_version":1,"signing_key_id":<POSITIVE_SIGNING_KEY_ID>}
```

Never compute or print the commitment in a shared transcript: even though it is
not reversible, it is an identifier for secret key material and belongs in the
authenticated registry/evidence set.

## 5. Exact epoch-admin requests

Set these local variables in a protected shell. `CORELINK_INTERNAL_ORIGIN` is
the directly reachable container origin, not the public witness origin.
`B054_ADMIN_TOKEN` is the SRE executor principal's current
`CORELINK_ERASE_AUTH_KEY`. `B054_SECURITY_APPROVAL_TOKEN` is the Security
approver principal's `CORELINK_ADMIN_APPROVER_AUTH_KEY`. They must be distinct,
separately held, and never logged. Epoch-admin requires both
`x-corelink-internal-auth` and `x-corelink-security-approval`; neither is an
`Authorization` bearer header.

```bash
export CORELINK_INTERNAL_ORIGIN='<DIRECT_CONTAINER_ORIGIN>'
export B054_ADMIN_TOKEN='<FROM_PROTECTED_STORE>'
export B054_SECURITY_APPROVAL_TOKEN='<FROM_SEPARATE_SECURITY_STORE>'
export DISPOSABLE_TENANT_ID='<CANONICAL_UUIDV7>'
export DISPOSABLE_REGION='<CANONICAL_REGION>'
```

The runtime accepts canonical regions supported by the audit implementation;
use the partition's stored canonical spelling and do not case-normalize it in
the runbook. Submit exact JSON with `jq` so shell interpolation cannot alter
types.

Provision the authenticated signing registry:

```bash
jq -cn \
  --arg jcs "$REGISTRY_JCS_B64" \
  --arg sig "$REGISTRY_SIGNATURE_B64" \
  '{operation:"provision_signing_key",registry_jcs_b64:$jcs,registry_signature_b64:$sig}' | \
curl --fail-with-body --silent --show-error \
  -H "x-corelink-internal-auth: $B054_ADMIN_TOKEN" \
  -H "x-corelink-security-approval: $B054_SECURITY_APPROVAL_TOKEN" \
  -H 'Content-Type: application/json' \
  --data-binary @- \
  "$CORELINK_INTERNAL_ORIGIN/_internal/audit/epoch-admin"
```

Provision the link-key registry:

```bash
jq -cn \
  --argjson link_key_id "$LINK_KEY_ID" \
  --argjson registered_at_ms "$LINK_REGISTERED_AT_MS" \
  '{operation:"provision_link_key",link_key_id:$link_key_id,registered_at_ms:$registered_at_ms}' | \
curl --fail-with-body --silent --show-error \
  -H "x-corelink-internal-auth: $B054_ADMIN_TOKEN" \
  -H "x-corelink-security-approval: $B054_SECURITY_APPROVAL_TOKEN" \
  -H 'Content-Type: application/json' \
  --data-binary @- \
  "$CORELINK_INTERNAL_ORIGIN/_internal/audit/epoch-admin"
```

Bootstrap E0 after a read-only preflight of the complete legacy prefix:

```bash
jq -cn \
  --arg tenant_id "$DISPOSABLE_TENANT_ID" \
  --arg region "$DISPOSABLE_REGION" \
  '{operation:"bootstrap_e0",tenant_id:$tenant_id,region:$region}' | \
curl --fail-with-body --silent --show-error \
  -H "x-corelink-internal-auth: $B054_ADMIN_TOKEN" \
  -H "x-corelink-security-approval: $B054_SECURITY_APPROVAL_TOKEN" \
  -H 'Content-Type: application/json' \
  --data-binary @- \
  "$CORELINK_INTERNAL_ORIGIN/_internal/audit/epoch-admin"
```

The current 0109 schema cannot represent empty E0 and E1 with the same start
sequence. After E0 bootstrap, emit and drain at least one genuine disposable E0
audit event and prove the witnessed D1 head has `next_sequence > 0` before
transitioning:

```bash
jq -cn \
  --arg tenant_id "$DISPOSABLE_TENANT_ID" \
  --arg region "$DISPOSABLE_REGION" \
  --argjson link_key_id "$LINK_KEY_ID" \
  '{operation:"transition_e1",tenant_id:$tenant_id,region:$region,link_key_id:$link_key_id}' | \
curl --fail-with-body --silent --show-error \
  -H "x-corelink-internal-auth: $B054_ADMIN_TOKEN" \
  -H "x-corelink-security-approval: $B054_SECURITY_APPROVAL_TOKEN" \
  -H 'Content-Type: application/json' \
  --data-binary @- \
  "$CORELINK_INTERNAL_ORIGIN/_internal/audit/epoch-admin"
```

Successful responses are exact operation/result objects with result
`provisioned`, `committed`, or `already_committed`. Any non-2xx,
`indeterminate`, unexpected operation, or unexpected result is a stop condition.

## 6. Disposable evidence checklist

Create one immutable evidence bundle directory named with UTC timestamp,
reviewed SHA, disposable tenant id, and region. Store sanitized command lines,
HTTP status/body, D1 query exports, challenged witness responses, R2 metadata,
workflow URLs, and reviewer signatures. Store no bearer token, private seed,
link key, API token, or unredacted process environment.

### 6.1 Preflight and E0 bootstrap

- [ ] Record reviewed `main` SHA and prove both deployed Workers report that
  deployment, without logging credentials.
- [ ] Record the independent-account inequality approval and custom-domain TLS
  evidence.
- [ ] Confirm `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME` is unset, lease mode is on,
  and all pinned public identifiers match the ceremony.
- [ ] Confirm the disposable partition's legacy head signature and complete
  sequence `0..next_sequence-1`; record row count and hashes, not payload secrets.
- [ ] Confirm signed registry readback is byte-identical and an exact retry
  returns `provisioned` without adding a row.
- [ ] Confirm witness challenged latest is signed `latest:null` before genesis.
- [ ] Run `bootstrap_e0`; retain the HTTP evidence and then independently query
  D1 for exactly one active E0, ledger sequence 0, a v2 signed head, and witness
  receipt sequence 0.
- [ ] Challenge witness latest again and prove exact equality with the D1 head
  message/signature and stored receipt.
- [ ] Retry the identical bootstrap; require `already_committed`, unchanged D1
  row counts, and unchanged witness latest/hash/sequence.

### 6.2 E0 advance and E0 to E1

- [ ] Emit one genuine disposable E0 event through its normal product path;
  invoke `POST /_internal/audit/drain` with the same internal-auth header until its
  response says `incomplete:false` for the relevant work.
- [ ] Prove `next_sequence > 0`, the E0 row uses algorithm 0/no link key, and D1
  exactly matches challenged witness latest.
- [ ] Provision the chosen link-key id; independently verify its registry
  signature and configured-key commitment without exposing the key.
- [ ] Run `transition_e1`; require one closed E0 and one active E1 with identical
  predecessor boundary, contiguous ledger sequences 0 then 1, and a new signed
  witness successor.
- [ ] Retry the exact transition; require `already_committed`, no new epoch,
  ledger, receipt, or witness successor, and no changed bytes.
- [ ] Emit and drain at least one genuine E1 event; prove algorithm 1 and the
  registered `link_key_id` are selected and the witnessed head advances once.

### 6.3 Restart

- [ ] Capture the approved disposable-container restart command and platform
  event. Do not restart unrelated tenants or the witness.
- [ ] After restart, prove forwarded bindings contain the expected names; never
  dump their values.
- [ ] Challenge witness latest before any new write and prove the restarted
  container's D1 head/ledger/registry view is exact.
- [ ] Drain with no pending work and prove no successor is appended.
- [ ] Emit one new disposable event, drain it, and prove exactly one contiguous
  D1 and witness successor under active E1.

### 6.4 Commit-unknown and forward-only recovery

- [ ] Record both exact authorization principals before inducing recovery:
  `audit-epoch-sre-executor` presents the SRE-held `CORELINK_ERASE_AUTH_KEY` in
  `x-corelink-internal-auth`; `audit-epoch-security-approver` presents the
  separately held `CORELINK_ADMIN_APPROVER_AUTH_KEY` in
  `x-corelink-security-approval`. The runtime rejects an absent, short, or
  byte-equal approver credential. Attach immutable references naming the two
  distinct human custodians; distinct bytes held by one person are not
  two-person control.
- [ ] In the disposable environment only, inject loss of the client response
  after the witness has durably appended but before/while D1 completion. Never
  mutate witness storage to manufacture the condition.
- [ ] Freeze the partition. With a fresh 32-byte challenge, prove whether signed
  latest is the exact prepared candidate or the exact predecessor.
- [ ] Prove D1 is still the exact predecessor before any retry. If D1 or witness
  is divergent, stop; do not reconcile automatically.
- [ ] Retry only the identical drain/admin operation. Require completion of the
  original exact D1 CAS or an idempotent already-committed result, never a newly
  prepared successor.
- [ ] Prove the final D1 head, ledger, receipt index, and challenged witness
  latest are byte-for-byte bound, with no gap, duplicate sequence, rewind, or
  second successor.

### 6.5 Archive and independent verification

The keyed archive implementation authenticates and prepares the complete
publication before its first R2 request. For each E1 segment it performs this
ordered boundary:

1. authenticate the current signing-key registry against the out-of-band trust
   root and require it to match the configured manifest signer;
2. authenticate the selected link-key registry with the root-authorized key
   named by its target epoch, then
   require the configured link key to match its domain-separated commitment;
3. load and verify the exact signed epoch ledger contiguously from genesis
   through the target epoch, resolving every entry's historical
   `signing_key_id` against the out-of-band roots and checking every D1 index
   field, previous-ledger hash and signature;
4. obtain a fresh signed witness latest, load and verify every stored receipt
   contiguously from witness genesis through that latest, authenticate each
   embedded audit head with its root-authorized historical signing key, and
   select the greatest authenticated boundary covered by the loaded epoch
   segment;
5. verify every selected keyed chain row, prepare the bounded data objects,
   build/sign the RFC-8785 manifest, and self-authenticate the manifest against
   the exact ledger, witness, object bytes/ranges/hashes and epoch key before
   any publication;
6. conditionally create every data object and immediately GET/compare its exact
   bytes under a bounded read ceiling;
7. conditionally create and exact-readback the content-addressed signed
   manifest envelope **last**; and
8. only then run one D1 batch that inserts/reuses the exact manifest index,
   marks only the exact selected rows `archived_at`, asserts every manifest and
   immutable row fact, and rolls the entire D1 batch back unless the exact CAS
   assertion succeeds.

The manifest object key is
`audit/manifests/<tenant>/<region>/epoch-<8-digit-epoch>/<manifest-hash>.jcs.json`.
Data objects always precede it. A data-write failure therefore cannot publish a
manifest. A manifest-write or D1-CAS failure may leave content-addressed data
objects in R2, but leaves D1 rows unarchived; an exact retry must reuse and
read-verify those bytes. No path overwrites an object or treats an HTTP PUT
acknowledgement without exact GET readback as evidence.

Invoke the archive sweep with the same dedicated internal credential:

```bash
curl --fail-with-body --silent --show-error \
  -X POST \
  -H "x-corelink-internal-auth: $B054_ADMIN_TOKEN" \
  "$CORELINK_INTERNAL_ORIGIN/_internal/audit/archive"
```

- [ ] Before invocation, record the exact disposable E1 row ids, sequence
  range, hashes, epoch/link ids, challenged witness latest, ledger hashes,
  registry digests, and the absence of `archived_at`/manifest index rows.
- [ ] Require a success response with the expected positive
  `rows_archived`/`chunks_created` counts, zero `partitions_failed`, zero
  quarantine counts, and `incomplete:false` after any required bounded retries.
- [ ] List the new R2 keys and prove every data object exists before the one
  content-addressed manifest envelope named by the D1 manifest hash. Retain
  provider timestamps/versions as ordering evidence without claiming more
  precision than the provider exposes.
- [ ] GET every data object and the manifest envelope independently; prove
  exact byte lengths and BLAKE3 hashes, exact RFC-8785 manifest bytes, canonical
  padded-base64 signature, sorted/non-overlapping object ranges, and complete
  coverage of the D1 indexed range.
- [ ] Prove the signed manifest's end head, witness sequence/hash, epoch ledger
  sequence/hash, algorithm, link-key id and signing-key id equal independently
  authenticated evidence—not merely the duplicated D1 manifest columns.
- [ ] Prove the final D1 transaction created exactly one byte-identical
  manifest-index row and set `archived_at` only on the exact selected row-id
  set. Re-read immutable chain columns and require zero change.
- [ ] Retry the archive sweep. Require no duplicate manifest/index, no object
  overwrite, exact existing-object readback, unchanged row facts, and counts
  consistent with no remaining disposable work.
- [ ] Inject each negative below only in an isolated copy/fixture of the
  disposable evidence. Never corrupt production D1, witness, or retained R2:
  unknown/wrong trust root; forged signing or link registry; missing/mismatched
  configured link key; ledger gap, bad previous hash, bad signature, or altered
  D1 index; unavailable/divergent challenged latest; missing/gapped/forged
  stored witness receipt; keyed row gap/hash mismatch; manifest/object
  byte/hash/range/extra-object mutation. Each must fail before `archived_at`,
  must not quarantine a keyed row, and must never return a clean result.
- [ ] Inject a data-object publication/readback failure and prove no manifest
  envelope and no D1 mutation. Inject a manifest publication/readback failure
  and prove no D1 mutation. Inject an exact-CAS/D1 failure after complete R2
  publication and prove the immutable objects remain reusable while all
  selected rows remain unarchived until the exact retry succeeds.
- [ ] Run the independent verifier using the OOB trust roots, full historical
  witness public-key registry, full historical link-key set, ledger,
  registries, receipts, manifests, and every named object. Require `VERIFY_OK`.
- [ ] Remove one required artifact in a copy of the evidence set and require
  non-zero `INDETERMINATE`; restore it and re-require `VERIFY_OK`.
- [ ] Prove the target bucket's factual retention and conditional-write controls
  under B-046 before making an immutable/WORM claim.

## 7. Expansion gate

Security and SRE sign the disposable evidence bundle only when every applicable
box passes and all negative tests fail closed. Expansion remains one partition
at a time. Each partition gets its own preflight, bootstrap, challenged-latest
comparison, E0 event, E1 transition, archive, independent verification, restart
and recovery evidence.

As of this runbook version, the authenticated keyed archive path and its
two-credential epoch-admin gate exist in the repository but have not been
deployed or evidenced live. The 2026-09-22 #1793 assessment has no protected
Security witness deployment run, account/domain/DO identity proof,
pre-provisioned secret-name readback, exact-main deployment receipt, or live
append/retry/stale/divergent/latest receipt set with independent signature
verification. The repository deployment workflow and in-process tests prove
implementation readiness only; a workflow definition or successful fixture run
is not a deployment or receipt readback. These observations do not prove that
a hidden environment, local credential, or eligible custodian is absent; they
leave the independent witness, deployed bindings, and live receipts unverified.
Distinct credential names or repository-only archive proof alone do not close
B-054.

## References

- `specs/03_architecture/adrs/ADR-0073-independent-audit-head-witness.md`
- `specs/04_sprints/S09/work_items/WI-S09-007-keyed-audit-chain-epoch.md`
- `.github/workflows/audit-witness-deploy.yml`
- `.github/workflows/audit-chain-daily-verify.yml`
- `apps/audit-witness-worker/wrangler.toml`
- `docs/internal/secrets-checklist.md`
- `crates/corelink-container/src/routes/audit_drain/b054_epoch_admin.rs`
- `crates/corelink-container/src/routes/audit_archive.rs`
