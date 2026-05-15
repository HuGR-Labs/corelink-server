# Domain — BYOK (Bring Your Own Key)

> **Estimated effort:** ~25 hours over Week 2.
> **Prerequisites:** Day 1–5 onboarding complete; comfortable reading
> async Rust; rough idea of envelope encryption.
> **Mentors (rotated quarterly — confirm with your manager):**
> - Primary: TBD (cryptography lead)
> - Secondary: TBD (BYOK platform engineer)
> - Backup: TBD (KMS provider integration owner)

## Why this domain matters

BYOK is one of three product wedges (see
`marketing/launch/BLOG-POSTS/01-introducing-corelink.md`). Four KMS
providers (AWS, GCP, Azure, Vault Transit) sit behind a single trait;
customers hold the kill switch; erasure produces a signed Ed25519
attestation. The 5-minute DEK-cache TTL is a code-path bound — not a
config knob. Touch this code carelessly and you can break the
strongest claim on our website.

## Must-read (in order)

1. `specs/03_architecture/key_management.md` — canonical envelope-encryption
   model (KEK/DEK split, cache bounds, rotation cadence).
2. `specs/03_architecture/security_model.md` §BYOK — invariants
   (DEK-cache TTL hard cap, kill-switch propagation deadline).
3. `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md` — the customer
   pitch; ground truth for "what we claim".
4. `crates/corelink-byok/src/lib.rs` — public trait + the four
   provider impls behind it.
5. `crates/corelink-byok-aws/src/lib.rs` — reference provider impl;
   the other three follow the same shape.
6. `crates/corelink-byok-revocation/src/lib.rs` — kill-switch path.
7. `crates/corelink-byok-vault/src/lib.rs` — Vault Transit (most
   different from the cloud KMS providers; worth reading for contrast).
8. `crates/corelink-byok-matrix-test/` — cross-provider conformance
   harness. Read at least one test.
9. `crates/corelink-erasure-attestation/src/lib.rs` — Ed25519
   attestation generation.
10. `specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md` — operational rotation
    procedure (system CMK; per-tenant CMKs use a different runbook).

## Hands-on exercises

1. **DEK-cache TTL invariant.** Find the const / fn that bounds DEK
   cache lifetime. Modify it locally to 6 minutes. Run the relevant
   property tests. Document which tests fail and why. Revert.
2. **Add a new field to the erasure attestation.** Append an optional
   `tenant_region: Option<Region>` to the attestation payload (gated
   behind `#[non_exhaustive]`). Update the Ed25519 signing input,
   regenerate test fixtures, ensure round-trip parses. Open as a draft
   PR for review (don't merge — this exercise is throwaway).
3. **Kill-switch dry-run.** Read `scripts/byok_kill_switch_drill.sh`.
   Execute it against the staging tenant `tenant_byok_smoke`. Observe
   the audit chain leaves emitted. Time the propagation. Compare to
   the SLO in `specs/03_architecture/slo_catalog.md`.

## What "comfortable in this domain" looks like by Day 30

- You can name the four KMS providers and one quirk of each.
- You can sketch the KEK → DEK → blob-encryption sequence on a
  whiteboard without notes.
- You can explain *why* DEK cache TTL is a code bound rather than a
  config bound (hint: trust model).
- You have shipped at least one BYOK-touching PR (even if just a test).

## Common pitfalls

- Don't add caching anywhere in the DEK path without consulting
  cryptography lead. The 5-min bound depends on no incidental caches.
- Don't log DEKs, KEK material, or anything derived from them — even
  in `debug!` macros. Clippy has a custom lint for this; if it fires,
  *do not* `#[allow]` it.
- Vault Transit's API differs subtly from the cloud KMS providers
  (specifically: it returns plaintext keys; the cloud providers don't
  unless you ask). Read the `corelink-byok-vault` tests carefully.
