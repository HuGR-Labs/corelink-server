---
id: "AUDIT-2026-05-15-CF-BINDING-REAL-PATTERN"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.2.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "cf-bindings", "r-prep", "r2", "d1", "kv", "durable-objects", "tenant-isolation", "pattern"]
---

# CF Binding Real-Impl Pattern — Replication Recipe for D1 / KV / DO

> **Audit Date:** 2026-05-15 · **Branch:** `wt/r-prep-cf-r2-real` · **Lane:** R-PREP (release-prep)
> **Reviewer:** Gustavo Schneiter

## 1. Context

`corelink-cf-bindings` historically shipped trait-stubs (`InMemoryFake` impls) for the four CF runtime surfaces — R2, D1, KV, and Durable Objects. R-PREP-CF-R2-REAL lands the first real binding (`CfR2BucketReal`) wrapping `worker::r2::Bucket`. This doc captures the template the D1 / KV / DO follow-ups MUST replicate so the four binding adapters share a single audit-fenced, tenant-prefix-enforced shape.

## 2. Template

### 2.1 Crate-level layout

```
crates/corelink-cf-bindings/src/
├── lib.rs                # per-module cfg-gates; no root cfg
├── r2_real.rs            # CfR2BucketReal (THIS PR)
├── d1_real.rs            # CfD1DatabaseReal  (next follow-up)
├── kv_real.rs            # CfKvNamespaceReal (next follow-up)
└── do_real.rs            # CfDurableObjectReal (next follow-up)
```

Each `*_real.rs` module is **dual-target**:

- `target_arch = "wasm32"` → wraps the real `worker::*` type.
- `target_arch != "wasm32"` → stub returning `*Error::Backend("WasmOnly: <op>")`.

The lib root carries `#![forbid(unsafe_code)]` but **does not** carry `#![cfg(target_arch = "wasm32")]`. Each `worker::*`-dependent module gates itself.

### 2.2 Typed scoped-key wrapper

Each module defines a `*ScopedKey` newtype constructible only via the bucket/namespace/object's `scoped_*` factory. The factory:

1. Rejects empty keys.
2. Rejects keys containing NUL bytes.
3. Accepts keys that already start with `<tenant_prefix>/` verbatim.
4. Prepends `<tenant_prefix>/` to tenant-local tails.
5. Rejects leading slashes or `//` segments (path-traversal defense).

The wrapper is `Clone + Debug` but **MUST NOT** be logged outside the audit hook (CTRL-PRIV-001).

### 2.3 Audit hook (fail-CLOSED)

```rust
pub type AuditFn =
    Arc<dyn Fn(R2Op, &str) -> Result<(), R2Error> + Send + Sync + 'static>;
```

- Called BEFORE every mutation (`put`, `delete`, multipart `complete` / `abort`).
- If the closure returns `Err`, the mutation is **not** performed.
- READ operations (`head`, `get`, `list`) also audit — production needs the probe trail for incident response.
- Default is a no-op (`Ok(())`); production boot replaces via `with_audit`.

### 2.4 R2Backend trait impl

The new wrapper STILL implements the canonical `corelink_worker::storage::r2::R2Backend` trait (or the analogous trait for D1/KV/DO). This preserves drop-in compatibility with `R2Writer` / `R2Reader` / `ScopedR2Writer` (and equivalents).

### 2.5 Extended operation surface

Beyond the trait, the wrapper exposes the broader R2 (resp. D1/KV/DO) toolkit:

| R2 op                       | Method                          |
|-----------------------------|---------------------------------|
| `HEAD key`                  | `head(&self, key)`              |
| `GET key`                   | `get_bytes(&self, key)`         |
| `PUT key` (idempotent)      | `put_if_absent(&self, key, body)` |
| `DELETE key`                | `delete(&self, key)`            |
| `LIST <prefix>`             | `list_keys(&self, limit)`       |
| `createMultipartUpload`     | `create_multipart_upload(...)`  |
| `uploadPart`                | `upload_part(...)`              |
| `completeMultipartUpload`   | `complete_multipart_upload(...)`|
| `abortMultipartUpload`      | `abort_multipart_upload(...)`   |

