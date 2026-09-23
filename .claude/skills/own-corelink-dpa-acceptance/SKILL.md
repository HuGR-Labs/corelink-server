---
name: own-corelink-dpa-acceptance
description: >-
  Own the pure-logic DPA consent engine when changing its six-field capture plus
  server-stamped timestamp,
  locale/hash enforcement, idempotency, RS256 receipt, audit, notification,
  service, or store contract. Do not use it to operate legal content, keys,
  storage, onboarding traffic, or deployment.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-dpa-acceptance
  manifest: crates/corelink-dpa-acceptance/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-dpa-acceptance-structural-normalization-20260921
---

# Ownership — corelink-dpa-acceptance

Candidate guide grounded in source and static references only; it proves neither production wiring nor runtime operation.

[Trigger](#s01) · [Authority](#s02) · [Reading](#s03) · [Decisions](#s04) · [Flow](#s05) · [Stops](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Trigger

Use for `ConsentProofPayload`, `LocaleBcp47`, `DpaAcceptanceService`, receipt signing/verification, audit event taxonomy, sink traits, or in-memory idempotency. The source describes six captured controls plus a server-stamped `submission_ts`. Do not use it as authority for D1, RSA key provisioning, legal notice publication, email delivery, HTTP status mapping, or live onboarding.

<a id="s02"></a>
## S02 — Authority and boundary

This package owns pure logic in `audit`, `error`, `jwt`, `locale`, `notify`, `schema`, `service`, and `store`, plus their public re-exports. It owns neither a legal notice corpus nor a configured key, durable store, transport, or route. `corelink-privacy`, `corelink-container`, and `e2e-signup-flow` are static direct consumers; static references do not establish their execution.

<a id="s03"></a>
## S03 — Reading route

| Question | Read |
|---|---|
| Six-field input and receipt shape | [R04](../../../docs/ownership/crates/corelink-dpa-acceptance/REFERENCE.md#r04) |
| Required ordering and invariants | [R05](../../../docs/ownership/crates/corelink-dpa-acceptance/REFERENCE.md#r05) |
| Consumer and effect boundary | [B03](../../../docs/ownership/crates/corelink-dpa-acceptance/BLAST_RADIUS.md#b03) |
| Static review procedure | [M01–M06](../../../docs/ownership/crates/corelink-dpa-acceptance/MAINTENANCE.md#m01) |

<a id="s04"></a>
## S04 — Decisions and invariants

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Six fields, locale, hash, or timestamp changes | Trace API-001/002 and INV-001/002 | `schema.rs`, `locale.rs`, `service.rs` | Legal text or client compatibility is required |
| Replay/idempotency changes | Trace API-003 and INV-003 | `service.rs`, `store.rs`, named property targets | Store atomicity or traffic semantics are needed |
| JWT algorithm, claims, `kid`, or key wrapper changes | Trace API-004 and INV-004 | `jwt.rs`, `service.rs` | A real key, rotation state, or receipt corpus is needed |
| Audit/notification ordering changes | Trace API-005 and INV-005 | `audit.rs`, `notify.rs`, `service.rs` | Delivery guarantees beyond traits are asserted |

<a id="s05"></a>
## S05 — Static workflow

1. Confirm the pinned baseline and manifest.
2. Read only the affected module, API, invariant, relation, and procedure.
3. Classify source fact, static consumer reference, and unknown separately.
4. Define the negative predicate before changing locale, hash, replay, or signature behavior.
5. Preserve the declared boundary: traits/fakes are not production adapters.
6. Hand off final bytes for independent review; author validation is not cold review.

<a id="s06"></a>
## S06 — Stop conditions

Stop and escalate if a claim needs a configured RSA key or `kid`, canonical legal text/version, D1 state or migration result, notification delivery, route mounting, user traffic, deployment, or runtime behavior. Also stop when a direct consumer requires a compatibility decision not supported by source.

<a id="s07"></a>
## S07 — Evidence and handoff

Report baseline, changed modules, API/INV/REL/PROC identifiers, source paths, static consumer set, and explicit unknowns. Record only commands actually run and their exit status. A documentary checker, source reading, or self-review never establishes runtime, deployment, or cold-review approval.
