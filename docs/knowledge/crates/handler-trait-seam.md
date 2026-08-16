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
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/routes/ac.rs"
  - "crates/corelink-container/src/routes/admin.rs"
  - "crates/corelink-container/src/routes/customer.rs"
  - "crates/corelink-container/src/routes/cas_erase.rs"
  - "crates/corelink-container/src/routes/public_revoke.rs"
  - "crates/corelink-container/src/customer_d1.rs"
  - "crates/corelink-container/src/byte_accounting.rs"
  - "crates/corelink-container/src/routes.rs"
checkpoint_sha: "14331ced6e8bbce7e71dffcfb881184d33551166"
provenance: "AUTHORED"
tags: ["handlers", "traits", "cas", "hot-path", "dependency-injection"]
timestamp: "2026-06-29T00:00:00Z"
---

# Handler-trait seam (CAS/AC/customer/erase/admin)

The container's HTTP route table never names a concrete storage type. Between an axum handler and the storage/business logic sits a thin layer of `Send + Sync + Debug` traits — `CasReadHandler` / `CasWriteHandler` / `CasDeleteHandler` / `CasListHandler` for CAS, the parallel `AcLookupHandler` / `AcUpdateHandler` / `AcDeleteHandler` / `AcListHandler` for the action cache, six `Customer*Handler` traits for the control plane, and `AdminReadHandler` / `AdminMutateHandler` for ops — each held by the route as an `Arc<dyn …Handler>`. The route delegates one verb to one trait object and maps the typed result to an HTTP status. This is the seam at which CoreLink swaps an in-memory fake (dev/CI/tests) for the real R2/D1-backed impl, and — more importantly — the single chokepoint at which cross-cutting concerns (byte accounting, GDPR erasure-gating, audit/SLI emission) are layered onto *every* cache surface at once by wrapping the shared trait objects, with no per-route code.

# Role

The seam exists for two reasons that the code makes concrete:

1. **Read/write are split into separate traits per surface, not one fat handler.** `CasReadHandler::read` and `CasWriteHandler::write` are distinct traits (`crates/corelink-handler-cas/src/handler.rs:34`, `:102`), as are `CasDeleteHandler::delete` (`:125`) and `CasListHandler::list` (`:145`). The split lets the route table compose read and write capability independently and lets a decorator wrap *only* the mutating half. The shared collaborator pair `(Arc<dyn AuditSink>, Arc<dyn SliObserver>)` is held by the concrete impl, so the cross-handler invariants (audit-fail-CLOSED ordering, SLI-emit-on-entry) are provable by composition rather than inheritance.

2. **One injection chokepoint fans the cross-cutting decorators out to every surface.** `routes.rs` builds the raw CAS handlers once (`crates/corelink-container/src/routes.rs:516`), wraps the *write + delete* trait objects in the byte-accounting decorator (`:536`), then wraps read/write/delete in the erasure-gate decorator (`:586`), and hands the resulting `Arc<dyn …>` to `CasRouteState`. Because native CAS, Bazel REAPI, OCI, and the cargo/brew/npm/pip language adapters all drive these *same* shared trait objects, wrapping at this one point gives all of them identical, atomic, fail-CLOSED accounting and a uniform erasure gate with zero per-surface duplication.

The `Arc<dyn>` (vs a generic `H: CasWriteHandler` type parameter) is deliberate: it lets two decorators (`AccountingCasHandler`, `TombstoneGatedCasHandler`) stack at runtime behind the same `CasRouteState` field type, and lets the *same* handler instance back several surfaces from one R2 connection.

# How it works

**The trait contract (CAS, canonical).** Each trait carries a documented emit-discipline its impls MUST honour: `read` audits `ReadAttempted` before the lookup and emits `Sli::AvailCasGet`/`LatencyCasGetP99` on every return path; `write` audits `WriteAttempted` *before* the mutation and aborts with `AuditFailed` without mutating if the audit emit fails, verifies the claimed hash before storing, and audits `WriteCommitted` only after the durable store. The in-process reference impl `InMemoryCasHandler` enforces exactly this ordering — its `write` emits `WriteAttempted` (`crates/corelink-handler-cas/src/handler.rs:400`) and stores only after (`:536`), and a `write_audit_failure_aborts_before_storing` test asserts storage stays empty when the audit sink is forced to fail. The `exists` HEAD-probe defaults to `read` (download + rehash) so test handlers keep working, but storage-backed impls MUST override it with a true HEAD — the default is correct but not cheap (`crates/corelink-handler-cas/src/handler.rs:74`), a Bazel `findMissingBlobs` egress amplification fix.

