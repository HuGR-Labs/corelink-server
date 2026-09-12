---
id: "B-054-keyed-audit-chain-epoch"
type: "architecture"
doc_status: "DRAFT"
audit_status: "AUDIT_PENDING"
version: "0.4.0"
created: "2026-09-01"
updated: "2026-09-09"
owner: "Security + platform operator"
final_approver: "Security + platform operator"
reviewers: []
supersedes: null
superseded_by: null
tags: ["b-054", "audit-chain", "keyed-hash", "epoch", "fail-closed"]
---

# B-054 — keyed audit-chain epoch contract

## Status and identity

This is the implementation contract for B-054. The pure runtime now exposes
the versioned algorithms/forward-only epoch state machine, and archive lines
carry explicit algorithm/epoch/key-id metadata with a fail-closed verifier
(`crates/corelink-audit-chain/src/epoch.rs`, `sealed_archive.rs`). The legacy
drain remains compatible with `Hasher::new()`. A v2 checkpoint is routed
exclusively through the witnessed transactional runtime and never falls back
to that legacy path.

The repository now also contains the ADR-0073 independent witness service and
Rust client/verifier, migration `0124`, the fail-closed witnessed D1 v2
in-epoch transaction, and the authenticated administrative boundary that
provisions immutable signing/link registries, verifies the complete legacy
prefix, bootstraps signed E0, and transitions a non-empty E0 to E1. Witness
commit-unknown retries are recovered by exact historical sequence/hash rather
than appending a new record. Keyed archives still remain unavailable unless
their complete authenticated manifest evidence verifies. B-054 therefore
remains **open** only for the bounded residual work listed below.

The historical, sealed `S09/WI-S09-007` is a synthetic-canary work item. This
addendum must not be read as a rewrite, completion, or replacement of that
frozen record despite the requested filename. The 0109 schema is only a
foundation; the runtime follow-on receives its own work package.

## Decision

Each `(tenant_id, region)` chain has ordered, non-overlapping epochs.  Legacy
epoch `0` uses the exact unkeyed link formula already present in sealed rows.  A
keyed epoch starts only through a signed ledger transition that names its
predecessor, head, range boundary, algorithm, and link-key id.  The active epoch
is authenticated by a **versioned signed head**, not inferred from a row shape,
key availability, a D1 default, or the largest epoch number.

The additive schema foundation is in `0109_audit_chain_epoch_contract.sql` and
`0110_audit_chain_epoch_row_metadata.sql`; no historical row is rewritten.
Migration `0124_audit_chain_witness_receipts.sql` adds append-only witness
receipts, complete-v2-head constraints, and the transaction assertion used to
roll back a D1 batch unless every expected seal, receipt, and head mutation is
present. The legacy drain continues to write explicit E0 metadata. An already
bootstrapped v2 partition uses the external witness before the atomic D1 batch
and fails closed on missing, stale, malformed, or unverifiable witness/ledger
state.

## Algorithms and identifiers

| id | algorithm | applicability | link bytes |
|---|---|---|---|
| `0` | `blake3-unkeyed-v1` | E0 and every existing segment | existing `BLAKE3(prev_hash || canonical_jcs)` byte-for-byte |
| `1` | `blake3-keyed-v2` | only a committed non-E0 epoch | `BLAKE3_keyed(K, "corelink/audit-chain/link/v2\\0" || prev_hash || canonical_jcs)` |

`canonical_jcs` is the persisted RFC-8785 byte sequence.  The domain string is
literal UTF-8 including its trailing NUL, and `K` is exactly 32 bytes.  Changing
either definition requires a new algorithm id and a new epoch; it never changes
the meaning of an existing id.

All ids in this contract are non-negative integers, except key ids, which are
strictly positive.  Hashes are exactly 64 lower-case hexadecimal BLAKE3-256
bytes.  The zero hash is 64 `0` characters.  Signatures are standard padded
base64 encodings of 64-byte Ed25519 signatures (88 characters).

## Versioned signed head

