---
type: "CrateCluster"
title: "Container/platform crate cluster"
description: "The composition layer — the Rust container HTTP binary that hosts the data plane, the per-region config-singleton Durable Object, and the Cloudflare binding adapters that wire trait-fakes onto real worker::* types."
source_files:
  - "crates/corelink-container/src/main.rs"
  - "crates/corelink-config-do/src/lib.rs"
  - "crates/corelink-config-do/src/types.rs"
  - "crates/corelink-cf-bindings/src/lib.rs"
checkpoint_sha: "a7c16588ead34ed6095e0aa9db67ddf77ac96688"
provenance: "AUTHORED"
tags: ["crates", "container", "platform", "cloudflare", "durable-object", "bindings"]
timestamp: "2026-06-26T00:00:00Z"
---

# Container/platform crate cluster

The ~70 pure-logic CoreLink crates are deliberately runtime-agnostic — they speak trait surfaces and ship in-memory fakes, never `worker::*` types — and this cluster is where that abstraction is finally bound to a real Cloudflare runtime and composed into a running server. It is grouped around the `trait-abstraction-defer` pattern's closure point: `corelink-container` is the HTTP binary that assembles every route and dependency at boot; `corelink-cf-bindings` provides the wasm32 adapters that map canonical traits onto `worker::r2/kv/d1/durable` types; `corelink-config-do` is the per-region config-singleton Durable Object that holds the feature flags and rate-limit tunables the whole fleet reads.

# Role

The cluster realises the [Rust container compute plane](/planes/container.md) and the [Worker → DO → Container request flow](/planes/request-flow.md). It is the only place where the pure-logic crates meet real I/O: the container hosts the composed data-plane router behind the Durable Object's HTTP port, the binding adapters inject CF-backed implementations into the same call sites tests fill with fakes, and the config DO is the atomic, CAS-versioned source of runtime configuration.

# How it works

- `corelink-container/src/main.rs` boots a single HTTP/1.1 stack on `PORT` (default 50051 — the port the DO reaches via `container.getTcpPort()`), serving the composed CAS/AC/Admin/audit/signup router, a `/_health` readiness probe, and the Stripe webhook route when its secret is set (`crates/corelink-container/src/main.rs:1-17`).
- Storage backing is chosen once at boot: `"r2"` when all `R2_S3_*` env vars are present (durable), else an ephemeral in-memory fallback requiring operator action — captured in a `OnceLock` set before the listener binds (`crates/corelink-container/src/main.rs:48-55`).
- `corelink-cf-bindings` maps the canonical host traits (`R2Backend`, `KvBackend`, D1/DO accessors) onto real `worker::*` types as wasm32-only adapters, with a native stub for `CfR2BucketReal` so the type is constructible on the host for trait-bound tests (`crates/corelink-cf-bindings/src/lib.rs:1-33`).
- `corelink-config-do` holds a versioned `ConfigPayload` (feature flags + rate-limit tunables + retention) with CAS atomic update via `expected_version`, fail-CLOSED audit-before-mutation ordering, and a rollback API (`crates/corelink-config-do/src/lib.rs:1-48`).

# Invariants

- The DO speaks only HTTP to the container port; the binary serves the real HTTP data plane there (the dead gRPC server that blocked the port was removed) (`crates/corelink-container/src/main.rs:11-17`).
- Config writes are atomic and fail-CLOSED: `update` rejects on `current_version != expected_version` with `VersionConflict`, and audit `emit` runs BEFORE state mutation so a failed emit aborts the write unchanged (`crates/corelink-config-do/src/lib.rs:27-48`).
- Config schema drift is a hard error, never a silent default — `ConfigPayload` uses `#[serde(deny_unknown_fields)]` (`crates/corelink-config-do/src/types.rs:34`).
- Binding adapters emit no `tracing::*` carrying R2 keys, D1 results, or KV values beyond a correlation-id / digest hash (CTRL-PRIV-001) (`crates/corelink-cf-bindings/src/lib.rs:35-40`).

# Gotchas

- The binding adapters are mostly `wasm32-unknown-unknown`-only; per-module `#[cfg(target_arch = "wasm32")]` gating keeps the crate buildable on native (host/CI) while preserving the production wasm path. `CfR2BucketReal`'s native stub returns `R2Error::Backend("WasmOnly: …")`.
- The in-memory R2/KV fakes in `corelink-worker` are NOT removed by the binding crate — they remain the canonical `cargo test --workspace` surface; the CF boot path constructs `Cf*Adapter` once per request into the same trait-bound call sites.
- A container booting on the in-memory storage fallback (missing `R2_S3_*`) is an operator-action condition, not a healthy state — `/_health` reports the backing kind captured at boot.

# Citations

1. `crates/corelink-container/src/main.rs:1-17` — the HTTP data-plane binary on the DO's container port (`getTcpPort`).
2. `crates/corelink-container/src/main.rs:11-17` — historical gRPC removal; the real HTTP data plane is served here.
3. `crates/corelink-container/src/main.rs:48-55` — boot-time storage-backing selection (`r2` vs in-memory fallback) in a `OnceLock`.
4. `crates/corelink-config-do/src/lib.rs:1-48` — the versioned config-singleton DO: payload, CAS update, audit ordering, rollback.
5. `crates/corelink-config-do/src/types.rs:34` — `#[serde(deny_unknown_fields)]`: schema drift → hard error.
6. `crates/corelink-config-do/src/lib.rs:27-48` — fail-CLOSED audit-before-mutation + CAS `expected_version` atomicity.
7. `crates/corelink-cf-bindings/src/lib.rs:1-33` — trait → `worker::*` adapter map + the `CfR2BucketReal` native stub.
8. `crates/corelink-cf-bindings/src/lib.rs:35-40` — CTRL-PRIV-001: no binding payloads in logs.
