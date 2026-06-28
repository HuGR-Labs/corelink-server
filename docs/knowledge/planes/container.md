---
type: "Plane"
title: "Rust container compute plane"
description: "The native Rust binary serving the composed axum data-plane on port 50051: route composition, env-gated mounts, shared handler objects, and fail-CLOSED boot guards."
source_files:
  - "crates/corelink-container/src/main.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/storage/r2_kv.rs"
checkpoint_sha: "0aad76e1d132cd98d35c814a5bb23008c226d08e"
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
   (`crates/corelink-container/src/main.rs:233-267`). A companion POSITIVE prod-arming assertion then
   refuses to boot a HALF-ARMED prod: when an INDEPENDENT R2-region signal says prod, the StorageEnv, PAT
   verifier, $-ceiling, byte-cap and request-count controls must ALL be armed or it FATAL-exits naming the
   missing one. The request-count control it asserts is specifically the OCI-scoped monthly op-cap (the one
   surface the Worker forwards RAW and so cannot meter at the edge); native CAS/AC/Bazel/Turbo +
   cargo/brew/npm/pip are op-count-metered at the WORKER edge, NOT by this container gate. The must-arm set
   also includes `ERASURE_SALT_KEY` (the GDPR DSR account-delete/erasure HMAC salt) — absent it, a prod boot
   looks healthy while every erasure call silently 500s, so it is asserted alongside the PAT/quota/byte-cap
   controls (`crates/corelink-container/src/main.rs:298-376`; `crates/corelink-container/src/main.rs:346-358`).
3. The composed router is built and given a global 10 MiB body limit + the `/_health` route, then bound
   to the listener on PORT (`crates/corelink-container/src/main.rs:568-573`,
   `crates/corelink-container/src/main.rs:835-837`).
4. Privileged routes are env-gated mounts: `/_internal/pat/mint`, `/internal/v1/auth/introspect`,
   `/internal/v1/billing/usage`, `/_internal/dsr/erase`, CAS-erase, tier-select, and the Stripe webhook
   each mount only when their secrets are present (`crates/corelink-container/src/main.rs:575-832`).
5. `build_with_factory` resolves the shared gates from env — the $-ceiling `QuotaGate`, the OCI-scoped
   request-count gate, the native PAT gate, and the byte-accountant — warning (not failing) when absent.
   The request-count gate is DELIBERATELY consumed only by the OCI router (not cloned into the native
   CAS/AC/Bazel/Turbo states like `quota`/`pat_gate`) because native ops are already op-count-metered at
   the Worker edge — cloning it would double-count (`crates/corelink-container/src/routes.rs:347-401`).
6. One set of CAS handler objects is wrapped with byte-accounting + the 410-Gone tombstone gate at a
   single chokepoint, then cloned into every billable surface; the shared `CasRouteState` is constructed
   here with both a `put_inflight` and a `read_inflight` per-tenant concurrency pool
   (`crates/corelink-container/src/routes.rs:403-492`).
7. Cache-adapter surfaces (cargo/brew/npm/pip/oci) mount only when the shared `adapter_pat::PatVerifier`
   and the D1 moat map build from env (`crates/corelink-container/src/routes.rs:652-784`).
8. The residency guard and the per-tenant rate-limit token-bucket are applied as outer layers over the
   composed data plane (`crates/corelink-container/src/routes.rs:791-833`).
9. The Turbo/sccache surface gets its durable backing from the container's R2 KV store: `R2KvStore`
   derives each object key as `<hmac_prefix16>/<opaque_key>` and fails CLOSED (`TurboBridgeError::Internal`)
   on the production path when the TDK is absent or the tenant is not a UUID, rather than degrade to a
   public, predictable prefix two same-millisecond UUIDv7 tenants could collide into
   (`crates/corelink-container/src/storage/r2_kv.rs:122-143`).

# Invariants
- A single HTTP listener on PORT (default 50051) serves the whole data plane — the DO's only target
  (`crates/corelink-container/src/main.rs:378-381`).
- In prod a missing native PAT gate is FATAL — the binary refuses to boot the data plane without its
  Argon2id possession backstop (`crates/corelink-container/src/main.rs:253-266`).
- Privileged routes fail CLOSED: absent secrets ⇒ the route is simply not mounted (404), never an open
  proxy (`crates/corelink-container/src/main.rs:575-588`).
- The 410-Gone erasure gate and byte-accounting are centralized at the shared CAS chokepoint, so every
  surface inherits them by construction (`crates/corelink-container/src/routes.rs:448-481`).
- The rate-limit + residency layers wrap exactly the data plane, never `/_health` or `/_internal/*`
  (which mount later in `main`) (`crates/corelink-container/src/routes.rs:793-833`).

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
5. `crates/corelink-container/src/main.rs:378-381` — PORT resolution (default 50051).
6. `crates/corelink-container/src/main.rs:568-573` — the 10 MiB global body limit + `/_health` route.
7. `crates/corelink-container/src/main.rs:575-588` — env-gated `/_internal/pat/mint` mount (fail-CLOSED).
8. `crates/corelink-container/src/main.rs:575-832` — the full set of env-gated privileged route mounts.
9. `crates/corelink-container/src/main.rs:835-837` — binding the composed router to the PORT listener.
10. `crates/corelink-container/src/routes.rs:333-341` — `build`/`build_with_factory` router composition.
11. `crates/corelink-container/src/routes.rs:347-401` — shared gates resolved from env (quota, OCI-scoped request-count, PAT, accountant); the request-count gate is OCI-only, not cloned into native states (would double-count vs the Worker edge).
12. `crates/corelink-container/src/routes.rs:403-492` — shared CAS handler objects + accounting/tombstone wrap (incl. `put_inflight`/`read_inflight` pools in the `CasRouteState` ctor).
13. `crates/corelink-container/src/routes.rs:448-481` — centralized 410-Gone erasure gate at the CAS chokepoint.
14. `crates/corelink-container/src/routes.rs:652-784` — env-gated cache-adapter (cargo/brew/npm/pip/oci) mounts.
15. `crates/corelink-container/src/routes.rs:791-833` — residency guard + per-tenant rate-limit outer layers.
16. `crates/corelink-container/src/routes.rs:793-833` — the rate-limit layer scoped to the data plane only.
17. `crates/corelink-container/src/main.rs:298-376` — positive prod-arming assertion: an independent R2-region signal ⇒ ALL launch controls must be armed (the request-count one as the OCI op-cap, plus `ERASURE_SALT_KEY` at `crates/corelink-container/src/main.rs:346-358`), else a FATAL boot refusal (no half-armed prod).
18. `crates/corelink-container/src/storage/r2_kv.rs:122-143` — `R2KvStore::object_key`: per-tenant `derive_prefix` HMAC key layout, fail-CLOSED on a non-derivable tenant rather than a public predictable prefix.