The historical v1 signature remains exactly the deployed JCS object
`{"head_hash", "next_sequence", "region", "tenant_id"}`.  It MUST NOT be
reinterpreted as a v2 signature.  Once a partition is epoch-enforced, its head
MUST have `head_message_version = 2` and MUST be verified over the exact RFC-8785
JCS representation of this object:

```text
{
  "epoch_id": <non-negative integer>,
  "epoch_ledger_hash": "<64 lower-case hex>",
  "epoch_ledger_sequence": <non-negative integer>,
  "head_hash": "<64 lower-case hex>",
  "head_message_version": 2,
  "next_sequence": <non-negative integer>,
  "region": "<canonical region>",
  "signing_key_id": <positive integer>,
  "tenant_id": "<canonical tenant id>"
}
```

`audit_chain_head.head_signature` signs those bytes, and
`audit_chain_head.signing_key_id` MUST equal the signed `signing_key_id`.
`epoch_id`, ledger sequence, and ledger hash are therefore authenticated at every
head advance.  A v2 verifier rejects a NULL field, a v1 message, an unknown
signing key, a mismatch between the head and signed object, or a signature over
any other byte sequence.  It never fills a missing `epoch_id` with `0`.  The
nullable `head_witness_sequence` and `head_witness_hash` columns are indexes for
the external record described below; they are never a substitute for fetching
and verifying it.

## External head witness (required for every v2 advance)

Every signed v2 head advance—not only an epoch transition—MUST be committed to
the independently administered witness before D1 is allowed to store that head.
This includes the signed E0 bootstrap and every subsequent in-epoch row/batch
advance.  Otherwise a D1 writer could replay an older, valid signed v2 head from
within the same epoch without changing the epoch-ledger root.

For a signed head, let `head_jcs` be its exact v2 JCS bytes and let `head_sig` be
the decoded 64-byte Ed25519 signature.  Define:

```text
head_record_hash = BLAKE3(
  "corelink/audit-chain/head-record/v1\\0" ||
  u64be(length(head_jcs)) || head_jcs || head_sig
)
```

The caller gives the witness the expected prior `(witness_sequence,
witness_record_hash)` and this exact RFC-8785 `witness_jcs` object:

```text
{
  "head_message_b64": "<standard padded base64 of exact head_jcs>",
  "head_record_hash": "<64 lower-case hex>",
  "head_signature_b64": "<88-character standard padded base64>",
  "previous_witness_hash": "<64 lower-case hex>",
  "region": "<canonical region>",
  "tenant_id": "<canonical tenant id>",
  "witness_sequence": <non-negative integer>,
  "witness_version": 1
}
```

Define `witness_record_hash = BLAKE3("corelink/audit-chain/head-witness/v1\\0"
|| witness_jcs)`. The stored `previous_witness_hash` and D1's
`head_witness_hash` use this value, never `head_record_hash`.

The first witnessed E0 head has `witness_sequence: 0` and the zero previous
hash.  Each later record has the immediately next sequence and prior record
hash.  The witness atomically compare-and-appends only when the supplied prior
pair equals its latest pair for the partition, retains the exact JCS plus a
receipt signed by a separately trusted witness key, and returns the new record
hash/sequence/receipt.  It rejects a duplicate sequence, a different successor,
or an unavailable prior record.  The witness receipt key is independent of the
head-signing and link keys.

The write order is mandatory: (1) read D1 and the external latest record and
require exact equality; (2) construct and sign the next v2 head; (3) obtain the
witness compare-and-append receipt; (4) in one D1 CAS, write the head and its
witness index.  The CAS includes the previously verified head fields and prior
witness index.  A crash or conflict after step 3 leaves external witness and D1
unequal; resume MUST fail closed and require incident-reviewed recovery, not
automatically replay/reconcile the candidate.  This deliberate availability
cost prevents a stale worker or D1 writer from completing a fork.

ADR-0073 selects the production boundary: a dedicated Security-administered
Cloudflare account runs one Durable Object per exact partition. The append API
wraps the exact `witness_jcs` bytes in this RFC-8785 request:

