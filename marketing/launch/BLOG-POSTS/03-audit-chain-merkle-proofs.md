<!-- DRAFT — pending Legal + Marketing + CEO sign-off. Do not publish. -->

# The CoreLink Audit Chain: An Append-Only Hash Chain, JCS Canonicalization, and a Trust Story You Can Replay

> **DRAFT — pending Marketing + Security + Compliance sign-off.**
> Technical deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 3) · INV-AUDIT-APPEND-ONLY · INV-OBS-AUDIT-CHAIN-INTEGRITY · `audit_immutability.tla`.

---

Audit chains are the part of a SaaS product that gets the least design attention and the most regulatory weight. They tend to be implemented as a logging side-effect — write events to a stream, retain for 7 years, ship to S3 with Object Lock, mark "compliant." That posture survives most audits and fails the only ones that matter, which are the ones where a regulator or a customer's external auditor asks: *prove this record was not altered between the event and the audit*.

The CoreLink audit chain is designed to make that proof structurally trivial. This post explains how, why, and what the customer-facing artifact looks like.

## "Prove you didn't see my data"

The most uncomfortable question a regulated customer can ask a SaaS vendor is some variant of *prove you did not see my data; prove you did not change the record after the fact; prove that the events you are showing me now are the events that actually happened*. The honest answer for most vendors is some flavor of "trust our SOC 2 report" — which is a perfectly fine answer at one level of abstraction and a deeply unsatisfying one at the level the customer is actually asking.

The strongest answer is a cryptographic one: *here is the construction; here is the head we published yesterday; here is the head we publish today; here is the deterministic recomputation that shows today's head is yesterday's head extended, event by event, with nothing inserted, removed, or reordered in between*. If the customer (or their auditor) can re-derive these values themselves from raw events, they do not need to trust the vendor's word about anything except the existence of the events, and those events are something the customer is receiving anyway.

CoreLink builds that answer.

## The properties we care about

Three properties anchor the design:

1. **Append-only.** Once a record is in the chain, it does not change and it cannot be removed. We call this `INV-AUDIT-APPEND-ONLY` and it is a CRITICAL invariant, enforced both by storage-layer Object Lock and by the chain construction itself. The same property is the safety property modeled in `audit_immutability.tla` and mechanically checked in CI.
2. **Independently verifiable.** A customer (or their auditor) must be able to take a chain head we publish, the raw events they received via webhook or API, and re-derive the chain head themselves. No CoreLink-side trust is required for verification.
3. **Canonical leaf form.** Two parties hashing the "same" record must produce the same hash. This is not free — JSON serialization is non-deterministic by default. We pin the canonical form to RFC 8785 (JSON Canonicalization Scheme) so the chain is replay-stable across implementations.

## The construction: a linear, append-only hash chain

CoreLink's chain construction is a **linear hash chain**, not a Merkle tree. Every audit event is chained to the one immediately before it: `head_n = BLAKE3(head_{n-1} || event_n)`. The genesis entry for a tenant chains against a fixed, published starting value. This is the same family of construction as a git commit chain or a blockchain block chain — deliberately simple, and simple is a feature here, not a compromise: the verification routine anyone runs is "recompute the hash of everything since the last head you trust, and check it matches."

Two properties fall out of this directly. **Order-sensitivity**: changing, inserting, or reordering any event changes every subsequent head, so a retroactive edit anywhere in the history is detectable by recomputing forward from any earlier head you already trust. **Tamper-evidence**: because each head folds in the full byte content of the prior head, you cannot forge a later head without knowing (and recomputing from) everything that came before it.

What this construction does **not** give you is what a Merkle tree gives you: a small, logarithmically-sized proof that one specific event is included in a much larger structure without recomputing the whole thing. There is no inclusion proof and no consistency proof in the RFC 6962 (Certificate Transparency) sense here — CoreLink's audit chain does not use a Merkle tree, so those proof types do not apply. Verifying a window of the chain means recomputing the hash chain across that window, which is linear in the number of events in the window, not logarithmic.

We publish daily heads. A customer's auditor can replay the chain of daily heads across an arbitrary window — a quarter, a year, the full retention period — recomputing forward and confirming each day's head is reproducible from the previous day's head plus that day's events, and verify the chain is append-only from start to end.

## RFC 8785 (JCS): why canonicalization is the unsexy half of the design

