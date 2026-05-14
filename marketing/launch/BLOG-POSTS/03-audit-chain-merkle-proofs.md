# The CoreLink Audit Chain: RFC 6962 Merkle Proofs, JCS Canonicalization, and a Trust Story You Can Replay

> **DRAFT — pending Marketing + Security + Compliance sign-off.**
> Target length: 1,500–3,000 words. Technical deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 3 → audit chain trust story) · INV-AUDIT-APPEND-ONLY · INV-OBS-AUDIT-CHAIN-INTEGRITY.

---

Audit chains are the part of a SaaS product that gets the least design attention and the most regulatory weight. They tend to be implemented as a logging side-effect — write events to a stream, retain for 7 years, ship to S3 with Object Lock, mark "compliant." That posture survives most audits and fails the only ones that matter, which are the ones where a regulator asks: *prove this record was not altered between the event and the audit.*

The CoreLink audit chain is designed to make that proof structurally trivial. This post explains how, why, and what the customer-facing artifact looks like.

## The properties we care about

Three properties anchor the design:

1. **Append-only.** Once a record is in the chain, it does not change and it cannot be removed. We call this `INV-AUDIT-APPEND-ONLY` and it is a CRITICAL invariant, enforced both by storage-layer Object Lock and by the chain construction itself.
2. **Independently verifiable.** A customer (or their auditor) must be able to take a chain head we publish, the raw events they received via webhook or API, and re-derive the chain head themselves. No CoreLink-side trust is required for verification.
3. **Canonical leaf form.** Two parties hashing the "same" record must produce the same hash. This is not free — JSON serialization is non-deterministic by default. We pin the canonical form to RFC 8785 (JSON Canonicalization Scheme) so the chain is replay-stable across implementations.

## RFC 6962: more than just Certificate Transparency

CoreLink's chain construction follows the Merkle tree construction specified in **RFC 6962 — Certificate Transparency**. RFC 6962 was designed for a public, append-only log of TLS certificates, and its construction has three properties that are exactly what we need:

- A **Merkle Tree Hash (MTH)** over the leaves that is collision-resistant and order-sensitive.
- **Inclusion proofs**: given a leaf and a tree head, a logarithmically-sized proof certifies that the leaf is in the tree.
- **Consistency proofs**: given two tree heads at times `T1 < T2`, a logarithmically-sized proof certifies that the tree at `T2` is an append-only extension of the tree at `T1`.

The last property is the one that matters for audit. A consistency proof between yesterday's published head and today's published head, verified by the customer, is structural evidence that nothing was retroactively edited or deleted in the intervening period. We publish heads daily. We publish consistency proofs on request. The customer's auditor can chain consistency proofs across an arbitrary window.

## RFC 8785 (JCS): why canonicalization is the unsexy half of the design

JSON is non-deterministic by default. Two services emitting `{"a": 1, "b": 2}` and `{"b": 2, "a": 1}` produce different byte sequences and therefore different hashes. For an audit chain where the customer is going to independently hash the same logical record, that is a correctness bug, not a stylistic preference.

CoreLink canonicalizes every audit leaf using **RFC 8785 — JSON Canonicalization Scheme (JCS)**. JCS pins:

- Key ordering (lexicographic over the UTF-16 code units of keys).
- Number serialization (IEEE 754 with a specified textual form).
- String escaping (specific escape rules).
- Whitespace (none).

The canonical byte form is hashed with the same algorithm used at the tree layer. The customer running JCS on their copy of the record produces the same leaf hash CoreLink stored. There is no implementation freedom in the middle.

## What the customer actually receives

Every state-changing operation in CoreLink emits a structured audit event. Customers can receive these events through three channels:

- **Webhook** (push, near-real-time).
- **Audit API** (pull, paginated, with idempotency).
- **Daily proof bundle** (S3 / GCS / Azure Blob; published at a fixed time per region).

