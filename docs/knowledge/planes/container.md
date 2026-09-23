---
type: "Plane"
title: "Rust container compute plane"
description: "The native Rust binary serving the composed axum data-plane on port 50051: route composition, env-gated mounts, shared handler objects, and fail-CLOSED boot guards."
source_files:
  - "crates/corelink-container/src/main.rs"
  - "crates/corelink-container/src/routes/build.rs"
  - "crates/corelink-container/src/routes/public_attestation.rs"
  - "crates/corelink-container/src/routes/failover.rs"
  - "crates/corelink-container/src/routes/otel_layer.rs"
  - "crates/corelink-container/src/storage/r2_kv.rs"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs"
  - "crates/corelink-container/src/routes/cas/batch_read.rs"
  - "crates/corelink-container/src/main_boot.rs"
source_blobs:
  - "crates/corelink-container/src/routes/public_attestation.rs@593a49601cfdbae3369eefb18a9dba5eb294727e"
  - "crates/corelink-container/src/routes/failover.rs@dd84b4b9ccc622c572c8df95671e71a149f0bc24"
  - "crates/corelink-container/src/routes/otel_layer.rs@339673b9398db90b1fb2d210dc84da69a4d590f4"
  - "crates/corelink-container/src/storage/r2_kv.rs@e419448890037056949450cb15b9154669297e1a"
  - "crates/corelink-container/src/main_boot.rs@245e547bb2d9bddba3dc006dee3c155f266978e1"

  - "crates/corelink-container/src/main.rs@b64f13689133f41f65f286ca7aca0bc1a17f53d9"
  - "crates/corelink-container/src/routes/build.rs@43036d91a76bf2e14d6d49c2bca705037fc02081"
  - "crates/corelink-container/src/routes/cas/batch_read.rs@762ed7b8f4726a6b034e5b2afd1980f3f66c20bf"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs@63671990ccb9f0e9e6cba8804457737b42c18120"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs@8dcd20ecd23850e331763d5ccfcf4d0260489f0e"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs@f4259f925d39cff5d70ae6fb8fc5996f2109dba2"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["planes", "container", "rust", "axum", "routing"]
timestamp: "2026-09-06T00:00:00Z"
---
# Rust container compute plane

The container plane is the `corelink-server` Rust binary — the only place the actual cache logic runs.
It binds a single HTTP/1.1 axum stack on port 50051 (the exact port the Durable Object proxies to) and
serves the composed data plane: native CAS/AC, Bazel REAPI, Turbo, sccache/cargo, the `_public`
package-manager adapters, admin, audit-export/analytics, signup, the Stripe webhook, and the internal
mint/introspect/usage/erase + audit-chain-drain routes. Its defining design property is fail-CLOSED env-gating: every privileged
or stateful route mounts only when its secrets + D1/R2 storage env are present, so dev/CI runs the same
binary with those routes simply absent. One shared set of CAS/AC handler objects (and one PAT gate, one
$-ceiling gate, one byte-accountant) is cloned into every billable surface, so accounting, the erasure
tombstone gate, and possession re-verification are inherited uniformly rather than re-implemented per
surface.

# Role
- The native compute plane that serves the real product surface on the DO's `getTcpPort` target port
  50051 (`crates/corelink-container/src/main.rs:1-17`).
- The router composer: `build_with_factory` assembles every data-plane sub-router with shared state via
  one `Router::new().merge(...)` chain — now also mounting the operator `admin_tenant_detail` read surface
  and the tenant-scoped `customer_runners` / `workspaces` surfaces
  (`crates/corelink-container/src/routes/build.rs:383-400`).

# How it works
1. `main` selects the storage backing (`"r2"` vs `"inmemory"`) once at boot and surfaces it on
   `/_health` (`crates/corelink-container/src/main.rs:247-260`).
