---
id: "AUDIT-2026-05-26-W35-ADAPTER-HOST-PREP"
type: "audit"
doc_status: "ACTIVE"
audit_status: "CLOSED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
closed: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-35", "adapter-host", "consolidation"]
references:
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-26-w34-adapter-cargo-v2.md"
  - "specs/_audits/2026-05-26-w34-adapter-npm-v2.md"
  - "specs/_audits/2026-05-26-w34-adapter-pip.md"
  - "specs/_audits/2026-05-26-w34-adapter-brew.md"
  - "specs/_audits/2026-05-26-w34-adapter-oci.md"
---

# Wave 35 — `corelink-adapter-host` SEAL Audit

## Summary

New crate `crates/corelink-adapter-host/` bridges the five Wave-34
adapter-local port traits onto the canonical workspace Stage-1 SPI traits.
Closes follow-up #3 from `specs/_audits/2026-05-26-wave-33-34-closure-followups.md §5`.

## Baseline

- **HEAD at SEAL**: `58b4f892a3c7cf5a0de404d53178655560a61c36` (worktree entry)
- **Branch**: `worktree-agent-acec4c2ecfa788af5`

## Scope

### New files

| File | LOC | Bridges implemented |
|---|---|---|
| `crates/corelink-adapter-host/src/lib.rs` | 39 | module re-exports + crate-level docs |
| `crates/corelink-adapter-host/src/cargo.rs` | 313 | `CargoCasBridge`, `CargoTenantBridge` |
| `crates/corelink-adapter-host/src/brew.rs` | 281 | `BrewCasBridge`, `BrewTenantBridge` |
| `crates/corelink-adapter-host/src/npm.rs` | 433 | `NpmCasBridge`, `NpmKvBridge<K>`, `NpmTenantBridge` |
| `crates/corelink-adapter-host/src/pip.rs` | 410 | `PipCasBridge`, `PipKvBridge<K>`, `PipTenantBridge` |
| `crates/corelink-adapter-host/src/oci.rs` | 498 | `OciBlobBridge`, `OciManifestKvBridge<K>`, `OciTenantBridge` |
| **Total** | **1974** | 5 adapters × up to 3 bridges each |

### Workspace changes (additive only)

- `Cargo.toml` `[workspace] members`: added `"crates/corelink-adapter-host"`.
- `Cargo.toml` `[workspace.dependencies]`: added `corelink-adapter-host = { path = "crates/corelink-adapter-host" }`.

## Architecture decisions

### KvBackend RPITIT → not object-safe

`corelink_worker::cache::kv::KvBackend` uses RPITIT (`impl Future`) and
is therefore not object-safe via `dyn`. The `NpmKvBridge<K>` and
`PipKvBridge<K>` bridges are generic over `K: KvBackend + Send + Sync +
fmt::Debug + 'static` rather than `Arc<dyn KvBackend>`. This is the only
correct approach; the trait surface cannot be changed without breaking all
existing `KvBackend` impls.

### CasReadRequest.principal is String

The `principal` field on `CasReadRequest`/`CasWriteRequest` is a plain
`String`. The bridge injects a service-account identity (`"svc-{adapter}-host"`)
at construction time. No hard-pause trigger #1 activation: the field type is
fully compatible with bridge-time synthesis.

### sync↔async via spawn_blocking

`CasReadHandler::read` and `CasWriteHandler::write` are sync traits; adapter
port traits are async. The bridge calls sync handlers via
`tokio::task::spawn_blocking` inside the async adapter port impls. No tokio
runtime issues observed; `spawn_blocking` is the canonical pattern for
offloading sync work from async contexts.

### OCI BlobStore chunked upload buffer

`BlobStore` has a chunked upload protocol (`open_upload → append_chunk → finalize_upload`).
The bridge maintains an in-process `Arc<Mutex<HashMap<String, Vec<u8>>>>` upload
session buffer. This is NOT durable across restarts; production deployment
should replace with a Durable Object or R2 multipart upload. Documented
in crate-level `caveats` section.

### KV timestamp encoding (npm + pip)

`KvStore::get` returns `(bytes, inserted_at_unix_ms)` but `KvBackend`
stores only raw bytes. The bridge encodes `inserted_at_unix_ms` as an
8-byte big-endian prefix. The decode path uses `.get(..8)` / `.get(8..)`
to satisfy the `clippy::indexing_slicing = "deny"` lint.

### OCI ManifestKvStore::list_prefix limitation

`KvBackend` has no prefix-list primitive. `OciManifestKvBridge::list_prefix`
always returns an empty `Vec`. This is a known bridge limitation documented
in the module docstring. Tag-list responses will be empty until the bridge
is replaced with a CF Workers KV binding.

## Acceptance criteria verification

| Criterion | Status | Evidence |
|---|---|---|
| `cargo build -p corelink-adapter-host` GREEN | PASS | output: `Finished dev profile` |
| `cargo test -p corelink-adapter-host` ≥ 25 tests | PASS | 44 tests, 0 failures |
| `cargo clippy -p corelink-adapter-host --all-targets -- -D warnings` clean | PASS | `Finished dev profile [0 errors, 0 warnings]` |
| `cargo build --workspace` GREEN | PASS | `Finished dev profile` |
| `#[non_exhaustive]` on every pub struct | PASS | verified per-file |
| `#![forbid(unsafe_code)]` at crate root | PASS | `lib.rs:32` |
| Zero `unwrap/expect/panic` in `src/` | PASS | deny lints enforced; only `#[cfg(test)]` allows |
| L2.10: every `.rs` ≤ 500 LOC | PASS | max is oci.rs @ 498 |

## Test count breakdown (44 total)

| Module | Tests |
|---|---|
| `cargo` | 8 (5 CAS + 2 tenant + 1 debug) |
| `brew` | 7 (4 CAS + 2 tenant + 1 debug) |
| `npm` | 9 (3 CAS + 3 KV + 2 tenant + 1 debug) |
| `pip` | 9 (3 CAS + 3 KV + 2 tenant + 1 debug) |
| `oci` | 11 (5 blob + 3 manifest-KV + 2 tenant + 1 debug) |
| **Total** | **44** |

## DCO + sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.

**End of Wave 35 adapter-host prep audit.**
