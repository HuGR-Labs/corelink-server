---
type: "ADR"
title: "ADR-0101 — Provider-neutral Object-Lock archive adapter contract"
source_files:
  - "specs/03_architecture/adrs/ADR-0101-object-lock-archive-adapter-contract.md"
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

The authoritative decision is recorded in
`specs/03_architecture/adrs/ADR-0101-object-lock-archive-adapter-contract.md`.