**The collaborators are themselves traits.** `AuditSink::emit` (`crates/corelink-handler-cas/src/audit.rs:122`) is fail-CLOSED (returns `Err` if the row cannot be durably written; the handler aborts on that error), and `SliObserver::observe` (`crates/corelink-handler-cas/src/observer.rs:47`) is infallible by design (an observer failure must not take the handler down). This is what makes the audit-before-mutation invariant a composable property of *any* impl, not a property hand-coded into each route.

**The live route consumes the trait, not the type.** `CasRouteState` declares `read: Arc<dyn CasReadHandler>`, `write: Arc<dyn CasWriteHandler>`, `delete`, `list` as four distinct trait-object fields (`crates/corelink-container/src/routes/cas.rs:141`). The handlers delegate one verb each: `handle_read` calls `state.read.read(req)` (`crates/corelink-container/src/routes/cas.rs:849`), `handle_write` calls `state.write.write(req)` (`:941`), `handle_delete` calls `state.delete.delete(req)` (`:1543`), `handle_list` calls `state.list.list(req)` (`:1598`). When R2 creds are present but the R2 handler refuses to build, the route mounts the fail-CLOSED `UnavailableCasHandler` whose every method returns a 503 sentinel (`crates/corelink-container/src/routes/cas.rs:440`) instead of silently degrading to the in-memory fake.

**Byte accounting wraps the write/delete trait objects.** `AccountingCasHandler` is itself an impl of `CasWriteHandler` (`crates/corelink-container/src/byte_accounting.rs:776`) and `CasDeleteHandler` (`:882`) that holds the inner `Arc<dyn CasWriteHandler>` + `Arc<dyn CasDeleteHandler>` and a `ByteAccountant`. Its `write` reserves bytes against `tenant_storage_state` *before* calling `self.write_inner.write(req)` (the `block_on_accrue` reservation at `:837`) — an over-cap or indeterminate reservation rejects so the durable PUT never runs — and releases the reservation if the inner write fails or stored nothing new (idempotent re-write). Its `delete` calls `self.delete_inner.delete(req)` (`:896`) then releases the reclaimed bytes. **F3.2 B4 — the `_public` shared-dedup namespace is UNOWNED:** it is not a billable tenant, so the decorator forces the genuine-unlimited shared-meter seed (`quota_seed = Some(0)`) for it (`crates/corelink-container/src/byte_accounting.rs:800`), which makes a moat write ALWAYS accrue-and-pass, SELF-SEED a missing per-region row (the `Some(0)` UPSERT INSERT branch), and never be capped by a stray finite `bytes_quota` — so the cross-tenant public write path cannot be outaged by the accounting layer, while `bytes_used` still tracks the shared cache's COGS. A non-`_public` tenant is charged byte-identically to before. Both hold a per-`(tenant, hash)` shard lock across the whole reserve→commit→release so a concurrent write-vs-delete of the same key cannot interleave their accounting (rt-nuclear C2). As of BYOK Wave 3b/3c the decorator no longer reserves the raw request length blindly: for a BYOK-`active` tenant it computes the COMMITTED (stored) size via `byok_committed_len`, which is now mode-aware — Mode A (convergent) reserves the plaintext length plus `BYOK_CLB1_OVERHEAD` (32 B: 4-byte magic + 12-byte nonce + 16-byte AEAD tag), and Mode B (random) reserves plaintext plus `BYOK_CLB2_OVERHEAD` (20 B: 4-byte magic + 16-byte tag, since the Mode-B nonce lives in the `byok_envelope` D1 row, not inline) — so `reserve == release` and `bytes_used` cannot drift once encryption is engaged; a non-BYOK / inactive tenant still reserves the plaintext length, byte-identical to before. The `AccountingAcHandler` is the exact mirror over `AcUpdateHandler`/`AcDeleteHandler`.

