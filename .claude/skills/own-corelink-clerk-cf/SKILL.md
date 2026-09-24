---
name: own-corelink-clerk-cf
description: Review static ownership and source-contract changes to the corelink-clerk-cf Cloudflare Worker binding crate.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-clerk-cf
  manifest: crates/corelink-clerk-cf/Cargo.toml
  source-commit: 1c99a5b8ce1db7b622db7811512432a5d070bbbe
  evidence-set: clerk-cf-source-static-20260920
  profile: "H"
---

# Own corelink-clerk-cf

Use this skill for `crates/corelink-clerk-cf`: source-level Cloudflare Worker adapters for Clerk JWKS fetch/KV cache, health/DO logic, audit wiring, optional tenant-region and billing wiring, and the DSR Statuspage cron. It records checked-in source and configuration only. It does not establish a selected target, feature resolution, Worker/DO/KV/D1/R2 binding existence, Cloudflare provider operation, deployment, cron delivery, secret availability, or runtime reachability.

[Baseline](#s01) · [Boundary](#s02) · [Bindings](#s03) · [Health DO](#s04) · [Features](#s05) · [Cron](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Establish the source baseline

**Condition:** beginning a change or reviewing a diff. **Action:** record the revision, `Cargo.toml`, `src/lib.rs`, `wrangler.toml`, and every affected source/test path. **Evidence:** path inventory and source revision. **Stop:** the supplied baseline differs or a claim needs executed Cargo/wasm evidence; obtain that separately rather than inferring it from comments or a manifest.

<a id="s02"></a>
## S02 — Keep the implementation/operation boundary explicit

**Condition:** a change mentions Cloudflare, Clerk, Statuspage, or a binding. **Action:** classify it as (a) checked-in adapter/configuration source, or (b) an external Worker, Durable Object, KV, D1, R2, Cloudflare account, Clerk endpoint, Statuspage endpoint, secret, deployment, or scheduled delivery. **Evidence:** source/config path for (a); authorized, scoped operational evidence is required for (b). **Stop:** do not turn `worker::*`, `#[event]`, `#[durable_object]`, `console_log!`, `wrangler.toml`, a placeholder ID, or a test stub into a claim that an external resource operates.

<a id="s03"></a>
## S03 — Guard the four-binding health composition

**Condition:** changing `health.rs`, `prod_wiring.rs`, `audit_sink.rs`, or a `corelink-cf-bindings` interface. **Action:** trace tenant input through `TenantContext`, shared `AuditSink`, and `CfRealBindings` to KV, D1, R2, and DO wrappers; preserve validation/audit/error order and name the host-stub path separately. **Evidence:** `src/{health,prod_wiring,audit_sink}.rs`, `tests/prod_wiring.rs`. **Stop:** a header is described as upstream JWT-validated in source, but this crate does not prove that upstream authentication ran; do not certify it.

<a id="s04"></a>
## S04 — Guard Clerk health Durable Object semantics

**Condition:** changing `clerk_health_do.rs`, DO migration/configuration, record routes, or TTL. **Action:** trace route parsing, actor tenant assertion, audit-before-mutation, record storage, and TTL sweep as distinct predicates; retain the pure `ClerkHealthLogic` versus wasm actor distinction. **Evidence:** `src/clerk_health_do.rs`, `tests/{clerk_health_do,prop_clerk_health_logic}.rs`, `wrangler.toml`. **Stop:** DO class declaration/migration text cannot prove an actor was created, migrated, alarmed, or stored data.

<a id="s05"></a>
## S05 — Review targets, optional dependencies, and features together

**Condition:** changing a dependency, cfg, `cf-billing-real`, or `tenant-region-real`. **Action:** compare the manifest target-specific DSR dependencies, optional dependencies, feature list, cfg-gated modules/reexports, and any consumer feature declaration. **Evidence:** `Cargo.toml`, `src/lib.rs`, `src/{prod_wiring,tenant_region_real}.rs`. **Stop:** Cargo resolution, a wasm build, or runtime feature selection is not contained in static source; report it as unknown unless separately evidenced.

<a id="s06"></a>
## S06 — Treat the Statuspage cron as a declared source route

**Condition:** changing `dsr_statuspage_cron.rs` or `[triggers]`. **Action:** trace the named vars/secret/D1 binding, 24-hour window, audit event constants, and `0 6 * * *` declaration; distinguish missing-binding source branches from an observed scheduled event. **Evidence:** `src/dsr_statuspage_cron.rs`, `wrangler.toml`, related scheduler/statuspage crate contracts. **Stop:** do not access a secret, D1 database, Statuspage API, Workers Logs, Logpush, scheduler, or Cloudflare dashboard without a separately authorized operation.

<a id="s07"></a>
## S07 — Hand off with H-profile limits

**Condition:** static review is ready. **Action:** report revision, affected relation(s), public/feature/cfg predicates, direct static consumers, validation not run, and operational unknowns. **Evidence:** exact paths and the three ownership documents. **Stop:** source review is not a target build, test result, cold review, deployment approval, provider verification, or runtime observation.

[Reference](../../../docs/ownership/crates/corelink-clerk-cf/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-clerk-cf/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-clerk-cf/MAINTENANCE.md#m01)
