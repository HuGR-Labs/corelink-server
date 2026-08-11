<!-- DRAFT — pending Legal + Marketing + CEO sign-off. Do not publish. -->

# BYOK at CoreLink: Envelope Encryption, Four KMS Providers, and Why the Kill Switch Matters

> **DRAFT — pending Marketing + Legal + Crypto SME sign-off.**
> Technical deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 2) · sprint S-14 BYOK · INV-BYOK-CRYPTO-SOVEREIGNTY · INV-ERASURE-ATTESTATION-SIGNED · ADR-S14-004 · ADR-S14-005 · ADR-S14-006.

---

Bring-Your-Own-Key is the most over-claimed term in vendor security. In its weakest form it means "we will allow you to bring an opaque token we still hold the keys to." In its strongest form it means "the vendor's operators cannot read your bytes, the vendor's incident response cannot bypass your revocation, and erasure produces an artifact your auditor can replay." CoreLink ships the strong form on AWS KMS today — with GCP, Azure, and Vault providers on the roadmap — and this post explains how, and where the seams are (including the roadmap ones).

## Why customer-managed keys matter

For unregulated workloads, vendor-managed encryption is fine. The cache vendor encrypts blobs at rest, manages the keys in its own KMS, and rotates them on a schedule. The threat model is narrow: an attacker who reaches the bytes at rest hits ciphertext rather than plaintext. That covers most teams.

It does not cover regulated workloads. A bank shipping internal tooling under banking-secrecy regulation, a healthcare ISV under HIPAA, an EU controller under GDPR Article 32, a Brazilian financial entity under LGPD plus sector-specific CVM/BCB guidance — each of these operates under a regime in which "the vendor holds the keys" is not, by itself, sufficient. The relevant question is not whether bytes are encrypted at rest. The relevant question is: *if you, the vendor, were compelled by lawful order or a malicious insider to produce plaintext, could you?* The acceptable answer in those regimes is "no, structurally."

That answer requires customer-managed keys, and it requires a specific kind of customer-managed key: one where the cryptographic boundary, not the policy boundary, is what stops the vendor from reading the data.

## The threat model we are willing to defend

CoreLink BYOK is designed against three concrete adversaries:

1. **A compromised CoreLink operator.** A vendor insider with full production access must not be able to read tenant plaintext at rest. The cryptographic boundary, not the policy boundary, must enforce this.
2. **A coerced CoreLink operator.** A lawful order compelling CoreLink to decrypt tenant data must produce a result that is operationally impossible without the customer's cooperation. The compelled party should not, in fact, be able to comply unilaterally.
3. **A regulator-driven erasure event.** A customer-initiated erasure must produce a verifiable, signed attestation that an auditor can replay against the customer's own root of trust — not against a vendor-issued certificate.

That third one is where most BYOK implementations quietly fail. We took it seriously.

## Architecture: envelope encryption, KEK at the customer, DEK at the cache

CoreLink uses standard envelope encryption. A customer KMS holds the **Key Encryption Key (KEK)**. CoreLink generates per-object **Data Encryption Keys (DEKs)**, wraps each DEK under the customer's KEK, and stores the wrapped DEK alongside the ciphertext. To read an object, CoreLink calls the customer's KMS to unwrap the DEK, decrypts the object, and discards the unwrapped DEK from a bounded in-memory cache.

The shape is conventional. The implementation choices that matter, and where most BYOK offerings differ, are at the seams.

### Per-blob DEK derivation

Every object has its own DEK. We do not key-share across blobs, across tenants, or across regions. The DEK is generated locally with a hardware random source, wrapped immediately, and the unwrapped form is held only as long as it is needed for the in-progress operation plus a bounded read-cache TTL.

A consequence of per-blob DEKs is that revocation has the granularity customers actually want. A customer can rotate a KEK without re-encrypting blobs (the wrapped DEKs are re-wrapped against the new KEK in an online migration); a customer can erase a tenant scope without touching unrelated tenants' blobs; a customer can disable the KEK and every blob under that KEK becomes unreadable in lockstep.

### Bounded DEK cache

The DEK cache TTL is hard-capped at 5 minutes. This is not a configuration. It is a code path. A long-lived unwrapped DEK is a long-lived bypass of customer revocation; we chose a tight bound and enforced it structurally. The cache is process-local, never replicated, never persisted. A region-local restart re-derives DEKs from the KMS. A regional failover re-derives DEKs from the destination region's KMS endpoint. There is no shadow store of unwrapped DEKs anywhere.

### AAD binding: cryptographic locality

Every DEK wrap and every payload encryption binds Additional Authenticated Data (AAD) containing, at minimum: `tenant_id`, `blob_hash`, and the cache identity. AEAD verification fails if any of these are tampered with. Concretely, a blob written under tenant T cannot be decrypted under a context that claims to be tenant T' — the AAD mismatch causes the AEAD primitive (AES-GCM-256 or ChaCha20-Poly1305 per provider matrix) to reject the operation. This is the cryptographic side of the `INV-TENANT-NO-CROSS-READ` invariant; the structural side is enforced at the data path and modeled in `tenant_isolation.tla`.