```text
{"append_request_version":1,"expected_latest":null|{"witness_record_hash":"<64 lower-case hex>","witness_sequence":<integer>},"witness_jcs_b64":"<standard padded base64>"}
```

Genesis alone uses `expected_latest:null`. The `Idempotency-Key` header is the
recomputed candidate witness hash. The signed receipt JCS contains
`committed_at_ms`, `head_record_hash`, `previous_witness_hash`,
`receipt_version:1`, partition, stable `witness_id`, positive `witness_key_id`,
and the committed witness sequence/hash. Its Ed25519 signature covers the
domain-separated, length-prefixed exact receipt bytes defined by ADR-0073.

Latest is a POST carrying a fresh 32-byte challenge, request version and exact
partition. Its signed response binds that challenge, observation time, witness
identity/key and either `latest:null` or the full stored record/receipt. A bare
GET, TLS success or 404 is never genesis/freshness evidence.

On every v2 resume and verification, the external **latest** witness record for
the partition MUST equal the D1 head's exact `head_jcs`, signature, partition,
and indexed witness sequence/hash.  “Found an old matching record” is
insufficient.  External ahead, D1 ahead, divergent bytes, a bad receipt,
missing record, or a list/fetch error is non-zero `INDETERMINATE`; no new seal
or `VERIFY_OK` is permitted.

## Authenticated append-only epoch ledger

`audit_chain_epoch_ledger` is authoritative evidence.  Its rows are inserted
only; the adjacent `audit_chain_epoch` is a query projection.  The migration's
triggers reject application-role update/delete attempts on the ledger, key
registries, and manifests.  Those triggers are defence against mistakes, not a
claim that D1 itself is immutable: a privileged D1 writer can alter D1.  The
signature chain and an independently retained witness are the integrity
boundary.

For entry `L`, define:

```text
ledger_hash(L) = BLAKE3(
  "corelink/audit-chain/epoch-ledger/v1\\0" || entry_jcs(L)
)
```

`entry_jcs` is the exact RFC-8785 JCS object signed by the signing key named
inside it.  It includes the partition, `ledger_version: 1`, `ledger_sequence`,
`previous_ledger_hash`, and all transition facts.  Entry 0 is E0 genesis and
uses the zero previous hash; every subsequent entry has sequence exactly one
greater than its predecessor, contains that predecessor's hash, and opens epoch
exactly one greater than its predecessor epoch.  The verifier recomputes every
hash, verifies every signature against the authenticated signing-key registry,
and rejects an omitted, duplicate, reordered, or divergent entry.

The signed v2 head must bind the last accepted `(epoch_ledger_sequence,
epoch_ledger_hash)`. A transition is never accepted merely because it is the
largest D1 row. Its D1 CAS also follows the external-head-witness ordering above,
inserts the successor ledger and projection rows, closes the predecessor
projection, and stores the newly signed v2 head. A zero-row CAS leaves the
external state ahead and is an incident, not a retry against a newer head. The
per-head witness makes an old ledger root or a within-epoch old head equally
unacceptable. ADR-0073 and the repository witness/client now implement this
boundary for already bootstrapped v2 in-epoch advances. It is not implied by
B-046/Object Lock, and it is not operational evidence until the independent
deployment and disposable-partition proof have completed.

## Canonical E0 checkpoint

E0 is an explicit signed genesis ledger entry, not an implicit NULL epoch or a
special case invented by a reader.  For partition `<tenant>`, `<region>` and
head signing key `<signing-key-id>`, `entry_jcs` is exactly the RFC-8785 bytes of:

```text
{
  "algorithm_id": 0,
  "checkpoint_type": "epoch-genesis",
  "checkpoint_version": 1,
  "epoch_id": 0,
  "ledger_sequence": 0,
  "ledger_version": 1,
  "link_key_id": null,
  "previous_ledger_hash": "0000000000000000000000000000000000000000000000000000000000000000",
  "region": "<region>",
  "signing_key_id": <signing-key-id>,
  "start_prev_hash": "0000000000000000000000000000000000000000000000000000000000000000",
  "start_sequence": 0,
  "tenant_id": "<tenant>"
}
```