D1 follow-up will surface `prepare / bind / first / all / exec / batch` plus transactional `with_savepoint`. KV will surface `get / put / delete / list / list_with_metadata`. DO will surface `id_from_name / get_namespace / fetch`.

### 2.6 Native stub semantics

```rust
#[cfg(not(target_arch = "wasm32"))]
impl CfR2BucketReal {
    pub fn stub_for_native_tests(tenant: TenantPrefix) -> Self { … }

    pub async fn head(&self, key: &str) -> Result<bool, R2Error> {
        let _ = self.scoped_key(key)?;    // validation runs on native
        Err(R2Error::Backend(format!("WasmOnly: {}", R2Op::Head)))
    }
    // … same shape for get/put/delete/list/multipart-*
}
```

Validation runs on native, so wrapper-layer tests pin the contract on host CI before the wasm32 build. The diagnostic prefix `WasmOnly:` is **stable** — upstream code may match on it.

### 2.7 Charter (HARD requirements per module)

- `#![forbid(unsafe_code)]` at crate root.
- No `unwrap` / `expect` / `panic` outside `#[cfg(test)]`.
- Audit fail-CLOSED on every mutation.
- Tenant prefix enforced (typed wrapper + runtime check).
- Idempotent operations (CAS PUT with INM=* semantics; KV PUT with `If-None-Match` if/when workers-rs surfaces it; D1 `INSERT … ON CONFLICT DO NOTHING`; DO `id_from_name` is idempotent by construction).
- ≥ 6 unit tests for the wrapper layer:
  - 3 tenant-prefix validation tests (empty / slash / NUL).
  - 3 scoped-key derivation tests (tail-only / already-prefixed / leading-slash rejection).
  - ≥ 1 audit fail-CLOSED test.
  - ≥ 1 error-mapping test.

## 3. Feature flag at consumer level

Each consumer (`apps/server`, `corelink-clerk-cf`, …) adds an optional feature flag:

```toml
[features]
cf-r2-real = ["dep:corelink-cf-bindings"]
cf-d1-real = ["dep:corelink-cf-bindings"]   # follow-up
cf-kv-real = ["dep:corelink-cf-bindings"]   # follow-up
cf-do-real = ["dep:corelink-cf-bindings"]   # follow-up
```

The flag is a **build-config witness** for the production cutover. On native it links the stub; on wasm32 it links the real binding. The Worker boot path swaps `InMemory*` for `Cf*Real` at construction time when the flag is on.

## 4. Quality gate (per module)

```bash
cargo build -p corelink-cf-bindings --target wasm32-unknown-unknown
cargo build -p corelink-cf-bindings                 # native stub
cargo clippy -p corelink-cf-bindings --tests -- -D warnings
cargo test  -p corelink-cf-bindings
python3 scripts/validate_specs.py
```

All five MUST pass before merge.

## 4.1 Per-binding replication checklist

| Binding | Module                                              | Wraps                              | Audit-fenced | Tenant prefix | Native stub | Tests | Status   |
|---------|-----------------------------------------------------|------------------------------------|--------------|---------------|-------------|-------|----------|
| R2      | `crates/corelink-cf-bindings/src/r2_real.rs`        | `worker::r2::Bucket`               | yes          | `<tnt>/`      | yes         | 18    | DONE     |
| KV      | `crates/corelink-cf-bindings/src/kv_real.rs`        | `worker::kv::KvStore`              | yes          | `<tnt>:`      | yes         | 18+   | DONE     |
| D1      | `crates/corelink-cf-bindings/src/d1_real.rs`        | `worker::D1Database`               | yes          | per-row col   | yes         | TBD   | parallel |
| DO      | `crates/corelink-cf-bindings/src/do_real.rs`        | `worker::durable::ObjectNamespace` | yes          | name-derived  | yes         | TBD   | pending  |

