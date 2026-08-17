---
type: "ComplianceControl"
title: "Data residency posture"
description: "CoreLink's launch data-residency reality: real EU (lhr/corelink-cas-eu) residency, an honest US/ENAM default, a single-bucket native-CAS launch posture, and the container-side residency guard that fail-closed rejects cross-region traffic."
source_files:
  - specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md
  - docs/security/2026-06-23-secreview-gdpr-residency.md
  - crates/corelink-container/src/routes/residency.rs
  - crates/corelink-container/src/storage/region_map.rs
checkpoint_sha: "55700b28c53c319211a62464855ad5dd69e1ac18"
provenance: "AUTHORED"
tags: ["compliance", "residency", "gdpr", "lgpd", "schrems-ii", "cas", "r2", "launch-posture"]
timestamp: "2026-06-26T00:00:00Z"
---

Data residency is a compliance control because Schrems II and LGPD Art. 16 require EU-subject bytes to reside **physically** in-region — a logical key prefix does not satisfy this (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:44-47`). CoreLink's posture has two halves: a documented launch decision (native CAS ships single-physical-region with an EU-customer guardrail until per-region buckets land) and a deployed, fail-closed container-side guard that refuses any request whose claimed region does not match the colo the container serves. The 2026-06-23 GDPR/residency secreview confirmed the EU residency claim is now genuinely real (prod-lhr physically lands EU bytes in EEUR) and that US is the honest default (BR/APAC were disclosed as roadmap at that date) (`docs/security/2026-06-23-secreview-gdpr-residency.md:21-24`). Since then WP4 provisioned `apac`: the APAC-located Tokyo/`nrt` bucket `corelink-cas-apac` now stores + serves apac CAS bytes in-region — a physical LOCATION hint, not a data-residency jurisdiction (R2 has no APAC jurisdiction the way it has EU), which leaves `sam` as the sole routable-but-unprovisioned macro (`crates/corelink-container/src/storage/region_map.rs:52`).

# Role

The residency posture governs where tenant bytes physically rest and stops a mis-bound or buggy regional route from landing one region's tenant bytes in another region's container. It is the compliance control that makes the EU-residency assurance honest and gives the native-CAS single-bucket limitation an explicit, scoped, owner-authorised waiver rather than a silent gap (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:55-57`). The container-side guard is defence-in-depth: the edge Worker is fail-closed AND the container independently refuses cross-region traffic (`crates/corelink-container/src/routes/residency.rs:11-13`).

# How it works