The specified signing key signs those bytes, and `ledger_hash(E0)` uses the
domain-separated construction above.  E0 means that the legacy formula governs
`[0, first-successor-start)` and has the all-zero prior hash.  It does **not**
assert an empty history.  Backfill must first recompute every actual legacy row
from sequence 0 to the current signed v1 head, verify that head, then atomically
insert E0 and replace that same head with a v2 signature bound to E0.  An empty
partition still receives E0 only after a signed v2 zero-head bootstrap; absence
of an archive prefix is never proof of emptiness.

## Successor checkpoint and epoch projection

An E`n` transition (`n > 0`) has `entry_type: "epoch-transition"` and its signed
JCS object contains, in addition to the shared ledger fields:

```text
{
  "algorithm_id": 1,
  "checkpoint_type": "epoch-transition",
  "checkpoint_version": 1,
  "epoch_id": <n>,
  "from_epoch_id": <n - 1>,
  "from_head_hash": "<current signed head hash>",
  "from_next_sequence": <current signed next sequence>,
  "link_key_id": <positive registered link key id>,
  "to_start_prev_hash": "<same current signed head hash>",
  "to_start_sequence": <same current signed next sequence>
}
```

It also includes `tenant_id`, `region`, `signing_key_id`, `ledger_version`,
`ledger_sequence`, and `previous_ledger_hash` exactly as described above.  E1
cannot use a genesis record after an existing E0 or any sealed row.  The first
E1 link uses `to_start_prev_hash`; the predecessor range ends exclusively at
`to_start_sequence`.

The 0109 schema makes these invalid states unrepresentable at the normal D1
boundary:

- algorithm 0 requires a NULL link key; algorithm 1 requires a registered key;
- E0 alone has NULL predecessor, zero start, and zero prior hash; every other
  epoch names the immediately preceding epoch through a same-partition composite
  foreign key;
- a partition has one active projection, unique epoch ids, and unique start
  sequences;
- an active projection has no end facts; a closed projection has all end facts
  and `end_sequence_exclusive >= start_sequence`;
- a manifest has one epoch/range, matching algorithm/key nullability, and either
  a non-empty exact record count or an explicit empty zero-length range.

The projection is not replaceable evidence: deletion is rejected, and its only
permitted update is one `active`→`closed` transition that leaves its identity,
algorithm, boundary, predecessor, and opening-ledger facts unchanged. The ledger
and manifests are fully append-only. Each protected table also has a `BEFORE
INSERT` conflict guard, so `INSERT OR REPLACE` cannot erase a conflicting row
when SQLite runs with `recursive_triggers = OFF` and would otherwise skip the
implicit-delete trigger.

Cross-row facts—the predecessor's end equalling the successor start, projection
facts equalling signed ledger facts, manifest algorithm/key/ledger facts matching
their referenced epoch, and no range overlap—remain mandatory transaction/verifier
checks.  SQL `CHECK` constraints alone cannot prove them.

## Key-id registry

Key ids are durable identities, never configuration aliases.  The signing-key
registry carries `ed25519-v1` public keys, a root-key id, exact canonical
registry bytes, and a root signature.  The verifier has the approved trust-root
public-key set out of band and verifies that registry before using a signing
key. Unknown, malformed, or unauthenticated key ids are errors. A key cannot
self-authenticate its own registry entry.

The signing-key registry's `registry_jcs` is exactly the RFC-8785 bytes of:

```text
{
  "algorithm": "ed25519-v1",
  "key_type": "audit-chain-head-signing",
  "public_key_b64": "<44-character padded base64 Ed25519 public key>",
  "registered_at_ms": <non-negative integer>,
  "registry_type": "audit-chain-signing-key-registry",
  "registry_version": 1,
  "signing_key_id": <positive integer>,
  "trust_root_key_id": "<non-empty root identity>"
}
```

The Ed25519 trust-root key selected by `trust_root_key_id` signs those bytes.
The root's public key is an operator-approved verifier input, not a D1 row. A
verifier must compare every JCS field with the indexed columns before it trusts
the entry.

