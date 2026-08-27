<!-- DRAFT — pending Legal + Marketing + CEO sign-off. Do not publish. -->

# The CoreLink Audit Chain: a BLAKE3 Hash Chain, JCS Canonicalization, and a Signed Head You Can Re-Derive

> **DRAFT — pending Marketing + Security + Compliance sign-off.**
> Technical deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 3) · INV-AUDIT-APPEND-ONLY · INV-OBS-AUDIT-CHAIN-INTEGRITY · `audit_immutability.tla`.

---

Audit chains are the part of a SaaS product that gets the least design attention and the most regulatory weight. They tend to be implemented as a logging side-effect — write events to a stream, retain for 7 years, ship to durable storage, mark "compliant." That posture survives most audits and fails the only ones that matter, which are the ones where a regulator or a customer's external auditor asks: *prove this record was not altered between the event and the audit*.

The CoreLink audit chain is designed to make that proof structurally trivial. This post explains how, why, and what the customer-facing artifact looks like — and is explicit about what we ship at GA versus what is honestly still on the roadmap.

## "Prove you didn't see my data"

The most uncomfortable question a regulated customer can ask a SaaS vendor is some variant of *prove you did not see my data; prove you did not change the record after the fact; prove that the events you are showing me now are the events that actually happened*. The honest answer for most vendors is some flavor of "trust our SOC 2 report" — which is a perfectly fine answer at one level of abstraction and a deeply unsatisfying one at the level the customer is actually asking.

The strongest answer is a cryptographic one: *here is the construction; here is the head we published earlier; here is the head we publish now; here is the deterministic recomputation that shows the current head is the prior head extended, event by event, with nothing inserted, removed, or reordered in between*. If the customer (or their auditor) can re-derive these values themselves from raw events, they do not need to trust the vendor's word about anything except the existence of the events, and those events are something the customer is receiving anyway.

CoreLink builds that answer, with the limits described below.

## The properties we care about

Three properties anchor the design:

1. **Append-only.** Once a record is in the chain, it does not change and it cannot be removed. We call this `INV-AUDIT-APPEND-ONLY` and it is a CRITICAL invariant, enforced by the chain construction itself. The same property is the safety property modeled in `audit_immutability.tla` and mechanically checked in CI.
2. **Independently verifiable.** A customer (or their auditor) must be able to take a chain head we publish, the raw events they received via webhook or API, and re-derive the chain head themselves. No CoreLink-side trust is required for verification, modulo the honest limits described in "Honest guarantee wording" below.
3. **Canonical leaf form.** Two parties hashing the "same" record must produce the same hash. This is not free — JSON serialization is non-deterministic by default. We pin the canonical form to RFC 8785 (JSON Canonicalization Scheme) so the chain is replay-stable across implementations.

## The construction: a linear BLAKE3 hash chain

CoreLink's chain construction is a **linear hash chain**, not a Merkle tree. Every audit event is chained to the one immediately before it: `next_hash = BLAKE3(prev_hash_bytes || canonical_bytes(event))`. The genesis entry for a tenant chains against a fixed, published starting value (all-zero `prev_hash`, `sequence_number = 0`). This is the same family of construction as a git commit chain or a blockchain block chain — deliberately simple, and simple is a feature here, not a compromise: the verification routine anyone runs is "recompute the hash of everything since the last head you trust, and check it matches."

Two properties fall out of this directly. **Order-sensitivity**: changing, inserting, or reordering any event changes every subsequent head, so a retroactive edit anywhere in the history is detectable by recomputing forward from any earlier head you already trust. **Tamper-evidence**: because each head folds in the full byte content of the prior head, you cannot forge a later head without knowing (and recomputing from) everything that came before it.

What this construction does **not** give you is what a Merkle tree gives you: a small, logarithmically-sized proof that one specific event is included in a much larger structure without recomputing the whole thing. There is no inclusion proof and no consistency proof in the RFC 6962 (Certificate Transparency) sense here — CoreLink's audit chain does not use a Merkle tree, so those proof types do not apply. Verifying a window of the chain means recomputing the hash chain across that window, which is linear in the number of events in the window, not logarithmic. (Merkle inclusion and consistency proofs are on the post-GA roadmap; see "Roadmap" below.)

The hash function at the chain layer is **BLAKE3**, un-keyed, streamed. BLAKE3 is also used elsewhere in CoreLink for content addressing where REAPI compatibility permits; SHA-256 is used where REAPI compatibility requires it, but the audit chain itself is BLAKE3, end to end.

## Honest guarantee wording (read this section)

The CoreLink audit chain is **tamper-evident** (detect-at-verify), not **tamper-proof**. We are precise on purpose.

