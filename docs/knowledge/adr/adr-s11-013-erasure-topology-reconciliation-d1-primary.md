---
type: "ADR"
title: "ADR-S11-013 — Erasure Backend Topology Reconciliation (canonical → shipped D1-primary)"
description: "How the 12-backend erasure model is reconciled to the shipped D1-primary reality — preserving the canonical contract while remapping each adapter's real transport and ratifying the D1 erase-vs-retain classification."
source_files:
  - "specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s11", "erasure", "dsr", "topology", "gdpr"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S11-013 — Erasure Backend Topology Reconciliation (canonical → shipped D1-primary)

The canonical erasure model assumes a Neon-Postgres-primary topology where D1 holds only `blob_meta` + `ac_meta`; the shipped system consolidated the control-plane and billing into **D1 + Clerk**, so D1 actually holds nearly all subject PII. Implementing the spec literally would erase 2 of ~25 PII-bearing tables — a GDPR Art. 17 / LGPD Art. 18 VI violation. This ADR reconciles the model to reality: the canonical 12-backend contract is preserved unchanged, but each adapter's real transport is remapped, and the owner-ratified D1 erase-vs-retain classification becomes the load-bearing change.

# Context

The canonical model and the 12 backend stubs describe a Neon-primary topology. Cold-verification of the shipped system (2026-06-11) contradicts it: Neon holds only `audit_events_shadow` (2 migrations), the account/consent tables live only in the deferred auth-spine migration (auth ships as Clerk, not deployed in prod), and D1 (`corelink-config-prod`) holds all subject PII — `tenant` (email_hash, clerk_user_id, stripe_customer_id), `pilot_signups` (plaintext email), `pat`, and ~20 more tenant-keyed tables. Implementing the spec literally would no-op the Neon adapters and erase only 2 of ~25 PII tables, leaving emails and identity undeleted — a GDPR Art. 17 erasure-incompleteness violation of the `INV-DATA-ERASURE-COMPLETE` invariant (`specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:29-54`).

# Decision

Reconcile the 12-backend canonical model to the shipped **D1-primary** topology. The canonical contract — `canonical_backend_kinds()`, the `dsr_erasure_log.backend` CHECK enum, the 12-adapter validation order — is preserved unchanged; what changes is what each adapter's transport *does* (`specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:56-62`). The remapping: `NeonMain`/`NeonBilling` become `NotApplicable` (control-plane is in D1); `R2Cas` is a refcount-aware delete (only when `blob_meta.refcount → 0`); `R2Ac` deletes per-tenant prefixes across 5 region buckets; `D1` is REAL + EXPANDED to hard-delete the full erase-set; `Stripe` does `Customer.update` PII-nullify (never `Customer.delete`, GAAP/fiscal); the 4 WORM backends pseudonymize (`specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:64-79`). The owner-ratified (2026-06-11) D1 erase-vs-retain classification names the ERASE-SET (subject PII, no retention basis: `tenant`, `tier_selections`, `pat`, `pilot_signups`, etc.) and the RETAIN-SET (legal/fiscal/audit basis, never hard-deleted, pseudonymize instead: `dsr_erasure_log`, the `stripe_*` mirror, audit tables) (`specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:81-113`).

# Consequences

Erasure is correct for the shipped system rather than literal to an unshipped spec, with no silent GDPR gap; the canonical 12-backend contract and `dsr_erasure_log` schema are unchanged (only transport semantics + one additive `BackendCompletion.subject_id_hash` field); and nothing deletes in prod until owner provisioning (queues + `ERASURE_SALT_KEY` + internal-auth key) — Wave 1 builds an inert, verified pipeline the owner flips live (`specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:135-146`). The R2 erasure design was later resolved: the multipart/chunk CAS path is not shipped, so `R2Cas` lists by prefix across all 5 regions (complete by construction) and `R2Ac` is D1-driven off `ac_meta`, both deriving `tenant_prefix` fail-CLOSED (`specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:157-172`).

# Citations

1. `specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:29-54` — Context: canonical Neon-primary vs shipped D1-primary reality + the GDPR-incompleteness risk.
2. `specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:56-62` — Decision: preserve the canonical contract, remap transports.
3. `specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:64-79` — the reconciled per-backend transport mapping.
4. `specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:81-113` — the owner-ratified D1 erase-set / retain-set classification.
5. `specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:135-146` — Consequences + prod-inert safety gate.
6. `specs/03_architecture/adrs/ADR-S11-013-erasure-topology-reconciliation-d1-primary.md:157-172` — the resolved R2 erasure design (LIST-by-prefix CAS + D1-driven AC).