- The edge Worker fans EU (`weur`) tenants out to the London (`lhr`) regional Worker and stamps the trusted `x-corelink-primary-region:<macro>` header derived from D1 `tenant.primary_region`; the container guard is the backstop that verifies that claim (`crates/corelink-container/src/routes/residency.rs:5-11`, `crates/corelink-container/src/routes/residency.rs:41-44`).
- The container reads its own serving colo from the `R2_CAS_REGION` env (default `iad`) — the SAME env the CAS handler keys its bucket from, so guard and storage agree by construction (`crates/corelink-container/src/routes/residency.rs:51-53`).
- The pure `residency_decision` returns `Allow` when the header is absent (local/IAD path, where the Worker never stamps the header) (`crates/corelink-container/src/routes/residency.rs:92-95`).
- When the header is present, the guard maps the macro via `colo_for_macro`: a match for the container's colo → `Allow`; a different colo, or an unknown/unprovisioned macro (e.g. `afr`) → `Reject` (`crates/corelink-container/src/routes/residency.rs:96-102`).
- `colo_for_macro` lives in the residency MACRO→colo map, the SINGLE SOURCE OF TRUTH for that mapping: `wnam`/`enam`→`iad`, `weur`→`lhr`, `sam`→`sam`, `apac`→`nrt`, and `afr`/unrecognised→`None` (a hard reject, never a fall-through to IAD) (`crates/corelink-container/src/storage/region_map.rs:76-84`). This map is the Schrems-II leak fix: `tenant.primary_region` in D1 holds a MACRO code, not a literal colo string, so the edge fan-out must translate MACRO→colo deterministically here rather than string-compare against `lhr`/`nrt` (which a macro never equals) (`crates/corelink-container/src/storage/region_map.rs:8-14`).
- This Rust map is MIRRORED byte-for-byte by `worker/src/region-map.ts`; the two MUST agree on the macro→colo table and the provisioned set, a contract pinned by the container lock-consistency tests and the worker routing test (`crates/corelink-container/src/storage/region_map.rs:1-6`).
- A reject is emitted as HTTP 409 `residency_violation` BEFORE any handler runs, with zero storage I/O on the reject path (`crates/corelink-container/src/routes/residency.rs:62-80`, `crates/corelink-container/src/routes/residency.rs:118-129`).
- EU residency is physical, not a key-prefix illusion: prod-lhr binds `corelink-cas-eu`/`corelink-ac-eu` with `jurisdiction = "eu"` and the EU S3 endpoint, so EU CAS+AC bytes land in EEUR (`docs/security/2026-06-23-secreview-gdpr-residency.md:99-108`).
- Native CAS's launch reality is a single physical R2 bucket (`corelink-cas-prod`) with region carried only in the key prefix `<region>/<tenant_prefix>/<digest>` from a global `R2_CAS_REGION` var, NOT the tenant's immutable `tenant.primary_region` (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:27-33`).

# Invariants

- Launch ships single-region native CAS as a KNOWN, explicit limitation and gates residency-sensitive EU customers until the per-region-bucket remediation (Option A) lands (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:55-57`).
- The launch guardrail: do NOT onboard a customer with a contractual EU data-residency requirement for native CAS bytes, OR set the `corelink-cas-prod` bucket's R2 jurisdiction to the strictest required region (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:75-80`).
- Region-in-key is a *logical* tag, not *physical* residency; one bucket has one physical R2 location, so the minimal "add `R2_CAS_REGION`" fix is a false-confidence trap and must not ship as "the residency fix" (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:40-51`).
- The guard is fail-closed: any claimed macro that does not map to the container's colo — including unknown/unprovisioned regions — is rejected, never served (`crates/corelink-container/src/routes/residency.rs:96-102`).
- The provisioned macro set is EXACTLY `{wnam, enam, weur, apac}` — the macros backed by a location/jurisdiction-correct R2 bucket — and signup rejects the rest; `apac` is now provisioned (WP4: the APAC-LOCATED bucket `corelink-cas-apac` in Tokyo/`nrt`, a physical location hint since R2 has no APAC data-residency jurisdiction the way it has EU), while `sam`/`afr` remain valid, D1-accepted macro codes but NOT provisioned, so the single-bucket launch posture (ADR-S14-009) still holds for them (`crates/corelink-container/src/storage/region_map.rs:52`). `sam` is now the SOLE routable-but-unprovisioned macro: it stays ROUTABLE (`colo_for_macro` still maps it) yet is deliberately NOT provisionable — Cloudflare has NO SAM region (a documented platform limit), so a `sam`-labelled tenant would mis-land in US storage under a false residency label, an LGPD violation (`crates/corelink-container/src/storage/region_map.rs:95-96`).
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
- `docs/security/2026-06-23-secreview-gdpr-residency.md:21-24`, `:99-108` — EU residency is real; US-default honest.
- `docs/security/2026-06-23-secreview-gdpr-residency.md:126-150`, `:152-162`, `:164-178`, `:202-206` — M-1/L-1/L-2 residuals + the no-SA-region fail-loud.
- `crates/corelink-container/src/routes/residency.rs:5-13`, `:41-53`, `:62-102`, `:118-129` — the container-side residency guard and pure decision.
- `crates/corelink-container/src/storage/region_map.rs:76-84` — `colo_for_macro`: the frozen MACRO→colo map (single source of truth), `None`/reject for `afr` + unrecognised.
- `crates/corelink-container/src/storage/region_map.rs:52`, `:95-96` — `PROVISIONED_MACROS` = `{wnam, enam, weur, apac}` + `is_provisioned_macro` (apac now provisioned; `sam` the sole routable-but-unprovisioned macro — no CF SAM region; afr rejectable).
- `crates/corelink-container/src/storage/region_map.rs:1-14` — the Schrems-II leak fix rationale + the byte-for-byte `worker/src/region-map.ts` mirror contract.
