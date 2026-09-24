---
schema: corelink-ownership/1.1
document: reference
package: corelink-clerk-cf
manifest: crates/corelink-clerk-cf/Cargo.toml
source_commit: 1c99a5b8ce1db7b622db7811512432a5d070bbbe
profile: H
state: draft
evidence_set: clerk-cf-source-static-20260920
---

# corelink-clerk-cf — ownership reference

This H-profile reference is limited to checked-in manifest, Rust, tests, and `wrangler.toml` evidence at `1c99a5b8ce1db7b622db7811512432a5d070bbbe`. It does not prove Cargo resolution or execution, a selected target, a Worker deployment, an instantiated Durable Object, existing KV/D1/R2 resources, a Cloudflare binding/provider operation, secret provision, Statuspage publication, cron delivery, log ingestion, or runtime use.

[Identity](#r01) · [Boundary](#r02) · [Source surfaces](#r03) · [Health and audit](#r04) · [DO](#r05) · [Features/targets](#r06) · [Cron/config](#r07) · [Consumers/unknowns](#r08)

<a id="r01"></a>
## R01 — Identity and intended compile surface

`crates/corelink-clerk-cf/Cargo.toml` names `corelink-clerk-cf`, exports both `cdylib` and `rlib`, and declares `worker` with `d1` and `http`. The manifest says its intended target is `wasm32-unknown-unknown`; it also declares host-only dev dependencies and source has native cfg branches/stubs for tests. That is an intended source/build shape, not a selected target or successful compilation.

| Area | Static source conclusion | Not established |
|---|---|---|
| Clerk JWKS | `CfJwksFetcher` uses `worker::Fetch`; `CfKvJwksCache` wraps `worker::kv::KvStore` | Clerk URL access, response, or KV operation |
| Four-binding health | `CfRealBindings` composes R2/D1/KV/DO wrappers with an `AuditSink` | a Worker, binding, resource, or request |
| Health DO | target-agnostic logic and wasm `ClerkHealthDo` actor shell are present | DO class/migration, storage, alarm, or traffic |
| Tenant region | feature-gated D1 store/prefetch source is present | feature selection, D1 read, resolved region |
| DSR Statuspage | wasm-gated scheduled-handler source and cron configuration are present | cron firing, secret availability, D1 read, API publish |

Evidence: `Cargo.toml`, `src/lib.rs`, `src/{cf_fetch,cf_kv,health,prod_wiring,clerk_health_do,tenant_region_real,dsr_statuspage_cron}.rs`, `wrangler.toml`.

<a id="r02"></a>
## R02 — Ownership boundary

This package owns the checked-in binding adapters and composition source: `CfJwksFetcher`, `CfKvJwksCache`, health response/error and entry-source, audit-sink formatting/adapters, pure health-DO logic plus wasm actor source, and optional wiring helpers. It depends on `corelink-clerk` for the JWKS traits/types, `corelink-cf-bindings` for scoped R2/D1/KV/DO wrappers, and feature-gated/domain crates for billing, audit-chain, privacy erasure, and Statuspage composition.

`wrangler.toml` is a declaration artifact, not an operational owner record. Its IDs and vars shown here are `PLACEHOLDER_*`; `STATUSPAGE_API_KEY` is named only as a secret lookup, and no secret value is in scope. External resource lifecycle, Cloudflare account/provider configuration, upstream JWT validation, Clerk authority, Statuspage API behavior, and log delivery belong to separate operational evidence/owners.

<a id="r03"></a>
## R03 — Static implementation map

| Source path | Source-level role | Contract limit |
|---|---|---|
| `cf_fetch.rs` | `JwksFetcher` adapter builds GET and parses 2xx bytes as `Jwks` | no outbound fetch observed |
| `cf_kv.rs` | KV cache key/envelope and minimum-60-second TTL source | no namespace or cache state observed |
| `prod_wiring.rs` | validates a tenant label and constructs the four wrapper bundle on wasm; supplies native test construction | wrapper construction source is not binding lookup success |
| `health.rs` | wasm `GET /health` entry source and target-specific health handling | header provenance, handler mounting, request/response unavailable |
| `audit_sink.rs` | source emits/captures NDJSON-shaped binding audit events | `console_log!` is not Logpush/retention evidence |
| `clerk_health_do.rs` | record routes, route parser, state logic, and cfg-gated DO actor | no actor/storage/alarm exists by source alone |
| `tenant_region_real.rs` | feature-gated D1 `tenant_config.region` prefetch/cache store | no D1 schema/query/region result observed |
| `dsr_statuspage_cron.rs` | wasm scheduled-handler composition and canonical constants | no scheduler/secret/publish execution observed |

<a id="r04"></a>
## R04 — Four-binding health and audit contract

For wasm source, `health::main` accepts only `GET /health`, reads `x-corelink-tenant`, calls `TenantContext::from_header_value`, then calls `build_real_bindings`. `handle_health_real` source uses a tenant-scoped KV read/write, R2 `head`, D1 insert with tenant bind anchor, and DO `stub_by_name("health")`. `prod_wiring` centralizes the named `CAS_BUCKET`, `CLERK_DB`, `CLERK_JWKS_KV`, and `CLERK_DO` lookups and gives their scoped wrappers a shared `AuditSink`.

Falsifiable source predicate: a change to tenant construction, wrapper selection, binding string, health operation order, or audit callback path changes this declared composition. Native `handle_health_real` intentionally drives wrapper stubs and reports `WasmOnly:` diagnostics after source-level validation/audit paths; it is not a Cloudflare substitute. Source comments call the tenant header upstream JWT-validated, but neither header issuance nor JWT verification is implemented/proved here. Evidence: `src/{health,prod_wiring,audit_sink}.rs`, `tests/prod_wiring.rs`.

<a id="r05"></a>
## R05 — Clerk health Durable Object source contract

`clerk_health_do.rs` provides target-agnostic `ClerkHealthLogic`, route/record/error types, and a wasm-gated `#[worker::durable_object] ClerkHealthDo`. The source route shape is `/record/{tenant}/{correlation_id}`. It maps malformed input, tenant mismatch, audit denial, missing records, and backend errors to differentiated outcomes; mutations include upsert, tombstone, and TTL sweep. `DEFAULT_TTL_MS` is `3_600_000`. The source describes audit-before-mutation, URL-tenant versus actor-name assertion, and a periodic alarm sweep.

Falsifiable source predicate: changing route parsing, tenant assertion, mutation/audit ordering, TTL, or actor cfg/migration declaration changes the source contract. `wrangler.toml` contains a `CLERK_DO` binding, `class_name = "ClerkHealthDo"`, and append-style migration rows `v1`/`v2`; it does not prove class registration, migration application, actor state, alarm execution, or strong-consistency behavior in a provider runtime. Evidence: `src/clerk_health_do.rs`, `tests/{clerk_health_do,prop_clerk_health_logic}.rs`, `wrangler.toml`.

<a id="r06"></a>
## R06 — Target, optional dependency, and feature boundary

The default feature set is empty. `cf-billing-real` enables optional `corelink-billing-stripe-materializer` and `corelink-audit-chain`; `tenant-region-real` enables optional `corelink-audit-chain` and `uuid` (with v5). `tenant_region_real` and selected `prod_wiring` exports are cfg-gated by the latter. Privacy erasure, Statuspage real, and DSR scheduler dependencies are declared only under `cfg(target_arch = "wasm32")`; host-only `tokio` and `proptest` are dev dependencies.

Falsifiable source predicate: changing a feature list, optional dependency, target-specific dependency section, or cfg changes the static availability route. A declaration does not identify a resolved feature set, prove optional code is compiled, or prove billing/region/cron behavior. Evidence: `Cargo.toml`, `src/lib.rs`, `src/{prod_wiring,tenant_region_real}.rs`.

<a id="r07"></a>
## R07 — Declared cron and configuration route

`dsr_statuspage_cron` declares `CRON_EXPRESSION = "0 6 * * *"`, a 86,400-second window, named Statuspage/D1 bindings, and four audit event type strings. Its wasm `scheduled` source obtains page ID, metric ID, API key, tenant ID, and `DSR_LOG_DB`; it records a missing-binding skip branch, builds a tenant-scoped D1 wrapper, aggregates a window, and calls the Statuspage wasm client source. `wrangler.toml` repeats the daily cron and declares dev/prod config variables, but all checked IDs/vars are placeholders.

This is a code/config connection only. It cannot show that Cloudflare invokes a schedule, a secret is provisioned, the D1 table has rows, a request reaches Statuspage, any log line is ingested, or a public metric changed. Evidence: `src/dsr_statuspage_cron.rs`, `wrangler.toml`; related trait/composition owners are `corelink-dsr-statuspage-scheduler`, `corelink-statuspage-real`, and `corelink-privacy-erasure-worker`.

<a id="r08"></a>
## R08 — Static consumers and explicit unknowns

The root manifest lists this package as a workspace member/dependency. Checked-in facades `corelink-auth::clerk_cf` and `corelink-adapters-cloud::clerk` re-export `corelink_clerk_cf::*`; that is a public-name route, not an execution route. `corelink-cf-bindings` documents this package as a static consumer, and billing/container source comments name selected composition adjacency. Tests name health DO, prod wiring, audit poison telemetry, tenant region, and prefetch-wire intent without an execution result in this artifact.

Unknown: complete resolved reverse dependency/feature graph; all direct and indirect callers; Cargo/wasm/native result; Worker module loading/routing; deployed binding names and resource IDs; DO migration/state/alarm; KV/D1/R2 data; audit log/Logpush destination and retention; upstream JWT/header authenticity; Clerk fetch; Statuspage secret/API result; cron delivery; tenant-region outcome; costs; security effectiveness; and independent cold review.

[Ownership guide](../../../../.claude/skills/own-corelink-clerk-cf/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
