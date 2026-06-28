---
type: "ADR"
title: "ADR-S14-002 — Tenant region-pinning enforcement (custom domain authoritative)"
description: "Why the TLS-terminated edge (not an X-Region header) is the authoritative request region. ADR specifies a 4-layer fail-closed stack + 30k property test; STATUS — the live container wiring is a single header→HTTP 409 router guard (residency.rs) + the D1 immutability trigger + the property test, not the full 4-layer/451 stack."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md"
  - "crates/corelink-container/src/routes/residency.rs"
checkpoint_sha: "03c2ae27deb7094fea4009927b90959533dae21e"
provenance: "AUTHORED"
tags: ["adr", "s14", "region", "residency", "schrems-ii"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-002 — Tenant region-pinning enforcement (custom domain authoritative)

With CoreLink serving four regions, a tenant pinned to WEUR served by the ENAM endpoint routes EU PII to US infrastructure — a Schrems II + LGPD Art. 33 catastrophe. This ADR (ACCEPTED) settles the central question — where `request_region` is derived from — in favor of the TLS-terminated edge over an attacker-controllable header, and SPECIFIES a four-layer fail-closed enforcement stack plus a 30k property test for INV-REGION-NO-CROSS-LEAK.

> **Status vs shipped code (read this first).** The 4-layer 451 stack described below is the ADR's **design target (spec text)**, not the live shape. What is actually wired in the container today is **ONE** thing: a single router layer `residency_guard` (`crates/corelink-container/src/routes/residency.rs:62`) that reads the edge-stamped, trusted `x-corelink-primary-region` header, compares the claimed macro-region to the colo THIS container serves, and returns **HTTP 409 `residency_violation`** (NOT 451) before any handler runs — fail-closed on an absent-colo-mapping / unknown macro. Of the four ADR layers: the D1 immutability trigger `trg_tenant_primary_region_immutable` IS shipped (migrations 0028/0064); `assert_request_residency` / `assert_write_residency` exist only as methods on a **test-only** enforcer in `crates/corelink-privacy/tests/residency_property_region_pinning_30k.rs` (the 30k property test), not as a live Tower layer; and the per-region `region_enforcer` Durable Object does NOT exist in code (it appears only in SQL comments). So treat "4-layer / 451 / SEALED" as the planned stack, and "single 1-line header→409 guard + immutability trigger + property test" as shipped reality.
>
> **Scope caveat — LOGICAL only, NOT physical CAS residency.** This guard enforces *logical-region consistency*: it 409s when the edge-stamped macro-region label disagrees with the colo this container serves (`residency_decision` compares the claimed macro → its expected colo against the container's own colo). It does **NOT** enforce *physical CAS residency*. At launch CAS is a **single US bucket** (`corelink-cas-prod`); the region is only a key-prefix within that one bucket, and the SAM bucket (`corelink-cas-sam`) is undeployed — per [ADR-S14-009 — CAS residency single-bucket launch posture](/adr/adr-s14-009-cas-residency-single-bucket-launch-posture.md). So a 409 never means "EU bytes are stored on EU disk"; physical at-rest residency is single-bucket-US and is NOT guaranteed by this 409 guard. Don't overstate the 409 as physical-residency enforcement.

# Context

Without runtime enforcement a WEUR-pinned tenant can be served by ENAM, routing EU PII to US infra (Schrems II / LGPD Art. 33, €20M+ exposure). The key decision is the source of `request_region`: an attacker-controlled `X-Region` header vs. the TLS-terminated custom domain (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:24-33`).

# Decision

The **TLS-terminated edge is authoritative** for `request_region` and client-supplied `X-Region` headers are ignored (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:36`). The ADR SPECIFIES a four-layers-plus-tests enforcement stack (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:38-52`) — see the Status note above for which layers are actually wired:

- `region_from_host` parses `{tenant_id}.{region}.corelink.humangr.com`, returning `None` (fail-closed) for unknown regions.
- (SPEC) A Tower `assert_request_residency` layer runs after auth and before any handler/backend, mapping a mismatch to HTTP 451. SHIPPED EQUIVALENT: the `residency_guard` router layer returns HTTP **409** on a cross-region request (`crates/corelink-container/src/routes/residency.rs:62`); the `assert_request_residency` symbol exists only in the property test.
- (SPEC) `assert_write_residency` is a per-write insert-check backstop for every backend write. STATUS: test-only symbol; no live per-write residency backstop is wired.
- (SHIPPED) A D1 trigger `trg_tenant_primary_region_immutable` makes `primary_region` immutable post-INSERT (migrations 0028/0064). (SPEC, NOT SHIPPED) a per-region `region_enforcer` DO caching `tenant_id → primary_region` (5min TTL, D1 fallback) — the DO does not exist in code.
- (SHIPPED) A 30k-iteration property test requires 0 cross-region leaks (`crates/corelink-privacy/tests/residency_property_region_pinning_30k.rs`).

Rejected: `X-Region` header (trivially spoofed, no TLS guarantee), geo-IP (probabilistic), D1-trigger-only (rejects writes but not reads), and a 10k test (insufficient confidence for a CRITICAL invariant) (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:72-80`).

# Consequences

- Positive (as SPECIFIED): client `X-Region` spoofing is impossible; defense-in-depth across 4 layers; ~99.999% confidence from the 30k test; Schrems II TIA + GDPR Art. 46 + LGPD Art. 33 attestation supported (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:56-61`). NOTE: the full 4-layer defense-in-depth is the design target; shipped today is the single edge-header 409 guard + immutability trigger + property test (see Status note).
- Negative: the `region_enforcer` DO adds ~$60/mo, the 5min TTL is a bounded stale-cache window (D1 fallback mitigates), and the immutability trigger forces a manual ticket for legitimate region migration (`specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:63-66`).

This is intended to enforce at runtime the region topology established by [ADR-S14-001 — Multi-region Terraform module](/adr/adr-s14-001-multi-region-terraform-module.md); the live runtime enforcement today is the `residency_guard` header→409 layer + the `primary_region` immutability trigger (the broader stack is deferred per the Status note).

# Citations

1. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:24-33` — the cross-region-leak risk and the header-vs-custom-domain question.
2. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:36-52` — custom-domain-authoritative decision and the 4-layer + 30k-test enforcement stack.
3. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:72-80` — header / geo-IP / trigger-only / 10k-test alternatives rejected.
4. `specs/03_architecture/adrs/ADR-S14-002-region-pinning-enforcement.md:56-66` — positive and negative consequences.
5. `crates/corelink-container/src/routes/residency.rs:62` — the LIVE shipped control: the `residency_guard` router layer (edge-stamped header → HTTP 409 `residency_violation`, fail-closed), the single-layer reality vs the spec's 4-layer/451 stack.
