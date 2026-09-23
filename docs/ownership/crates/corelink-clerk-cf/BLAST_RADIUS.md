---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-clerk-cf
manifest: crates/corelink-clerk-cf/Cargo.toml
source_commit: 1c99a5b8ce1db7b622db7811512432a5d070bbbe
profile: H
state: draft
evidence_set: clerk-cf-source-static-20260920
---

# corelink-clerk-cf — blast radius

Atomic static relations only. A manifest dependency, re-export, cfg, source value flow, test name, Worker macro, or Wrangler declaration is not proof that a target was selected, code executed, a Cloudflare resource exists, or a provider operation occurred.

[JWKS](#b01) · [Health bindings](#b02) · [Audit](#b03) · [Health DO](#b04) · [Features/region](#b05) · [Cron](#b06) · [Facades/config](#b07)

<a id="b01"></a>
## B01 — JWKS fetch/cache relation

**Dependency / flow / impact:** `corelink-clerk` JWKS traits → `CfJwksFetcher` / `CfKvJwksCache` → `worker::Fetch` / `worker::kv::KvStore`; URL or instance hash → GET/cache key/envelope/TTL → parse or mapped error. A change can alter trait compatibility, cache representation, TTL floor, or error path.

**Predicate:** `cf_fetch.rs`, `cf_kv.rs`, their `corelink-clerk` trait signatures, or `worker` dependency changes.

**Evidence:** `Cargo.toml`; `src/{cf_fetch,cf_kv}.rs`; `src/lib.rs` re-exports.

**Unknown/stop:** no URL, Fetch call, KV namespace/key/value/expiry, Clerk response, or network outcome is observed; do not assert any external cache/fetch behavior.

<a id="b02"></a>
## B02 — Health request to four scoped wrapper relation

**Dependency / flow / impact:** source header `x-corelink-tenant` → `TenantContext` → `CfRealBindings` → scoped KV/D1/R2/DO wrappers → health response/error source. A change may reject a label earlier, alter a scoped key/query/name, or change the order/coverage of source-level binding calls.

**Predicate:** tenant parsing, binding name, wrapper type, health query/key/name, or error mapping changes.

**Evidence:** `src/{health,prod_wiring}.rs`; `tests/prod_wiring.rs`; `crates/corelink-cf-bindings` API source.

**Unknown/stop:** source says upstream validated the header, but it does not prove JWT validation, route mounting, a Worker request, binding lookup, R2/D1/KV/DO resource, or tenant isolation in a provider runtime.

<a id="b03"></a>
## B03 — Shared audit sink relation

**Dependency / flow / impact:** `TenantContext.label` plus wrapper operation → `AuditSink::{r2,d1,kv,do_}` → NDJSON-shaped `AuditEvent` or native recorder → wrapper decision. A change can alter field formatting, an operation label, recorder failure behavior, or the source-level audit fence seen by the wrappers.

**Predicate:** `AuditEvent`, escaping, adapter closure, sink backend, or caller ordering changes.

**Evidence:** `src/audit_sink.rs`; `src/prod_wiring.rs`; `tests/{prod_wiring,audit_sink_mutex_poison_telemetry}.rs`.

**Unknown/stop:** `console_log!` does not prove Workers Logs, Logpush configuration, audit-store write, retention, queryability, or any operational alert; recorder behavior is not provider behavior.

<a id="b04"></a>
## B04 — Health DO route/actor relation

**Dependency / flow / impact:** DO namespace name plus `/record/{tenant}/{cid}` source route → `ParsedRoute`/tenant assertion → `ClerkHealthLogic` → record/upsert/tombstone/sweep result. A change can affect route compatibility, cross-tenant rejection source logic, audit-before-mutation, retention predicate, or public error/status mapping.

**Predicate:** route grammar, `HealthDoOp`, `HealthDoError`, TTL, actor cfg, or Wrangler class/migration row changes.

**Evidence:** `src/clerk_health_do.rs`; `tests/{clerk_health_do,prop_clerk_health_logic}.rs`; `wrangler.toml`.

**Unknown/stop:** no `ClerkHealthDo` class is verified as deployed/migrated; no namespace, actor instance, storage record, alarm, TTL deletion, consistency characteristic, or runtime authorization is established.

<a id="b05"></a>
## B05 — Optional billing and tenant-region relation

**Dependency / flow / impact:** feature selection → optional dependencies/cfg-gated module → billing binder or `D1TenantConfigStore`/prefetch helper → consumer compilation/source route. Tenant label → UUID namespace helper/cache key → scoped D1 query source → resolver input. A change can add/remove a public cfg route or change static tenant-region cache/query semantics.

**Predicate:** `cf-billing-real`, `tenant-region-real`, target cfg, UUID derivation, SQL constant, prefetch error, or re-export changes.

**Evidence:** `Cargo.toml`; `src/{lib,prod_wiring,tenant_region_real}.rs`; `tests/{tenant_region_wire,cf_worker_prefetch_wire}.rs`.

**Unknown/stop:** no resolved feature set, target selection, D1 schema/query, cache lifetime, actual tenant region, billing materialization, or downstream handler execution is proven.

<a id="b06"></a>
## B06 — DSR Statuspage cron relation

**Dependency / flow / impact:** Wrangler cron string and named bindings → wasm `scheduled` source → scoped D1 row source / 24-hour aggregate / report bridge / Statuspage client source → audit event string. A change can change static schedule/config name compatibility, source fail/skip arm, query window, payload construction, or audit vocabulary.

**Predicate:** `[triggers]`, binding constant, secret/var name, tenant scope, window, event type, or composition call changes.

**Evidence:** `wrangler.toml`; `src/dsr_statuspage_cron.rs`; `Cargo.toml` wasm target dependencies; related source in scheduler/statuspage/erasure packages.

**Unknown/stop:** a declared cron does not prove scheduler delivery. The placeholder config does not prove binding existence; the secret name does not prove secret provision; source does not prove D1 rows, an HTTP call, Statuspage acceptance, public page mutation, or audit-log delivery.

<a id="b07"></a>
## B07 — Facade and checked-in configuration relation

**Dependency / flow / impact:** `corelink-auth::clerk_cf` and `corelink-adapters-cloud::clerk` → `pub use corelink_clerk_cf::*`; a public export/type/feature change can compile-break static facade consumers. `wrangler.toml` names an intended POC script/configuration, bindings, observability, and prod override; source/config drift can make intended names disagree.

**Predicate:** public re-export, manifest dependency, exported module/type, Wrangler binding/class/trigger/variable name changes.

**Evidence:** root `Cargo.toml`; `crates/corelink-auth/{Cargo.toml,src/clerk_cf.rs}`; `crates/corelink-adapters-cloud/{Cargo.toml,src/clerk.rs}`; `crates/corelink-clerk-cf/{src/lib.rs,wrangler.toml}`.

**Unknown/stop:** static facades do not prove a caller, and configuration text does not prove an active Worker, environment, production override, observability service, deployment, or provider account state.

[Back to index](#b07)

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-clerk-cf/SKILL.md#s01)
