---
type: "ComplianceControl"
title: "Data residency posture"
description: "CoreLink's launch data-residency reality: a single-physical-bucket native-CAS launch posture (region-in-key is a LOGICAL tag, NOT physical residency — per-region CAS buckets are owner-gated/unbuilt-in-code per ADR-S14-009), a genuinely per-region AC path, an honest US/ENAM default, the standing 'do NOT onboard a contractual-EU-residency customer for native CAS' guardrail, and the container-side residency guard that fail-closed rejects cross-region traffic with a 409."
source_files:
  - specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md
  - docs/security/2026-06-23-secreview-gdpr-residency.md
  - crates/corelink-container/src/routes/residency.rs
  - crates/corelink-container/src/routes/cas.rs
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["compliance", "residency", "gdpr", "lgpd", "schrems-ii", "cas", "r2", "launch-posture"]
timestamp: "2026-06-26T00:00:00Z"
---

Data residency is a compliance control because Schrems II and LGPD Art. 16 require EU-subject bytes to reside **physically** in-region — a logical key prefix does not satisfy this (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:44-47`). CoreLink's posture has two halves: a documented launch decision (native CAS ships single-physical-region with an EU-customer guardrail until per-region buckets land) and a deployed, fail-closed container-side guard that refuses any request whose *present* region header does not match the colo the container serves (an absent region header returns Allow — the F-015/L-2 gap detailed in the gotchas, not a mismatch-refusal). The 2026-06-23 GDPR/residency secreview records that the prod-lhr env is *configured* to bind the EU buckets (`corelink-cas-eu`/`corelink-ac-eu`, `jurisdiction = "eu"`) and that US is the honest default while BR/APAC are disclosed as roadmap (`docs/security/2026-06-23-secreview-gdpr-residency.md:21-24`). But the **authoritative** launch posture is ADR-S14-009: a full per-region native-CAS topology is owner-gated and *unbuilt in code* ("cannot be completed in code alone"), the default single bucket carries region only as a logical key tag, so native-CAS EU physical residency is **not yet a blanket assurance** — the standing launch invariant remains "do NOT onboard a customer with a contractual EU data-residency requirement for native CAS" (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:55-57`, `:75-80`). The genuinely per-region path is **AC** (`corelink-ac-<region>` buckets), not native CAS.

# Role