### The kill switch

A customer who disables their KEK in their KMS causes every subsequent unwrap call to fail. Within at most 5 minutes (the DEK cache TTL bound), every in-flight DEK expires. After that, CoreLink simply cannot read the tenant's data. We call this property `INV-BYOK-CRYPTO-SOVEREIGNTY` and we treat it as a CRITICAL invariant. The 60-second portion of the practical SLA — the time between the customer's disable action and observable unwrap failure on the data path — is bounded by KMS propagation in each provider; we publish provider-specific revocation-latency numbers in the trust center.

The kill switch under load is the part we are proudest of. The cache does not slow down for everyone when one tenant is being killed; it simply produces hard-fail errors for that tenant's data path while every other tenant proceeds normally. We exercise this in synthetic drill weekly.

## The four providers

CoreLink's BYOK substrate targets four KMS backends around one common envelope-encryption core, each pinned to provider-specific minimum-privilege bindings and module-validation references. **AWS KMS is available at GA; GCP Cloud KMS, Azure Key Vault, and HashiCorp Vault are on the roadmap (rollout in progress — see the provider-status table in the docs).**

| Provider | Module verification | Notes |
|---|---|---|
| **AWS KMS** | FIPS 140-3 (current AWS KMS HSM fleet, post-2024 module list) | IAM-scoped key policy required. CoreLink documents the minimum-privilege key policy. `arn:aws:kms:<region>:<account>:key/<id>` as the CMK reference. |
| **GCP Cloud KMS** | FIPS 140-2 L3 (HSM-backed key tier) | IAM-binding scoped to the CoreLink service identity. Hostnames pinned to FIPS endpoints where the customer's regulatory posture requires. |
| **Azure Key Vault** | FIPS 140-2 L2/L3 (managed HSM tier where required) | RBAC role assignment required. Managed HSM SKU recommended for FIPS L3 posture. |
| **HashiCorp Vault Transit** | FIPS 140-2 L2 (Vault Enterprise with HSM seal) | Documented for self-hosted Vault Enterprise. Transit engine with `convergent_encryption=false` (per-DEK uniqueness). PKCS#11 transit not currently in scope. |

For each provider, CoreLink publishes:

- The minimum-privilege key policy / IAM binding required.
- The key rotation cadence assumed by the integration.
- The expected unwrap-latency budget (p99 ≤ 100 ms for the read path; the cache hit path does not call KMS at all).
- The failure mode if the customer's KMS is unreachable (the data path returns a documented hard-fail error class; no fallback decryption path exists by design).

Specific FIPS-validation references for each provider are maintained at `corelink-docs.humangr.com/trust/byok-providers` and are versioned alongside the integration. When a provider rolls a new module validation, the documentation rolls with it.

### FIPS endpoint selection

For customers under FIPS-relevant procurement, CoreLink supports explicit FIPS-endpoint pinning at the provider connection layer. AWS KMS `kms-fips.<region>.amazonaws.com`, GCP `<region>-cloudkms.googleapis.com` with FIPS hardware-backed key class, Azure Managed HSM, and Vault Enterprise HSM seal are the supported endpoint families. The pinning is configurable per BYOK binding and enforced at the data path; a non-FIPS endpoint will not be used for a FIPS-pinned binding.

## A concrete walk-through

The end-to-end shape of a BYOK write looks like this:

```
$ corelink put \
    --tenant acme-prod \
    --blob ./large-artifact.tar.zst \
    --byok-cmk-arn arn:aws:kms:us-west-2:123456789012:key/abcd-... \
    --aad-binding tenant=acme-prod
```

Behind that single command:

1. CoreLink reads the blob, computes its BLAKE3 digest.
2. CoreLink generates a fresh DEK (AES-256, hardware RNG).
3. CoreLink encrypts the blob with the DEK under AES-GCM-256, binding AAD = `tenant_id || blob_hash || cache_id`.
4. CoreLink calls `kms:Encrypt` against the customer's CMK to wrap the DEK. The KMS call carries the same AAD as encryption context.
5. The wrapped DEK is stored alongside the ciphertext. The unwrapped DEK is discarded immediately on the write path.
6. An audit-chain leaf is appended capturing the operation, the CMK reference, the blob hash, and the AAD context.

The corresponding read is symmetric. If the DEK is in the bounded in-memory cache (within 5 minutes of a prior read), the read does not call KMS. Otherwise the read calls `kms:Decrypt` with the wrapped DEK and the AAD encryption context, receives the unwrapped DEK, decrypts the ciphertext, and serves the blob. The unwrapped DEK enters the cache subject to the 5-minute TTL.

## Rotation, audit, operational details

