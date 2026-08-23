---
type: "StorageComponent"
title: "R2 AC ×5 regional buckets"
description: "How the Action Cache is stored across five per-region R2 buckets (region in the bucket name), the canonical Region enum behind them, and the indirect cross-region replication SLI."
source_files:
  - "crates/corelink-region/src/region.rs"
  - "crates/corelink-region/src/r2_crr.rs"
  - "crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs"
  - "crates/corelink-container/src/routes/ac.rs"
checkpoint_sha: "8af9ed65caf286d3f800e91d3f823face3aefd31"
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
- The per-region AC bucket (named by the container's `R2_AC_BUCKET`) that the DSR erase sweep and the
  live AC route both resolve against (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:1-11`).

# How it works
1. `Region` is the canonical 4-value enum (`wnam`/`enam`/`weur`/`sam`) that maps to the CF R2
   `locationHint`, the D1 location, and the per-region domain
   (`crates/corelink-region/src/region.rs:8-27`).
2. AC is stored as per-region R2 buckets whose NAME is the container's `R2_AC_BUCKET`
   (`corelink-ac-iad`/`-sam`/`-nrt`/`-syd` on the US endpoint, `corelink-ac-eu` on the physically separate
   EU endpoint) — region in the bucket NAME, the opposite of CAS (region in the key)
   (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:1-11`).
3. The DSR erase adapter targets THIS container's OWN `R2_AC_BUCKET` — the SAME env the AC write path
   (`routes/ac.rs`) reads, default `corelink-ac-iad` — resolved in `R2AcEraseAdapter::new` and stored as
   `write_bucket` (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:109-110`,
   `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:87`). It opens ONE S3 client on that bucket
   and sweeps every region KEY-prefix WITHIN it (the single-source `region_map::CAS_REGIONS`, aliased
   `AC_REGIONS`, is now a set of key-prefixes, not bucket names) — NOT a fan-out across hardcoded
   `corelink-ac-<region>` names (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:67`,
   `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:138`,
   `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:142`). Completeness comes from the
   fail-CLOSED multi-region erase fan-out reaching every region's container, so each container erasing its
   own bucket makes the union complete — while never listing a bucket on a different jurisdiction's
   endpoint. This closes the pre-2026-08-18 bug that swept hardcoded names and NEVER listed the EU
   `corelink-ac-eu` (a GDPR Art.17 EU false-completion).
4. The live AC route resolves its bucket + region from env, defaulting to `corelink-ac-iad` / `iad`
   (`crates/corelink-container/src/routes/ac.rs:364-365`).
5. The region's uppercase R2 `locationHint` (`crates/corelink-region/src/region.rs:44-53`) requests
   placement IN the hinted region WHERE it is set — but this only pins placement to the extent the
   deployed buckets are genuinely per-region. Today only `corelink-ac-eu` is a real per-region (EU)
   bucket; the other deployed non-EU AC buckets (`lhr`/`iad`/…) are physically ENAM/US regardless of the
   colo suffix in their name. Genuine per-region physical placement for the full set + CRR/multi-region
   is disclosed deferred (see the CRR SLI below and the gotcha).
6. Cross-region replication has no direct managed lag metric, so an indirect SLI writes a synthetic
   probe object per region every 5 minutes and confirms presence in the sibling region
   (`crates/corelink-region/src/r2_crr.rs:1-34`).

# Invariants
- WEUR MUST use the EU DO jurisdiction (Schrems II + GDPR Art. 46); any other jurisdiction for WEUR is a
  CRITICAL compliance gap (`crates/corelink-region/src/region.rs:112-127`,
  `crates/corelink-region/src/region.rs:140-158`).
- The AC regions are the single-source-of-truth `region_map::CAS_REGIONS` re-export (no independent AC
  copy to drift), now swept as key-prefixes within one bucket; the erase bucket is THIS container's
  `R2_AC_BUCKET` read via `env_or` (default `corelink-ac-iad`) — byte-for-byte the same env the write path
  reads, so an absent env never yields an empty bucket name
  (`crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:67`,
  `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:58`,
  `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:109-110`,
  `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:87`).
- AC bucket + region are read through `env_or` defaults, so an absent/empty env never yields an empty
  bucket name (`crates/corelink-container/src/routes/ac.rs:364-365`).
- CRR lag has a 24h p99 ceiling, and a synthetic object still missing past 24h is a hard SEV-2 incident
  regardless of burn rate (`crates/corelink-region/src/r2_crr.rs:41-58`).

# Gotchas
- The five AC bucket suffixes (`sam,iad,lhr,nrt,syd`) are colo strings, NOT the four macro `Region`
  codes — they are a different vocabulary; do not conflate the bucket suffix with `Region::as_str`.
- `corelink-region` is wasm32-clean and has no tokio in `src/`; the CRR probe's CF Worker PUT/HEAD wiring
  is deferred — this crate ships the trait boundary + metric constants, not the live probe loop.

# Citations
1. `crates/corelink-region/src/region.rs:8-27` — canonical 4-value `Region` enum + its CF R2/D1/domain mapping.
2. `crates/corelink-region/src/region.rs:44-53` — `r2_location_hint` (uppercase per CF API) requesting placement where set; only `corelink-ac-eu` is genuinely per-region physical, the deployed non-EU AC buckets are physically US (CRR/multi-region deferred).
3. `crates/corelink-region/src/region.rs:112-127` — `DoJurisdiction`: WEUR mandatory EU (Schrems II).
4. `crates/corelink-region/src/region.rs:140-158` — `expected_for_region` / `is_valid_for_region` jurisdiction rule.
5. `crates/corelink-region/src/r2_crr.rs:1-34` — indirect CRR lag SLI via per-region synthetic probe objects.
6. `crates/corelink-region/src/r2_crr.rs:41-58` — CRR 24h p99 ceiling + missing-object SEV-2 threshold.
7. `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:1-11` — AC stored as per-region R2 buckets whose name is the container's `R2_AC_BUCKET` (`corelink-ac-iad`/`-sam`/`-nrt`/`-syd` US, `corelink-ac-eu` EU).
8. `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:67` — the AC sweep set is the single-source-of-truth `region_map::CAS_REGIONS` re-export (aliased `AC_REGIONS`), now a set of region key-PREFIXES swept within one bucket.
9. `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:58`, `:87`, `:109-110` — the erase bucket is THIS container's OWN `R2_AC_BUCKET` (default `corelink-ac-iad`, same env the write path reads), stored as `write_bucket`.
10. `crates/corelink-container/src/routes/ac.rs:364-365` — live AC route bucket/region env resolution (`corelink-ac-iad`/`iad`).
