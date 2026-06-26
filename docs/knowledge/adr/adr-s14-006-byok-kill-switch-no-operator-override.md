---
type: "ADR"
title: "ADR-S14-006 — BYOK CMK revocation kill switch (hard-fail, no operator override)"
description: "Why CMK revocation propagates in <=5 min via 60s detection + 300s hard DEK-cache TTL, with a compile-time absence of any operator override and conservative network-partition degrade."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s14", "byok", "kill-switch", "crypto-sovereignty"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-006 — BYOK CMK revocation kill switch (hard-fail, no operator override)

Crypto sovereignty — the customer's ability to revoke their Customer-Managed Key and have CoreLink immediately cease all access — is the entire value proposition of BYOK enterprise; without an enforced kill switch, BYOK is theater. This ADR (ACTIVE) records three hard decisions: a ≤5 min global revocation SLA, the compile-time *absence* of any operator override, and conservative (read-only) degrade on network partition.

# Context

BYOK grants crypto sovereignty; three questions needed explicit decisions: how fast must revocation propagate (≤5 min p99 per INV-BYOK-CRYPTO-SOVEREIGNTY), should an operator ever bypass the kill switch (no), and on a KMS-unreachable partition should CoreLink assume revoked (conservative) or accessible (optimistic) (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:24-39`).

# Decision

- **D-1 — ≤5 min SLA** via two composed hard limits: a 60s KMS access-check cadence and a 300s hard DEK-cache TTL (`DekCache::new` rejects `ttl_seconds > 300`), giving a 360s (6 min) worst-case customer-perceived total (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:43-56`).
- **D-2 — no operator override, by compile-time absence**: there is no bypass/disable/advisory-mode/override API on `RevocationDetector` or field on `RevocationConfig`, code review must hard-reject any PR adding one, and the invariant is non-waivable — because an override path would be found by SOC 2 CC6.1 / GDPR Art. 32 auditors and permanently lose customer trust (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:58-74`).
- **D-3 — conservative degrade on partition**: after ≥3 consecutive failed cycles (~3 min) the tenant degrades to read-only, because a false-positive notification is recoverable (≤60s) but a silent revocation miss is not (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:76-89`).
- **D-4 / D-5**: no pre-kill countdown UI (explicit revoke = intent; recovery is fast) and a multi-channel customer alert returning `Ok` if ≥1 channel succeeds (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:91-102`).

# Consequences

- Positive: customer crypto sovereignty enforced; SOC 2 CC6.1 + GDPR Art. 17/32 + LGPD Art. 38 satisfied; weekly chaos drill verifies the SLA in staging (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:116-119`).
- Neutral/negative (accepted): check_access adds ~$10/mo, and conservative degrade may briefly disrupt a customer on a partition, recoverable in ≤60s (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:120-122`).
- Enforces INV-BYOK-CRYPTO-SOVEREIGNTY (cache inaccessible ≤5 min, no bypass via cached DEK > 5 min) and INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (`specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:126-129`).

The 300s DEK-cache TTL this kill switch relies on is the D2 decision of [ADR-S14-004 — BYOK adapter trait](/adr/adr-s14-004-byok-trait-envelope-encryption.md).

# Citations

1. `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:24-39` — the three kill-switch design questions.
2. `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:43-56` — D-1: the 60s + 300s composed ≤5-min SLA.
3. `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:58-89` — D-2 compile-time no-override and D-3 conservative partition degrade.
4. `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:91-102` — D-4 no-countdown and D-5 multi-channel alert.
5. `specs/03_architecture/adrs/ADR-S14-006-byok-kill-switch-no-operator-override.md:116-129` — consequences and the invariants enforced.
