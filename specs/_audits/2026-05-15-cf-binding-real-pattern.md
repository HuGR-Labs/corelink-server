---
id: "AUDIT-2026-05-15-CF-BINDING-REAL-PATTERN"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.0.0"
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

## 6. Verification

- `CfR2BucketReal` wraps `worker::r2::Bucket` with: head / get / put / delete / list + multipart create/upload_part/complete/abort.
- Tenant prefix enforced (`TenantPrefix::new` + `scoped_key`).
- Native stub returns `R2Error::Backend("WasmOnly: …")`.
- 18 unit tests in `r2_real::tests` (well over the ≥ 6 minimum).
- wasm32-unknown-unknown build green.
- Native build green.
- clippy `-D warnings` green on `--tests`.

## 7. Out of scope

- Full `MultipartAdapter` trait implementation for the multipart surface — `corelink-r2-multipart` owns that contract; `CfR2BucketReal::create_multipart_upload` / `upload_part` / `complete_multipart_upload` / `abort_multipart_upload` are the verbs that adapter will wire onto.
- Cross-tenant key collision detection beyond the prefix check — the canonical tenant prefix is a 16-hex HMAC produced by `corelink_tenant_path::TenantPrefix` (collision probability ≈ 2^-96 per pair); this crate trusts the upstream derivation.
- Migration of legacy `cf_r2.rs` / `cf_d1.rs` / `cf_kv.rs` / `cf_do.rs` stubs — those remain as the minimal `R2Backend` / etc. adapters for callers that don't need the extended surface. The two coexist.
