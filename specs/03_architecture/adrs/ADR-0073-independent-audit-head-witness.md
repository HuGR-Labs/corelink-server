---
id: "ADR-0073"
type: "adr"
doc_status: "ACTIVE"
audit_status: "AUDIT_PENDING"
version: "1.0.0"
created: "2026-09-09"
updated: "2026-09-09"
owner: "Security + platform operator"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "audit-chain", "witness", "durable-object", "b-054"]
---

# ADR-0073 — independent linearizable audit-head witness

## Decision

B-054 uses a dedicated Cloudflare Worker plus one Durable Object per exact
`(tenant_id, region)` partition as its synchronous, pre-D1 head witness. It is
deployed in a Cloudflare account administered by Security, with credentials,
deploy rights, logs, backup and Ed25519 receipt key custody disjoint from the
CoreLink production account and its D1 administrators. Deploying this code in
the CoreLink account does **not** satisfy this ADR.

The witness exposes only authenticated compare-and-append and signed-latest
operations. Durable Object serialization and one storage transaction provide
the linearization point. The exact witness record remains the B-054 RFC-8785
object and BLAKE3 hash. A separately held Ed25519 key signs a domain-separated
receipt. The client pins the witness id, receipt trust root/public-key registry,
HTTPS origin and protocol version. Missing/malformed configuration, transport
errors, unsigned responses, unknown keys, stale latest, bad signatures or
storage errors are `INDETERMINATE` and block both D1 mutation and `VERIFY_OK`.

ADR-0066 is unchanged: Rekor remains asynchronous public transparency after a
commit. Its fail-open submission is never the B-054 linearization point.

## Wire and storage contract

`POST /v1/audit-chain/head-witness/compare-and-append` accepts the exact request
defined in the B-054 work item and an `Idempotency-Key` equal to the recomputed
candidate witness hash. Genesis expects `null`; successors name the exact latest
sequence/hash. The inner exact JCS is decoded and re-canonicalized, both record
hashes are recomputed, the partition and sequence/predecessor are checked, then
the record, signed receipt and latest pointer are written atomically. An exact
retry returns the stored receipt; a different successor or stale predecessor is
409 and never mutates state.

`POST /v1/audit-chain/head-witness/latest` binds a fresh 32-byte challenge into
an Ed25519-signed snapshot, including an explicit `latest: null` for genesis.
An unsigned GET/404 is not evidence. Exact records and receipts are retained by
fixed-width sequence keys; latest is an index, not the only copy.

Receipt signatures cover
`"corelink/audit-chain/head-witness-receipt/v1\0" || u64be(len(receipt_jcs)) || receipt_jcs`.
Latest signatures use the analogous `head-witness-latest/v1` domain. Integers
must be non-negative JavaScript-safe integers; base64 is standard padded and
canonical; hashes are lowercase BLAKE3-256 hex; unknown fields/versions fail.

## Failure and recovery

A timeout is commit-unknown: freeze the partition and query signed latest with
a new challenge. If the witness committed but D1 did not, ordinary drain and
verification remain stopped. Dual-authorized recovery may complete only the
original exact D1 CAS after independently proving that D1 is still the exact
predecessor and witness latest is the prepared candidate. It never deletes or
rewinds the witness, invents a new genesis, or auto-reconciles a divergent head.

The D1 step is one transaction/CAS binding every predecessor head, signature,
epoch/ledger and witness field while writing the sealed rows, ledger/projection
changes, next signed head and receipt index. A zero-row CAS is an incident.

## Bounded implementation and rollout

1. Land the isolated `apps/audit-witness-worker` protocol, DO storage, unit and
   real-workerd concurrency tests; keep it unreachable from the production drain.
2. Land the Rust HTTP client/verifier with redirects/proxies disabled, pinned
   origin/witness/key registry, challenged latest verification and fault tests.
3. Land v2 drain + D1 transactional CAS and mixed-epoch archive verifier; prove
   crash-before/after-witness and concurrent-successor mutations.
4. Security deploys the witness from the exact reviewed `main` SHA with
   separate-account credentials on the protected environment's pinned custom
   HTTPS domain (there is no `workers.dev` fallback), and records public
   keys/config without private values. Apply schemas/readers, bootstrap
   one disposable partition E0, exercise E0→E1/restart/archive/recovery, then
   expand one partition at a time. B-054 remains open until these live receipts
   and independent archive proof exist.

## Rejected alternatives

- Rekor as pre-write witness: no per-partition compare-and-append and explicitly
  fail-open under ADR-0066.
- CoreLink D1/R2 or a DO in the same administrative account: self-witnessing.
- A configurable generic HTTP endpoint without pinned receipt verification:
  transport success is not independently verifiable evidence.
