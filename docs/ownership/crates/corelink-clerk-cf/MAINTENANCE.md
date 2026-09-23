---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-clerk-cf
manifest: crates/corelink-clerk-cf/Cargo.toml
source_commit: 1c99a5b8ce1db7b622db7811512432a5d070bbbe
profile: H
state: draft
evidence_set: clerk-cf-source-static-20260920
---

# corelink-clerk-cf — maintenance

These are H-profile source/static review procedures. They do not authorize Cargo/test execution, target builds, network access, Wrangler, Cloudflare/Clerk/Statuspage calls, Workers/DO/KV/D1/R2 inspection or mutation, secret access, deployment, cron triggering, log queries, or an independent-review claim.

[Baseline](#m01) · [Bindings](#m02) · [JWKS](#m03) · [Health DO](#m04) · [Features/region](#m05) · [Cron](#m06) · [Handoff](#m07)

<a id="m01"></a>
## M01 — Fix the source baseline

**Mode:** STATIC_SOURCE. **Prerequisite:** requested revision and changed paths are known. **Predicate:** `Cargo.toml`, `src/lib.rs`, `wrangler.toml`, and affected sources belong to the recorded revision. **Action:** inventory changed public symbols, cfgs, manifest entries, and configuration names before drawing a conclusion. **Evidence:** revision/path output and exact file paths. **Stop/recovery:** stop on an ambiguous revision or dirty overlap; recover by obtaining the intended source snapshot, not by silently choosing a newer commit.

<a id="m02"></a>
## M02 — Review four-binding health wiring

**Mode:** STATIC_SOURCE. **Prerequisite:** `health`, `prod_wiring`, `audit_sink`, or wrapper-facing code changes. **Predicate:** tenant input, `CfRealBindings` members, named bindings, and audit callback path remain traceable for KV/D1/R2/DO. **Action:** trace `x-corelink-tenant` through `TenantContext`, `build_real_bindings`, each wrapper call, and error mapping; distinguish the wasm source path from native `WasmOnly:` test stubs. **Evidence:** `src/{health,prod_wiring,audit_sink}.rs`, `tests/prod_wiring.rs`. **Stop/recovery:** stop if authentication provenance, a binding lookup, a resource, or a live request is needed; escalate the exact external predicate to the responsible operation owner.

<a id="m03"></a>
## M03 — Review JWKS fetch and KV cache changes

**Mode:** STATIC_SOURCE. **Prerequisite:** `cf_fetch.rs`, `cf_kv.rs`, Clerk trait, serialization, or TTL change. **Predicate:** request method/error mapping, cache key/envelope, freshness timestamp handling, and TTL floor are explicit in source. **Action:** compare inputs, output/error types, `worker::send::SendFuture` wrapping, and cache serialization before/after. **Evidence:** `src/{cf_fetch,cf_kv}.rs`, `Cargo.toml`, `corelink-clerk` trait definitions. **Stop/recovery:** stop if the conclusion requires a real Clerk JWKS response, fetch TLS/egress policy, KV minimum behavior, namespace/key contents, or expiry; do not use comments as operational evidence.

<a id="m04"></a>
## M04 — Review health Durable Object changes

**Mode:** STATIC_SOURCE. **Prerequisite:** health-DO route, record, tenant, audit, TTL, actor, or migration/configuration change. **Predicate:** route parsing, actor-name tenant assertion, audit-before mutation, error/status mapping, and expiration predicate have an explicit before/after source relation. **Action:** trace `GET`/`POST`/`DELETE` and sweep separately through `ClerkHealthLogic`; compare target-agnostic logic with the cfg-gated actor shell and `wrangler.toml` class/migration names. **Evidence:** `src/clerk_health_do.rs`, `tests/{clerk_health_do,prop_clerk_health_logic}.rs`, `wrangler.toml`. **Stop/recovery:** stop if proof needs a deployed class, applied migration, DO storage/alarm, traffic, or provider consistency result; request authorized operational evidence.

<a id="m05"></a>
## M05 — Review target, feature, optional-dependency, and region changes

**Mode:** STATIC_MANIFEST. **Prerequisite:** feature/cfg/dependency/tenant-region/prefetch change. **Predicate:** feature entries, optional dependencies, target-specific dependencies, cfg-gated module/re-export, and direct static consumer relation agree. **Action:** compare `cf-billing-real` and `tenant-region-real` end-to-end through `Cargo.toml`, `lib.rs`, `prod_wiring.rs`, and `tenant_region_real.rs`; trace tenant label/UUID/query/cache/error source routes. **Evidence:** those files and `tests/{tenant_region_wire,cf_worker_prefetch_wire}.rs`. **Stop/recovery:** stop where resolved features, wasm/native compilation, D1 migration/schema, tenant data, or downstream runtime behavior is required; record it as unknown pending separate evidence.

<a id="m06"></a>
## M06 — Review DSR Statuspage cron/configuration drift

**Mode:** STATIC_SOURCE_CONFIG. **Prerequisite:** `dsr_statuspage_cron.rs`, statuspage dependency, secret/var name, D1 name, audit type, or trigger change. **Predicate:** code constants and `wrangler.toml` agree on the declared schedule/binding names, and secret-versus-var classification is preserved in source. **Action:** compare `CRON_EXPRESSION`, 24-hour window, named inputs, tenant scoping, missing-binding branch, audit types, and trigger row; identify each external owner in the aggregate/bridge/publish chain. **Evidence:** `src/dsr_statuspage_cron.rs`, `wrangler.toml`, `Cargo.toml`, and related source contracts. **Stop/recovery:** stop if the task

requires API key access, D1 inspection, schedule execution, Statuspage publication, Workers Logs/Logpush verification, or production configuration; those require explicit scoped authority.

<a id="m07"></a>
## M07 — Reconcile consumers and hand off

**Mode:** STATIC_GRAPH. **Prerequisite:** public symbol, feature, config name, or manifest edge changed. **Predicate:** direct static re-export/dependency edges are separated from review categories and operational claims. **Action:** inspect `corelink-auth::clerk_cf`, `corelink-adapters-cloud::clerk`, `corelink-cf-bindings`, and named related crates; update B01–B07 with exact paths, then hand off revision, predicates, unrun validation, and unknowns. **Evidence:** manifests/imports plus [Reference](REFERENCE.md#r08) and [Blast radius](BLAST_RADIUS.md#b01). **Stop/recovery:** stop if a complete reverse graph, feature-resolved graph, runtime consumer, provider operation, or cold

review is requested without its own evidence; request the correct owner/evidence rather than certifying it.

[Back to index](#m07)

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-clerk-cf/SKILL.md#s01)
