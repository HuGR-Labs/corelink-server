# BYOK at CoreLink: Envelope Encryption, Four KMS Providers, and Why the Kill Switch Matters

> **DRAFT — pending Marketing + Legal + Crypto SME sign-off.**
> Target length: 1,500–3,000 words. Technical deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 2) · sprint S-14 BYOK · INV-BYOK-CRYPTO-SOVEREIGNTY · INV-ERASURE-ATTESTATION-SIGNED.

---

Bring-Your-Own-Key is the most over-claimed term in vendor security. In its weakest form it means "we will allow you to bring an opaque token we still hold the keys to." In its strongest form it means "the vendor's operators cannot read your bytes, the vendor's incident response cannot bypass your revocation, and erasure produces an artifact your auditor can replay." CoreLink ships the strong form across four KMS providers, and this post explains how — and where the seams are.

## The threat model we are willing to defend

CoreLink BYOK is designed against three concrete adversaries:

1. **A compromised CoreLink operator.** A vendor insider with full production access must not be able to read tenant plaintext at rest. The cryptographic boundary, not the policy boundary, must enforce this.
2. **A coerced CoreLink operator.** A lawful order compelling CoreLink to decrypt tenant data must produce a result that is operationally impossible without the customer's cooperation. The compelled party should not, in fact, be able to comply unilaterally.
3. **A regulator-driven erasure event.** A customer-initiated erasure must produce a verifiable, signed attestation that an auditor can replay against the customer's own root of trust — not against a vendor-issued certificate.

That third one is where most BYOK implementations quietly fail. We took it seriously.

## Architecture: envelope encryption, KEK at the customer, DEK at the cache

CoreLink uses standard envelope encryption. A customer KMS holds the **Key Encryption Key (KEK)**. CoreLink generates per-object **Data Encryption Keys (DEKs)**, wraps them under the customer's KEK, and stores the wrapped DEK alongside the ciphertext. To read an object, CoreLink calls the customer's KMS to unwrap the DEK, decrypts the object, and discards the unwrapped DEK from a bounded in-memory cache.

The seams that matter:

- **DEK cache TTL is hard-capped at 5 minutes.** This is not a configuration. It is a code path. A long-lived unwrapped DEK is a long-lived bypass of customer revocation; we chose a tight bound and enforced it structurally.
- **DEK cache is process-local, never replicated, never persisted.** A region-local restart re-derives DEKs from the KMS. A regional failover re-derives DEKs from the destination region's KMS endpoint. There is no "shadow store" of unwrapped DEKs anywhere.
- **The kill switch is at the customer.** A customer who disables their KEK in their KMS causes every subsequent unwrap call to fail. Within at most 5 minutes (the DEK cache TTL bound), every in-flight DEK expires. After that, CoreLink simply cannot read the tenant's data. We call this property `INV-BYOK-CRYPTO-SOVEREIGNTY` and we treat it as a CRITICAL invariant.

## The four providers

CoreLink supports four KMS backends at GA:

| Provider | Module verification | Notes |
|---|---|---|
| **AWS KMS** | FIPS 140-3 (current AWS KMS HSM fleet) | IAM-scoped key policy required. CoreLink documents the minimum-privilege key policy. |
| **GCP KMS** | FIPS 140-2 L3 (HSM-backed key tier) | IAM-binding scoped to the CoreLink service identity. |
| **Azure Key Vault** | FIPS 140-2 L2/L3 (managed HSM tier where required) | RBAC role assignment required. |
| **HashiCorp Vault** | FIPS 140-2 L2 (Vault Enterprise with HSM seal) | Documented for self-hosted Vault Enterprise. PKCS#11 transit not currently in scope. |

For each provider, CoreLink publishes:

- The minimum-privilege key policy / IAM binding required.
- The key rotation cadence assumed by the integration.
- The expected unwrap-latency budget (p99 ≤ 100 ms for the read path; the cache hit path does not call KMS at all).
- The failure mode if the customer's KMS is unreachable (the data path returns a documented hard-fail error class; no fallback decryption path exists by design).