KEK rotation is a customer operation. CoreLink does not initiate KEK rotation; it observes it. When a customer rolls the CMK, CoreLink re-wraps existing DEKs against the new KEK in an online background job, with progress visible in the customer's dashboard and recorded in the audit chain. Customers can run with multiple active CMK versions; CoreLink chooses the right one per object based on the wrapped-DEK metadata.

Every BYOK operation produces an audit-chain leaf. A customer whose internal audit posture requires reconciling CoreLink's KMS calls against their own KMS unwrap logs can do so directly: the audit-chain leaves carry the CMK reference and a request correlation identifier that matches what the customer's KMS provider logs on its side.

## What CoreLink does not do

Several things some vendors do, and we explicitly do not:

- **No vendor escrow of customer keys.** CoreLink does not hold a copy of the KEK, encrypted or otherwise. There is no break-glass path that re-derives plaintext from CoreLink-side material alone.
- **No operator-readable tier.** CoreLink does not offer a tier in which CoreLink operators can be granted decrypt capability for support purposes. If we cannot read the data, support is debugged through the customer's own audit chain and KMS unwrap logs, not through plaintext access.
- **No soft kill switch.** Kill is final. The customer disables their KEK and the data is unreadable to CoreLink until they re-enable it. We do not offer a kill-with-grace-period because that is, definitionally, not a kill.

## Erasure attestation

When a customer initiates erasure under DSR / DSAR / right-to-erasure, CoreLink performs a verifiable crypto-erasure. A customer-served signed **Ed25519 erasure attestation** — on the near-term roadmap — will contain:

- Tenant identifier and scope of erasure.
- Audit-chain leaf hashes for the erased objects (so the chain remains verifiable after the underlying bytes are gone).
- Timestamp.
- The CoreLink-side signing key identifier (publishable, rotation-tracked at `corelink-docs.humangr.com/trust`).
- A NIST SP 800-88 Rev. 1 crypto-erase classification.

The attestation is retained for 7 years (regulator-driven) and is independently re-verifiable by the customer using CoreLink's published signing key — the invariant `INV-ERASURE-ATTESTATION-SIGNED` makes the signing path a structural requirement.

The auditor's question, "Show me proof you deleted my customer's data," is answered with an artifact, not a screenshot.

## Trade-offs we made

A BYOK implementation is a series of trade-offs. The ones we made consciously:

**Performance vs. revocation latency.** A longer DEK cache TTL would give us higher cache hit performance on warm reads at the cost of a longer kill-switch window. We picked 5 minutes; any shorter starts to hurt operationally, any longer starts to weaken the kill story. The choice is a code path, not a config knob.

**Provider breadth vs. integration depth.** Supporting four providers at GA means each integration is necessarily narrower than a single-provider, deep optimization would allow. We took the trade because customers operate where they operate, and a BYOK that forces a provider switch is not a BYOK customers will adopt.

**Convergent encryption vs. uniqueness.** Some cache vendors use convergent encryption to dedupe ciphertext across tenants. We do not. Cross-tenant ciphertext convergence is, structurally, a side channel that lets the vendor (or an attacker who reaches storage) infer that two tenants hold the same plaintext. We chose per-blob DEKs and per-tenant uniqueness over the dedup gain.

## Common questions

**Does BYOK affect cache hit rate or latency?** Cache-hit reads do not call the customer KMS — the DEK is unwrapped at first read into the bounded in-memory cache and re-used until expiry. Cache-miss reads pay one unwrap RTT. We publish provider-specific latency budgets.

**What happens to my data if my KMS is unavailable?** The data path hard-fails until the KMS is reachable again. CoreLink does not maintain a fallback decryption path. This is a deliberate choice — a fallback is, definitionally, a vendor-side bypass of customer revocation.

**What happens if CoreLink is compelled by lawful order to decrypt?** CoreLink cannot decrypt. The unwrap call is to the customer's KMS, under the customer's policy. We have built the integration so that the compelled party cannot in fact comply unilaterally.

**Do I need BYOK to use CoreLink?** No. BYOK is an Enterprise-tier capability. Team and Free tiers use CoreLink-managed envelope encryption with the same `INV-CAS-INTEGRITY` guarantees but without customer-held KEKs.

## Where to go next

- **Trust center:** `corelink-docs.humangr.com/trust`
- **BYOK provider matrix:** `corelink-docs.humangr.com/trust/byok-providers`
- **Erasure attestation spec:** `corelink-docs.humangr.com/trust/erasure-attestation`
- **Kill switch runbook:** `corelink-docs.humangr.com/runbooks/byok-kill-switch`

— Crypto and Trust at CoreLink

---

## Internal notes (strip before publish)

- Target length: 2,200–2,500 words.
- Pending review: Crypto SME (canonical sign-off slot 11 in WI-S20-008 §16).
- All compliance claims trace to spec contract §5.2 + WI-S20-008 §2.1.2 + sprint S-14 spec contract + INV registry + ADR-S14-004/005/006.
- No specific dollar amounts present.