JSON is non-deterministic by default. Two services emitting `{"a": 1, "b": 2}` and `{"b": 2, "a": 1}` produce different byte sequences and therefore different hashes. For an audit chain where the customer is going to independently hash the same logical record, that is a correctness bug, not a stylistic preference.

It is also subtler than it sounds. Number serialization is non-trivial: should `1.0` and `1` hash to the same value? (No — they are different JSON values.) Should `1e2` and `100` hash to the same value? (Yes, per RFC 8785 — they denote the same number.) What about NaN and Infinity? (Out of scope — JCS does not serialize them.) String escaping has multiple legal representations. Unicode normalization is its own rabbit hole.

CoreLink canonicalizes every audit leaf using **RFC 8785 — JSON Canonicalization Scheme (JCS)**. JCS pins:

- Key ordering (lexicographic over the UTF-16 code units of keys).
- Number serialization (IEEE 754 with a specified textual form, following the I-JSON profile).
- String escaping (specific escape rules, no unnecessary escapes).
- Whitespace (none).

The canonical byte form is hashed with the same algorithm used at the chain layer (BLAKE3). The customer running JCS on their copy of the record produces the same event hash CoreLink stored. There is no implementation freedom in the middle. We publish a reference verification implementation in Rust and TypeScript; both produce bitwise-identical canonical output for any well-formed input.

## Append-only as a structural invariant

The append-only property is enforced at two layers. The chain construction itself is append-only — appending to the hash chain produces a new head whose relationship to the previous head is computable and verifiable by direct recomputation. A retroactive edit would produce a head that does not recompute to match any prior published head, and that failure is detectable by anyone running the verification routine.

We additionally enforce append-only at the storage layer with Object Lock retention in COMPLIANCE mode. The chain construction would, in principle, make storage-layer tamper-evidence redundant — a retroactive edit would be detectable by recomputation regardless of storage controls. We still use Object Lock because tamper-evidence is not the same thing as tamper-resistance, and a regulator's standard of evidence is satisfied more readily by a layered control than by a clever one.

The TLA+ specification `audit_immutability.tla` models the abstract property: for any two chain heads at times `T1 < T2`, the leaf at any index `i ≤ |chain(T1)|` is identical in both. The specification is in the CI loop. CI fails if the model checker finds a counterexample.

## What the customer actually receives

Every state-changing operation in CoreLink emits a structured audit event. Customers can receive these events through three channels:

