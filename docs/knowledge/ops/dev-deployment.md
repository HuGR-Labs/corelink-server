---
type: "Runbook"
title: "CF Worker deployment POC: wasm32 trait-adapter gotchas"
description: "What it takes to deploy a Rust Cloudflare Worker (corelink-clerk-cf) — the production trait impls plus the two wasm32 landmines (ring, !Send KV futures) and their fixes."
source_files:
  - "crates/corelink-clerk-cf/Cargo.toml"
  - "crates/corelink-clerk-cf/src/cf_kv.rs"
  - "crates/corelink-clerk-cf/src/cf_fetch.rs"
  - "docs/dev/cf-worker-deployment-poc.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "dev", "cloudflare", "wasm32", "worker", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# CF Worker deployment POC: wasm32 trait-adapter gotchas

Deploying a Rust crate as a real Cloudflare Worker is not "just `wrangler deploy`": the
`corelink-clerk-cf` POC records the production trait implementations (a KV JWKS cache, a JWKS fetcher,
a `/health` handler) and, more importantly, the two wasm32 landmines every Worker-bound crate hits —
`jsonwebtoken`/`ring` refusing to compile to `wasm32-unknown-unknown`, and Cloudflare KV futures being
`!Send` against a `+ Send` trait bound. This runbook is the reference for anyone wiring a new crate
into the edge plane so they don't rediscover the same two bugs. Related: [the Cloudflare Worker edge plane](/planes/worker-edge.md).

# Role

It is the dev-side deployment guide for the Worker plane: the trait-adapter shape plus the
feature-gating and `SendFuture` patterns that make a native Rust crate actually compile and run on
Cloudflare's wasm runtime.

# How it works

- The crate provides production impls of the two `corelink-clerk` trait abstractions `docs/dev/cf-worker-deployment-poc.md:13-16`.
- `CfKvJwksCache` stores a JSON envelope so a belt-and-suspenders freshness check works independently of KV's own TTL `docs/dev/cf-worker-deployment-poc.md:18-25`.
- `CfJwksFetcher` is a zero-size struct over `worker::Fetch` because the Fetch API is a runtime global, not a binding `docs/dev/cf-worker-deployment-poc.md:27-31`.
- The `/health` handler reads+writes KV and inserts into a D1 table end-to-end `docs/dev/cf-worker-deployment-poc.md:33-41`.
- `wrangler.toml` declares the KV + D1 bindings with placeholder IDs to be replaced before deploy `docs/dev/cf-worker-deployment-poc.md:42-47`.
- Deploy is `wrangler dev` (local smoke) then `wrangler deploy` after creating real CF resources `docs/dev/cf-worker-deployment-poc.md:147-163`.

# Invariants

- Any wasm32-bound crate depending on `corelink-clerk` MUST use `default-features = false` to exclude `ring` (the `jwt-adapter` feature gates `jsonwebtoken`) — the crate's own dep declares exactly that (`crates/corelink-clerk-cf/Cargo.toml:29-33`).
- CF KV/Fetch futures are `!Send` and MUST be wrapped in `worker::send::SendFuture` to satisfy the `+ Send` trait bound — done at every KV impl boundary (`crates/corelink-clerk-cf/src/cf_kv.rs:89`, `crates/corelink-clerk-cf/src/cf_kv.rs:126`, `crates/corelink-clerk-cf/src/cf_kv.rs:154`) and in the Fetch impl (`crates/corelink-clerk-cf/src/cf_fetch.rs:58`).
- The crate must build clean for wasm32 under `-D warnings` in both dev and release profiles before deploy `docs/dev/cf-worker-deployment-poc.md:128-145`.

# Gotchas

- `ring`'s `wasm32_unknown_unknown_js` feature only affects RNG — it does NOT skip the C compilation of the RSA/EC primitives, so the feature alone will not fix the build `docs/dev/cf-worker-deployment-poc.md:50-62`.
- `+ Send` on the trait future aliases is a "polite fiction" for CF impls (Workers are single-threaded); the long-term fix is a `?Send` profile or trait extraction `docs/dev/cf-worker-deployment-poc.md:106-114`.

# Citations

1. `docs/dev/cf-worker-deployment-poc.md:13-16` — production impls of the two clerk traits.
2. `docs/dev/cf-worker-deployment-poc.md:18-25` — `CfKvJwksCache` envelope + freshness check.
3. `docs/dev/cf-worker-deployment-poc.md:27-31` — `CfJwksFetcher` zero-size Fetch adapter.
4. `docs/dev/cf-worker-deployment-poc.md:33-41` — the `/health` KV+D1 handler.
5. `docs/dev/cf-worker-deployment-poc.md:42-47` — wrangler.toml bindings.
6. `docs/dev/cf-worker-deployment-poc.md:50-62` — the ring/wasm32 root cause.
7. `docs/dev/cf-worker-deployment-poc.md:50-77` — default-features=false / jwt-adapter fix (POC doc).
7b. `crates/corelink-clerk-cf/Cargo.toml:29-33` — `corelink-clerk = { workspace = true, default-features = false }` (the ring-exclusion enforcer).
8. `docs/dev/cf-worker-deployment-poc.md:84-114` — !Send KV futures + SendFuture wrap (POC doc).
8b. `crates/corelink-clerk-cf/src/cf_kv.rs:89`, `crates/corelink-clerk-cf/src/cf_kv.rs:126`, `crates/corelink-clerk-cf/src/cf_kv.rs:154`, `crates/corelink-clerk-cf/src/cf_fetch.rs:58` — the `worker::send::SendFuture::new` wraps (the enforcer).
9. `docs/dev/cf-worker-deployment-poc.md:106-114` — `+ Send` polite-fiction note.
10. `docs/dev/cf-worker-deployment-poc.md:128-145` — wasm32 zero-warning build verification.
11. `docs/dev/cf-worker-deployment-poc.md:147-163` — wrangler dev/deploy steps.
