---
type: "ADR"
title: "ADR-0101 — Provider-neutral Object-Lock archive adapter contract"
source_files:
  - "specs/03_architecture/adrs/ADR-0101-object-lock-archive-adapter-contract.md"
source_blobs:
  - "specs/03_architecture/adrs/ADR-0101-object-lock-archive-adapter-contract.md@02d81051b663138d6d0ca207562542ddcae70068"
checkpoint_sha: "e4b5c3cd2b0757eabc82f278555f0dcd40aa216c"
tags: ["adr", "object-lock", "retention", "archive", "capability-gate"]
---

# ADR-0101 — Provider-neutral Object-Lock archive adapter contract

CoreLink chooses a portable archive adapter contract and a verified wrapper
for Compliance retention. The wrapper negotiates immutable put, retain-until
and legal-hold readback, delete denial, residency metadata, and a durable audit
receipt before it exposes an immutable write. It has no fallback backend.

The current R2 path is explicitly unavailable for this contract. No provider,
account, region, credentials, provisioning, or WORM guarantee is approved by
this ADR; those remain the work of #1877.

## Citation

1. `specs/03_architecture/adrs/ADR-0101-object-lock-archive-adapter-contract.md:42-65` — Decision: the portable adapter contract, required capability evidence, verified consumer surface, explicit R2-unavailable behavior, and CI-only in-memory fixture.
2. `specs/03_architecture/adrs/ADR-0101-object-lock-archive-adapter-contract.md:67-79` — Consequences: provider selection remains deferred, current R2 archival stays non-WORM, and failed verification has no fallback.