KV separator is `:` (not `/`) — KV keys have no path-tree semantics; the `:` collation surfaces nicely in the Cloudflare dashboard prefix browser and avoids a parity-confusion with R2 paths.

## 5. Follow-ups

- **R-PREP-CF-D1-REAL** — Apply the template to D1 (`worker::D1Database`). Trait surface is owned by the D1-using crates (no canonical `D1Backend` trait yet — define one as part of the follow-up).
- **R-PREP-CF-KV-REAL** — `CfKvNamespaceReal` lands on branch `wt/r-prep-cf-kv-real` (`crates/corelink-cf-bindings/src/kv_real.rs`). Wraps `worker::kv::KvStore`; implements `corelink_worker::cache::kv::KvBackend`. Tenant prefix uses `:` separator (KV-idiomatic). Tenant-prefix equality uses `subtle::ConstantTimeEq` for the leading-segment probe. **DONE.**
- **R-PREP-CF-DO-REAL** — Apply to Durable Objects (`worker::durable::ObjectNamespace`). Counter-style stubs at `crates/corelink-cf-bindings/src/cf_do.rs` are the starting point.

Each follow-up should land in its own worktree (`wt/r-prep-cf-{d1,kv,do}-real`) and link back to this doc.

### 5.1 Per-binding replication checklist

Tracks which CF runtime surfaces have a real-impl binding shipped against the template above. Each row ticks once the full quality gate (native + wasm32 build + clippy + test + spec-validator) has been satisfied and the binding is wired into `crate::lib`.

| Binding | Module                                          | Wave   | wasm32 build | native stub | clippy `-D warnings` | tests           | Status      |
|---------|-------------------------------------------------|--------|--------------|-------------|----------------------|-----------------|-------------|
| R2      | `crates/corelink-cf-bindings/src/r2_real.rs`    | 13     | green        | green       | green                | 18              | **shipped** |
| D1      | `crates/corelink-cf-bindings/src/d1_real.rs`    | 14     | green        | green       | green                | 18 (+13 inline) | **shipped** |
| KV      | `crates/corelink-cf-bindings/src/kv_real.rs`    | 14     | green        | green       | green                | 23 (+16 inline) | **shipped** |
| DO      | `crates/corelink-cf-bindings/src/do_real.rs`    | 14     | green        | green       | green                | 25 (+5 inline)  | **shipped** |

**D1 row notes (wave 14):** `CfD1DatabaseReal` wraps `worker::D1Database` with 5 core ops (`prepare`, `bind`, `first`, `all`, `run`). Tenant-prefix enforcement is two-layered: (1) `TenantScopedQuery` rejects SQL missing `WHERE tenant_id = ?` (SELECT/UPDATE/DELETE) or missing `tenant_id` in the column list (INSERT); (2) bind-time `subtle::ConstantTimeEq` check forces the first positional parameter to match the anchored `TenantId`. Audit fail-CLOSED on `run` mutations (pre-emission before dispatch + post-emission when `D1ResultMeta::changes > 0`).

