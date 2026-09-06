---
type: "CrateCluster"
title: "Handler-trait seam (CAS/AC/customer/erase/admin)"
description: "The hot-path Arc<dyn …Handler> abstraction between the container's HTTP routes and the storage/business logic — split read/write/delete/list trait objects per surface, dependency-injected at one chokepoint so every cache surface inherits accounting, erasure-gating, and audit/SLI discipline by composition rather than per-route plumbing."
source_files:
  - "crates/corelink-handler-cas/src/handler.rs"
  - "crates/corelink-handler-cas/src/lib.rs"
  - "crates/corelink-handler-cas/src/audit.rs"
  - "crates/corelink-handler-cas/src/observer.rs"
  - "crates/corelink-handler-ac/src/handler.rs"
  - "crates/corelink-handler-customer/src/handler.rs"
  - "crates/corelink-handler-cas-erase/src/handler.rs"
  - "crates/corelink-handler-admin/src/handler.rs"
  - "crates/corelink-container/src/routes/cas/foundation.rs"
  - "crates/corelink-container/src/routes/cas/single.rs"
  - "crates/corelink-container/src/routes/cas/batch.rs"
  - "crates/corelink-container/src/routes/cas/list_delete.rs"
  - "crates/corelink-container/src/routes/ac/part-00.rs"
  - "crates/corelink-container/src/routes/admin/part-00.rs"
  - "crates/corelink-container/src/routes/admin/part-01.rs"
  - "crates/corelink-container/src/routes/customer/part-00.rs"
  - "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01.rs"
  - "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs"
  - "crates/corelink-container/src/routes/public_revoke.rs"
  - "crates/corelink-container/src/routes/public_mirror.rs"
  - "crates/corelink-container/src/customer_d1_seams.rs"
  - "crates/corelink-container/src/customer_d1_maps_calendar.rs"
  - "crates/corelink-container/src/customer_d1_handler_state.rs"
  - "crates/corelink-container/src/customer_d1_overview_usage.rs"
  - "crates/corelink-container/src/customer_d1_billing_keys.rs"
  - "crates/corelink-container/src/customer_d1_team_audit.rs"
  - "crates/corelink-container/src/customer_d1_byok_config.rs"
  - "crates/corelink-container/src/customer_d1_byok_writer.rs"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs"
  - "crates/corelink-container/src/routes/build.rs"
  - "crates/corelink-container/src/routes/cas/foundation.rs"
  - "crates/corelink-container/src/routes/cas/batch.rs"
  - "crates/corelink-container/src/routes/cas/single.rs"
  - "crates/corelink-container/src/routes/cas/list_delete.rs"
  - "crates/corelink-container/src/customer_d1_overview_usage.rs"
  - "crates/corelink-container/src/customer_d1_billing_keys.rs"
  - "crates/corelink-container/src/customer_d1_handler_state.rs"
  - "crates/corelink-container/src/customer_d1_team_audit.rs"
  - "crates/corelink-container/src/customer_d1_seams.rs"
  - "crates/corelink-container/src/customer_d1_byok_config.rs"
  - "crates/corelink-container/src/customer_d1_byok_writer.rs"
  - "crates/corelink-container/src/customer_d1_maps_calendar.rs"
