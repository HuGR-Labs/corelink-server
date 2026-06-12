---
id: "ADR-0065"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-12"
updated: "2026-06-12"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "durable-object", "event-log", "hugit-p2", "primitive", "append-only"]
---

# ADR-0065 — Event-Log DO: a Thin, Generic, Append-Only Primitive (CoreLink hosts, consumer owns the chain)

## Status

ACTIVE — tech-lead decision (hugit-P2 seam D, 2026-06-12). Implementation tracked
as WP-D (`worker/src/event_log_do.ts`); ISOLATED (no dependency on other seams).

## Context

The hugit-P2 front (campaign #3) needs a **per-tenant, append-only, totally-ordered
event log** to back hugit's integrity chain. Today CoreLink ships only the
`CoreLinkServer` + `RolloutController` Durable Objects (`worker/src/{durable_object,
rollout_controller}.ts`) — there is no general-purpose append log. The open question:
does CoreLink **host** hugit's log (and if so, how much of hugit's chain semantics
leak into CoreLink), or does CoreLink provide a generic primitive that hugit builds on?

## Decision

CoreLink provides a **minimal, generic, append-only event-log Durable Object**
primitive — NOT a chain-aware one:

- Surface: `append(tenant_id, payload) -> { seq, ts_ms }` and
  `read(tenant_id, from_seq, limit) -> entries[]`. Monotonic per-tenant `seq` from
  the DO's single-writer ordering guarantee; nothing else.
- **NOT chain-aware:** no hashing, no Merkle tree, no signatures, no hash-linking.
  The consumer (hugit) layers its integrity chain (hash-linked entries, Merkle roots,
  witnessing — see ADR-0066) **on top** of the raw ordered log.
- One DO instance per tenant (isolation). Bounded by retention + size caps (the DO
  storage budget); old entries roll off / archive to R2 on a documented policy.

## Rationale

- **Generic + reusable:** an ordered append log is useful to any consumer; keeping it
  domain-agnostic means CoreLink doesn't take on hugit's (or any one consumer's)
  integrity model.
- **Right separation of concerns:** the DO's job is *ordering + durability*; integrity
  semantics (what a "valid chain" means) belong to the consumer that defines them.
- **Smaller, auditable surface:** two methods, no crypto — easy to reason about and
  test; the crypto risk lives in the consumer, not the shared primitive.

## Alternatives rejected

- **Chain-aware log DO** (CoreLink computes hash-links / Merkle roots): couples CoreLink
  to hugit's integrity model, is less reusable, and concentrates crypto-correctness risk
  in the shared platform layer.
- **D1-table append log** (no DO): loses the single-writer total-order guarantee under
  concurrency; the DO is the right ordering primitive.

## Consequences

- hugit owns + tests its chain logic against this primitive's contract.
- CoreLink must document the retention/rolloff + the `seq` monotonicity guarantee as a
  frozen contract (consumers depend on it).
- Pairs with ADR-0066 (the public witness for the consumer's chain).

## References

- `worker/src/{durable_object,rollout_controller}.ts` (existing DOs).
- hugit-P2 handoff `docs/handoff/2026-06-11-corelink-response-hugit-p2-waveplan.md` (seam D).
- ADR-0066 (transparency log) · ADR-0067 (secrets broker).