**Erasure-gating wraps read/write/delete.** `TombstoneGatedCasHandler` likewise impls `CasReadHandler` (`crates/corelink-container/src/routes/cas_erase.rs:986`) and `CasWriteHandler` (`:1018`), consulting a `TombstoneStore` trait (`:81`) so a tombstoned read 404s and a re-PUT of a tombstoned hash is refused — applied at the *same* chokepoint as accounting, so every surface inherits the GDPR gate. The read gate uses the (bloom-fast-pathed) `is_tombstoned`, but the WRITE gate uses the AUTHORITATIVE `TombstoneStore::is_tombstoned_authoritative` (`:99`) so a within-window cross-instance erase can never be resurrected by a re-PUT slipping through a stale local bloom (finding H3). Note the pure-logic crate `corelink-handler-cas-erase` defines *no* trait object: it owns I/O-free decision functions (`prepare_erase` at `crates/corelink-handler-cas-erase/src/handler.rs:145`, `read_gate` at `:164`) that the container's `cas_erase.rs` wiring composes over D1/R2 transports. It is the seam's logic kernel, not part of the `Arc<dyn>` chain. The `CasBlobEraser` half of that seam (the R2 byte-deleter, distinct from the tombstone gate) has a SECOND consumer beyond the per-tenant DSR erase: the F3.2 `_public` blob revocation route holds the same `Arc<dyn CasBlobEraser>` (`crates/corelink-container/src/routes/public_revoke.rs:177`) and drives it with the literal `_public` sentinel namespace to hard-delete a poisoned cross-tenant public blob, mounted fail-CLOSED from env alongside `cas_erase` under the shared `CORELINK_ERASE_AUTH_KEY` (`crates/corelink-container/src/routes/public_revoke.rs:492`) — reusing the frozen erase seam rather than duplicating the R2 delete.

**AC / admin / customer follow the same shape.** AC delegates `state.lookup.lookup(req)` / `update` / `delete` / `list` at `crates/corelink-container/src/routes/ac.rs:553`, `:636`, `:729`, `:776`. Admin delegates `state.read.read(req)` (`crates/corelink-container/src/routes/admin.rs:874`) and `state.mutate.mutate(req)` (`:752`) — the mutate contract additionally enforces dual-approval (reject `DualApprovalMissing`/self-approval) per the trait doc at `crates/corelink-handler-admin/src/handler.rs:220`. The admin surface's constant-time internal-auth gate `internal_auth_ok` is now `pub(crate)` (`crates/corelink-container/src/routes/admin.rs:86`) so the sibling operator per-tenant read module `admin_tenant_detail`, merged into the SAME router (`crates/corelink-container/src/routes.rs:793`), reuses the exact same gate rather than re-implementing the compare — one source of truth for the operator-auth boundary. The customer control plane is six narrow traits (`CustomerOverviewHandler` at `crates/corelink-handler-customer/src/handler.rs:43`, plus usage/billing/keys/team/audit) whose **live production impl** is `D1CustomerHandler` in `crates/corelink-container/src/customer_d1.rs` — it impls all six (`:947`, `:1011`, `:1086`, `:1203`, `:1428`, `:1646`) over an inner `Arc<dyn CustomerD1>` D1-over-HTTP transport. `build_handlers_from_env` puts the *same* `D1CustomerHandler` `Arc` behind all six route-state slots when D1 creds are present (`crates/corelink-container/src/routes/customer.rs:125`), falling back to `InMemoryCustomerHandler` in dev/CI; the route then delegates `state.overview.overview(req)` (`:506`), `state.usage.usage(req)` (`:562`), `state.billing.billing(req)` (`:665`), etc.

# Invariants