source_blobs:
  - "crates/corelink-handler-cas/src/handler.rs@d3a5d462c39f5fd62b3b9fe7a305ce72c340646f"
  - "crates/corelink-handler-cas/src/lib.rs@f54c2425b3c509a715451f400cb74485b57c1edf"
  - "crates/corelink-handler-cas/src/audit.rs@aba086e34060ae5faec4441f6ac9d9c43e7443db"
  - "crates/corelink-handler-cas/src/observer.rs@5d499c7be8e8036bf43b7c837d6994f094b069a8"
  - "crates/corelink-handler-ac/src/handler.rs@cd7d9fb33fb7c39e49154bafaa3425228c8b5f2d"
  - "crates/corelink-handler-customer/src/handler.rs@0eeb3f875e452fe4b96dfc42aa2dedcf6a542c3f"
  - "crates/corelink-handler-cas-erase/src/handler.rs@e7e7615d6e450e41b8dad3e02fe8501f8992db1e"
  - "crates/corelink-handler-admin/src/handler.rs@90ec0468bf20d3790194086e0a633547bb3fd564"
  - "crates/corelink-container/src/routes/cas/foundation.rs@c2348f6ddb4567e8c902fd237213f86957300cdc"
  - "crates/corelink-container/src/routes/cas/single.rs@bbb3c5514c7c720d2374a0bd29ac6cf2fbc46fab"
  - "crates/corelink-container/src/routes/cas/batch.rs@bc3b531744e3659d850eb1f415c2ec015df00e55"
  - "crates/corelink-container/src/routes/cas/list_delete.rs@0a5e23ba9d812af9ffe5bd76e1f9a88d91dc12eb"
  - "crates/corelink-container/src/routes/ac/part-00.rs@80864cc770f76f6a66647c9772d1491fb949ddf5"
  - "crates/corelink-container/src/routes/admin/part-00.rs@944b79eebf91ec4c783f85615fa5d3a49956cda4"
  - "crates/corelink-container/src/routes/admin/part-01.rs@b78c6cb1cedb54d8c61696eba61ca7e41043d1af"
  - "crates/corelink-container/src/routes/customer/part-00.rs@c5c7814f2affd23f52bdc09421a0cafefd015b1a"
  - "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01.rs@9eb23ec8de2a77768c545add01be3d0b33803e5d"
  - "crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs@1491ff8bcf742b7db1dcc52d8b5443e8fec22496"
  - "crates/corelink-container/src/routes/public_revoke.rs@7818f29203e884ea1873022545c37ad497b8344d"
  - "crates/corelink-container/src/routes/public_mirror.rs@629a9ac8cd3ae6cc13cdf9f890144082c1317169"
  - "crates/corelink-container/src/customer_d1_seams.rs@86565d70a10640bbbce51d9626dae6cbda4e1c7b"
  - "crates/corelink-container/src/customer_d1_maps_calendar.rs@e7e470f8d4c7f025b8e5478ed6d2dcceea3646c5"
  - "crates/corelink-container/src/customer_d1_handler_state.rs@ef744353ef1918f048534fd2acaf19767e76074c"
  - "crates/corelink-container/src/customer_d1_overview_usage.rs@61bab5af3938b1565d99e3f904d388870a7e79cb"
  - "crates/corelink-container/src/customer_d1_billing_keys.rs@6053009007785f810e570f4b251f281c7fcddef0"
  - "crates/corelink-container/src/customer_d1_team_audit.rs@6a27f720a87866a6adc45d9f292029d0006aa504"
  - "crates/corelink-container/src/customer_d1_byok_config.rs@5fdaa13e0b9b2ae4803baf4bd013df810c05cc71"
  - "crates/corelink-container/src/customer_d1_byok_writer.rs@443d99400a36e408f92d3fcd214ead88656e46ad"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs@03bfa805e26de98868ef7ab0e9f15492c9119887"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs@bd65dfc70297a7fea8ef25cb614c6404feac220e"
  - "crates/corelink-container/src/routes/build.rs@7a9035bbb859a3717cfceec3dbcb77622e5e1268"
checkpoint_sha: "fc7ec9bb9c5d8711cabc4b93c989062e71d2955f"
provenance: "AUTHORED"
tags: ["handlers", "traits", "cas", "hot-path", "dependency-injection"]
timestamp: "2026-06-29T00:00:00Z"
---
# Handler-trait seam (CAS/AC/customer/erase/admin)

The container's HTTP route table never names a concrete storage type. Between an axum handler and the storage/business logic sits a thin layer of `Send + Sync + Debug` traits — `CasReadHandler` / `CasWriteHandler` / `CasDeleteHandler` / `CasListHandler` for CAS, the parallel `AcLookupHandler` / `AcUpdateHandler` / `AcDeleteHandler` / `AcListHandler` for the action cache, six `Customer*Handler` traits for the control plane, and `AdminReadHandler` / `AdminMutateHandler` for ops — each held by the route as an `Arc<dyn …Handler>`. The route delegates one verb to one trait object and maps the typed result to an HTTP status. This is the seam at which CoreLink swaps an in-memory fake (dev/CI/tests) for the real R2/D1-backed impl, and — more importantly — the single chokepoint at which cross-cutting concerns (byte accounting, GDPR erasure-gating, audit/SLI emission) are layered onto *every* cache surface at once by wrapping the shared trait objects, with no per-route code.

# Role

The seam exists for two reasons that the code makes concrete:

