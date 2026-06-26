---
type: "ADR"
title: "ADR-MULTI-REGION-V1 — Multi-region container deployments v1 (per-region worker envs)"
description: "Why v1 multi-region uses per-region wrangler env blocks + per-region container apps (Approach A) rather than per-request region routing in the container."
source_files:
  - "specs/03_architecture/adrs/ADR-MULTI-REGION-V1.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "multi-region", "containers", "cloudflare", "wrangler", "r2", "deploy"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-MULTI-REGION-V1 — Multi-region container deployments v1 (per-region worker envs)

CoreLink production ran a single Cloudflare Container app pinned to IAD even though five R2 bucket sets
(`iad`, `sam`, `lhr`, `nrt`, `syd`) were provisioned. Because container region selection is *per-deploy*
not *per-request* — the container reads `R2_AC_BUCKET` / `R2_AC_REGION` at startup and a single
instance serves exactly one region — exercising the other four regions required four additional
container applications. This ADR chooses **Approach A**: per-region wrangler env blocks + per-region
container apps, deliberately deferring the heavier per-request-routing refactor. It was implemented
2026-05-30 (closing #377 phase 1).

# Context

The other four regions had empty AC, chunk, and manifest buckets. A region-taxonomy divergence is noted
as open debt: `corelink-region/src/region.rs` defines a 4-value enum of CF location-hint names
(`wnam`, `enam`, `weur`, `sam`) while the prod buckets use 5 CF airport codes — LHR/NRT/SYD are absent
from the Rust enum. This divergence is flagged but explicitly NOT resolved in this ADR.

# Decision

**Approach A: per-region wrangler.toml env blocks + per-region container apps.** Deploy
`[env.prod-sam|lhr|nrt|syd]`, each targeting a distinct worker name (`corelink-prod-<region>`), a
distinct CF Container app, the same container image tag as IAD (no Rust change), region-specific
`R2_AC_BUCKET` / `R2_AC_REGION` / `R2_CHUNK_BUCKET` / `R2_CHUNK_REGION` vars, and a regional route
`<region>.corelink-api.humangr.com/*`. CAS stays a single global IAD bucket (content-addressed,
hash-deduped); each regional worker env's DO binding creates a separate, region-isolated DO namespace;
existing IAD tenants are unaffected. The rejected alternative — Approach B, per-request region routing
in the container — is deferred: it needs a container-side refactor, adds ~5–20ms D1-lookup latency per
object op, and requires resolving the Region-enum divergence first.

# Consequences

- Five container applications in the CF account (was one), with four extra wrangler deploys per release
  and per-region secrets provisioned separately (`wrangler secret put --env prod-<region>`).
- Regional subdomains require DNS CNAMEs; operational cost is ~5× container instances at full load
  (CF Containers bill per instance-hour).
- Open follow-ups remain: the Region-enum divergence, per-tenant signup routing by CF-Ray colo,
  per-region CAS, the unwired manifest buckets, and WEUR/LHR `DoJurisdiction::Eu` enforcement
  (tracked by ADR-S14-002).

# Citations

1. `specs/03_architecture/adrs/ADR-MULTI-REGION-V1.md:24-39` — the Context: single IAD app, per-deploy
   region selection, and the region-taxonomy enum-vs-bucket divergence.
2. `specs/03_architecture/adrs/ADR-MULTI-REGION-V1.md:40-65` — the Decision: Approach A per-region env
   blocks + container apps, global CAS, isolated DO namespaces.
3. `specs/03_architecture/adrs/ADR-MULTI-REGION-V1.md:168-177` — the Consequences: five apps, extra
   deploys, per-region secrets, DNS, and ~5× steady-state cost.