- A relying party that holds an **independently-pinned prior head** (one they obtained from CoreLink and then anchored themselves, e.g. by mirroring it into their own records or a third-party witness before any subsequent edit could occur) **can detect** a retroactive edit of a sealed row. The recomputed head will not match.
- A party that holds **only what CoreLink hands them** cannot by itself distinguish an honest chain from an insider-recomputed one for the chain body. An insider with write access to D1 who rewrites a sealed row can in principle recompute the chain forward from that edit; recomputation alone will not catch them, because they are recomputing against their own edit. This is the honest limit of a linear hash chain without an independent witness.
- What raises the bar against a D1 insider **today** is the **signed head** (CF-6). Every checkpoint advance signs the canonical head tuple `(tenant_id, region, head_hash, next_sequence)` with a per-region Ed25519 signing key. On drain resume, the head signature is verified fail-closed: tamper produces a SEV-1 incident and the drain refuses to extend the chain. An insider who rewrites a sealed row changes `head_hash`; the old signature no longer verifies; and they cannot produce a fresh valid signature because they do not hold the signing seed. The signed head is therefore the load-bearing tamper-evidence control for a D1-write insider, not the chain construction in isolation.
- A signed head is not, by itself, a third-party time anchor. "CoreLink signed this head" only proves CoreLink signed it. For a third-party time anchor — RFC 3161 timestamping, public transparency-log witnessing — see "Roadmap" below.

This is the strongest guarantee we can offer at GA and we want customers to reason about it on those terms, not on stronger ones.

## RFC 8785 (JCS): why canonicalization is the unsexy half of the design

JSON is non-deterministic by default. Two services emitting `{"a": 1, "b": 2}` and `{"b": 2, "a": 1}` produce different byte sequences and therefore different hashes. For an audit chain where the customer is going to independently hash the same logical record, that is a correctness bug, not a stylistic preference.

It is also subtler than it sounds. Number serialization is non-trivial: should `1.0` and `1` hash to the same value? (No — they are different JSON values.) Should `1e2` and `100` hash to the same value? (Yes, per RFC 8785 — they denote the same number.) What about NaN and Infinity? (Out of scope — JCS does not serialize them.) String escaping has multiple legal representations. Unicode normalization is its own rabbit hole.

CoreLink canonicalizes every audit leaf using **RFC 8785 — JSON Canonicalization Scheme (JCS)**. JCS pins:

- Key ordering (lexicographic over the UTF-16 code units of keys).
- Number serialization (IEEE 754 with a specified textual form, following the I-JSON profile).
- String escaping (specific escape rules, no unnecessary escapes).
- Whitespace (none).

The canonical byte form is hashed with BLAKE3 at the chain layer. The customer running JCS on their copy of the record produces the same event hash CoreLink stored. There is no implementation freedom in the middle. We publish a reference verification implementation in Rust and TypeScript; both produce bitwise-identical canonical output for any well-formed input.

## The event envelope: CloudEvents 1.0

Each link in the chain is a **CloudEvents 1.0** envelope with CoreLink extensions. The CloudEvents core fields are `specversion`, `type`, `source`, `subject`, `id`, `time`, and `data`. The CoreLink extensions are `tenant_id`, `region`, `sequence_number`, and `prev_hash`. The genesis event has `prev_hash` = the all-zero published starting value and `sequence_number` = 0. The canonical byte form fed into BLAKE3 is the JCS form of this envelope.

## The producer: hourly drain, and the honest pre-seal window

The live producer is the **hourly drain**, exposed internally as `POST /_internal/audit/drain`. Events are written first to a D1 `audit_outbox` table **plain and unchained** as they are emitted by the data path, and the drain then seals them into the chain on the hour.

The honest consequence: there is a **window of up to one hour** between the moment an event lands in the outbox and the moment the drain seals it into the chain. During that window, the row is mutable in D1 and has no chain protection. Once the drain seals it, it is part of the chain and is covered by everything in this post. Before sealing, it is not.

We state this plainly because it is the right way to describe the system: an unsealed outbox row is not yet a chain row, and any tamper-evidence guarantee applies to **sealed** rows. Closing this window — moving to write-time chaining — is on the post-GA roadmap; see "Roadmap" below.

## Append-only as a structural invariant

The append-only property of the **sealed chain** is enforced by the chain construction itself: appending to the hash chain produces a new head whose relationship to the previous head is computable and verifiable by direct recomputation. A retroactive edit to a sealed row would produce a head that does not recompute to match any prior signed head, and that failure is detectable by anyone running the verification routine against an independently-pinned prior head.

