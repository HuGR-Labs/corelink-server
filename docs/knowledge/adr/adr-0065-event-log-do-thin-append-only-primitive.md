---
type: "ADR"
title: "ADR-0065 — Event-Log DO: a thin, generic, append-only primitive"
description: "Why CoreLink hosts a domain-agnostic append-only log Durable Object and the consumer (hugit) owns the integrity chain."
source_files:
  - "specs/03_architecture/adrs/ADR-0065-event-log-do-thin-append-only-primitive.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "durable-object", "event-log", "primitive", "append-only", "hugit-p2"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0065 — Event-Log DO: a thin, generic, append-only primitive

The hugit-P2 campaign needed a per-tenant, totally-ordered, append-only event log to back hugit's
integrity chain, and CoreLink had to decide how much of the consumer's chain semantics leak into the
platform. This ADR draws the line: CoreLink hosts the *ordering + durability* primitive and nothing
more — the consumer layers its hash-linking / Merkle / witnessing on top. It is the foundational
decision the transparency-log integration (ADR-0066) and the secrets broker (ADR-0067) sit beside as
sibling hugit-P2 seams. The decision is ACTIVE and ISOLATED — its WP carries no dependency on the
other seams.

# Context

CoreLink already shipped only the `CoreLinkServer` + `RolloutController` Durable Objects; there was no
general-purpose append log, so the open question was whether CoreLink *hosts* hugit's chain (and how
much chain semantics leak in) or provides a generic primitive hugit builds on. A single-writer DO is
the natural ordering primitive; a D1-table log loses the total-order guarantee under concurrency.

# Decision

CoreLink provides a **minimal, generic, append-only event-log Durable Object** — explicitly NOT
chain-aware. The frozen surface is `append(tenant_id, payload) -> { seq, ts_ms }` and
`read(tenant_id, from_seq, limit) -> entries[]`, with a monotonic per-tenant `seq` derived from the
DO's single-writer ordering guarantee and nothing else: no hashing, no Merkle tree, no signatures, no
hash-linking. One DO instance per tenant for isolation, bounded by retention + size caps with a
documented roll-off / R2 archive policy. The consumer layers its integrity chain on top of the raw
ordered log.

# Consequences

- hugit owns and tests its chain logic against this primitive's contract; the crypto-correctness risk
  lives in the consumer, not the shared platform layer.
- CoreLink must document the retention/roll-off and the `seq` monotonicity guarantee as a frozen
  contract because consumers depend on it.
- It pairs with ADR-0066 (the public witness for the consumer's chain) and is a sibling of ADR-0067.

# Citations

1. `specs/03_architecture/adrs/ADR-0065-event-log-do-thin-append-only-primitive.md:24-31` — the
   Context: the need for a per-tenant ordered append log and the host-vs-primitive question.
2. `specs/03_architecture/adrs/ADR-0065-event-log-do-thin-append-only-primitive.md:33-45` — the
   Decision: the minimal generic surface, monotonic `seq`, and the explicit NOT-chain-aware stance.
3. `specs/03_architecture/adrs/ADR-0065-event-log-do-thin-append-only-primitive.md:65-70` — the
   Consequences: consumer owns the chain, CoreLink freezes the retention + monotonicity contract.
