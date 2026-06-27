---
type: "ADR"
title: "ADR-S14-005 — BYOK 4-provider semantics (Azure AAD flow + Vault mTLS + cross-provider matrix)"
description: "Provider-specific BYOK decisions: a two-layer AES-GCM inner key to give Azure wrapKey AAD binding, mTLS + cert pinning for customer-hosted Vault, and parse-time cross-provider tamper detection."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md"
  - "crates/corelink-byok/src/lib.rs"
  - ".github/workflows/byok_matrix_weekly.yml"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s14", "byok", "azure", "vault", "mtls"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-005 — BYOK 4-provider semantics (Azure AAD flow + Vault mTLS + cross-provider matrix)

Extending the `KmsProvider` trait of ADR-S14-004 to all four providers surfaced three provider-specific problems that needed explicit ratification: Azure Key Vault's `wrapKey` has no native AAD, customer-hosted HashiCorp Vault needs strong mutual auth, and DEK tampering must be detectable without a shared secret across providers. This ADR (ACTIVE, WI-S14-005) records the solutions and makes the 16-cell matrix a mandatory CI gate.

# Context

The provider-agnostic `wrap_dek(dek, key_id, encryption_context)` interface must work across AWS/GCP/Azure/Vault, but three provider-specific facts force design decisions: Azure `wrapKey` does not support AAD, Vault is customer-hosted and needs mTLS with base64 Transit context, and cross-provider DEK tampering must be detectable without a shared secret (`specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:24-36`).

# Decision

- **Azure two-layer AAD flow**: generate an inner AES-256-GCM key `K_inner`, encrypt the DEK under it with the `encryption_context` as AAD, then Azure-`wrapKey(K_inner)` via RSA-OAEP-256; the GCM tag cryptographically binds the AAD so any context tamper fails on unwrap (`specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:39-77`).
- **Vault mTLS + CA pinning + 30d expiry alert**: CoreLink presents a client cert, pins the customer's CA hash (rejecting rogue-CA MITM), and fires a SEV-3 `MtlsCertExpiringSoon` at ≤30 days; the Transit `context` is base64(JSON(ctx)), re-verified at decrypt (`specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:78-107`).
- **Cross-provider tamper detection**: the `WrappedDek.provider` field is re-verified at unwrap, each provider uses an incompatible ciphertext layout (so a swapped row fails at parse before any KMS call), and a detected mismatch emits a CRITICAL `corelink.byok.cross_provider_tamper_attempt` audit event (`specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:108-130`).
- **16-cell matrix CI gate** (4 providers × 4 ops) auto-fails the PR on any broken cell, with a weekly staging cron catching provider API drift within 7 days (`specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:132-142`).

# Consequences

- Positive: AAD binding enforced cryptographically for all 4 providers; mTLS is the strongest auth for customer-hosted Vault; cross-provider tampering is detectable at parse time; the matrix gate keeps provider regressions off main (`specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:146-150`).
- Negative / trade-offs: Azure adds ~2ms (inner GCM layer); Vault mTLS pushes cert management to the customer (mitigated by the expiry alert + renewal runbook); Azure's FIPS 140-2 L2 is below AWS/Vault's FIPS 140-3 L1, so FIPS-140-3-strict customers should use AWS or Vault Enterprise (`specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:152-157`).

This builds on the trait defined in [ADR-S14-004 — BYOK adapter trait + envelope-encryption flow](/adr/adr-s14-004-byok-trait-envelope-encryption.md).

# Status vs shipped code

BYOK is **unwired in the deployed container**. The provider semantics ship as crate code under the
`corelink-byok` umbrella (`crates/corelink-byok/src/lib.rs:1-10`, re-exporting `KmsProvider` / `Dek` /
`WrappedDek` / `EnvelopeEncryptor` from `corelink-byok-core`), but no live CAS blob is BYOK-encrypted at
rest — at-rest BYOK envelope encryption is listed as pure-logic-skeleton / not-wired in CLAUDE.md's
"Designed ≠ wired". Two further mismatches with the Decision prose: the 16-cell matrix runs as a
**weekly staging cron** (`.github/workflows/byok_matrix_weekly.yml:16-19`, `cron: '17 6 * * 1'` +
`workflow_dispatch`), **not** a per-PR CI gate; and the consequences (Azure ~2ms, Vault mTLS, parse-time
tamper detection) describe the designed flow, not a control exercised on production traffic. Treat this
ADR as ratified design + tested crate logic, not a live at-rest-encryption posture.

# Citations

1. `specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:24-36` — the three provider-specific problems.
2. `specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:39-77` — the Azure two-layer AAD flow.
3. `specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:78-130` — Vault mTLS + pinning and cross-provider tamper detection.
4. `specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:132-142` — the 16-cell matrix CI gate.
5. `specs/03_architecture/adrs/ADR-S14-005-byok-gcp-azure-vault.md:146-157` — positive consequences and trade-offs.
6. `crates/corelink-byok/src/lib.rs:1-10` — BYOK ships as crate code (`corelink-byok` umbrella) but is unwired in the deployed container (no live at-rest BYOK-encrypted CAS blob).
7. `.github/workflows/byok_matrix_weekly.yml:16-19` — the 16-cell matrix runs as a WEEKLY staging cron (`cron '17 6 * * 1'` + `workflow_dispatch`), NOT the per-PR CI gate the Decision prose claims.
