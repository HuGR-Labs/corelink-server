---
type: "ADR"
title: "ADR-S14-002 — Tenant region-pinning enforcement (custom domain authoritative)"
description: "Why the TLS-terminated custom domain (not an X-Region header) is the authoritative request region. The DESIGNED enforcement is a 4-layer fail-closed stack + 30k property test, but that stack is a crate-only skeleton (not in the deployed graph) and the 30k test exercises the skeleton, not the live path — what actually ships is a single header-based HTTP 409 `residency_guard` (see Status vs shipped code)."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md"
  - "crates/corelink-privacy/src/residency.rs"
  - "crates/corelink-container/src/routes/residency.rs"
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

# Status vs shipped code

This ADR narrates the **designed** 4-layer stack, not the deployed one. The full stack
(`region_from_host` / Tower `assert_request_residency` 451 / `assert_write_residency` / the
`region_enforcer` DO) lives **crate-only** as a pure-logic skeleton in `corelink-privacy`
(`crates/corelink-privacy/src/residency.rs:1-8`, per the `trait-abstraction-defer` charter), and the
deployed container does **not** depend on `corelink-privacy` (its privacy dependency is
`corelink-privacy-erasure-worker`). The `region_enforcer` Durable Object does not exist. What ships
and runs in prod is a single header-based guard: the `residency_guard` Tower middleware that rejects a
cross-region request with **HTTP 409 `residency_violation`** before any handler — the
`residency_guard` reject branch returns `residency_violation(&claimed_macro, &own_colo)`
(`crates/corelink-container/src/routes/residency.rs:77`), which emits the literal
`StatusCode::CONFLICT` (409) envelope (`crates/corelink-container/src/routes/residency.rs:124`) — not the 451 path, and residency is **US-only
in reality** (all R2 buckets are ENAM; see [ADR-S14-001](/adr/adr-s14-001-multi-region-terraform-module.md)).
Treat the 4-layer stack as designed intent, the 409 guard as the live control.

# Citations

1. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:24-33` — the cross-region-leak risk and the header-vs-custom-domain question.
2. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:36-52` — custom-domain-authoritative decision and the 4-layer + 30k-test enforcement stack.
3. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:72-80` — header / geo-IP / trigger-only / 10k-test alternatives rejected.
4. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:56-66` — positive and negative consequences.
5. `crates/corelink-privacy/src/residency.rs:1-8` — the 4-layer region-pinning stack ships as a pure-logic skeleton in `corelink-privacy` (NOT a container dep); the `region_enforcer` DO is unbuilt.
6. `crates/corelink-container/src/routes/residency.rs:77` (the `residency_guard` reject branch returning `residency_violation(...)`), `:124` (the literal `StatusCode::CONFLICT` 409 envelope) — the DEPLOYED control: HTTP 409 `residency_violation` on a cross-region request (the header-based live path, not the designed 451 stack).
