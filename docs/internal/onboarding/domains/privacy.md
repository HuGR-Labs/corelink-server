# Domain — Privacy (DSR + consent + residency)

> **Estimated effort:** ~30 hours over Week 2 (denser than other
> domains — privacy spans law + code + ops).
> **Prerequisites:** Day 1–5 complete. *Strongly recommended:* skim
> GDPR Art. 15–22 once before starting. LGPD equivalence helps too.
> **Mentors (rotated quarterly):**
> - Primary: DPO (or DPO delegate — non-engineer; legal counterpart).
> - Secondary: TBD (DSR pipeline owner — engineering).
> - Tertiary: TBD (residency / data-path engineer).

## Why this domain matters

CoreLink is sold into regulated industries. DSR (Data Subject Request)
handling — access, export, deletion — has 30-day legal deadlines.
Residency is structurally enforced: four enumerated regions (WNAM,
ENAM, WEUR, SAM), no cross-region leak. Consent state drives whether
ML / analytics features can read tenant data. Mess any of these up and
the consequence is regulatory, not just engineering.

## Must-read (in order)

1. `specs/03_architecture/privacy_model.md` — the canonical model.
2. `marketing/launch/BLOG-POSTS/04-multi-region-residency.md` — what
   we sell to regulated buyers.
3. `crates/corelink-dsr/src/lib.rs` — DSR fulfillment pipeline
   (access, export, erasure).
4. `crates/corelink-erasure-attestation/src/lib.rs` — Ed25519 proof
   that erasure happened (cross-link to BYOK domain).
5. `crates/corelink-residency-policy/` (if present — else read the
   residency invariants in `invariant_registry.md`).
6. `specs/_legal/DPA-*.md` — Data Processing Addendum templates;
   read at least one in full.
7. `specs/_runbooks/RB-DPA-CHANGE.md` — operational procedure for DPA
   amendments.
8. `crates/corelink-dpa-acceptance/src/lib.rs` — DPA acceptance
   tracking (audited).
9. `crates/corelink-dpa-versioning/src/lib.rs` — DPA versioning model.
10. GDPR Art. 15 (right of access), Art. 17 (right to erasure), Art. 28
    (processor obligations). Reference, not deep-read.

## Hands-on exercises

1. **DSR access run.** Issue a synthetic DSR-access request against
   the staging tenant `tenant_dsr_smoke`. Watch the pipeline produce
   the export bundle. Verify the bundle is signed and that residency
   metadata is correct.
2. **DSR erasure + attestation.** Same tenant, now run DSR-erasure.
   Read the resulting Ed25519 attestation. Verify the signature
   yourself with `openssl` or `corelink-client-verify`. Confirm
   subsequent reads of the tenant fail-CLOSED.
3. **Residency invariant.** Pick a CAS-write code path. Trace where
   the residency check happens. Construct a property test that
   confirms a `Region::WEUR` write cannot land in a `Region::WNAM`
   bucket. (If the test already exists, identify it.)

## What "comfortable in this domain" looks like by Day 30

- You can name the four residency regions and the structural
  enforcement point (data-path, not config).
- You can sketch the DSR pipeline (intake → fulfillment → export →
  attestation) on a whiteboard.
- You understand the difference between *consent withdrawal*
  (downgrades feature exposure) and *erasure* (deletes data + emits
  attestation).
- You have shipped at least one privacy-touching PR.

## Common pitfalls

- **30-day legal deadline is non-negotiable.** Any code that delays
  DSR fulfillment crosses a regulatory line, not a UX line. If you
  see latency creep in DSR fulfillment, treat it as P1.
- **Residency is structural, not configurational.** Never wire a
  cross-region call behind a feature flag "for testing". Build a
  per-region staging tenant instead.
- **Consent state is not a boolean.** It is a 4-state machine (granted
  / withdrawn / never-prompted / required-but-unprompted). Read the
  model before assuming.
- **DPO is your mentor for the legal side.** They are not on the
  engineering Slack; book time explicitly. They will catch
  misunderstandings that engineers can't.