The link-key registry maps a positive `link_key_id` only to algorithm 1 and to
an irreversible, domain-separated commitment:

```text
link_key_commitment = BLAKE3(
  "corelink/audit-chain/link-key-commitment/v1\\0" || K
)
```

Its canonical registry JCS is signed by a registered head-signing key.  It never
contains `K`, a seed, or a Cloudflare secret name.  A runtime accepts a configured
32-byte key only if it recomputes the recorded commitment for the selected id.
The write-only keyring may be supplied as decimal ids mapped to 64-lowercase-hex
key material, but no keyring value belongs in D1, R2, committed config, a
migration, documentation, or CI output.  Rotation adds new registry and epoch
entries; it neither updates old material nor reuses an id.  Every historical key
needed during retention must remain available.  Missing, malformed, mismatched,
or unknown material fails seal and verification closed.

The link-key registry's `registry_jcs` is exactly the RFC-8785 bytes of:

```text
{
  "algorithm_id": 1,
  "key_commitment_hex": "<64 lower-case hex>",
  "key_type": "audit-chain-link",
  "link_key_id": <positive integer>,
  "registered_at_ms": <non-negative integer>,
  "registry_type": "audit-chain-link-key-registry",
  "registry_version": 1,
  "signing_key_id": <positive registered signing key id>
}
```

The named registered signing key signs those bytes.  Reusing either a link-key
id or commitment is rejected; an entry is never updated to point an existing id
at different secret material.

## Authenticated immutable archive manifest

One manifest covers exactly one contiguous range in exactly one epoch.  Its
`manifest_jcs` is exact RFC-8785 JCS and has no optional security fields:

```text
{
  "algorithm_id": <0 or 1>,
  "end_head_hash": "<64 lower-case hex>",
  "end_head_witness_hash": "<64 lower-case hex>",
  "end_head_witness_sequence": <non-negative integer>,
  "end_sequence_exclusive": <integer>,
  "epoch_id": <integer>,
  "epoch_ledger_hash": "<64 lower-case hex>",
  "epoch_ledger_sequence": <integer>,
  "is_empty": <true or false>,
  "link_key_id": <null or positive integer>,
  "manifest_type": "audit-chain-archive-manifest",
  "manifest_version": 1,
  "objects": [
    {"blake3_hash":"<64 lower-case hex>","byte_length":<integer>,"object_key":"<UTF-8 key>","record_count":<integer>,"start_sequence":<integer>}
  ],
  "record_count": <integer>,
  "region": "<region>",
  "signing_key_id": <positive integer>,
  "start_prev_hash": "<64 lower-case hex>",
  "start_sequence": <integer>,
  "tenant_id": "<tenant>"
}
```

`objects` is sorted by its UTF-8 `object_key` bytes.  For a non-empty manifest,
object record ranges concatenate exactly to `[start_sequence,
end_sequence_exclusive)` and sum to `record_count`; for an empty manifest,
`objects` is empty, count is zero, and start equals end.  The signature is over
these bytes and `manifest_hash` is
`BLAKE3("corelink/audit-chain/archive-manifest/v1\\0" || manifest_jcs)`.
The D1 row is only an index: the object named by `manifest_hash`, its exact
bytes, signature, named end-head witness record/receipt, and all named data
objects must be fetched and verified.  The manifest's end-head witness must
contain the exact signed v2 head whose hash is `end_head_hash`, and its record
must chain to the witness latest record used by the verification request.

“Immutable” is a required deployment property, not a label this repository may
apply to current R2.  The implementation must conditionally create content
addressed data and manifest objects, read them back byte-for-byte, deny normal
writer update/delete permissions, retain the signed manifest and witness in an
independent administrative domain, and demonstrate the storage provider's
retention/conditional-write semantics.  Until B-046 and the witness assessment
establish those facts, archive loss, listing failure, overwrite capability, or
missing manifest is `INDETERMINATE`; neither an Object-Lock nor WORM claim may be
made.

## State machine, verifier, and rollout