2. A fail-CLOSED boot guard refuses to start in prod if the native PAT verifier did not build — a
   PRESENT-but-malformed signing-key sibling is FATAL, not a silent downgrade
   (`crates/corelink-container/src/main.rs:245-279`). A companion POSITIVE prod-arming assertion then
   refuses to boot a HALF-ARMED prod: when an INDEPENDENT R2-region signal says prod, the StorageEnv, PAT
   verifier, $-ceiling, byte-cap and request-count controls must ALL be armed or it FATAL-exits naming the
   missing one. The request-count control it asserts is specifically the OCI-scoped monthly op-cap (the one
   surface the Worker forwards RAW and so cannot meter at the edge); native CAS/AC/Bazel/Turbo +
   cargo/brew/npm/pip are op-count-metered at the WORKER edge, NOT by this container gate. The must-arm set
   also includes `ERASURE_SALT_KEY` (the GDPR DSR account-delete/erasure HMAC salt) — absent it, a prod boot
   looks healthy while every erasure call silently 500s, so it is asserted alongside the PAT/quota/byte-cap
   controls (`crates/corelink-container/src/main.rs:309-396`; `crates/corelink-container/src/main.rs:366-378`).
3. The composed router is built and given a global 10 MiB body limit for non-cache routes + the `/_health` route; cache-entry routes apply the shared 64 MiB inner limit, then the router is bound
   to the listener on PORT (`crates/corelink-container/src/main.rs:494-499`,
   `crates/corelink-container/src/main.rs:928-929`).
4. Privileged routes are env-gated mounts: `/_internal/pat/mint`, `/internal/v1/auth/introspect`,
   `/internal/v1/billing/usage`, `/_internal/dsr/erase`, the S-09 `POST /_internal/audit/drain`
   audit-chain drain (seals the `audit_outbox` trail into the BLAKE3 tamper-evident chain — internal-auth
   gated, env-gated on D1; sits alongside the `/_internal/dsr/*` routes), CAS-erase, tier-select, and the
   Stripe webhook each mount only when their secrets are present
   (`crates/corelink-container/src/main.rs:500-847`). The Stripe-webhook mount here is NOT a stub:
   gated on `STRIPE_WEBHOOK_SECRET`, it is a LIVE, signature-verified billing materializer —
   `D1SubscriptionStateHandler` (+ `build_tier_selector`) writes `subscription_state='active'`+tier to
   `tier_selections` over D1-HTTP, and the Worker routes `/v1/billing/stripe-webhook` to THIS container
   (the shared `_system` DO) as the sole signature-verifier
   (`crates/corelink-container/src/main.rs:815-836`). It is one of TWO live activation writers (the other
   is the signup-worker), so go-live-readiness flags a divergent-tier-map risk: both must key the same
   `STRIPE_PRICE_ID_{SOLO,STARTER,PRO,MAX}` env map or a real `customer.subscription.updated` resolves to
   `UnknownPlan` → 422 (`crates/corelink-container/src/main_boot.rs:163-175`). See `launch/money-path`.
4b. UNAUTHENTICATED public-verifier mount (Artifact 1): when the D1 `StorageEnv` is present, `main`
   merges the `public_attestation` router (`GET /v1/public/attestation/{request_id}` +
   `GET /v1/public/keys/erasure/{region}.pub`) directly onto the data-plane app — deliberately OUTSIDE
   the ratelimit/residency/auth layers (same placement as the `/_health` + `/_internal/*` family) and with
   NO internal-auth gate, because an erasure proof is publicly verifiable. The container is the SOLE
   authority; the Worker forwards `/v1/public/*` to it as a pure `_anonymous` pass-through with NO PAT
   (`crates/corelink-container/src/main.rs:705-718`, router `crates/corelink-container/src/routes/public_attestation.rs:103-118` — axum-0.8 `{param}` capture syntax). M22(a): being outside `ratelimit_layer.rs` meant this router had NO container-side rate limiting at all — internet-reachable + unauthenticated. The router now wraps its two routes in its OWN scoped per-IP token-bucket layer (`PublicVerifierRateLimitState`, `rate_limit_public_verifier`) — a dedicated limiter instance local to this router, never the shared data-plane gate, so it cannot re-throttle CAS/Bazel/Turbo/OCI; generous budget (20 req/s, burst 60) and FAIL-OPEN when the trusted `x-corelink-client-ip` header is absent, wired via `PublicVerifierRateLimitState::new()` at the `main.rs` mount call.
