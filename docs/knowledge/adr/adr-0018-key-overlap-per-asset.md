---
type: "ADR"
title: "ADR-0018 — INV-KEY-OVERLAP is per-asset-class, not a global 24h"
description: "Resolves three incompatible values for the key-rotation overlap invariant by making it per-asset-class (24h / 7d / 30d) with a 30d hard upper bound."
source_files:
  - "specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "key-management", "rotation", "overlap", "invariant"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0018 — INV-KEY-OVERLAP is per-asset-class, not a global 24h

Key rotation needs an overlap window where both the old and new key are valid, but CoreLink's `INV-KEY-OVERLAP` invariant was defined three incompatible ways across the repo, so the sprints that claimed to follow it actually violated it. This ADR (a DRAFT) makes the invariant explicitly per-asset-class — a fast 24h for short-blast-radius signing keys, 7d for TB-scale re-wrap keys, 30d for long-lived attestation keys — with a 30d hard ceiling. It matters because it gives the property test a real oracle and the SOC 2 auditor a consistent rotation policy. It governs the same key material as [BYOK envelope encryption](/storage/byok-envelope-encryption.md).

# Context

A Round-2 audit found `INV-KEY-OVERLAP` defined with three incompatible values: the key-management doc said "up to 24h" (a single global value), S-13 declared a 7d TDK overlap, and S-14 declared a 30d Ed25519 attestation-key overlap — so the canonical invariant was being violated by the very sprints citing it, leaving the property test with no oracle and NIST SP 800-57 rotation policy unmet (`specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:21-27`).

# Decision

Make `INV-KEY-OVERLAP` per-asset-class, anchored to the canonical key-management table: 24h for PAT-signing, audit-chain, and admin-signing keys (short blast radius, fast rotation); 7d for the tenant derivation key (TDK) and BYOK customer CMK (TB-scale background re-wrap needs a realistic window); 30d for the Ed25519 attestation key (long-lived signing, 7-year attestation retention) — with a hard 30d upper bound requiring an explicit ADR + Security-lead sign-off to exceed (`specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:31-42`). A single global 24h or 7d, and a per-tenant configurable overlap, were all rejected (`specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:64-66`).

# Consequences

- Sprint contracts S-13/S-14 become coherent with the invariant, the property test gets a clear per-asset oracle, and SOC 2 documentation is consistent (`specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:53-56`).
- Cost is a more complex property test (five asset classes) and per-asset documentation overhead for any new class (`specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:58-60`).

# Citations

1. `specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:21-27` — the three incompatible overlap values and the resulting violation.
2. `specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:31-42` — the decision: per-asset-class table + 30d hard bound.
3. `specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:64-66` — rejected single-global and per-tenant alternatives.
4. `specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:53-56` — coherence, oracle, and audit consequences.
5. `specs/03_architecture/adrs/ADR-0018-key-overlap-per-asset.md:58-60` — property-test and documentation cost.