1. **Read/write are split into separate traits per surface, not one fat handler.** `CasReadHandler::read` and `CasWriteHandler::write` are distinct traits (`crates/corelink-handler-cas/src/handler.rs:34`, `:139`), as are `CasDeleteHandler::delete` (`:162`) and `CasListHandler::list` (`:182`). The split lets the route table compose read and write capability independently and lets a decorator wrap *only* the mutating half. The shared collaborator pair `(Arc<dyn AuditSink>, Arc<dyn SliObserver>)` is held by the concrete impl, so the cross-handler invariants (audit-fail-CLOSED ordering, SLI-emit-on-entry) are provable by composition rather than inheritance.

2. **One injection chokepoint fans the cross-cutting decorators out to every surface.** `routes/build.rs` builds the raw CAS handlers once (`crates/corelink-container/src/routes/build.rs:97`), wraps the *write + delete* trait objects in the byte-accounting decorator (`crates/corelink-container/src/routes/build.rs:117`), then wraps read/write/delete in the erasure-gate decorator (`crates/corelink-container/src/routes/build.rs:167`), and hands the resulting `Arc<dyn …>` to `CasRouteState`. Because native CAS, Bazel REAPI, OCI, and the cargo/brew/npm/pip language adapters all drive these *same* shared trait objects, wrapping at this one point gives all of them identical, atomic, fail-CLOSED accounting and a uniform erasure gate with zero per-surface duplication.

The `Arc<dyn>` (vs a generic `H: CasWriteHandler` type parameter) is deliberate: it lets two decorators (`AccountingCasHandler`, `TombstoneGatedCasHandler`) stack at runtime behind the same `CasRouteState` field type, and lets the *same* handler instance back several surfaces from one R2 connection.

# How it works

**The trait contract (CAS, canonical).** Each trait carries a documented emit-discipline its impls MUST honour: `read` audits `ReadAttempted` before the lookup and emits `Sli::AvailCasGet`/`LatencyCasGetP99` on every return path; `write` audits `WriteAttempted` *before* the mutation and aborts with `AuditFailed` without mutating if the audit emit fails, verifies the claimed hash before storing, and audits `WriteCommitted` only after the durable store. The in-process reference impl `InMemoryCasHandler` enforces exactly this ordering — its `write` emits `WriteAttempted` (`crates/corelink-handler-cas/src/handler.rs:437`) and stores only after (`:482`), and a `write_audit_failure_aborts_before_storing` test asserts storage stays empty when the audit sink is forced to fail. The `exists` HEAD-probe defaults to `read` (download + rehash) so test handlers keep working, but storage-backed impls MUST override it with a true HEAD — the default is correct but not cheap (`crates/corelink-handler-cas/src/handler.rs:74`), a Bazel `findMissingBlobs` egress amplification fix. The read trait carries a SECOND optional capability over the same surface: `exists_batch` asks the presence question about many digests at once and defaults to `None` (`crates/corelink-handler-cas/src/handler.rs:114`), meaning "no batch capability — loop `exists`". Returning `Option<Result<…>>` rather than a plain `Result` is what keeps the seam additive: every impl that does not opt in keeps the per-digest loop byte-identical, so a storage-backed impl can collapse the per-digest round trips without any test handler changing semantics.

**The collaborators are themselves traits.** `AuditSink::emit` (`crates/corelink-handler-cas/src/audit.rs:122`) is fail-CLOSED (returns `Err` if the row cannot be durably written; the handler aborts on that error), and `SliObserver::observe` (`crates/corelink-handler-cas/src/observer.rs:47`) is infallible by design (an observer failure must not take the handler down). This is what makes the audit-before-mutation invariant a composable property of *any* impl, not a property hand-coded into each route.

**The live route consumes the trait, not the type.** `CasRouteState` declares `read: Arc<dyn CasReadHandler>`, `write: Arc<dyn CasWriteHandler>`, `delete`, `list` as four distinct trait-object fields (`crates/corelink-container/src/routes/cas/foundation.rs:200`). The handlers delegate one verb each: `handle_read` calls `state.read.read(req)` (`crates/corelink-container/src/routes/cas/batch.rs:309`), `handle_write` calls `state.write.write(req)` (`crates/corelink-container/src/routes/cas/single.rs:435`), `handle_delete` calls `state.delete.delete(req)` (`crates/corelink-container/src/routes/cas/list_delete.rs:34`), `handle_list` calls `state.list.list(req)` (`crates/corelink-container/src/routes/cas/list_delete.rs:103`). When R2 creds are present but the R2 handler refuses to build, the route mounts the fail-CLOSED `UnavailableCasHandler` whose every method returns a 503 sentinel (`crates/corelink-container/src/routes/cas/single.rs:507`) instead of silently degrading to the in-memory fake.