Specific FIPS-validation references for each provider are maintained at `docs.corelink.dev/trust/byok-providers` and are versioned alongside the integration.

## What CoreLink does not do

Several things that some vendors do, and we explicitly do not:

- **No "vendor escrow" of customer keys.** CoreLink does not hold a copy of the KEK, encrypted or otherwise. There is no break-glass path that re-derives plaintext from CoreLink-side material alone.
- **No "operator-readable" tier.** CoreLink does not offer a tier in which CoreLink operators can be granted decrypt capability "for support purposes." If we cannot read the data, support is debugged through the customer's own audit chain and KMS unwrap logs, not through plaintext access.
- **No "soft kill switch."** Kill is final. The customer disables their KEK and the data is unreadable to CoreLink until they re-enable it. We do not offer a kill-with-grace-period because that is, definitionally, not a kill.

## Erasure attestation: the artifact your auditor wants

When a customer initiates erasure under DSAR / DSR / right-to-erasure, CoreLink produces a signed **Ed25519 erasure attestation** containing:

- Tenant identifier and scope of erasure.
- Audit-chain leaf hashes for the erased objects (so the chain remains verifiable after the underlying bytes are gone).
- Timestamp.
- The CoreLink-side signing key identifier (publishable, rotation-tracked at `corelink.dev/trust`).
- A NIST SP 800-88 Rev. 1 crypto-erase classification.

The attestation is retained for 7 years (regulator-driven) and is independently re-verifiable by the customer using CoreLink's published signing key — the invariant `INV-ERASURE-ATTESTATION-SIGNED` makes the signing path a structural requirement.

The auditor's question, "Show me proof you deleted my customer's data," is answered with an artifact, not a screenshot.

## What the customer signs up for

BYOK is a shared-responsibility model. CoreLink's commitment is to provide the envelope-encryption substrate, the bounded DEK cache, the kill-switch semantics, and the attestation artifact. The customer's commitment is to:

- Hold the KEK in a KMS with appropriate FIPS validation for the customer's regulatory posture.
- Configure the IAM / RBAC policy correctly per CoreLink's published minimum-privilege spec.
- Monitor KMS unwrap call patterns as part of the customer's own audit posture.
- Operate the kill switch through the KMS, not through CoreLink — that is the point.

We document this shared-responsibility split in plain language at `docs.corelink.dev/trust/byok-responsibility`.

## Common questions

**Does BYOK affect cache hit rate or latency?** Cache-hit reads do not call the customer KMS — the DEK is unwrapped at first read into the bounded in-memory cache and re-used until expiry. Cache-miss reads pay one unwrap RTT. We publish provider-specific latency budgets.

**What happens to my data if my KMS is unavailable?** The data path hard-fails until the KMS is reachable again. CoreLink does not maintain a fallback decryption path. This is a deliberate choice — a fallback is, definitionally, a vendor-side bypass of customer revocation.

**What happens if CoreLink is compelled by lawful order to decrypt?** CoreLink cannot decrypt. The unwrap call is to the customer's KMS, under the customer's policy. We have built the integration so that the compelled party cannot in fact comply unilaterally.

**Do I need BYOK to use CoreLink?** No. BYOK is an Enterprise-tier capability. Team and Free tiers use CoreLink-managed envelope encryption with the same `INV-CAS-INTEGRITY` guarantees but without customer-held KEKs.

## Where to go next

- **Trust center:** `corelink.dev/trust`
- **BYOK provider matrix:** `docs.corelink.dev/trust/byok-providers`
- **Erasure attestation spec:** `docs.corelink.dev/trust/erasure-attestation`
- **Kill switch runbook:** `docs.corelink.dev/runbooks/byok-kill-switch`

— Crypto & Trust at CoreLink

---

## Internal notes (strip before publish)

- Word count: ~1,500. Within target.
- Pending review: Crypto SME (canonical sign-off slot 11 in WI-S20-008 §16).
- All compliance claims trace to spec contract §5.2 + WI-S20-008 §2.1.2 + sprint S-14 spec contract + INV registry.
- No specific dollar amounts present.
