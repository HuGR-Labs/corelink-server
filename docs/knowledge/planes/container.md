---
type: "Plane"
title: "Rust container compute plane"
description: "The native Rust binary serving the composed axum data-plane on port 50051: route composition, env-gated mounts, shared handler objects, and fail-CLOSED boot guards."
source_files:
  - "crates/corelink-container/src/main.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/routes/turbo_v8.rs"
  - "crates/corelink-container/src/storage/r2_kv.rs"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
provenance: "AUTHORED"
tags: ["planes", "container", "rust", "axum", "routing"]
timestamp: "2026-06-26T00:00:00Z"
---

# Rust container compute plane

The container plane is the `corelink-server` Rust binary — the only place the actual cache logic runs.
It binds a single HTTP/1.1 axum stack on port 50051 (the exact port the Durable Object proxies to) and
serves the composed data plane: native CAS/AC, Bazel REAPI, Turbo, sccache/cargo, the `_public`
package-manager adapters, admin, audit-export/analytics, signup, the Stripe webhook, and the internal
mint/introspect/erase routes. Its defining design property is fail-CLOSED env-gating: every privileged
or stateful route mounts only when its secrets + D1/R2 storage env are present, so dev/CI runs the same
binary with those routes simply absent. One shared set of CAS/AC handler objects (and one PAT gate, one
$-ceiling gate, one byte-accountant) is cloned into every billable surface, so accounting, the erasure
tombstone gate, and possession re-verification are inherited uniformly rather than re-implemented per
surface.

# Role
- The native compute plane that serves the real product surface on the DO's `getTcpPort` target port
  50051 (`crates/corelink-container/src/main.rs:1-17`).
- The router composer: `build_with_factory` assembles every data-plane sub-router with shared state
  (`crates/corelink-container/src/routes.rs:333-341`).

# How it works
1. `main` selects the storage backing (`"r2"` vs `"inmemory"`) once at boot and surfaces it on
   `/_health` (`crates/corelink-container/src/main.rs:218-231`).
2. A fail-CLOSED boot guard refuses to start in prod if the native PAT verifier did not build — a
   PRESENT-but-malformed signing-key sibling is FATAL, not a silent downgrade
   (`crates/corelink-container/src/main.rs:233-267`).
3. The composed router is built and given a global 10 MiB body limit + the `/_health` route, then bound
   to the listener on PORT (`crates/corelink-container/src/main.rs:459-464`,
   `crates/corelink-container/src/main.rs:725-728`).
4. Privileged routes are env-gated mounts: `/_internal/pat/mint`, `/internal/v1/auth/introspect`,
   `/internal/v1/billing/usage`, `/_internal/dsr/erase`, CAS-erase, tier-select, and the Stripe webhook
   each mount only when their secrets are present (`crates/corelink-container/src/main.rs:466-723`).
5. `build_with_factory` resolves the shared gates from env — the $-ceiling `QuotaGate`, the request-
   count gate, the native PAT gate, and the byte-accountant — warning (not failing) when absent
   (`crates/corelink-container/src/routes.rs:347-390`).
6. One set of CAS handler objects is wrapped with byte-accounting + the 410-Gone tombstone gate at a
   single chokepoint, then cloned into every billable surface
   (`crates/corelink-container/src/routes.rs:392-480`).
7. Cache-adapter surfaces (cargo/brew/npm/pip/oci) mount only when the shared `adapter_pat::PatVerifier`
   and the D1 moat map build from env (`crates/corelink-container/src/routes.rs:641-773`).
8. The residency guard and the per-tenant rate-limit token-bucket are applied as outer layers over the
   composed data plane (`crates/corelink-container/src/routes.rs:780-822`).

# Invariants
- A single HTTP listener on PORT (default 50051) serves the whole data plane — the DO's only target
  (`crates/corelink-container/src/main.rs:269-274`).
- In prod a missing native PAT gate is FATAL — the binary refuses to boot the data plane without its
  Argon2id possession backstop (`crates/corelink-container/src/main.rs:253-266`).