```text
legacy v1 head --full legacy verify + signed E0 bootstrap--> E0/v2
E0/v2 --witnessed signed CAS transition--> E0 closed + E1/v2
E1/v2 --witnessed signed CAS transition--> E1 closed + E2/v2
```

Before any v2 head advance or transition, the drain acquires the partition
lease, resolves the sealed tail, validates rows from the declared epoch boundary,
verifies equality of the current D1 head and external latest witness, verifies
the key registry, and obtains the next witness append receipt. It must leave
outbox rows unsealed and return an integrity error if any signature, key, epoch,
range, ledger, witness, or manifest condition fails. It must not advance a head,
mark a row emitted, fall back to E0, use `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME`, or
report successful audit emission.

A verifier receives ledger, registries, witness receipts, heads, manifests, and
objects—not a supplied date slice alone. `VERIFY_OK` requires all of: valid
trust-root and registry signatures; contiguous signed ledger from E0; a v2 D1
head exactly equal to the external latest witnessed head; one active epoch and
exact closed ranges; exact predecessor boundary; selected algorithm/key
recomputation; tenant/region and sequence continuity; the manifest's end-head
witness chain; and signed manifest coverage of every requested row. Empty is
clean only with a valid explicit empty manifest. Any unavailable key, malformed
value, list/fetch failure, witness mismatch/gap, gap/overlap, duplicate active
epoch, missing witness, or unknown version returns non-zero `INDETERMINATE`,
never a clean result.

Roll out readers and schema first, preserve all current unkeyed regression
vectors, then backfill signed E0 one fully verified partition at a time.  Test a
disposable partition through E0→E1, restart, archive, witness, independent
verify, historic-key rotation, and a forward-only recovery.  The old
`AUDIT_CHAIN_TRUST_UNSIGNED_RESUME` escape is prohibited during E0 bootstrap and
all subsequent epoch operations.  A rollback is a newly witnessed forward epoch;
deleting/altering epochs or registry entries, moving the head backward, or
rewriting links is never rollback.

## Required implementation proof and residual blockers

The runtime PR must include focused unit/integration/property and mutation
controls that fail for: omitting `epoch_id` or message version from a head;
wrong E0 bytes/signature; an E0 bootstrap without witness append; any in-epoch
head advance without a witness append; D1 head versus latest-witness mismatch;
key/algorithm mismatch; predecessor from another partition; active/closed range
violations; stale CAS/concurrent transition; altered/deleted/replayed ledger
state; missing or forged registry/witness; E0 re-genesis after data;
missing/malformed/overlapping manifests; archive data loss; historic-key loss;
and any fallback of unknown evidence to unkeyed or `VERIFY_OK`. The daily fixture
must contain a mixed-epoch positive, malformed E0, rolled-back head,
within-epoch replay, missing witness, and missing-manifest negative.

The repository now implements strict witness wire verification, witnessed
atomic D1 advancement, signed E0 bootstrap, the E0→E1 administrative
transition, and the live keyed archive boundary. The archive authenticates the
complete signed epoch ledger and witness history with each artifact's
root-authorized historical signing key, publishes epoch/content-addressed data
objects and a signed content-addressed manifest last, reads every publication
back exactly, and only then performs an exact-row D1 transaction. Because
migration 0109 makes `(tenant_id, region, start_sequence)` unique, the current
boundary rejects an empty E0 transition before witnessing; at least one
witnessed E0 event must exist first. Remaining blockers are bounded: compile
and exercise the boundary in real workerd; deploy the witness under ADR-0073's
independent administration and record disposable-partition bootstrap,
transition, restart, archive, mutation and historical-key-rotation evidence;
exercise root/signing/link/witness-key custody and retention; complete B-046's
factual storage assessment; and provision two distinct human custodians for the
runtime-enforced SRE-executor plus Security-approver credentials. The current
GitHub principal's membership/collaborator queries returned one visible account;
they do not establish eligibility or custody, so the second distinct custodian
remains unverified. Distinct secret values alone are not accepted as two-person
control. B-054 is therefore open.
