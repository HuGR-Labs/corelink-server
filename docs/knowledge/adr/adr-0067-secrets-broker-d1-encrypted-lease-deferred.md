---
type: "ADR"
title: "ADR-0067 — Secrets broker: defer the broker, design as a D1-encrypted lease"
description: "Why the per-tenant secrets broker is deferred for launch and pre-designed as a D1 envelope-encrypted TTL lease that works within Cloudflare's write-only secret constraint."
source_files:
  - "specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "secrets", "broker", "d1", "deferred", "hugit-p2"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0067 — Secrets broker: defer the broker, design as a D1-encrypted lease

hugit-P2 wanted a per-tenant secrets broker (lease a secret with a TTL, rotate, revoke), but the
broker collides with a hard Cloudflare platform constraint: Worker secrets are *write-only* — set via
`wrangler secret put`, never read back by code or API — so a classic broker that hands out the
platform secret is impossible on CF. This ADR makes two moves: defer the full broker for launch (it is
not launch-critical) and record the eventual design now as a D1 envelope-encrypted lease so the build
is later a transcription, not a fresh design. It is the third hugit-P2 seam alongside ADR-0065 /
ADR-0066.

# Context

A naive broker over CF secrets is a non-starter because the store is write-only. The broker is also not
on the launch path, so the live question was how to keep launch lean while still committing to a design
that works within the platform wall and bounds the leak blast radius flagged as the seam's risk.

# Decision

1. **Defer the full broker for launch.** The interim is the existing operator-managed flat-file / env
   PAT model, documented as such.
2. **Design the eventual broker as a D1-encrypted lease.** Secrets live envelope-encrypted in D1 under
   a KMS/root key CoreLink controls — NOT in the CF write-only secret store — so CoreLink *can* read +
   lease its own encrypted secrets, sidestepping the write-only wall. The broker leases a short-lived,
   TTL-bounded decrypted value to the consumer, cached briefly, never exposing a long-lived raw secret;
   rotation/revocation = re-encrypt / drop the D1 row + expire the cache. Plaintext-in-D1 is rejected.

# Consequences

- Launch ships on the flat-file/env interim.
- The eventual broker needs an owner-provisioned KMS/root key, a D1 `secret_lease` table (encrypted
  blob + TTL + version), and the lease/rotate/revoke surface.
- The short lease + TTL cache bounds the blast radius if a lease leaks and enables fast revocation; the
  posture cross-refs the DSR signer key-management direction (same envelope-encryption stance).

# Citations

1. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:24-29` — the
   Context: the write-only CF-secret constraint that blocks a naive broker.
2. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:31-40` — the
   Decision: defer for launch + the D1 envelope-encrypted TTL-lease design.
3. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:57-62` — the
   Consequences: the interim, and the KMS key + `secret_lease` table the eventual broker needs.
4. `specs/03_architecture/adrs/ADR-0067-secrets-broker-d1-encrypted-lease-deferred.md:55` — the
   Alternatives-rejected: plaintext-in-D1 is rejected (secrets must be envelope-encrypted at rest).