A **daily verifier** walks the chain in order, recomputes each link, and compares in constant time. The first divergence fails closed with a `chain_break` meta-audit and a SEV-1 incident. The verifier is the system of record for "is the sealed chain internally consistent right now."

We do **not** at GA enforce append-only at the storage layer with Object Lock retention in COMPLIANCE mode or any equivalent storage-immutability primitive. R2 does not offer Object Lock, and the equivalent for our storage backends is not wired. Storage-layer immutability is on the post-GA roadmap; see "Roadmap" below. This means that today, tamper-evidence rests on the chain construction plus the signed head plus the daily verifier, not on a storage primitive that would prevent rewrites at the storage layer. We want this distinction on the record.

The TLA+ specification `audit_immutability.tla` models the abstract property: for any two chain heads at times `T1 < T2`, the leaf at any index `i ≤ |chain(T1)|` is identical in both. The specification is in the CI loop. CI fails if the model checker finds a counterexample.

## What the customer actually receives

Every state-changing operation in CoreLink emits a structured audit event. Customers can receive these events through two channels at GA:

- **Webhook** (push, near-real-time, with retry and idempotency keys).
- **Audit API** (pull, paginated, with stable cursors).

A third channel — a **daily proof bundle** published to customer-configured R2 storage — is on the post-GA roadmap, along with the scheduled cron that would publish it. See "Roadmap" below.

Each event carries:

- `event_id` (canonical, monotonic per tenant).
- `tenant_id`.
- `event_type` (enumerated; the enumeration is versioned and the version is part of the hashed entry).
- `payload` (operation-specific, JCS-canonicalized).
- `chain_position` (the event's index in the tenant's chain).
- `chain_head_at_emit` (the chain head as of this event).

## The customer verification flow (today)

A customer wishing to verify a single event runs three steps:

1. JCS-canonicalize the event payload.
2. Hash it with BLAKE3 into the chain entry form: `next_hash = BLAKE3(prev_hash_bytes || canonical_bytes(event))`.
3. Confirm the resulting entry, combined with the chain head immediately before it, recomputes to the `chain_head_at_emit` published for that event.

A customer wishing to verify a window — typically what an external auditor wants — runs four:

1. Pull all events in the window via the Audit API.
2. Pull the chain head at the start and end of the window.
3. Recompute the chain forward from the start-of-window head, folding in every event in order.
4. Confirm the recomputed head matches the published end-of-window head (this is the structural append-only check for sealed rows).

The verification is mechanical and the cost is linear in the number of events being checked; the trust is in the math, not in CoreLink. Recomputing a full window is deliberately cheap for CoreLink's per-tenant event volumes — it is not the logarithmic-proof model a Merkle transparency log would give you, and we do not claim it is. The signed head is what a customer compares the recomputed head against; if the signature verifies and the recomputation matches, the window is intact against a D1-write insider.

An open-source verification toolkit (`github.com/HumanGuardrail/corelink-audit-verify`) is a **placeholder** at the time of this draft. We will open the repository in line with GA. Customers should not depend on it being live before then.

## Performance numbers

The chain machinery is engineered to keep audit append off the critical path of data operations. Audit events are emitted to a per-tenant log shard, batched, and folded into the hash chain by the hourly drain. The data-path latency cost of audit emission is bounded by the SLO catalog; current target is sub-millisecond p99 contribution to the parent operation (`DRAFT — final numbers pending CAP-GA-007 staging attestation`).

Verification cost is linear in the size of the window being checked, not logarithmic — a property of the linear hash-chain construction, not a Merkle tree. At the per-tenant event rates we anticipate, recomputing a full day's chain is inexpensive: a single pass over that day's events.

We publish bench numbers at GA-day. Today, in the embargoed launch documents, these numbers are placeholders pending final confirmation against the 30-day sustained staging data.

## Retention

The audit chain is retained against policy `CTRL-AUDIT-005` with a **7-year** retention target. This is a CoreLink-side operational commitment, not a cryptographic property of the chain; the chain itself remains verifiable across the full retention window as long as the raw events and chain heads from that window are available to the verifier.

## What this is not

This design is not a substitute for proper internal audit logging at the customer's side. The CoreLink audit chain records CoreLink-side events: tenant operations against CAS and AC, BYOK key operations, admin-plane actions, billing events, residency operations. It does not record events that occur inside the customer's own systems — those remain the customer's responsibility.

The design is also not a substitute for SOC 2 or equivalent control attestation. The chain proves integrity of the recorded events; SOC 2 attests to the operating effectiveness of the controls that decide which events are recorded in the first place. Both matter; they are complementary.

## Roadmap (post-GA, tracked as WI-S09-007)

