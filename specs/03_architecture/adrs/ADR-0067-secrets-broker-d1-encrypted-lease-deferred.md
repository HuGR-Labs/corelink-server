---
id: "ADR-0067"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-12"
updated: "2026-06-12"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "secrets", "broker", "hugit-p2", "d1", "deferred"]
---

# ADR-0067 — Secrets Broker: Defer the Broker; Design as a D1-Encrypted Lease

## Status

ACTIVE — tech-lead decision (hugit-P2 seam F, 2026-06-12). WP-F; the broker is
**deferred** for launch, design recorded for the eventual build.

## Context

hugit-P2 wants a per-tenant **secrets broker** (lease a secret to a consumer with a TTL,
rotate, revoke). This is **platform-constrained**: Cloudflare Worker secrets are
**write-only** — set via `wrangler secret put`, never read back by code or API. So a
classic "broker that hands out the platform secret" is impossible on CF.

## Decision

1. **Defer the full broker for launch** — it is not launch-critical. Interim: the existing
   operator-managed flat-file / env PAT model.
2. **Design the eventual broker as a D1-encrypted lease** (recorded now so the build is
   mechanical later): secrets live **envelope-encrypted in D1** (encrypted under a
   KMS/root key CoreLink controls — NOT in the CF write-only secret store); the broker
   leases a **short-lived, TTL-bounded** decrypted value to the consumer, cached briefly,
   never exposing a long-lived raw secret. Rotation/revocation = re-encrypt / drop the D1
   row + expire the cache.

## Rationale

- **Scope control:** the broker isn't on the launch path; deferring keeps launch lean.
- **Works WITHIN the CF constraint:** CoreLink owns the D1 envelope + the root key, so it
  *can* read+lease its own encrypted secrets — sidestepping the write-only-CF-secret wall
  that blocks a naive broker.
- **Lease + TTL cache mitigates the seam-F "TOUCHES" risk** flagged in the wave plan
  (short cache → bounded blast radius if a lease leaks; fast revocation).

## Alternatives rejected

- **Build the broker now:** scope creep on a non-launch-critical seam.
- **Broker over CF secrets directly:** impossible (write-only); a non-starter on-platform.
- **Plaintext-in-D1:** rejected — secrets must be envelope-encrypted at rest.

## Consequences

- Launch ships on the flat-file/env interim (documented as such).
- The eventual broker needs: a KMS/root key (owner-provisioned), a D1 `secret_lease`
  table (encrypted blob + TTL + version), and the lease/rotate/revoke surface.
- Cross-refs the DSR signer key-management direction (same envelope-encryption posture).

## References

- CLAUDE.md (CF secrets are write-only). hugit-P2 handoff (seam F).
- ADR-0065 / ADR-0066 (sibling hugit-P2 seams).