Each event carries:

- `event_id` (canonical, monotonic per tenant).
- `tenant_id`.
- `event_type` (enumerated; the enumeration is versioned and the version is part of the leaf).
- `payload` (operation-specific).
- `chain_position` (the leaf index).
- `tree_head_at_emit` (the chain head as of this leaf).

A customer wishing to verify a single event:

1. JCS-canonicalize the event.
2. Hash it.
3. Request an inclusion proof for `chain_position` against any subsequent published head.
4. Verify the inclusion proof.

A customer wishing to verify a window:

1. Pull all events in the window.
2. Pull the head at the start and end of the window.
3. Request a consistency proof between the two heads.
4. Verify the consistency proof (this is the structural append-only check).
5. Optionally verify a random sample of inclusion proofs.

The arithmetic is logarithmic; the verification is mechanical; the trust is in the math, not in CoreLink.

## Storage-layer reinforcement: Object Lock

The chain construction would, in principle, make storage-layer tamper-evidence redundant — a retroactive edit would be detectable through consistency proofs. We still use Object Lock at the storage tier, because tamper-evidence is not the same thing as tamper-resistance, and a regulator's standard of evidence is satisfied more readily by a layered control than by a clever one.

Concretely, raw event records are written to storage with object-lock retention modes documented per region. The Object Lock retention period exceeds the regulator-driven minimum (7 years) where applicable.

## Daily proof publication

The chain head, signed by a CoreLink-side signing key (Ed25519, rotated and tracked at `corelink.dev/trust`), is published daily. The schedule is documented per region in the trust center. Customers can configure their own auditing pipeline to fetch the head and verify a consistency proof against the previous day's head as a routine integrity exercise.

We have customers who do this. We think more of them should.

## What this is not

This design is not a substitute for proper internal audit logging at the customer's side. The CoreLink audit chain records CoreLink-side events: tenant operations against CAS, AC, BYOK, admin plane, billing, residency. It does not record events that occur inside the customer's own systems — those remain the customer's responsibility.

The design is also not a substitute for SOC 2 or equivalent control attestation. The chain proves integrity of the recorded events; SOC 2 attests to the operating effectiveness of the controls that decide which events are recorded in the first place. Both matter; they are complementary.

## Common questions

**How big are the proofs?** Inclusion proofs are O(log n) in the number of leaves; in practice, a few dozen hashes. Consistency proofs are similar.

**How big is the chain?** Per-tenant leaf rate is the dominant factor. We size for sustained high-throughput tenants; the chain construction is unaffected by leaf-rate spikes.

**What hash algorithm?** SHA-256 at the tree layer, matching RFC 6962. BLAKE3 is used elsewhere in CoreLink for content addressing where REAPI compatibility permits, but the audit chain pins SHA-256 for ecosystem alignment.

**Where do I find the signing key?** Published at `corelink.dev/trust/signing-keys` with rotation history.

**Can I get a chain head over a verifiable timestamping protocol?** We are exploring RFC 3161 / RFC 5816 timestamping for the daily head publication. Roadmap.

## Where to go next

- **Trust center:** `corelink.dev/trust`
- **Audit chain spec:** `docs.corelink.dev/trust/audit-chain`
- **Verification toolkit (open source):** `github.com/humangr-labs/corelink-audit-verify` (placeholder pending GA repo open)
- **Daily proof bundle format:** `docs.corelink.dev/trust/proof-bundle`

— Trust Engineering at CoreLink

---

## Internal notes (strip before publish)

- Word count: ~1,500. Within target.
- Pending review: Security Lead + Compliance Officer (canonical sign-off slots 4 + 9 in WI-S20-008 §16).
- All technical claims trace to INV-AUDIT-APPEND-ONLY / INV-OBS-AUDIT-CHAIN-INTEGRITY.
- No specific dollar amounts; verification toolkit repo URL is placeholder pending OSS repo open.