**Byte accounting wraps the write/delete trait objects.** `AccountingCasHandler` is itself an impl of `CasWriteHandler` (`crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs:703`) and `CasDeleteHandler` (`crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs:179`) that holds the inner `Arc<dyn CasWriteHandler>` + `Arc<dyn CasDeleteHandler>` and a `ByteAccountant`. Its `write` reserves bytes against `tenant_storage_state` *before* calling `self.write_inner.write(req)` (the `block_on_accrue` reservation at `crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs:139`) — an over-cap or indeterminate reservation rejects so the durable PUT never runs — and releases the reservation if the inner write fails or stored nothing new (idempotent re-write). Its `delete` calls `self.delete_inner.delete(req)` (`crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs:179`) then releases the reclaimed bytes. **F3.2 B4 — the `_public` shared-dedup namespace is UNOWNED:** it is not a billable tenant, so the decorator forces the genuine-unlimited shared-meter seed (`quota_seed = Some(0)`) for it (`crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs:808`), which makes a moat write ALWAYS accrue-and-pass, SELF-SEED a missing per-region row (the `Some(0)` UPSERT INSERT branch), and never be capped by a stray finite `bytes_quota` — so the cross-tenant public write path cannot be outaged by the accounting layer, while `bytes_used` still tracks the shared cache's COGS. A non-`_public` tenant is charged byte-identically to before. Both hold a per-`(tenant, hash)` shard lock across the whole reserve→commit→release so a concurrent write-vs-delete of the same key cannot interleave their accounting (rt-nuclear C2). As of BYOK Wave 3b/3c the decorator no longer reserves the raw request length blindly: for a BYOK-`active` tenant it computes the COMMITTED (stored) size via `byok_committed_len`, which is now mode-aware — Mode A (convergent) reserves the plaintext length plus `BYOK_CLB1_OVERHEAD` (32 B: 4-byte magic + 12-byte nonce + 16-byte AEAD tag), and Mode B (random) reserves plaintext plus `BYOK_CLB2_OVERHEAD` (20 B: 4-byte magic + 16-byte tag, since the Mode-B nonce lives in the `byok_envelope` D1 row, not inline) — so `reserve == release` and `bytes_used` cannot drift once encryption is engaged; a non-BYOK / inactive tenant still reserves the plaintext length, byte-identical to before. The `AccountingAcHandler` is the exact mirror over `AcUpdateHandler`/`AcDeleteHandler`.

**Erasure-gating wraps read/write/delete.** `TombstoneGatedCasHandler` likewise impls `CasReadHandler` (`crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs:202`) and `CasWriteHandler` (`crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs:400`), consulting a `TombstoneStore` trait (`:81`) so a tombstoned read 404s and a re-PUT of a tombstoned hash is refused — applied at the *same* chokepoint as accounting, so every surface inherits the GDPR gate. The read gate uses the (bloom-fast-pathed) `is_tombstoned`, but the WRITE gate uses the AUTHORITATIVE `TombstoneStore::is_tombstoned_authoritative` (`:99`) so a within-window cross-instance erase can never be resurrected by a re-PUT slipping through a stale local bloom (finding H3). Note the pure-logic crate `corelink-handler-cas-erase` defines *no* trait object: it owns I/O-free decision functions (`prepare_erase` at `crates/corelink-handler-cas-erase/src/handler.rs:145`, `read_gate` at `:168`) that the container's `cas_erase.rs` wiring composes over D1/R2 transports. It is the seam's logic kernel, not part of the `Arc<dyn>` chain. The `CasBlobEraser` half of that seam (the R2 byte-deleter, distinct from the tombstone gate) has a SECOND consumer beyond the per-tenant DSR erase: the F3.2 `_public` blob revocation route holds the same `Arc<dyn CasBlobEraser>` (`crates/corelink-container/src/routes/public_revoke.rs:194`) and drives it with the literal `_public` sentinel namespace to hard-delete a poisoned cross-tenant public blob, mounted fail-CLOSED from env alongside `cas_erase` under the shared `CORELINK_ERASE_AUTH_KEY` (`crates/corelink-container/src/routes/public_revoke.rs:648`) — reusing the frozen erase seam rather than duplicating the R2 delete.

