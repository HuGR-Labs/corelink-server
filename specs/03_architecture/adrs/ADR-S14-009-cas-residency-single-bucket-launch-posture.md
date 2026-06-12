---
id: "ADR-S14-009"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-11"
updated: "2026-06-11"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s14", "residency", "cas", "schrems-ii", "lgpd", "gdpr", "r2", "launch-posture"]
---

# ADR-S14-009 — CAS Residency: Single-Bucket Launch Posture + Per-Region-Bucket Remediation

## Status

ACTIVE — launch posture decision (tech-lead, owner-delegated 2026-06-11). The
remediation (Option A below) is specified + tracked, gated on R2 bucket
provisioning (bundled with the launch-secrets gate).

## Context

The native CAS (content-addressed storage) write path — `R2CasHandler` in
`crates/corelink-container/src/storage/r2_s3.rs` — writes whole blobs to a
**single R2 bucket** (`corelink-cas-prod`, S3 API) with the storage region
carried only in the **key prefix** (`<region>/<tenant_prefix>/<digest>`). The
region comes from a **global** `R2_CAS_REGION` env var (default `iad`), read in
`routes/cas.rs`; the container does **not** consult the tenant's immutable
`tenant.primary_region` (migration 0028) at write time.

This was cold-verified 2026-06-11 (task #29) and has **two layers**:

1. **Missing per-region var.** `wrangler.toml` regional envs (`prod-sam`/`-lhr`/
   `-nrt`/`-syd`) set `R2_AC_REGION` + `R2_CHUNK_REGION` but **omit
   `R2_CAS_REGION`** → every regional container default-tags CAS keys `iad`.
2. **Single physical bucket (the load-bearing gap).** Even with the var fixed,
   a single `corelink-cas-prod` bucket has **one physical R2 location**.
   Region-in-key is a *logical* tag, not *physical* residency. By contrast the
   **AC** path (`corelink-ac-<region>`) and the (not-yet-shipped) **chunk** path
   (`corelink-chunk-<region>`) use **per-region buckets** for physical
   separation. CAS has none. Schrems II / LGPD Art. 16 require EU-subject bytes
   to reside **physically** in-region — a logical key prefix does not satisfy
   this.

So the **minimal** "add `R2_CAS_REGION`" fix is a **false-confidence trap**: it
makes keys look region-correct while bytes stay co-located. It must not ship as
"the residency fix".

## Decision

**Launch with single-region CAS (the current single-bucket reality), recorded
as a KNOWN, explicit limitation — NOT a silent gap — and gate
residency-sensitive EU customers until the remediation (Option A) lands.**

Rationale:

- The DSR erasure pipeline (WI-S11-008 Wave 1) just shipped + was verified
  against the single-bucket topology (the `R2Cas` adapter lists the 5 region
  *prefixes* within the one bucket). Re-cascading that topology immediately,
  pre-launch, adds avoidable risk to freshly-verified GDPR-critical code.
- The complete fix (per-region CAS buckets) requires **R2 bucket provisioning**
  (an operator/deploy action, like the AC buckets + the launch-secrets gate) —
  it cannot be fully completed in code alone, so it is naturally owner-gated.
- Pre-launch the CAS cache is empty (prod D1 was rebuilt 0-row), so the
  remediation has **zero data-migration cost now or shortly after** — there is
  no urgency penalty to sequencing it as a fast-follow.
- This is an **explicit, documented, tracked** decision (this ADR + task #29),
  authorised by the owner's delegation of the call — it satisfies the
  zero-silent-debt mandate (the gap is visible, scoped, and scheduled).

### Launch guardrail

Until Option A lands, **do not onboard a customer with a contractual EU
data-residency requirement** for CAS bytes, OR set the `corelink-cas-prod`
bucket's R2 **jurisdiction** to the strictest required region. The AC path is
already per-region; this guardrail concerns native CAS blobs only.

## Option A — remediation (specified; gated on bucket provisioning)

Make CAS physically region-resident, mirroring the AC/chunk pattern:

1. **Provision** per-region buckets `corelink-cas-<region>` (sam/iad/lhr/nrt/syd)
   in the correct R2 jurisdictions (operator step; bundle with the
   launch-secrets gate).
2. **wrangler.toml**: set `R2_CAS_BUCKET = "corelink-cas-<region>"` +
   `R2_CAS_REGION = "<region>"` in each regional env (the container already reads
   `R2_CAS_BUCKET` via `env_or`, so no container code change for the write path).
3. **DSR `R2Cas` erase adapter** (`routes/dsr/adapter_r2_cas.rs`): switch from
   "one bucket, 5 region prefixes" to **one client per region bucket** (mirror
   the `R2Ac` adapter's per-region-bucket fan-out). This is the only code change
   and is mechanical given the `R2Ac` precedent.
4. **(Defense-in-depth, optional)** gate the CAS write path on
   `derived_region == tenant.primary_region` and 403-reject a cross-region write,
   so a future config regression cannot silently leak again.

No data migration is required if executed before real CAS data accumulates.

## Consequences

- **Positive:** the residency gap is explicit + scoped; the DSR pipeline ships
  on a stable topology; the remediation is a mechanical, precedent-backed change.
- **Negative:** native CAS is single-physical-region until Option A; the launch
  guardrail (no EU-residency-contract customers / set bucket jurisdiction) must
  be honoured operationally.
- **Tracking:** task #29 (remediation) + the launch guardrail above.

## References

- `crates/corelink-container/src/routes/cas.rs` (global `R2_CAS_REGION`).
- `crates/corelink-container/src/storage/r2_s3.rs` (`R2CasHandler` single-bucket write).
- `crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs` (single-bucket / 5-prefix erase).
- `wrangler.toml` (per-region `R2_AC_REGION`/`R2_CHUNK_REGION`, no `R2_CAS_REGION`).
- `migrations/d1/0028_tenant_primary_region.sql` (immutable `tenant.primary_region`).
- ADR-S14-002 (region pinning enforcement — the AC/routing precedent).
