---
type: "ADR"
title: "ADR-S14-002 — Tenant region-pinning enforcement (custom domain authoritative)"
description: "Why the TLS-terminated custom domain (not an X-Region header) is the authoritative request region, enforced by a 4-layer fail-closed stack + 30k property test."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s14", "region", "residency", "schrems-ii"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-002 — Tenant region-pinning enforcement (custom domain authoritative)

With CoreLink serving four regions, a tenant pinned to WEUR served by the ENAM endpoint routes EU PII to US infrastructure — a Schrems II + LGPD Art. 33 catastrophe. This ADR (ACCEPTED, WI-S14-002 SEALED) settles the central question — where `request_region` is derived from — in favor of the TLS-terminated custom domain over an attacker-controllable header, and backs it with a four-layer fail-closed enforcement stack and a 30k property test for INV-REGION-NO-CROSS-LEAK.

# Context

Without runtime enforcement a WEUR-pinned tenant can be served by ENAM, routing EU PII to US infra (Schrems II / LGPD Art. 33, €20M+ exposure). The key decision is the source of `request_region`: an attacker-controlled `X-Region` header vs. the TLS-terminated custom domain (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:24-33`).

# Decision

The **custom domain is authoritative** for `request_region` and `X-Region` headers are ignored (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:36`). The enforcement stack is four layers plus tests (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:38-52`):

- `region_from_host` parses `{tenant_id}.{region}.corelink.humangr.com`, returning `None` (fail-closed) for unknown regions.
- A Tower `assert_request_residency` layer runs after auth and before any handler/backend, mapping a mismatch to HTTP 451.
- `assert_write_residency` is a per-write insert-check backstop for every backend write.
- A D1 trigger `trg_tenant_primary_region_immutable` makes `primary_region` immutable post-INSERT, and a per-region `region_enforcer` DO caches `tenant_id → primary_region` (5min TTL, D1 fallback).
- A 30k-iteration property test (PR gate; 100k nightly) requires 0 cross-region leaks.

Rejected: `X-Region` header (trivially spoofed, no TLS guarantee), geo-IP (probabilistic), D1-trigger-only (rejects writes but not reads), and a 10k test (insufficient confidence for a CRITICAL invariant) (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:72-80`).

# Consequences

- Positive: `X-Region` spoofing is impossible; defense-in-depth across 4 layers; ~99.999% confidence from the 30k test; Schrems II TIA + GDPR Art. 46 + LGPD Art. 33 attestation supported (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:56-61`).
- Negative: the `region_enforcer` DO adds ~$60/mo, the 5min TTL is a bounded stale-cache window (D1 fallback mitigates), and the immutability trigger forces a manual ticket for legitimate region migration (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:63-66`).

This enforces at runtime the region topology established by [ADR-S14-001 — Multi-region Terraform module](/adr/adr-s14-001-multi-region-terraform-module.md).

# Citations

1. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:24-33` — the cross-region-leak risk and the header-vs-custom-domain question.
2. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:36-52` — custom-domain-authoritative decision and the 4-layer + 30k-test enforcement stack.
3. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:72-80` — header / geo-IP / trigger-only / 10k-test alternatives rejected.
4. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:56-66` — positive and negative consequences.