5. `build_with_factory` resolves the shared gates from env — the $-ceiling `QuotaGate`, the OCI-scoped
   request-count gate, the native PAT gate, and the byte-accountant — warning (not failing) when absent.
   The request-count gate is DELIBERATELY consumed only by the OCI router (not cloned into the native
   CAS/AC/Bazel/Turbo states like `quota`/`pat_gate`) because native ops are already op-count-metered at
   the Worker edge — cloning it would double-count (`crates/corelink-container/src/routes/build.rs:34-88`).
6. One set of CAS handler objects is wrapped with byte-accounting + the 410-Gone tombstone gate at a
   single chokepoint, then cloned into every billable surface; the shared `CasRouteState` is constructed
   here with both a `put_inflight` and a `read_inflight` per-tenant concurrency pool. CAS byte-buffering
   reads additionally reserve weighted permits from the process-wide `GLOBAL_CAS_READ_BUDGET`; the
   singleton is backed by `CAS_READ_GLOBAL_BUDGET_BYTES` and shared across every router/state in this process
   (`crates/corelink-container/src/routes/build.rs:90-183`).
7. Cache-adapter surfaces (cargo/brew/npm/pip/oci) mount only when the shared `adapter_pat::PatVerifier`
   and the D1 moat map build from env (`crates/corelink-container/src/routes/build.rs:408-565`).
8. Four Tower layers wrap the composed data plane (NOT `/_health` or `/_internal/*`, which mount later in
   `main`): the residency guard, the WI-MULTI-REGION-V1 read-side failover guard
   (`crates/corelink-container/src/routes/failover.rs:635-678`), the per-tenant rate-limit token-bucket,
   and — outermost, so it observes the final status — the OTel-export seam, mounted only when
   `CORELINK_OBSERVABILITY_EXPORT_VARIANT` selects a configured vendor
   (`crates/corelink-container/src/routes/otel_layer.rs:413-439`,
   `crates/corelink-container/src/routes/build.rs:569-632`).
9. The Turbo/sccache surface gets its durable backing from the container's R2 KV store: `R2KvStore`
   derives each object key as `<hmac_prefix16>/<opaque_key>` and fails CLOSED (`TurboBridgeError::Internal`)
   on the production path when the TDK is absent or the tenant is not a UUID, rather than degrade to a
   public, predictable prefix two same-millisecond UUIDv7 tenants could collide into
   (`crates/corelink-container/src/storage/r2_kv.rs:127-148`).

# Invariants
- A single HTTP listener on PORT (default 50051) serves the whole data plane — the DO's only target
  (`crates/corelink-container/src/main.rs:447-450`).
- In prod a missing native PAT gate is FATAL — the binary refuses to boot the data plane without its
  Argon2id possession backstop (`crates/corelink-container/src/main.rs:281-294`).
- Privileged routes fail CLOSED: absent secrets ⇒ the route is simply not mounted (404), never an open
  proxy (`crates/corelink-container/src/main.rs:501-514`).
- The 410-Gone erasure gate and byte-accounting are centralized at the shared CAS chokepoint, so every
  surface inherits them by construction (`crates/corelink-container/src/routes/build.rs:154-173`).
- CAS read memory is bounded at two levels: eight concurrent reads per authenticated tenant and a
  process-wide `CAS_READ_GLOBAL_BUDGET_BYTES` weighted semaphore. A saturated or closed global budget
  fails closed with 503 before buffering and its log fields are low-cardinality
  (`crates/corelink-container/src/routes/cas/foundation_core.rs:186-245` and `:300-315`; per-tenant guard at `crates/corelink-container/src/routes/cas/foundation_state.rs:258-299`).
- The rate-limit + residency layers wrap exactly the data plane, never `/_health` or `/_internal/*`
  (which mount later in `main`) (`crates/corelink-container/src/routes/build.rs:610-645`).

# Gotchas
- The native CAS/AC/Bazel/Turbo plane trusts the Worker-injected `x-corelink-tenant-id`, with the
  container-side PAT gate as the possession backstop; adapter routes additionally re-verify the bearer
  PAT against D1 (Option-B).
