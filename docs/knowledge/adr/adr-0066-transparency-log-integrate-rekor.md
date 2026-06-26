---
type: "ADR"
title: "ADR-0066 — Transparency log: integrate public Rekor/sigstore, do not rebuild"
description: "Why CoreLink submits witnessed entries to the public Rekor transparency log instead of operating its own."
source_files:
  - "specs/03_architecture/adrs/ADR-0066-transparency-log-integrate-rekor.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "transparency-log", "rekor", "sigstore", "audit", "hugit-p2"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0066 — Transparency log: integrate public Rekor/sigstore, do not rebuild

hugit-P2 wanted tamper-evident *public* witnessing of audit/attestation entries, and CoreLink's
existing per-tenant Merkle-proof export is only a *private* artifact a tenant must trust CoreLink to
produce. This ADR chooses to integrate the public Rekor (sigstore) transparency log rather than build
or self-host one — turning the trust property from "verifiable *via* CoreLink" into "verifiable
*against* CoreLink." It is the public-witness sibling of the raw append log (ADR-0065) being witnessed,
ACTIVE and async/post-hoc so it never touches the write hot path.

# Context

`routes/audit_export.rs` produced per-tenant audit exports with Merkle proofs but there was no public
witness, so a tenant had to trust CoreLink's own export. The question was whether to build a
CoreLink-operated transparency log, integrate a public one, or defer entirely. A trustworthy
transparency log (append-only, gossip-able, independently audited) is hard to operate, and sigstore
already runs one at scale with public trust CoreLink cannot match by self-hosting.

# Decision

**Integrate the public Rekor transparency log — do not build our own.** CoreLink provides a thin
submission seam: take a signed entry, submit it to the public Rekor instance, and record the returned
inclusion proof / log index alongside the entry. The existing Merkle-proof export stays as the private
artifact; Rekor adds the public, independently-verifiable witness anyone can check without trusting
CoreLink. CoreLink is a *submitter*, not a log operator, and submission is post-hoc and off the write
path so it adds no hot-path latency.

# Consequences

- Add a Rekor client and store `{log_index, inclusion_proof}` per witnessed entry.
- A network dependency on the public Rekor instance means submission is best-effort + retried
  out-of-band; a Rekor outage degrades witnessing but never the write path — the entry is already
  durably logged via ADR-0065 (fail-open on the witness).
- Rejected alternatives: a CoreLink-operated log (large scope, weaker self-witnessing trust) and
  deferring entirely (leaves only the private Merkle export, losing the differentiator hugit-P2 asked
  for).

# Citations

1. `specs/03_architecture/adrs/ADR-0066-transparency-log-integrate-rekor.md:23-28` — the Context: the
   private-only Merkle export and the build-vs-integrate-vs-defer question.
2. `specs/03_architecture/adrs/ADR-0066-transparency-log-integrate-rekor.md:30-36` — the Decision: the
   thin Rekor submission seam recording the inclusion proof, off the write path.
3. `specs/03_architecture/adrs/ADR-0066-transparency-log-integrate-rekor.md:48-61` — the rejected
   alternatives (CoreLink-operated log / deferring entirely) and the Consequences: the Rekor client +
   stored proof and the fail-open witnessing posture.
