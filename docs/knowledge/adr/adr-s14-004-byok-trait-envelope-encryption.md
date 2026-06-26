---
type: "ADR"
title: "ADR-S14-004 — BYOK adapter trait + envelope-encryption flow"
description: "The six BYOK envelope-encryption decisions: random CSPRNG DEK, 5-min hard DEK-cache TTL, 96-bit random GCM nonce, mandatory AAD context binding, async KmsProvider trait, and the 16-cell matrix test."
source_files:
  - "specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s14", "byok", "crypto", "kms", "envelope-encryption"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S14-004 — BYOK adapter trait + envelope-encryption flow

CoreLink's BYOK (Bring Your Own Key) enterprise tier must hold four properties at once: crypto sovereignty (customer CMK revocation makes the cache inaccessible ≤5 min), multi-cloud portability across 4 KMS providers, FIPS 140-3 compliance, and structural prevention of DEK reuse / nonce reuse / cross-blob key swap. This ADR (ACCEPTED) records the six contentious envelope-encryption decisions that satisfy them, and is the trait foundation the other-provider and kill-switch ADRs build on.

# Context

BYOK must simultaneously deliver crypto sovereignty (revoke → inaccessible ≤5 min via 60s detection + 5min DEK TTL), portability across AWS/GCP/Azure/Vault with one behavior contract, FIPS 140-3 compliance, and security guarantees that the DEK is never deterministic, the nonce never reused, and cross-blob key swap is prevented (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:34-47`).

# Decision

- **D1 — random CSPRNG DEK, not BLAKE3-derived**: a deterministic DEK would make one blob's compromise compromise all blobs with the same input; NIST SP 800-57 §5.6.2 mandates an approved RBG (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:51-68`).
- **D2 — DEK cache TTL 5 min hard**: `DekCache::new` rejects `ttl_seconds > 300` with no override/advisory mode, because the kill-switch SLA (60s + 5min) is non-waivable per the spec contract (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:72-88`).
- **D3 — AES-256-GCM 96-bit random nonce per write**: GCM nonce reuse under one key is catastrophic; 96-bit random needs ~2^48 writes for a birthday collision, and distributed counter-nonces are prohibitive in stateless multi-region Workers (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:92-101`).
- **D4 — mandatory AAD `encryption_context = {tenant_id, blob_hash}`**: without it an intercepted wrapped DEK for B1 can be substituted for B2 (NIST SP 800-130 §6.2 cross-entity binding) (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:105-114`).
- **D5 — `KmsProvider` async trait** with a generic `EnvelopeEncryptor<P>`, allowing monomorphised production and `dyn` test paths without pinning to one SDK (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:118-126`).
- **D6 — 16-cell matrix test** (4 providers × 4 ops) with Pending (non-blocking) cells until each adapter lands, catching provider drift in CI (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:130-139`).

# Consequences

- Positive: DEK compromise blast radius bounded per-write; kill-switch SLA enforced in code; cross-blob swap structurally prevented; multi-cloud extension is additive; matrix test prevents silent provider divergence (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:145-150`).
- Negative / trade-offs: random DEK needs a KMS call on every non-cached read (mitigated by the 5-min cache), `async-trait` adds a box allocation per call, and the 5-min cache is a window where revoked access is still served (contractually accepted at 6min p99) (`specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:152-158`).

This trait is extended to the other three providers in [ADR-S14-005 — BYOK 4-provider semantics](/adr/adr-s14-005-byok-gcp-azure-vault.md) and consumed by the kill switch in [ADR-S14-006](/adr/adr-s14-006-byok-kill-switch-no-operator-override.md).

# Citations

1. `specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:34-47` — the four simultaneous BYOK requirements.
2. `specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:51-101` — D1 random DEK, D2 5-min hard TTL, and D3 96-bit random nonce.
3. `specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:105-139` — D4 mandatory AAD binding, D5 async KmsProvider trait, and D6 matrix test.
4. `specs/03_architecture/adrs/ADR-S14-004-byok-trait-envelope-encryption.md:145-158` — positive consequences and trade-offs.