**The `_public` WRITE side reuses the moat put-seam + the shared SSRF guard (F3.2 inc4b mirror).** The structural sibling of `public_revoke.rs` (the `_public` *delete* side above) is `public_mirror.rs` — the `_public` *write* side. Its admin-gated `POST /_internal/admin/public-mirror/promote` promotes an owner-pinned upstream base layer into the shared cache via `fetch_verify_promote` (`crates/corelink-container/src/routes/public_mirror.rs:338`), which is fail-CLOSED at every step: the digest must be `is_allowlisted` before anything is fetched (`:355`), the bytes are pulled from the FIXED upstream registry (never a caller URL) through the SINGLE audited SSRF guard — the container reuses `corelink_adapter_host::upstream_ssrf::ssrf_safe_redirect_policy` for the 307→CDN redirect (`:469`) and `host_is_internal_ip` for the anonymous-token realm (`:503`) rather than re-implementing SSRF — then `OciDigest::verify_against_bytes` re-hashes the fetched bytes against the declared digest BEFORE any write (`:369`), and only then does it `MoatCache::put` into the `_public` namespace with the shared-meter seed `Some(0)` (`:374`). This is the SECOND and LAST legitimate `_public` writer (the exhaustive set is `{ tenant OCI finalize_upload, this inc4b admin mirror }`); the read pull-through (WP-G) reuses the SAME `fetch_verify_promote` helper but passes a *tenant* namespace, so it adds no third `_public` writer. The route mounts fail-CLOSED from env (`:166`) — unmounted without `CORELINK_ADMIN_AUTH_KEY` (shared `CORELINK_INTERNAL_AUTH_KEY` fallback) or the D1/R2 storage env the moat needs. It gates on the ADMIN key, not a dedicated mirror key: the edge maps `/_internal/admin/*` to the admin consumer and forwards that header verbatim, so a dedicated key could never match (this is unlike the erase-keyed `public_revoke`, whose no-fallback erase key is the H4 REVOKE-path control — the mirror only PROMOTES with verify-before-write, so admin-level auth is correct). As of WP-E Roll-1 the shipped allowlist activates exactly the alpine pin, so a promote of that digest succeeds while every other digest rejects at the allowlist gate; client-side `_public` dedup stays OFF, so only this server mirror can populate `_public` (server-populates-first).

**AC / admin / customer follow the same shape.** AC delegates `state.lookup.lookup(req)` / `update` / `delete` / `list` at `crates/corelink-container/src/routes/ac/part-00.rs:557`, `:641`, `:734`, `crates/corelink-container/src/routes/ac/part-00.rs:641`. Admin delegates `state.read.read(req)` (`crates/corelink-container/src/routes/admin/part-01.rs:166`) and `state.mutate.mutate(req)` (`crates/corelink-container/src/routes/admin/part-01.rs:227`) — the mutate contract additionally enforces dual-approval (reject `DualApprovalMissing`/self-approval) per the trait doc at `crates/corelink-handler-admin/src/handler.rs:220`. The admin surface's constant-time internal-auth gate `internal_auth_ok` is now `pub(crate)` (`crates/corelink-container/src/routes/admin/part-00.rs:85`) so the sibling operator per-tenant read module `admin_tenant_detail`, merged into the SAME router (`crates/corelink-container/src/routes/build.rs:380`), reuses the exact same gate rather than re-implementing the compare — one source of truth for the operator-auth boundary. The customer control plane is six narrow traits (`CustomerOverviewHandler` at `crates/corelink-handler-customer/src/handler.rs:45-45`, plus usage/billing/keys/team/audit) whose **live production impl** is `D1CustomerHandler` in the extracted modules `customer_d1_overview_usage.rs`, `customer_d1_billing_keys.rs`, and `customer_d1_team_audit.rs` — all six impls use the inner `Arc<dyn CustomerD1>` D1-over-HTTP transport (`crates/corelink-container/src/customer_d1_overview_usage.rs:4`, `crates/corelink-container/src/customer_d1_billing_keys.rs:3`, `crates/corelink-container/src/customer_d1_team_audit.rs:7`). `build_handlers_from_env` puts the *same* `D1CustomerHandler` `Arc` behind all six route-state slots when D1 creds are present (`crates/corelink-container/src/routes/customer/part-00.rs:133-155`), falling back to `InMemoryCustomerHandler` in dev/CI; the route then delegates `state.overview.overview(req)` (`:564-566`), `state.usage.usage(req)` (`:626-628`), `state.billing.billing(req)` (`:721`), etc.

# Invariants

