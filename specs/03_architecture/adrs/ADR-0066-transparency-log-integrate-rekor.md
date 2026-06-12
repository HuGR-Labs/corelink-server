---
id: "ADR-0066"
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
tags: ["adr", "transparency-log", "rekor", "sigstore", "hugit-p2", "audit"]
---

# ADR-0066 — Transparency Log: Integrate Public Rekor/sigstore, Do Not Rebuild

## Status

ACTIVE — tech-lead decision (hugit-P2 seam E, 2026-06-12). WP-E; ISOLATED, async/post-hoc.

## Context

hugit-P2 wants tamper-evident **public** witnessing of audit/attestation entries.
Today `routes/audit_export.rs` produces per-tenant audit exports with **Merkle proofs**,
but there is **no public witness** — a tenant must trust CoreLink's own export. The
question: build a CoreLink-operated transparency log, integrate a public one, or defer?

## Decision

**Integrate the public Rekor transparency log (sigstore) — do not build our own.**
CoreLink provides a thin **submission seam**: take a signed entry → submit to the public
Rekor instance → record the returned inclusion proof / log index alongside the entry.
The existing Merkle-proof export stays as the *private* artifact; Rekor adds the
*public, independently-verifiable* witness anyone can check without trusting CoreLink.

## Rationale

- **Don't rebuild a solved public good.** A trustworthy transparency log (append-only,
  gossip-able, independently audited) is hard; sigstore/Rekor already operate one at
  scale with public trust we cannot match by self-hosting.
- **Stronger trust property.** A public witness is verifiable *against* CoreLink, not
  *via* CoreLink — exactly what "transparency" should mean.
- **Minimal surface + async.** CoreLink is a submitter, not a log operator; submission
  is post-hoc and off the write path (no latency added to the hot path).

## Alternatives rejected

- **CoreLink-operated transparency log:** large scope, weaker trust (self-witnessing),
  ongoing operational burden.
- **Defer entirely:** leaves the private Merkle export as the only artifact — acceptable
  for launch, but the public witness is the differentiator hugit-P2 asked for; integrating
  Rekor is cheap enough to do rather than defer.

## Consequences

- Add a Rekor client + store `{log_index, inclusion_proof}` per witnessed entry.
- Network dependency on the public Rekor instance → submission is best-effort + retried
  out-of-band; a Rekor outage degrades witnessing, never the write path (fail-open on the
  witness, the entry itself is already durably logged via ADR-0065).

## References

- `routes/audit_export.rs` (existing Merkle-proof export).
- ADR-0065 (the append log being witnessed). hugit-P2 handoff (seam E).