- **Audit-before-mutation, fail-CLOSED.** Every mutating impl emits the `*Attempted` audit row *before* the storage mutation and aborts with `AuditFailed` (no mutation) if the emit fails — pinned by the in-memory CAS `write` ordering at `crates/corelink-handler-cas/src/handler.rs:400`/`:440` and the fail-CLOSED `AuditSink::emit` contract at `crates/corelink-handler-cas/src/audit.rs:122`.
- **SLI emit on every return path.** `read`/`write`/`delete`/`list` emit their availability + latency SLI tuple on *every* exit including error paths (the trait doc at `crates/corelink-handler-cas/src/handler.rs:34`; the fake's `emit(true)` on the denial path).
- **Read and write are separable trait objects.** `CasReadHandler` and `CasWriteHandler` are distinct traits (`crates/corelink-handler-cas/src/handler.rs:34`, `:102`) so a decorator can wrap only the mutating half and a read-only build can omit the write half.
- **Accounting reserves before the durable write.** `AccountingCasHandler::write` reserves bytes before `write_inner.write` (`crates/corelink-container/src/byte_accounting.rs:837`); an over-cap reservation means the inner PUT never runs.
- **The `_public` shared namespace is unowned (never fail-closed, never capped).** `AccountingCasHandler::write` forces `quota_seed = Some(0)` when `req.tenant` is the `_public` moat namespace (`crates/corelink-container/src/byte_accounting.rs:800`), so a shared-cache write self-seeds its meter row and always accrues-and-passes — the accounting layer can never outage the cross-tenant public write path.
- **Cross-cutting decorators stack at one chokepoint.** `routes.rs` wraps the shared write/delete objects in accounting (`crates/corelink-container/src/routes.rs:536`) then read/write/delete in the erasure gate (`:586`) once, so all surfaces sharing those `Arc<dyn>` inherit both.
- **One shared handler instance backs many surfaces.** The same `cas_read`/`cas_write` `Arc`s are cloned into the cargo/brew/Bazel states (`crates/corelink-container/src/routes.rs:651`), so the decorators apply uniformly.

# Gotchas

- **`corelink-handler-cas-erase` defines no trait object.** Unlike the other four crates, it is a pure-logic kernel (`prepare_erase`, `read_gate`, `validate_digest`) — the `TombstoneStore` trait and the `TombstoneGatedCasHandler` `CasReadHandler`/`CasWriteHandler` impls live in the container's `cas_erase.rs` (`crates/corelink-container/src/routes/cas_erase.rs:81`, `:983`), not in the handler crate. Don't look for an `Arc<dyn EraseHandler>`; there isn't one.
- **The customer production impl lives in the container, not the handler crate.** `corelink-handler-customer` ships only the six traits + an in-memory fake; the live D1-backed `D1CustomerHandler` is in `crates/corelink-container/src/customer_d1.rs:947`. The handler crate is the *contract*, the container is the *impl* — a `customer_d1.rs:140 impl CustomerD1` is a *second* transport trait the handler delegates to, not the route-facing trait.
- **`exists` HEAD-probe must be overridden by storage-backed impls.** The default `CasReadHandler::exists` (`crates/corelink-handler-cas/src/handler.rs:74`) falls back to a full `read` (download + rehash). A storage adapter that forgets to override it turns Bazel `findMissingBlobs` into tens of GiB of egress per request — correct but ruinously expensive.
- **Byte accounting wraps write+delete only, not read/list.** `crates/corelink-container/src/routes.rs:531` binds only the write/delete decorator tuple, leaving read/list unwrapped (they are not write surfaces). If a future read path needs accounting, it does *not* get it for free from this decorator.
- **Decorator order matters: accounting is inner, erasure-gate is outer.** `routes.rs` wraps accounting first (`:536`) then the tombstone gate (`:586`) around the already-accounted handler, so a tombstoned re-PUT is refused by the outer gate *before* the inner accounting reserves bytes — reversing the order would reserve bytes for a write the erasure gate then rejects.
- **`AuditSink` is fail-CLOSED but `SliObserver` is fail-OPEN.** A failed audit emit aborts the operation; a failed SLI observe must not. Conflating them (making the observer fallible-and-aborting) would let a metrics hiccup take down the data plane.

# Citations

- `crates/corelink-handler-cas/src/handler.rs:34` — `pub trait CasReadHandler` definition (`read`).
- `crates/corelink-handler-cas/src/handler.rs:74` — default `exists` HEAD-probe falling back to `read`.
- `crates/corelink-handler-cas/src/handler.rs:102` — `pub trait CasWriteHandler` definition (`write`).
- `crates/corelink-handler-cas/src/handler.rs:125` — `pub trait CasDeleteHandler` definition (`delete`).
- `crates/corelink-handler-cas/src/handler.rs:145` — `pub trait CasListHandler` definition (`list`).
- `crates/corelink-handler-cas/src/handler.rs:400` — `InMemoryCasHandler::write` emits `WriteAttempted` before mutation.
- `crates/corelink-handler-cas/src/handler.rs:440` — durable store happens after the attempt audit.
- `crates/corelink-handler-cas/src/lib.rs:70` — public re-export of the CAS handler traits + in-memory fake.
- `crates/corelink-handler-cas/src/audit.rs:122` — `pub trait AuditSink::emit` (fail-CLOSED collaborator).
- `crates/corelink-handler-cas/src/observer.rs:47` — `pub trait SliObserver::observe` (infallible collaborator).
- `crates/corelink-handler-ac/src/handler.rs:323` — `pub trait AcLookupHandler`.
- `crates/corelink-handler-ac/src/handler.rs:341` — `pub trait AcUpdateHandler`.
- `crates/corelink-handler-ac/src/handler.rs:359` — `pub trait AcDeleteHandler`.
- `crates/corelink-handler-ac/src/handler.rs:376` — `pub trait AcListHandler`.
- `crates/corelink-handler-customer/src/handler.rs:43` — `pub trait CustomerOverviewHandler` (one of six control-plane traits).
- `crates/corelink-handler-customer/src/handler.rs:77` — `pub trait CustomerBillingHandler`.
- `crates/corelink-handler-admin/src/handler.rs:192` — `pub trait AdminReadHandler`.
- `crates/corelink-handler-admin/src/handler.rs:220` — `pub trait AdminMutateHandler` (dual-approval contract).
- `crates/corelink-handler-cas-erase/src/handler.rs:145` — `prepare_erase` pure decision fn (no trait object in this crate).
- `crates/corelink-handler-cas-erase/src/handler.rs:168` — `read_gate` pure decision fn.
- `crates/corelink-container/src/routes/cas.rs:141` — `CasRouteState` holds `Arc<dyn CasReadHandler>` etc. (the seam, route side).
- `crates/corelink-container/src/routes/cas.rs:440` — fail-CLOSED `UnavailableCasHandler` impl of the read trait.
- `crates/corelink-container/src/routes/cas.rs:849` — `handle_read` delegates `state.read.read(req)`.
- `crates/corelink-container/src/routes/cas.rs:1543` — `handle_delete` delegates `state.delete.delete(req)`.
- `crates/corelink-container/src/routes/cas.rs:1598` — `handle_list` delegates `state.list.list(req)`.
- `crates/corelink-container/src/routes/ac.rs:553` — `state.lookup.lookup(req)` delegation.
- `crates/corelink-container/src/routes/ac.rs:636` — `state.update.update(req)` delegation.
- `crates/corelink-container/src/routes/admin.rs:874` — `state.read.read(req)` delegation.
- `crates/corelink-container/src/routes/admin.rs:934` — `state.mutate.mutate(req)` delegation.
- `crates/corelink-container/src/routes/admin.rs:86` — `internal_auth_ok` is now `pub(crate)` (the shared constant-time operator-auth gate reused by `admin_tenant_detail`).
- `crates/corelink-container/src/routes/customer.rs:125` — `build_handlers_from_env` wires the same `D1CustomerHandler` Arc behind all six slots.
- `crates/corelink-container/src/routes/customer.rs:506` — `state.overview.overview(req)` delegation.
- `crates/corelink-container/src/routes/customer.rs:665` — `state.billing.billing(req)` delegation.
- `crates/corelink-container/src/customer_d1.rs:947` — `impl CustomerOverviewHandler for D1CustomerHandler` (live production impl).
- `crates/corelink-container/src/customer_d1.rs:1086` — `impl CustomerBillingHandler for D1CustomerHandler`.
- `crates/corelink-container/src/routes/cas_erase.rs:81` — `pub trait TombstoneStore` (container-side).
- `crates/corelink-container/src/routes/cas_erase.rs:986` — `impl CasReadHandler for TombstoneGatedCasHandler` (erasure-gate decorator).
- `crates/corelink-container/src/routes/cas_erase.rs:1018` — `impl CasWriteHandler for TombstoneGatedCasHandler`.
- `crates/corelink-container/src/routes/public_revoke.rs:177` — the `_public` revocation route holds the SAME `Arc<dyn CasBlobEraser>` seam (second consumer beyond DSR erase).
- `crates/corelink-container/src/routes/public_revoke.rs:492` — `build_state_from_env` mounts the revoke route fail-CLOSED, reusing the erase seam + `CORELINK_ERASE_AUTH_KEY`.
- `crates/corelink-container/src/byte_accounting.rs:776` — `impl CasWriteHandler for AccountingCasHandler` (accounting decorator).
- `crates/corelink-container/src/byte_accounting.rs:800` — `_public` unowned override forces `quota_seed = Some(0)` (F3.2 B4 shared-meter mode).
- `crates/corelink-container/src/byte_accounting.rs:837` — reserves bytes (`block_on_accrue`) before `write_inner.write(req)`.
- `crates/corelink-container/src/byte_accounting.rs:882` — `impl CasDeleteHandler for AccountingCasHandler`.
- `crates/corelink-container/src/routes.rs:516` — `cas::build_handlers()` produces the raw `Arc<dyn>` tuple.
- `crates/corelink-container/src/routes.rs:536` — wraps write+delete in `AccountingCasHandler` at the chokepoint.
- `crates/corelink-container/src/routes.rs:586` — wraps read/write/delete in `TombstoneGatedCasHandler` at the same chokepoint.
- `crates/corelink-container/src/routes.rs:651` — clones the shared CAS `Arc`s into the cargo/brew/Bazel surfaces.
- `crates/corelink-container/src/routes.rs:793` — `.merge(admin_tenant_detail::router(...))` mounts the operator per-tenant read module beside `admin::router`.
