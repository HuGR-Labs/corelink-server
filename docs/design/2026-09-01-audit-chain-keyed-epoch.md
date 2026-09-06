# Keyed audit-chain epoch — decision record

Date: 2026-09-02
Status: runtime algorithm/state and archive compatibility landed; production
witnessed rollout remains open
Owner: Security + platform operator

The full normative design is [the B-054 epoch contract](../../specs/04_sprints/S09/work_items/WI-S09-007-keyed-audit-chain-epoch.md).
The pure runtime now exposes the explicit algorithm ids and forward-only epoch
state machine (`crates/corelink-audit-chain/src/epoch.rs`). Archive lines carry
algorithm/epoch/key-id metadata and verify against the selected epoch without
falling back. The D1 drain writes explicit E0 metadata for newly sealed rows and
refuses a v2 head until the witnessed transactional binding is deployed.

Current sealed rows use unkeyed BLAKE3 links. The drain's Ed25519-signed head is
verified fail-closed on resume when its signing seed is configured, but it does
not version the link algorithm. Replacing the hash constructor in place would
make historical rows unverifiable and risks both a chain break and an algorithm
fork. B-054 therefore requires an authenticated, per-tenant/per-region epoch
ledger, a signed boundary checkpoint, CAS-based transition, key-id registry, and
archive coverage manifest.  The signed-head message must be versioned and bind
`epoch_id` plus the authenticated epoch-ledger root; a prior valid signed head
cannot be relabelled as another epoch.

The contract makes four non-negotiable choices:

- Legacy links remain `blake3-unkeyed-v1` forever; an old segment is never
  recomputed using a later key.
- A new keyed segment starts only after a witnessed checkpoint binds the old
  signed head, next sequence, old/new epoch ids, algorithm, key id, and prior
  ledger hash.
- The independent witness commits every v2 signed-head advance, including E0
  bootstrap and ordinary in-epoch advances; resume requires the external latest
  record to equal the exact D1 head/signature, so an old valid head cannot be
  replayed within an epoch.
- Missing/malformed epoch evidence or unavailable historical key is a failed
  seal/verification, never a fallback, a reset, or a clean result.
- Rotation creates another forward epoch and retains historical keys for the
  audit-retention lifetime. It is not an overwrite of key material or a rewind.

The link key must be a distinct Cloudflare write-only secret. Neither the
Ed25519 head-signing seed nor a secret value belongs in D1, R2, committed config,
this document, or CI output. The exact new secret registration and deployment
forwarding are implementation work and must pass the repository's secrets
matrix/checklist gates.

B-046 remains independent: it determines the factual R2/Object-Lock posture.
This record makes no Object-Lock, external-witness, write-time-chaining, or
runtime-implementation claim.  The contract now requires a separately
administered linearizable witness before keyed mode can prevent authenticated D1
head replay; it must monotonically commit every v2 head, not only epoch
transitions. An archive alone is insufficient. The additive 0108/0109 schema
is now present; it remains inert for v2 until a
runtime supplies v2 head signing, transactional ledger writes,
archive manifests, witness receipts, full state/error tests, non-vacuous mutation
proof, and an operator-approved rollout/rotation record.  B-054 remains open.