The following are honestly planned and **not shipped at GA**. We list them so customers can plan around what is live today versus what is coming.

1. **Storage-layer immutability (R2 Object Lock or equivalent).** R2 does not currently offer Object Lock, and no equivalent storage-immutability primitive is wired at GA. Storage-level append-only in COMPLIANCE-equivalent mode is on the post-GA roadmap. Until then, tamper-evidence rests on the chain construction plus the signed head plus the daily verifier.
2. **Merkle inclusion + consistency proofs and a hosted in-browser proof viewer.** The current construction is a linear chain, so the proof types a Merkle tree would give — `O(log n)` inclusion proofs and consistency proofs between two heads — are not available. They are on the roadmap as a separate increment that would coexist with (not replace) the linear chain. Until shipped, verification is the linear recomputation flow described above.
3. **Customer R2 daily proof-bundle export and its scheduled cron.** Publishing a daily bundle — day's head, prior day's head, signing-key reference — to customer-configured R2 storage is on the post-GA roadmap. At GA, the customer's channels are the Webhook and the Audit API only.
4. **Rekor / sigstore public witness of the signed head.** Mirroring each daily signed head into Rekor's public transparency log would give a public, third-party anchor independent of CoreLink. This is on the post-GA roadmap; threat model is "CoreLink itself is fully compromised" and is not addressed by the current design.
5. **Write-time chaining that closes the ≤1h pre-seal window.** Moving from hourly-drain sealing to sealing at write time would eliminate the window during which an outbox row is mutable in D1 with no chain protection. On the post-GA roadmap.
6. **RFC 3161 third-party timestamping.** For customers whose audit posture requires a third-party time anchor on the daily head, RFC 3161 / RFC 5816 timestamping is on the post-GA roadmap. At GA, the only time anchor is the `time` field on the CloudEvents envelope and the publication discipline around the daily signed head.

We will close these out as funded increments land. Customers who need any of them at GA should talk to us before signing so we can scope around the honest gap.

## Common questions

**How expensive is verification?** It is linear, not logarithmic — recomputing a hash chain over `n` events costs `n` BLAKE3 hashes. There is no Merkle tree here, so there is no `O(log n)` inclusion or consistency proof. At the per-tenant event volumes CoreLink sizes for, a full day's recomputation is inexpensive; we do not claim sub-linear proof sizes.

**How big is the chain?** Per-tenant event rate is the dominant factor. We size for sustained high-throughput tenants; the chain construction is unaffected by rate spikes.

**What hash algorithm?** BLAKE3 at the chain layer (`BLAKE3(prev || event)`), un-keyed, streamed. SHA-256 is used elsewhere in CoreLink where REAPI compatibility requires it; the audit chain is BLAKE3 end to end.

**Where do I find the signing key?** Published at `corelink-docs.humangr.com/trust/signing-keys` with rotation history. The signing key signs the canonical head tuple `(tenant_id, region, head_hash, next_sequence)` per checkpoint advance.

**Is the chain tamper-proof?** No. It is tamper-evident for sealed rows, against a D1-write insider, by virtue of the signed head and the independently-pinnable prior head. See "Honest guarantee wording" above.

**What is the pre-seal window?** Up to one hour. Events are written to a D1 `audit_outbox` plain and unchained at emit time, and the hourly drain seals them into the chain on the hour. During the pre-seal window the row is mutable and not yet chain-protected. Closing this window is on the post-GA roadmap.

## Where to go next

- **Trust center:** `corelink-docs.humangr.com/trust`
- **Audit chain spec:** `corelink-docs.humangr.com/trust/audit-chain`
- **Verification toolkit (open source):** `github.com/HumanGuardrail/corelink-audit-verify` (placeholder pending GA repo open)
- **Daily proof bundle format:** `corelink-docs.humangr.com/trust/proof-bundle` (roadmap; not live at GA)

— Trust Engineering at CoreLink

---

## Internal notes (strip before publish)

- Target length: 1,800–2,200 words.
- Pending review: Security Lead + Compliance Officer (canonical sign-off slots 4 + 9 in WI-S20-008 §16).
- All technical claims trace to INV-AUDIT-APPEND-ONLY / INV-OBS-AUDIT-CHAIN-INTEGRITY / `audit_immutability.tla`. Note: the invariant text itself currently overstates storage-level append-only (it claims Object Lock in COMPLIANCE mode as an enforced control); the prose in this draft is the honest one — chain construction + signed head + daily verifier — and the invariant text should be updated to match (tracked as WI-S09-007).
- No specific dollar amounts. Verification toolkit repo URL is placeholder pending OSS repo open.
- Daily proof-bundle format URL is a roadmap link at GA; not a live artifact.