- Privileged routes fail CLOSED: absent secrets ⇒ the route is simply not mounted (404), never an open
  proxy (`crates/corelink-container/src/main.rs:466-479`).
- The SHARED CAS handlers (native CAS/AC, Bazel, cargo, `_public`) inherit the tombstone-410 gate +
  `AccountingCasHandler` byte-accounting from the single CAS chokepoint
  (`crates/corelink-container/src/routes.rs:437-470`); Turborepo, however, writes through its OWN
  `R2KvStore` (separate bucket — `R2_TURBO_BUCKET`, default `corelink-turbo-prod`,
  `crates/corelink-container/src/storage/r2_kv.rs:248`) — the `AccountingCasHandler` decorator does
  NOT cover it, so it does NOT inherit the shared 410 tombstone gate (turbo's quota/PAT/accounting are
  wired separately at its route) (`crates/corelink-container/src/routes/turbo_v8.rs:978-980`).
- The rate-limit + residency layers wrap exactly the data plane, never `/_health` or `/_internal/*`
  (which mount later in `main`): the per-tenant rate-limit middleware is applied by the executed
  `router = router.layer(axum::middleware::from_fn_with_state(rate_limit_state, rate_limit_layer))`
  (`crates/corelink-container/src/routes.rs:825`), over the data-plane router assembled at `:782-822`.

# Gotchas
- The native CAS/AC/Bazel/Turbo plane trusts the Worker-injected `x-corelink-tenant-id`, with the
  container-side PAT gate as the possession backstop; adapter routes additionally re-verify the bearer
  PAT against D1 (Option-B).
- A historical tonic gRPC server on 50051 was dead weight that blocked the port; the binary now serves
  the real HTTP data plane there directly.

# Citations
1. `crates/corelink-container/src/main.rs:1-17` — the binary's role + the 50051 HTTP listener doc.
2. `crates/corelink-container/src/main.rs:218-231` — boot-time storage-backing selection for `/_health`.
3. `crates/corelink-container/src/main.rs:233-267` — the fail-CLOSED native-PAT-gate boot guard.
4. `crates/corelink-container/src/main.rs:253-266` — the FATAL `std::process::exit(1)` on a missing prod gate.
5. `crates/corelink-container/src/main.rs:269-274` — PORT resolution (default 50051).
6. `crates/corelink-container/src/main.rs:459-464` — the 10 MiB global body limit + `/_health` route.
7. `crates/corelink-container/src/main.rs:466-479` — env-gated `/_internal/pat/mint` mount (fail-CLOSED).
8. `crates/corelink-container/src/main.rs:466-723` — the full set of env-gated privileged route mounts.
9. `crates/corelink-container/src/main.rs:725-728` — binding the composed router to the PORT listener.
10. `crates/corelink-container/src/routes.rs:333-341` — `build`/`build_with_factory` router composition.
11. `crates/corelink-container/src/routes.rs:347-390` — shared gates resolved from env (quota, PAT, accountant).
12. `crates/corelink-container/src/routes.rs:392-480` — shared CAS handler objects + accounting/tombstone wrap.
13. `crates/corelink-container/src/routes.rs:437-470` — centralized 410-Gone erasure gate at the CAS chokepoint.
13b. `crates/corelink-container/src/routes/turbo_v8.rs:978-980` — Turbo's OWN `R2KvStore` is NOT decorated by `AccountingCasHandler`, so it does NOT inherit the shared 410 tombstone gate (separate bucket).
13c. `crates/corelink-container/src/storage/r2_kv.rs:248` — the Turbo `R2KvStore` bucket = `R2_TURBO_BUCKET`, default `corelink-turbo-prod`.
14. `crates/corelink-container/src/routes.rs:641-773` — env-gated cache-adapter (cargo/brew/npm/pip/oci) mounts.
15. `crates/corelink-container/src/routes.rs:780-822` — residency guard + per-tenant rate-limit outer layers.
16. `crates/corelink-container/src/routes.rs:825` — the EXECUTED `.layer(...rate_limit_layer)` application of the per-tenant rate-limit middleware over the data-plane router built at `:782-822`.