- **Webhook** (push, near-real-time, with retry and idempotency keys).
- **Audit API** (pull, paginated, with stable cursors).
- **Daily proof bundle** (S3 / GCS / Azure Blob; published at a fixed time per region, including the day's head, signing-key reference, and the prior day's head so the day's head can be independently recomputed and verified).

Each event carries:

- `event_id` (canonical, monotonic per tenant).
- `tenant_id`.
- `event_type` (enumerated; the enumeration is versioned and the version is part of the hashed entry).
- `payload` (operation-specific, JCS-canonicalized).
- `chain_position` (the event's index in the tenant's chain).
- `chain_head_at_emit` (the chain head as of this event).

## The customer verification flow

A customer wishing to verify a single event runs three steps:

1. JCS-canonicalize the event payload.
2. Hash it with BLAKE3 into the chain entry form.
3. Confirm the resulting entry, combined with the chain head immediately before it, recomputes to the `chain_head_at_emit` published for that event.

A customer wishing to verify a window — typically what an external auditor wants — runs four:

1. Pull all events in the window via the Audit API or via the daily proof bundle.
2. Pull the chain head at the start and end of the window.
3. Recompute the chain forward from the start-of-window head, folding in every event in order.
4. Confirm the recomputed head matches the published end-of-window head (this is the structural append-only check).

The verification is mechanical and the cost is linear in the number of events being checked; the trust is in the math, not in CoreLink. Recomputing a full window is deliberately cheap for CoreLink's per-tenant event volumes — it is not the logarithmic-proof model a Merkle transparency log would give you, and we do not claim it is.

We publish an open-source verification toolkit (`github.com/HumanGuardrail/corelink-audit-verify`, placeholder repository pending GA-day open) so customers do not have to write the proof-verification routines themselves.

## Performance numbers

The chain machinery is engineered to keep audit append off the critical path of data operations. Audit events are emitted to a per-tenant log shard, batched, and folded into the hash chain asynchronously. The data-path latency cost of audit emission is bounded by the SLO catalog; current target is sub-millisecond p99 contribution to the parent operation (`DRAFT — final numbers pending CAP-GA-007 staging attestation`).

Verification cost is linear in the size of the window being checked, not logarithmic — a property of the linear hash-chain construction, not a Merkle tree. At the per-tenant event rates we anticipate, recomputing a full day's chain is inexpensive: a single pass over that day's events. The daily proof bundle, including signing-key reference and the prior day's head, fits comfortably in a single object well under a megabyte per region per tenant at typical operation rates.

We publish bench numbers at GA-day. Today, in the embargoed launch documents, these numbers are placeholders pending final confirmation against the 30-day sustained staging data.

## Daily proof publication

The chain head, signed by a CoreLink-side Ed25519 signing key (rotated and tracked at `corelink-docs.humangr.com/trust`), is published daily per region. The schedule is documented per region in the trust center. Customers can configure their own auditing pipeline to fetch the head and recompute it against the previous day's head as a routine integrity exercise.

We have customers who do this. We think more of them should.

## What this is not

This design is not a substitute for proper internal audit logging at the customer's side. The CoreLink audit chain records CoreLink-side events: tenant operations against CAS and AC, BYOK key operations, admin-plane actions, billing events, residency operations. It does not record events that occur inside the customer's own systems — those remain the customer's responsibility.

The design is also not a substitute for SOC 2 or equivalent control attestation. The chain proves integrity of the recorded events; SOC 2 attests to the operating effectiveness of the controls that decide which events are recorded in the first place. Both matter; they are complementary.

## Limits and roadmap

Three things on the roadmap that are explicitly out of GA scope:

**Signed timestamps.** The daily head is signed by a CoreLink-side Ed25519 key. For customers whose audit posture requires a third-party time anchor, we are exploring RFC 3161 / RFC 5816 timestamping for the daily head publication. The integration is on the post-GA roadmap; the threat model is "did CoreLink rewrite both the chain and the publication time stamps simultaneously" which the current design addresses through publication discipline rather than third-party anchoring.

**Rekor integration.** For customers operating in the sigstore / supply-chain ecosystem, mirroring CoreLink chain heads into Rekor's public transparency log would give a public, third-party anchor for the daily heads at no additional verification cost. We are scoping this for a post-GA increment.

**Per-tenant separated chains.** GA ships per-tenant logical chains rooted in a tenant-scoped tree. For customers whose threat model requires complete cryptographic separation from other tenants' chain structure (rather than the logical separation we ship at GA), a per-tenant fully separated chain is on the roadmap with a clear cost: more daily proof bundles, more storage, more verification surface. We will ship it when a customer demands it.

## Common questions

**How expensive is verification?** It is linear, not logarithmic — recomputing a hash chain over `n` events costs `n` hashes. There is no Merkle tree here, so there is no `O(log n)` inclusion or consistency proof. At the per-tenant event volumes CoreLink sizes for, a full day's recomputation is inexpensive; we do not claim sub-linear proof sizes.

**How big is the chain?** Per-tenant event rate is the dominant factor. We size for sustained high-throughput tenants; the chain construction is unaffected by rate spikes.

**What hash algorithm?** BLAKE3 at the chain layer (`BLAKE3(prev || event)`). BLAKE3 is also used elsewhere in CoreLink for content addressing where REAPI compatibility permits; SHA-256 is used where REAPI compatibility requires it, but the audit chain itself is BLAKE3.

**Where do I find the signing key?** Published at `corelink-docs.humangr.com/trust/signing-keys` with rotation history.

## Where to go next

- **Trust center:** `corelink-docs.humangr.com/trust`
- **Audit chain spec:** `corelink-docs.humangr.com/trust/audit-chain`
- **Verification toolkit (open source):** `github.com/HumanGuardrail/corelink-audit-verify` (placeholder pending GA repo open)
- **Daily proof bundle format:** `corelink-docs.humangr.com/trust/proof-bundle`

— Trust Engineering at CoreLink

---

## Internal notes (strip before publish)

- Target length: 1,800–2,200 words.
- Pending review: Security Lead + Compliance Officer (canonical sign-off slots 4 + 9 in WI-S20-008 §16).
- All technical claims trace to INV-AUDIT-APPEND-ONLY / INV-OBS-AUDIT-CHAIN-INTEGRITY / `audit_immutability.tla`.
- No specific dollar amounts. Verification toolkit repo URL is placeholder pending OSS repo open.
