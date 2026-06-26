---
type: "StorageComponent"
title: "R2 AC ×5 regional buckets"
description: "How the Action Cache is stored across five per-region R2 buckets (region in the bucket name), the canonical Region enum behind them, and the indirect cross-region replication SLI."
source_files:
  - "crates/corelink-region/src/region.rs"
  - "crates/corelink-region/src/r2_crr.rs"
  - "crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs"
  - "crates/corelink-container/src/routes/ac.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["storage", "r2", "action-cache", "region", "residency"]
timestamp: "2026-06-26T00:00:00Z"
---

# R2 AC ×5 regional buckets

The Action Cache stores REAPI action results differently from CAS: where [CAS uses one
bucket](/storage/r2-cas-bucket.md) with the region in the key, AC uses **per-region buckets**
`corelink-ac-{sam,iad,lhr,nrt,syd}` — the residency region is in the bucket NAME. The canonical 4-value
`Region` enum in `corelink-region` is the single source of truth for how a region maps to a Cloudflare
R2 `locationHint`, a D1 location, and a per-region domain, and it carries the load-bearing data-residency
rule (WEUR must pin its Durable Object to the EU jurisdiction for Schrems II). This topology is the
durable tier behind the [Action Cache surface](/surfaces/action-cache.md).

# Role
- The canonical region identifier + residency primitives (bucket/db naming, DO jurisdiction) shared
  across the multi-region foundation (`crates/corelink-region/src/region.rs:8-27`).
- The per-region AC bucket map the DSR erase sweep and the live AC route both resolve against
  (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:1-10`).

# How it works
1. `Region` is the canonical 4-value enum (`wnam`/`enam`/`weur`/`sam`) that maps to the CF R2
   `locationHint`, the D1 location, and the per-region domain
   (`crates/corelink-region/src/region.rs:8-27`).
2. AC is stored as five per-region R2 buckets `corelink-ac-{sam,iad,lhr,nrt,syd}` — region in the bucket
   NAME, the opposite of CAS (region in the key)
   (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:1-10`).
3. The fixed AC region set is a five-element constant; a once-per-account erase sweeps all five buckets
   so it is robust to a write region that changed over time
   (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:50-56`).
4. The live AC route resolves its bucket + region from env, defaulting to `corelink-ac-iad` / `iad`
   (`crates/corelink-container/src/routes/ac.rs:360-361`).
5. The region's uppercase R2 `locationHint` is what physically pins object placement to that region
   (`crates/corelink-region/src/region.rs:44-53`).
6. Cross-region replication has no direct managed lag metric, so the **designed** indirect SLI is a
   synthetic probe object written per region every 5 minutes whose presence the sibling region confirms.
   The live probe loop (R2 PUT primary → R2 HEAD replica) is **deferred** — the crate ships only the
   `R2CrrProbe` trait boundary + the cadence / 24 h ceiling metric constants, not the running probe
   (`crates/corelink-region/src/r2_crr.rs:14-34`).

# Invariants
- WEUR MUST use the EU DO jurisdiction (Schrems II + GDPR Art. 46); any other jurisdiction for WEUR is a
  CRITICAL compliance gap (`crates/corelink-region/src/region.rs:112-127`,
  `crates/corelink-region/src/region.rs:140-158`).
- The five AC regions are a fixed set and the AC bucket prefix defaults to `corelink-ac-`, overridable
  only via env in non-prod (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:84-90`).
- AC bucket + region are read through `env_or` defaults, so an absent/empty env never yields an empty
  bucket name (`crates/corelink-container/src/routes/ac.rs:360-361`).
- CRR lag has a 24h p99 ceiling, and a synthetic object still missing past 24h is a hard SEV-2 incident
  regardless of burn rate (`crates/corelink-region/src/r2_crr.rs:41-58`).

# Gotchas
- The five AC bucket suffixes (`sam,iad,lhr,nrt,syd`) are colo strings, NOT the four macro `Region`
  codes — they are a different vocabulary; do not conflate the bucket suffix with `Region::as_str`.
- `corelink-region` is wasm32-clean and has no tokio in `src/`; the CRR probe's CF Worker PUT/HEAD wiring
  is deferred — this crate ships the trait boundary + metric constants, not the live probe loop.

# Citations
1. `crates/corelink-region/src/region.rs:8-27` — canonical 4-value `Region` enum + its CF R2/D1/domain mapping.
2. `crates/corelink-region/src/region.rs:44-53` — `r2_location_hint` (uppercase per CF API) pinning placement.
3. `crates/corelink-region/src/region.rs:112-127` — `DoJurisdiction`: WEUR mandatory EU (Schrems II).
4. `crates/corelink-region/src/region.rs:140-158` — `expected_for_region` / `is_valid_for_region` jurisdiction rule.
5. `crates/corelink-region/src/r2_crr.rs:1-34` — indirect CRR lag SLI via per-region synthetic probe objects.
6. `crates/corelink-region/src/r2_crr.rs:41-58` — CRR 24h p99 ceiling + missing-object SEV-2 threshold.
7. `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:1-10` — AC stored as per-region buckets `corelink-ac-{sam,iad,lhr,nrt,syd}`.
8. `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:50-56` — the fixed five-region AC sweep set.
9. `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:84-90` — AC bucket-prefix default + `R2_AC_BUCKET_PREFIX` override.
10. `crates/corelink-container/src/routes/ac.rs:360-361` — live AC route bucket/region env resolution (`corelink-ac-iad`/`iad`).