- **Audit-before-mutation, fail-CLOSED.** Every mutating impl emits the `*Attempted` audit row *before* the storage mutation and aborts with `AuditFailed` (no mutation) if the emit fails — pinned by the in-memory CAS `write` ordering at `crates/corelink-handler-cas/src/handler.rs:452-452`/`:478-484` and the fail-CLOSED `AuditSink::emit` contract at `crates/corelink-handler-cas/src/audit.rs:122`.
- **SLI emit on every return path.** `read`/`write`/`delete`/`list` emit their availability + latency SLI tuple on *every* exit including error paths (the trait doc at `crates/corelink-handler-cas/src/handler.rs:497-497`; the fake's `emit(true)` on the denial path).
- **Read and write are separable trait objects.** `CasReadHandler` and `CasWriteHandler` are distinct traits (`crates/corelink-handler-cas/src/handler.rs:34`, `:139`) so a decorator can wrap only the mutating half and a read-only build can omit the write half.
- **Accounting reserves before the durable write.** `AccountingCasHandler::write` reserves bytes before `write_inner.write` (`crates/corelink-container/src/byte_accounting/b126_m2_impl_02.rs:138`); an over-cap reservation means the inner PUT never runs.
- **The `_public` shared namespace is unowned (never fail-closed, never capped).** `AccountingCasHandler::write` forces `quota_seed = Some(0)` when `req.tenant` is the `_public` moat namespace (`crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs:822`), so a shared-cache write self-seeds its meter row and always accrues-and-passes — the accounting layer can never outage the cross-tenant public write path.
- **Cross-cutting decorators stack at one chokepoint.** `routes/build.rs` wraps the shared write/delete objects in accounting (`crates/corelink-container/src/routes/build.rs:117`) then read/write/delete in the erasure gate (`crates/corelink-container/src/routes/build.rs:167`) once, so all surfaces sharing those `Arc<dyn>` inherit both.
- **One shared handler instance backs many surfaces.** The same `cas_read`/`cas_write` `Arc`s are cloned into the cargo/brew/Bazel states (`crates/corelink-container/src/routes/build.rs:232`), so the decorators apply uniformly.
- **The `_public` writer set is exhaustive (two), and every `_public` write is digest-verified fail-CLOSED.** The only legitimate writers into the shared namespace are the tenant OCI `finalize_upload` and the inc4b admin mirror; the mirror's `fetch_verify_promote` runs the `is_allowlisted` gate (`crates/corelink-container/src/routes/public_mirror.rs:355`) and `OciDigest::verify_against_bytes` (`:369`) BEFORE the `MoatCache::put` into `_public` (`:374`), so an arbitrary or lying digest can never reach the shared slot. The read pull-through reuses the SAME helper but promotes per-tenant, adding no third `_public` writer.
- **The `_public` audit namespace has an explicit residency arm.** Public revocation writes use the `_public` sentinel and pin the outbox row to `wnam`; the tenant-residency trigger does not infer a customer region for this global namespace (`crates/corelink-container/src/routes/public_revoke.rs:614-626`).

# Gotchas

- **`corelink-handler-cas-erase` defines no trait object.** Unlike the other four crates, it is a pure-logic kernel (`prepare_erase`, `read_gate`, `validate_digest`) — the `TombstoneStore` trait and the `TombstoneGatedCasHandler` `CasReadHandler`/`CasWriteHandler` impls live in the container's `cas_erase.rs` (`crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs:237`, `crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs:400`), not in the handler crate. Don't look for an `Arc<dyn EraseHandler>`; there isn't one.
- **The customer production impl lives in the container, not the handler crate.** `corelink-handler-customer` ships only the six traits + an in-memory fake; the live D1-backed `D1CustomerHandler` is in `crates/corelink-container/src/customer_d1_overview_usage.rs:138`. The handler crate is the *contract*, the container is the *impl* — a `customer_d1.rs:140 impl CustomerD1` is a *second* transport trait the handler delegates to, not the route-facing trait.
- **A batch capability is opt-in, and `None` must stay the safe answer.** `CasReadHandler::exists_batch` returns `Option<Result<Vec<bool>, CasHandlerError>>` and defaults to `None` (`crates/corelink-handler-cas/src/handler.rs:114`). An impl that opts in inherits every per-digest guarantee `exists` makes — cross-tenant denial before any storage work is dispatched, audit rows committed before any result is returned — and must answer in REQUEST order, not completion order. An impl answering a different number of flags than it was asked about would silently mis-report presence, so a caller refuses a length mismatch rather than guessing.
- **`exists` HEAD-probe must be overridden by storage-backed impls.** The default `CasReadHandler::exists` (`crates/corelink-handler-cas/src/handler.rs:492-492`) falls back to a full `read` (download + rehash). A storage adapter that forgets to override it turns Bazel `findMissingBlobs` into tens of GiB of egress per request — correct but ruinously expensive.
- **Byte accounting wraps write+delete only, not read/list.** `crates/corelink-container/src/routes/build.rs:112` binds only the write/delete decorator tuple, leaving read/list unwrapped (they are not write surfaces). If a future read path needs accounting, it does *not* get it for free from this decorator.
- **Decorator order matters: accounting is inner, erasure-gate is outer.** `routes/build.rs` wraps accounting first (`crates/corelink-container/src/routes/build.rs:117`) then the tombstone gate (`crates/corelink-container/src/routes/build.rs:167`) around the already-accounted handler, so a tombstoned re-PUT is refused by the outer gate *before* the inner accounting reserves bytes — reversing the order would reserve bytes for a write the erasure gate then rejects.
- **`AuditSink` is fail-CLOSED but `SliObserver` is fail-OPEN.** A failed audit emit aborts the operation; a failed SLI observe must not. Conflating them (making the observer fallible-and-aborting) would let a metrics hiccup take down the data plane.

# Citations

- `crates/corelink-handler-cas/src/handler.rs:34` — `pub trait CasReadHandler` definition (`read`).
- `crates/corelink-handler-cas/src/handler.rs:74` — default `exists` HEAD-probe falling back to `read`.
- `crates/corelink-handler-cas/src/handler.rs:114` — default `exists_batch` returns `None` (no batch capability ⇒ the caller loops `exists`).
- `crates/corelink-handler-cas/src/handler.rs:139` — `pub trait CasWriteHandler` definition (`write`).
- `crates/corelink-handler-cas/src/handler.rs:162` — `pub trait CasDeleteHandler` definition (`delete`).
- `crates/corelink-handler-cas/src/handler.rs:182` — `pub trait CasListHandler` definition (`list`).
- `crates/corelink-handler-cas/src/handler.rs:437` — `InMemoryCasHandler::write` emits `WriteAttempted` before mutation.
- `crates/corelink-handler-cas/src/handler.rs:491-499` — durable store happens after the attempt audit.
- `crates/corelink-handler-cas/src/lib.rs:70` — public re-export of the CAS handler traits + in-memory fake.
- `crates/corelink-handler-cas/src/audit.rs:122` — `pub trait AuditSink::emit` (fail-CLOSED collaborator).
- `crates/corelink-handler-cas/src/observer.rs:47` — `pub trait SliObserver::observe` (infallible collaborator).
- `crates/corelink-handler-ac/src/handler.rs:323` — `pub trait AcLookupHandler`.
- `crates/corelink-handler-ac/src/handler.rs:341` — `pub trait AcUpdateHandler`.
- `crates/corelink-handler-ac/src/handler.rs:359` — `pub trait AcDeleteHandler`.
- `crates/corelink-handler-ac/src/handler.rs:376` — `pub trait AcListHandler`.
- `crates/corelink-handler-customer/src/handler.rs:79-79` — `pub trait CustomerOverviewHandler` (one of six control-plane traits).
- `crates/corelink-handler-customer/src/handler.rs:77` — `pub trait CustomerBillingHandler`.
- `crates/corelink-handler-admin/src/handler.rs:192` — `pub trait AdminReadHandler`.
- `crates/corelink-handler-admin/src/handler.rs:220` — `pub trait AdminMutateHandler` (dual-approval contract).
- `crates/corelink-handler-cas-erase/src/handler.rs:145` — `prepare_erase` pure decision fn (no trait object in this crate).
- `crates/corelink-handler-cas-erase/src/handler.rs:168` — `read_gate` pure decision fn.
- `crates/corelink-container/src/routes/cas/foundation.rs:200` — `CasRouteState` holds `Arc<dyn CasReadHandler>` etc. (the seam, route side).
- `crates/corelink-container/src/routes/cas/single.rs:507` — fail-CLOSED `UnavailableCasHandler` impl of the read trait.
- `crates/corelink-container/src/routes/cas/batch.rs:309` — `handle_read` delegates `state.read.read(req)`.
- `crates/corelink-container/src/routes/cas/list_delete.rs:34` — `handle_delete` delegates `state.delete.delete(req)`.
- `crates/corelink-container/src/routes/cas/list_delete.rs:103` — `handle_list` delegates `state.list.list(req)`.
- `crates/corelink-container/src/routes/ac/part-00.rs:557` — `state.lookup.lookup(req)` delegation.
- `crates/corelink-container/src/routes/ac/part-00.rs:640` — `state.update.update(req)` delegation.
- `crates/corelink-container/src/routes/admin/part-01.rs:166` — `state.read.read(req)` delegation.
- `crates/corelink-container/src/routes/admin/part-01.rs:226` — `state.mutate.mutate(req)` delegation.
- `crates/corelink-container/src/routes/admin/part-00.rs:85` — `internal_auth_ok` is now `pub(crate)` (the shared constant-time operator-auth gate reused by `admin_tenant_detail`).
- `crates/corelink-container/src/routes/customer/part-00.rs:574` — `build_handlers_from_env` wires the same `D1CustomerHandler` Arc behind all six slots.
- `crates/corelink-container/src/routes/customer/part-00.rs:637` — `state.overview.overview(req)` delegation.
- `crates/corelink-container/src/routes/customer/part-00.rs:733` — `state.billing.billing(req)` delegation.
- `crates/corelink-container/src/customer_d1_overview_usage.rs:4-12` — `impl CustomerOverviewHandler for D1CustomerHandler` (live production impl).
- `crates/corelink-container/src/customer_d1_billing_keys.rs:3-12` — `impl CustomerBillingHandler for D1CustomerHandler`.
- `crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01.rs:17` — `pub trait TombstoneStore` (container-side).
- `crates/corelink-container/src/routes/cas_erase/b126_m2_impl_01.rs:35` — `impl CasReadHandler for TombstoneGatedCasHandler` (erasure-gate decorator).
- `crates/corelink-container/src/routes/cas_erase/b126_m2_impl_02.rs:208` — `impl CasWriteHandler for TombstoneGatedCasHandler`.
- `crates/corelink-container/src/routes/public_revoke.rs:194` — the `_public` revocation route holds the SAME `Arc<dyn CasBlobEraser>` seam (second consumer beyond DSR erase).
- `crates/corelink-container/src/routes/public_revoke.rs:648` — `build_state_from_env` mounts the revoke route fail-CLOSED, reusing the erase seam + `CORELINK_ERASE_AUTH_KEY`.
- `crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs:698` — `impl CasWriteHandler for AccountingCasHandler` (accounting decorator).
- `crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs:722` — `_public` unowned override forces `quota_seed = Some(0)` (F3.2 B4 shared-meter mode).
- `crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs:759` — reserves bytes (`block_on_accrue`) before `write_inner.write(req)`.
- `crates/corelink-container/src/byte_accounting/b126_m2_impl_01.rs:804` — `impl CasDeleteHandler for AccountingCasHandler`.
- `crates/corelink-container/src/routes/build.rs:97` — `cas::build_handlers()` produces the raw `Arc<dyn>` tuple.
- `crates/corelink-container/src/routes/build.rs:117` — wraps write+delete in `AccountingCasHandler` at the chokepoint.
- `crates/corelink-container/src/routes/build.rs:167` — wraps read/write/delete in `TombstoneGatedCasHandler` at the same chokepoint.
- `crates/corelink-container/src/routes/build.rs:232` — clones the shared CAS `Arc`s into the cargo/brew/Bazel surfaces.
- `crates/corelink-container/src/routes/build.rs:380` — `.merge(admin_tenant_detail::router(...))` mounts the operator per-tenant read module beside `admin::router`.
0. `crates/corelink-container/src/routes/cas/foundation.rs:4` — executable CAS module includes.
1. `crates/corelink-container/src/customer_d1_seams.rs:32` — executable customer split modules.
1. `crates/corelink-container/src/customer_d1_handler_state.rs:75-82` — D1 handler state constructor.
1. `crates/corelink-container/src/customer_d1_team_audit.rs:18-25` — team/audit handler logic.
1. `crates/corelink-container/src/customer_d1_seams.rs:56-63` — D1 customer transport implementation.
1. `crates/corelink-container/src/customer_d1_byok_config.rs:184-189` — active BYOK configuration predicate.
1. `crates/corelink-container/src/customer_d1_byok_writer.rs:90-98` — BYOK activation validation.
1. `crates/corelink-container/src/customer_d1_maps_calendar.rs:27-34` — billing-status mapping used by the customer response model.