**KV row notes (wave 14):** `CfKvNamespaceReal` wraps `worker::kv::KvStore` with 6 ops (`get` / `get_bytes` / `put` (via `put_with_ttl`) / `put_bytes` / `list` / `delete`). Tenant prefix uses `:` separator (KV-idiomatic, distinct from R2's `/`); leading-prefix probe uses `subtle::ConstantTimeEq`. Audit fail-CLOSED on every mutation (`put`/`delete`/`list`).

**DO row notes (wave 14):** `CfDurableObjectReal` wraps `worker::ObjectNamespace` / `worker::Stub`. Tenant-scoped naming `tenant:<id>:<purpose>` via `TenantScopedName` newtype; constant-time tenant-id cmp; audit fail-CLOSED on every `stub.fetch_*` and `stub.get_*`; `FakeDoRouter` injection lets native tests exercise the full round-trip without wasm32 toolchain.

## 6. Verification

### 6.1 R2 (wave 13)

- `CfR2BucketReal` wraps `worker::r2::Bucket` with: head / get / put / delete / list + multipart create/upload_part/complete/abort.
- Tenant prefix enforced (`TenantPrefix::new` + `scoped_key`).
- Native stub returns `R2Error::Backend("WasmOnly: …")`.
- 18 unit tests in `r2_real::tests` (well over the ≥ 6 minimum).
- wasm32-unknown-unknown build green.
- Native build green.
- clippy `-D warnings` green on `--tests`.

### 6.2 D1 (wave 14)

- `CfD1DatabaseReal` wraps `worker::D1Database` with: `prepare` / `bind` / `first` / `all` / `run` (5 core ops).
- Tenant scope enforced (`TenantId::new` + `TenantScopedQuery` + bind-time `subtle::ConstantTimeEq` on the first positional parameter).
- Native stub returns `D1Error::Backend("WasmOnly: …")` after validation + audit pre-emission.
- 18 integration tests in `tests/d1_real.rs` (+ 13 inline tests in `d1_real::tests`).
- wasm32-unknown-unknown build green.
- Native build green.
- clippy `-D warnings` green on `--tests`.
- Charter compliance: `#![forbid(unsafe_code)]`, no `unwrap`/`expect`/`panic` outside test, `D1Error` and `D1Op` are `#[non_exhaustive]`, audit fail-CLOSED on every mutation, no tokio runtime in src (the wasm32 async surface uses `worker`'s native futures).

## 7. CF Worker production adoption

Wave 15 promotes the four real-binding wrappers from "shipped & tested in
isolation" (waves 13 + 14) to the canonical binding-access path for the
clerk-cf Cloudflare Worker. Every binding-access site in the request
handler chain now flows through a `Cf*Real` adapter; no raw
`worker::*` binding lookup remains outside the centralised boot path.

### 7.1 Wiring sites

The boot path lives in `crates/corelink-clerk-cf` and is split across
three modules so the adapter construction, audit fan-out, and handler
logic stay testable on native CI:

| Module                                                   | Role                                                                              |
|----------------------------------------------------------|-----------------------------------------------------------------------------------|
| `crates/corelink-clerk-cf/src/prod_wiring.rs`            | Boot — reads `env.bucket / env.d1 / env.kv / env.durable_object` once per request, wraps each in its `Cf*Real` adapter, attaches the shared `AuditSink`. |
| `crates/corelink-clerk-cf/src/audit_sink.rs`             | Audit fan-out — one `AuditSink` per request adapts to R2/D1/KV/DO `AuditFn` shapes. Emits one canonical NDJSON line per call via `worker::console_log!` on wasm32; in-memory recorder on native for tests. |
| `crates/corelink-clerk-cf/src/health.rs`                 | `GET /health` handler — exercises all four bindings (KV get/put, R2 head, D1 INSERT, DO stub_by_name) using the wrappers exclusively. |

The binding-name map registered in `crates/corelink-clerk-cf/wrangler.toml`:

| Binding name      | CF surface      | Adapter                  | Tenant anchor                      |
|-------------------|-----------------|--------------------------|------------------------------------|
| `CAS_BUCKET`      | R2 bucket       | `CfR2BucketReal`         | `TenantPrefix` (separator `/`)     |
| `CLERK_DB`        | D1 database     | `CfD1DatabaseReal`       | `TenantId` (CT-eq on first bind)   |
| `CLERK_JWKS_KV`   | KV namespace    | `CfKvNamespaceReal`      | `KvTenantPrefix` (separator `:`)   |
| `CLERK_DO`        | DO namespace    | `CfDurableObjectReal`    | `DoTenantPrefix` (`tenant:<id>:`)  |

The CF Worker entry point (`#[worker::event(fetch)]`) reads the
JWT-validated tenant from the `x-corelink-tenant` request header,
constructs a `TenantContext` (re-validating shape per binding anchor),
and threads the resulting `CfRealBindings` bundle to `handle_health_real`.

### 7.2 Audit sink wire-up

A single `AuditSink` is built at request entry (`AuditSink::console_ndjson(tenant_label)`) and adapted four times into the per-binding
`AuditFn` shape via:

- `sink.r2()`  → `Arc<dyn Fn(R2Op, &str) -> Result<(), R2Error> + Send + Sync>`
- `sink.d1()`  → `Arc<dyn Fn(D1Op, &str) -> Result<(), D1Error> + Send + Sync>`
- `sink.kv()`  → `Arc<dyn Fn(KvOp, &str) -> Result<(), KvError> + Send + Sync>`
- `sink.do_()` → `Arc<dyn Fn(DoOp, &str) -> Result<(), DoError> + Send + Sync>`

Each binding wrapper is then equipped via `.with_audit(...)`. The emitted
events fan into `worker::console_log!` on wasm32 (CF Logpush ingest);
the in-memory recorder variant is used for native `tests/prod_wiring.rs`.

Canonical NDJSON shape (one line per call):

```json
{"surface":"r2","op":"head","tenant":"<tenant-label>","subject":"<scoped-key>"}
```

CTRL-PRIV-001: the `subject` field is the validated scoped key / SQL
preview / DO name — never raw blob bytes, JWT secrets, or D1 row
payloads. The pre-mutation audit is fail-CLOSED on every binding: if
the sink errors (recorder mutex poisoned in tests; production console
emission is infallible), the binding op is NOT performed.

### 7.3 Quality gate (wave 15)

```bash
cargo build  -p corelink-clerk-cf --target wasm32-unknown-unknown    # green
cargo build  -p corelink-clerk-cf                                     # green
cargo clippy -p corelink-clerk-cf --target wasm32-unknown-unknown -- -D warnings   # green
cargo clippy -p corelink-clerk-cf --tests -- -D warnings              # green
cargo test   -p corelink-clerk-cf                                     # 14/14 (8 unit + 6 integration)
python3 scripts/validate_specs.py                                     # green
```

### 7.4 Cross-tenant defenses verified

`crates/corelink-clerk-cf/tests/prod_wiring.rs` pins the cross-tenant
defenses on native CI (the same wrapper code path executes on wasm32):

| Surface | Defense                                                       | Test                                              |
|---------|---------------------------------------------------------------|---------------------------------------------------|
| KV      | Silent re-namespacing under the anchored tenant prefix        | `cross_tenant_kv_isolation_silent_renamespace`    |
| R2      | Silent re-namespacing under the anchored tenant prefix        | `cross_tenant_r2_isolation_silent_renamespace`    |
| D1      | Hard `tenant_bind:` rejection (CT-eq on first positional)     | `cross_tenant_d1_bind_rejected_close`             |
| DO      | Hard `tenant_scope:` rejection (CT-eq on tenant-id segment)   | `cross_tenant_do_resolve_rejected_close`          |

KV and R2 use namespace isolation (any access from an A-anchored
adapter is silently re-prefixed under A); D1 and DO use hard CT-eq
rejection at the bind / name-validate gate. Both modes satisfy the
fail-CLOSED contract — under no path does an A-anchored adapter
return data from tenant B's namespace.

## 8. Out of scope

- Full `MultipartAdapter` trait implementation for the multipart surface — `corelink-r2-multipart` owns that contract; `CfR2BucketReal::create_multipart_upload` / `upload_part` / `complete_multipart_upload` / `abort_multipart_upload` are the verbs that adapter will wire onto.
- Cross-tenant key collision detection beyond the prefix check — the canonical tenant prefix is a 16-hex HMAC produced by `corelink_tenant_path::TenantPrefix` (collision probability ≈ 2^-96 per pair); this crate trusts the upstream derivation.
- Migration of legacy `cf_r2.rs` / `cf_d1.rs` / `cf_kv.rs` / `cf_do.rs` stubs — those remain as the minimal `R2Backend` / etc. adapters for callers that don't need the extended surface. The two coexist.
