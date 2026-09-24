# Ownership wave 006 — BYOK and privacy

Four-artifact, source/static-only contract from
[WAVE_001_PLAN.md](WAVE_001_PLAN.md#frozen-artifact-contract). This is the
expanded authoring lane: authors remain conflict-free and never edit shared
state; reviews and integration remain centralized.

The existing OKF bundle is the verified canonical reference for relevant
cross-cutting concepts. Authors route to it and apply package-specific facts;
they do not re-validate, duplicate, fork, or silently redefine OKF policy.

| WP | Package | Profile | Owned paths |
|---|---|---|---|
| W006-BYOK | `corelink-byok` | H | `own-corelink-byok`, `crates/corelink-byok/` |
| W006-ATTEST | `corelink-erasure-attestation` | S | `own-corelink-erasure-attestation`, `crates/corelink-erasure-attestation/` |
| W006-PRIVACY | `corelink-privacy` | H | `own-corelink-privacy`, `crates/corelink-privacy/` |
| W006-ERASURE | `corelink-privacy-erasure-worker` | H | `own-corelink-privacy-erasure-worker`, `crates/corelink-privacy-erasure-worker/` |
| W006-PSEUDO | `corelink-privacy-pseudonymize` | S | `own-corelink-privacy-pseudonymize`, `crates/corelink-privacy-pseudonymize/` |
| W006-APPROVAL | `corelink-dual-approval` | S | `own-corelink-dual-approval`, `crates/corelink-dual-approval/` |

## Static anchors and exclusions

- **BYOK:** local umbrella/core/provider/revocation code, public provider
  features and mutually-exclusive `compile_error!` gates are source contracts.
  No provider selection, KMS key, HTTP, credential, encryption operation or
  native/wasm execution follows from them.
- **Attestation:** signing/canonicalization/evidence/key/verify surfaces are
  source territory. R2/D1 retention/index, key rotation and verification
  service operation are unobserved external composition.
- **Privacy:** hybrid absorbed local modules and external thin facades must be
  separated. Re-export does not transfer DSR, erasure worker, pseudonymizer or
  DPA implementation ownership, nor establish retention/runtime operation.
- **Erasure worker:** high-complexity local orchestration and backend traits;
  distinguish its fake/static contracts from R2/D1/Neon/KV/Stripe/Loki/queue/
  cron/provider operation. Regulatory descriptions are not legal certification.
- **Pseudonymize:** source helper and marker verification only; no live salt,
  KMS, data or irreversible erasure conclusion.
- **Dual approval:** source gate/nonce/HMAC/collusion rules only; no real
  identity, MFA, authority, audit delivery or administrative operation claim.

All authors use `apply_patch`, run the declared profile checker and
`git diff --check`, and commit only four owned paths. A fresh reviewer returns
four separate verdicts after authored bytes; no issue may be emitted.