- A historical tonic gRPC server on 50051 was dead weight that blocked the port; the binary now serves
  the real HTTP data plane there directly.
- The Stripe webhook is signature-verified by `Stripe-Signature` HMAC, NOT a Bearer PAT, so the Worker
  forwards the RAW body un-PAT'd to the shared `_system` DO; the container is the SOLE signature-verifier
  and tenant-resolver, and the route is checked BEFORE the generic `/v1/*` PAT-required arm so it is
  never 401'd.

# Citations

4a. `crates/corelink-container/src/routes/public_attestation.rs:103-118` — anonymous erasure-attestation router and its endpoint mounts.
18a. `crates/corelink-container/src/routes/cas/foundation_state.rs:258-299` — per-tenant read-concurrency guard and fail-closed acquisition.
1. `crates/corelink-container/src/main.rs:1-17` — the binary's role + the 50051 HTTP listener doc.
2. `crates/corelink-container/src/main.rs:247-260` — boot-time storage-backing selection for `/_health`.
3. `crates/corelink-container/src/main.rs:245-279` — the fail-CLOSED native-PAT-gate boot guard.
4. `crates/corelink-container/src/main.rs:281-294` — the FATAL `std::process::exit(1)` on a missing prod gate.
5. `crates/corelink-container/src/main.rs:447-450` — PORT resolution (default 50051).
6. `crates/corelink-container/src/main.rs:494-499` — the 10 MiB non-cache global body limit + `/_health` route; native CAS/Bazel/Turbo cache routes override it with `corelink_hash::CACHE_ENTRY_MAX_BYTES`.
7. `crates/corelink-container/src/main.rs:501-514` — env-gated `/_internal/pat/mint` mount (fail-CLOSED).
8. `crates/corelink-container/src/main.rs:500-847` — the full set of env-gated privileged route mounts, including the S-09 `POST /_internal/audit/drain` audit-chain drain (BLAKE3 tamper-evident seal of `audit_outbox`; internal-auth gated, env-gated on D1) and, two mounts later, the S-09 offsite `POST /_internal/audit/archive` (`crates/corelink-container/src/main.rs:679-700`) — the edge-probe audit-emit route `POST /_internal/audit/cas-attempted` (`crates/corelink-container/src/main.rs:590-610`) now sits between them — which copies rows the drain has ALREADY sealed into immutable NDJSON chunks in the R2 audit bucket — a SEPARATE mount on purpose, so an R2 outage can never abort or corrupt a D1 seal — alongside the `/_internal/dsr/*` family — the single `dsr::router` now mounts ALL five data-subject-rights legs (`erase` Art.17 + `verify`, plus `access` Art.15 / `portability` Art.20 / `rectification` Art.16 added by the DSAR-completion work), sharing one internal-auth gate. The irreversible-erase mounts — CAS-erase, the `dsr::router` erase legs, `audit/drain`, and `/_internal/dsr/anchor` — now gate on their DEDICATED keys (`CORELINK_ERASE_AUTH_KEY` / `CORELINK_DSR_ANCHOR_AUTH_KEY`) with NO fallback to the shared `CORELINK_INTERNAL_AUTH_KEY` (finding H4), so a shared-key leak cannot drive an erase or forge an erasure-legitimacy anchor. The F3.2 `_public` blob-revocation kill-switch (`POST /_internal/public/revoke`) mounts here too (`crates/corelink-container/src/main.rs:748`), fail-CLOSED on the same `CORELINK_ERASE_AUTH_KEY` — its path is deliberately OUTSIDE `/_internal/admin/*` so the worker edge front-gate maps it to the erase consumer (matching the handler's erase-key gate), not the admin key. Conversely the F3.2 public-base mirror (`POST /_internal/admin/public-mirror/promote`) mounts in this same range (`crates/corelink-container/src/main.rs:766`) but sits UNDER `/_internal/admin/*`, so the edge maps it to the admin consumer and it gates on `CORELINK_ADMIN_AUTH_KEY` (shared `CORELINK_INTERNAL_AUTH_KEY` fallback) — a dedicated mirror key could never match the forwarded header, and admin-level auth is correct because the mirror only PROMOTES with verify-before-write (not the erase/REVOKE control of finding H4).
8b. `crates/corelink-container/src/main.rs:815-836` — the LIVE Stripe-webhook materializer mount: signature-verified `D1SubscriptionStateHandler` (+ `build_tier_selector`) writes `subscription_state='active'`+tier to `tier_selections` over D1-HTTP (one of two activation writers; the Worker routes `/v1/billing/stripe-webhook` to this `_system` DO as the sole signature-verifier — see `WebhookState::new` + `STRIPE_WEBHOOK_ROUTE` at the mount).
9. `crates/corelink-container/src/main.rs:928-929` — binding the composed router to the PORT listener.
10. `crates/corelink-container/src/routes/build.rs:383-400` — the `Router::new().merge(...)` composition chain (now incl. `admin_tenant_detail`, `byok_admin`, `customer_runners`, `workspaces`, and the `dsr::portal` self-service privacy surface).
11. `crates/corelink-container/src/routes/build.rs:34-88` — shared gates resolved from env (quota, OCI-scoped request-count, PAT, accountant); the request-count gate is OCI-only, not cloned into native states (would double-count vs the Worker edge).
12. `crates/corelink-container/src/routes/build.rs:90-183` — shared CAS handler objects + accounting/tombstone wrap (incl. `put_inflight`/`read_inflight` pools in the `CasRouteState` ctor).
13. `crates/corelink-container/src/routes/build.rs:154-173` — centralized 410-Gone erasure gate at the CAS chokepoint.
14. `crates/corelink-container/src/routes/build.rs:408-565` — env-gated cache-adapter (cargo/brew/npm/pip/oci) mounts.
15. `crates/corelink-container/src/routes/build.rs:569-632` — residency guard + per-tenant rate-limit outer layers.
16. `crates/corelink-container/src/routes/cas/foundation_core.rs:186-245` and `:300-315` — process-wide weighted CAS read budget and bounded fail-closed acquisition.
17. `crates/corelink-container/src/routes/cas/single_handlers.rs:13-21` and `crates/corelink-container/src/routes/cas/batch_read.rs:124-136` — RAII guards held by single GET and batch-read.
16. `crates/corelink-container/src/routes/build.rs:610-645` — the rate-limit layer scoped to the data plane only.
17. `crates/corelink-container/src/main.rs:309-396` — positive prod-arming assertion: an independent R2-region signal ⇒ ALL launch controls must be armed (the request-count one as the OCI op-cap, plus `ERASURE_SALT_KEY` at `crates/corelink-container/src/main.rs:366-378`), else a FATAL boot refusal (no half-armed prod).
18. `crates/corelink-container/src/storage/r2_kv.rs:127-148` — `R2KvStore::object_key`: per-tenant `derive_prefix` HMAC key layout, fail-CLOSED on a non-derivable tenant rather than a public predictable prefix.
19. `crates/corelink-container/src/routes/failover.rs:635-678` — `failover_guard`: the WI-MULTI-REGION-V1 read-side failover Tower layer — inert in a clean/empty low-traffic window; each newly observed under-floor 5xx event is indeterminate once, while a following clean event cannot re-count it; an authenticated internal heartbeat is wired through `FailoverLayerState` so staleness fails closed without data-plane traffic, then 3 consecutive degraded observations (hysteresis latch, released after 5 consecutive healthy) are required before blocking writes (503 `failover_readonly`) + stamping a sibling read-region hint. Public `/_health` never refreshes the heartbeat.
20. `crates/corelink-container/src/routes/otel_layer.rs:413-439` — `otel_export_layer`: the OUTERMOST data-plane layer, streaming a canonical `MetricPoint` + `TraceSpan` per request through the configured exporter's fail-OPEN boundary; mounted only when `CORELINK_OBSERVABILITY_EXPORT_VARIANT` selects a vendor.
1. `crates/corelink-container/src/main_boot.rs:23-25` — fail-closed production boot predicate.