The residency posture governs where tenant bytes physically rest and stops a mis-bound or buggy regional route from landing one region's tenant bytes in another region's container. It is the compliance control that makes the EU-residency assurance honest and gives the native-CAS single-bucket limitation an explicit, scoped, owner-authorised waiver rather than a silent gap (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:55-57`). The container-side guard is defence-in-depth: the edge Worker is fail-closed AND the container independently refuses cross-region traffic (`crates/corelink-container/src/routes/residency.rs:11-13`).

# How it works

- The edge Worker fans EU (`weur`) tenants out to the London (`lhr`) regional Worker and stamps the trusted `x-corelink-primary-region:<macro>` header derived from D1 `tenant.primary_region`; the container guard is the backstop that verifies that claim (`crates/corelink-container/src/routes/residency.rs:5-11`, `crates/corelink-container/src/routes/residency.rs:41-44`).
- The container reads its own serving colo from the `R2_CAS_REGION` env (default `iad`) — the SAME env the CAS handler stamps into the object key's region prefix, so the guard and the storage *key's* region tag agree by construction (`crates/corelink-container/src/routes/residency.rs:51-53`). The physical bucket, however, is keyed from a SEPARATE env, `R2_CAS_BUCKET` (default `corelink-cas-prod`, `crates/corelink-container/src/routes/cas.rs:414`) — so a matching `R2_CAS_REGION` tag does NOT by itself guarantee a region-local bucket; that is exactly the logical-vs-physical gap ADR-S14-009 flags.
- The pure `residency_decision` returns `Allow` when the header is absent (local/IAD path, where the Worker never stamps the header) (`crates/corelink-container/src/routes/residency.rs:92-95`).
- When the header is present, the guard maps the macro via `colo_for_macro`: a match for the container's colo → `Allow`; a different colo, or an unknown/unprovisioned macro (e.g. `afr`) → `Reject` (`crates/corelink-container/src/routes/residency.rs:96-102`).
- A reject is emitted as HTTP 409 `residency_violation` BEFORE any handler runs, with zero storage I/O on the reject path (`crates/corelink-container/src/routes/residency.rs:62-80`, `crates/corelink-container/src/routes/residency.rs:118-129`).
- The **AC** path is genuinely per-region by construction (`corelink-ac-<region>` buckets). For native **CAS**, the prod-lhr env is *configured* to point `R2_CAS_BUCKET` at `corelink-cas-eu` (`jurisdiction = "eu"`, EU S3 endpoint) per the secreview (`docs/security/2026-06-23-secreview-gdpr-residency.md:99-108`) — but ADR-S14-009, the authoritative launch posture, records that a full per-region CAS topology is owner-gated/unbuilt-in-code and the default single bucket carries region only as a logical key tag; so native-CAS EU *physical* residency is NOT yet a blanket assurance and the do-not-onboard-EU-native-CAS guardrail stands (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:40-51`, `:55-57`, `:75-80`).
- Native CAS's launch reality is a single physical R2 bucket (`corelink-cas-prod`) with region carried only in the key prefix `<region>/<tenant_prefix>/<digest>` from a global `R2_CAS_REGION` var, NOT the tenant's immutable `tenant.primary_region` (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:27-33`).

# Invariants

- Launch ships single-region native CAS as a KNOWN, explicit limitation and gates residency-sensitive EU customers until the per-region-bucket remediation (Option A) lands (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:55-57`).
- The launch guardrail: do NOT onboard a customer with a contractual EU data-residency requirement for native CAS bytes, OR set the `corelink-cas-prod` bucket's R2 jurisdiction to the strictest required region (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:75-80`).
- Region-in-key is a *logical* tag, not *physical* residency; one bucket has one physical R2 location, so the minimal "add `R2_CAS_REGION`" fix is a false-confidence trap and must not ship as "the residency fix" (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:40-51`).
- The guard is fail-closed: any claimed macro that does not map to the container's colo — including unknown/unprovisioned regions — is rejected, never served (`crates/corelink-container/src/routes/residency.rs:96-102`).
- A South-America (`sam`) tenant's objects are physically ENAM; the residency-verifier maps NO R2 location code to `sam` (R2 has no SA region), so a BR physical-residency claim always fails-loud → VIOLATION, never a false green (`docs/security/2026-06-23-secreview-gdpr-residency.md:202-206`).

# Gotchas

- An absent header returns `Allow` (the F-015/L-2 latent gap): a direct hit to a public regional host carries no `x-corelink-primary-region`, so the guard is a no-op on that path. It is not material today only because the lhr env's storage IS the EU bucket regardless of the header — if an EU env is ever bound to a non-EU bucket again, the absent-header `Allow` removes the backstop (`docs/security/2026-06-23-secreview-gdpr-residency.md:164-178`).
- The DSR CAS erase adapter does NOT sweep the `corelink-chunk-*`/`corelink-manifest-*` R2 buckets (M-1). This is only latent because the container has zero multipart write-sites today; it MUST be fixed before the multipart/OCI-chunk path is wired, or chunked content would survive a "complete" erasure (`docs/security/2026-06-23-secreview-gdpr-residency.md:126-150`).
- prod-lhr chunk/manifest buckets are still physically US/ENAM (L-1); do not enable multipart on prod-lhr until `corelink-chunk-eu`/`corelink-manifest-eu` exist (`docs/security/2026-06-23-secreview-gdpr-residency.md:152-162`).
- The remediation (per-region CAS buckets) cannot be completed in code alone — it requires operator R2 bucket provisioning, so it is naturally owner-gated; the related ADR concept lives at `adr/adr-s14-009-cas-residency-single-bucket-launch-posture` (verify before linking).

# Citations

- `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:27-33` — single-bucket native-CAS write reality (global `R2_CAS_REGION`, region-in-key).
- `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:40-51` — logical-vs-physical, the false-confidence trap.
- `specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:55-57`, `:75-80` — launch decision + EU-customer guardrail.
- `docs/security/2026-06-23-secreview-gdpr-residency.md:21-24`, `:99-108` — prod-lhr is configured to bind the EU buckets; US-default honest (AC is per-region; native-CAS per-region residency stays owner-gated per ADR-S14-009).
- `crates/corelink-container/src/routes/cas.rs:414` — the CAS handler keys its physical bucket from `R2_CAS_BUCKET` (default `corelink-cas-prod`), a SEPARATE env from the `R2_CAS_REGION` key tag.
- `docs/security/2026-06-23-secreview-gdpr-residency.md:126-150`, `:152-162`, `:164-178`, `:202-206` — M-1/L-1/L-2 residuals + the no-SA-region fail-loud.
- `crates/corelink-container/src/routes/residency.rs:5-13`, `:41-53`, `:62-102`, `:118-129` — the container-side residency guard and pure decision.
