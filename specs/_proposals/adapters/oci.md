# Adapter Contract — OCI Registry (Docker, containerd, podman, buildah)

**Wave:** 34 · **Crate target:** `corelink-adapter-oci` · **Owner:** Gustavo Schneiter · **Authored:** 2026-05-26

## 1. Headline + scope

CoreLink becomes a **full OCI Distribution Spec v1.1 registry**. `docker push` / `docker pull`, `podman`, `buildah`, `containerd`, Kubernetes image pulls, BuildKit cache, all consume CoreLink as a standard OCI registry endpoint. Manifests (JSON) cached in KV; blobs (layers + configs) stored in CAS.

This is the most complex of the 5 adapters: bi-directional (push AND pull), multi-step uploads, manifest mutation, scoped auth flow.

**In scope:** OCI Distribution Spec v1.1 endpoints (~10 endpoints). Push + pull. Manifest list / index support. Token-based auth (bearer realm). Per-tenant repository namespace.
**Out of scope:** OCI Image Spec validation beyond manifest schema, image signing (cosign/notation — separate hardening), garbage collection at the registry level (delegated to `corelink-gc`), cross-region replication of cached blobs (delegated to `corelink-replication`), helm chart OCI artifacts (work by accident; not explicitly tested).

## 2. Upstream protocol summary

OCI Distribution Spec v1.1: https://github.com/opencontainers/distribution-spec/blob/main/spec.md

Canonical endpoints (the agent must implement ALL of these):

| Endpoint | Op | Notes |
|---|---|---|
| `GET /v2/` | API version check | 200 if registry alive |
| `GET /v2/<name>/blobs/<digest>` | Pull blob | 200 + bytes / 404 |
| `HEAD /v2/<name>/blobs/<digest>` | Check blob existence | 200 / 404 |
| `POST /v2/<name>/blobs/uploads/` | Start blob upload | 202 + `Location:` header |
| `PATCH /v2/<name>/blobs/uploads/<uuid>` | Stream chunk | 202 + `Range:` header |
| `PUT /v2/<name>/blobs/uploads/<uuid>?digest=<digest>` | Finalize blob | 201 (created) + verify digest |
| `GET /v2/<name>/manifests/<reference>` | Pull manifest | 200 + JSON; reference can be tag OR digest |
| `HEAD /v2/<name>/manifests/<reference>` | Check manifest existence | 200 / 404 |
| `PUT /v2/<name>/manifests/<reference>` | Push manifest | 201 + `Docker-Content-Digest:` header |
| `DELETE /v2/<name>/manifests/<reference>` | Delete manifest (optional; off at v1 — defer to admin plane) | 405 Method Not Allowed |
| `GET /v2/<name>/tags/list` | List tags | 200 + JSON |
| `GET /v2/_catalog` | List repositories (optional; cross-tenant leak risk — DISABLED at v1) | 401 |

Auth flow: client → `GET /v2/` → if 401, registry returns `Www-Authenticate: Bearer realm="...",service="...",scope="..."` → client fetches token from realm → retries with `Authorization: Bearer <token>`. We implement the realm endpoint within the same adapter for simplicity.

## 3. Mapping to CoreLink

| OCI op | CoreLink call |
|---|---|
| `GET /v2/<name>/blobs/<digest>` | `CasStore::get(tenant, Digest::from_oci_digest(digest))` |
| `HEAD /v2/<name>/blobs/<digest>` | `CasStore::exists(tenant, Digest::from_oci_digest(digest))` |
| `POST /v2/<name>/blobs/uploads/` | allocate upload session (in-memory or `corelink-cas` multipart-init); return UUID |
| `PATCH /v2/<name>/blobs/uploads/<uuid>` | append chunk to multipart upload (uses `corelink-cas::r2_storage::multipart`) |
| `PUT /v2/<name>/blobs/uploads/<uuid>?digest=X` | finalize multipart → CasStore::put with declared `Digest`; verify chunks SHA matches header `X` → reject + audit if mismatch |
| `GET /v2/<name>/manifests/<reference>` | `metadata_kv.get(tenant, "oci_manifest:<name>:<reference>")` |
| `PUT /v2/<name>/manifests/<reference>` | parse JSON → validate schema → compute manifest digest → `metadata_kv.put(...)` + audit emit `oci.manifest.push` |
| `GET /v2/<name>/tags/list` | `metadata_kv.get(tenant, "oci_tags:<name>")` |

OCI digest format is `sha256:<hex>` (or `sha512:<hex>`, etc.). CoreLink `Digest` is BLAKE3 only. **Bridge:** the adapter stores a `(tenant, oci_digest) → CasStore key` mapping in KV. The CAS layer doesn't know about SHA256; the adapter does.

## 4. Crate structure

```
crates/corelink-adapter-oci/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # ~50 LOC re-exports
│   ├── server.rs               # axum router; v2 endpoint dispatch (~280 LOC)
│   ├── pull/
│   │   ├── mod.rs              # ~50 LOC
│   │   ├── blob.rs             # GET/HEAD blob handlers (~150 LOC)
│   │   └── manifest.rs         # GET/HEAD manifest handlers (~150 LOC)
│   ├── push/
│   │   ├── mod.rs              # ~50 LOC
│   │   ├── upload.rs           # POST/PATCH/PUT blob upload (~220 LOC; multipart state machine)
│   │   └── manifest.rs         # PUT manifest (validation + storage) (~180 LOC)
│   ├── tags.rs                 # GET /tags/list (~80 LOC)
│   ├── auth.rs                 # bearer realm endpoint + PAT → tenant (~180 LOC)
│   ├── digest.rs               # OCI digest (sha256:<hex>) ↔ CAS Digest bridge (~110 LOC)
│   ├── audit.rs                # audit emit (~100 LOC)
│   ├── config.rs               # config (HUGR_OCI_ADAPTER_*) (~90 LOC)
│   ├── error.rs                # ~80 LOC
│   └── tests/
│       ├── smoke_pull.rs       # `crane pull` against adapter end-to-end (~180 LOC)
│       ├── smoke_push.rs       # `crane push` against adapter end-to-end (~200 LOC)
│       ├── prop_digest.rs      # OCI ↔ CAS digest bridge property (~100 LOC)
│       ├── prop_manifest.rs    # manifest schema validation (~120 LOC)
│       └── adversarial.rs      # forged auth, oversize, digest mismatch, _catalog leak (~180 LOC)
```

Estimated: ~2470 LOC across 18 files. Most complex adapter. Largest expected: `server.rs` (280), `upload.rs` (220), `manifest.rs (push)` (180). All ≤500 per L2.10.

### Dep graph

```
corelink-adapter-oci
  ├── corelink-cas              (blob storage; multipart upload)
  ├── corelink-auth             (PAT → tenant)
  ├── corelink-audit            (fail-CLOSED emit)
  ├── corelink-core             (TenantId, Digest, SecretWrap)
  ├── corelink-telemetry        (tracing)
  └── corelink-adapters-cloud   (KV for manifests + tags; reqwest unused here — no upstream proxy)
```

Note: OCI adapter is NOT a caching proxy in front of Docker Hub. It IS a registry. Customers push to it; pull from it. No upstream call.

## 5. Trait interface

```rust
#[non_exhaustive]
pub struct OciAdapterConfig {
    pub bind_addr: std::net::SocketAddr,
    pub bearer_realm: url::Url,            // points back to this adapter's /token endpoint by default
    pub blob_size_limit_bytes: u64,        // default 5 GiB (container layers can be huge)
    pub multipart_chunk_size_bytes: u64,   // default 16 MiB
    pub enable_catalog: bool,              // MUST default false (cross-tenant leak risk)
    pub cas: std::sync::Arc<dyn corelink_cas::CasStore>,
    pub metadata_kv: std::sync::Arc<dyn corelink_adapters_cloud::cf::kv::KvStore>,
    pub tenant_resolver: std::sync::Arc<dyn corelink_auth::TenantResolver>,
    pub auditor: std::sync::Arc<dyn corelink_audit::ports::AuditEmitter>,
}

pub async fn run_oci_adapter(config: OciAdapterConfig) -> Result<(), OciAdapterError>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OciAdapterError {
    #[error("bind: {0}")]
    Bind(std::io::Error),
    #[error("auth: {0}")]
    Auth(String),
    #[error("cas: {0}")]
    Cas(String),
    #[error("kv: {0}")]
    Kv(String),
    #[error("digest mismatch: declared {declared}, computed {computed}")]
    DigestMismatch { declared: String, computed: String },
    #[error("blob exceeds limit: {0} bytes")]
    BlobOversized(u64),
    #[error("manifest schema invalid: {0}")]
    ManifestInvalid(String),
    #[error("upload session not found: {0}")]
    UploadSessionMissing(String),
    #[error("audit: {0}")]
    Audit(String),
}
```

## 6. Auth + multi-tenancy

OCI uses scoped bearer tokens. Flow:

1. Client `GET /v2/` → adapter returns `401` + `Www-Authenticate: Bearer realm="<adapter>/token",service="corelink-oci",scope="repository:<name>:pull"`.
2. Client `GET <realm>?service=...&scope=...` with `Authorization: Basic <base64(user:pat)>` (where user = `hugr` and pat = `hugr-pat_xxx`).
3. Adapter validates PAT, returns short-lived (1h) opaque token with embedded tenant + scope.
4. Client retries original op with `Authorization: Bearer <token>`.

Token format: `corelink-pat_<tenant>_<scope>_<expiry>_<hmac(...)>`. HMAC keyed on `HUGR_OCI_TOKEN_KEY` env (separate from PAT-signing key). Constant-time compare on verify (`subtle::ConstantTimeEq`).

PAT scopes: `oci:repository:<name>:pull`, `oci:repository:<name>:push`. Granular per repo within a tenant.

**Cross-tenant isolation:** every blob key in CAS is prefixed with `tenant_path`; every manifest in KV is keyed `oci:<tenant>:<repo>:<reference>`. Catalog (`GET /v2/_catalog`) DISABLED by default (would otherwise enumerate cross-tenant repos).

## 7. Cache invalidation

| Layer | Invalidation |
|---|---|
| **Blobs (CAS)** | Never invalidated. OCI blobs are immutable (digest = content). Aged out via `corelink-gc`. |
| **Manifests (KV)** | Mutable: `PUT /v2/<name>/manifests/<tag>` overwrites for `<tag>` references (not for digest references). Audit emit on every overwrite. |
| **Tags (KV)** | Listed at `GET /v2/<name>/tags/list`; updated on each manifest PUT. |

Manifest delete (`DELETE /v2/<name>/manifests/<reference>`) returns `405 Method Not Allowed` at v1. Operator-driven manifest removal goes through admin plane only.

## 8. Tests

| Tier | Tests | Acceptance |
|---|---|---|
| **Smoke pull** | `crane pull localhost:<port>/corelink-test/hello:latest /tmp/img.tar` against pre-seeded blobs + manifest in CoreLink. | Exit 0; image bytes match seed |
| **Smoke push** | `crane push /tmp/some-image.tar localhost:<port>/corelink-test/newimg:v1` with auth. Then `crane pull` retrieves it back. | Both succeed; manifest digest matches |
| **Property — digest bridge** | Roundtrip: `oci_digest_to_cas(cas_to_oci_digest(d)) == d` for all 32-byte BLAKE3 inputs. Likewise for SHA256 inputs after bridge. | 1k cases / 10k nightly |
| **Property — manifest schema** | Parse-roundtrip valid manifests of types: `application/vnd.oci.image.manifest.v1+json`, `application/vnd.docker.distribution.manifest.v2+json`, `application/vnd.oci.image.index.v1+json`. | All schemas valid |
| **Adversarial** | (1) Push declared `digest=sha256:X` but bytes hash to `sha256:Y` → reject + audit `oci.push.digest_mismatch`. (2) Oversize blob (>5 GiB) → 413 + audit. (3) Forged bearer token (HMAC fails) → 401 + audit. (4) Tenant A blob pull via tenant B's repo name → 404 (not 403; don't leak existence). (5) `_catalog` request → 401 (disabled). (6) Half-uploaded blob abandoned for >1h → cleaned up; subsequent finalize on same UUID → 404. (7) Tag override race: two concurrent PUT manifests on `<repo>:latest` → atomic via KV CAS; last writer wins; both audited. | All 7 pass |

Test harness: `crane` (Google's CLI for OCI registry interaction; vendored as test dep). `wiremock` NOT needed (no upstream proxy).

## 9. Acceptance criteria

- [ ] `cargo build -p corelink-adapter-oci`: green
- [ ] `cargo test -p corelink-adapter-oci`: ≥18 tests passing (smoke + property + adversarial per §8)
- [ ] `cargo clippy -p corelink-adapter-oci --all-targets -- -D warnings`: clean
- [ ] wasm32 workspace stays green
- [ ] `#[non_exhaustive]` on every public type
- [ ] `#![forbid(unsafe_code)]`
- [ ] Zero `unwrap/expect/panic` in `src/`
- [ ] **Declared-digest verification MANDATORY** on every `PUT /blobs/uploads/<uuid>?digest=X` — bytes' actual hash MUST equal X, else reject + audit fail-CLOSED
- [ ] Audit emit before every state-mutation (blob put, manifest put, tag update)
- [ ] PAT + bearer token wrapped in `SecretString`; constant-time compare on verify
- [ ] Catalog endpoint DISABLED by default (`enable_catalog = false`)
- [ ] L2.10: no file >500 LOC; sweet-spot ≤200; largest expected ~280 (server.rs dispatch)
- [ ] SEAL audit `specs/_audits/2026-05-2X-w34-adapter-oci.md`
- [ ] Smoke tests §8 rows 1-2 (pull + push roundtrip) pass with real `crane` binary
- [ ] DCO + Co-Authored-By

## 10. Out of scope / explicit deferrals

- **Image signing (cosign / notation).** Separate hardening initiative. Sigs travel as separate OCI artifacts; we'd store them as opaque blobs. v2.
- **Helm chart OCI artifacts.** May work by accident (Helm uses the same `PUT /v2/.../manifests/...` flow). Not explicitly tested at v1.
- **`_catalog` cross-repo listing.** Permanently DISABLED unless customer enables per-tenant; cross-tenant `_catalog` would leak existence info. Future per-tenant config.
- **DELETE methods.** No `DELETE /v2/.../manifests/...` and no `DELETE /v2/.../blobs/...` at v1. Out-of-band admin plane only.
- **Cross-region replication of cached blobs.** Delegated to `corelink-replication` per existing multi-region design.
- **Garbage collection of orphan blobs** (manifests deleted but blobs still referenced). Delegated to `corelink-gc`.
- **OCI Image Spec validation** (config + layer order rules). We only validate manifest JSON schema; we don't enforce image-spec-level semantics (would require running the image).
- **Anonymous reads / public registry.** v1 requires PAT. Public-namespace registry is post-GA, tied to cross-tenant dedup.
- **Cross-tenant dedup of identical layers.** Per-tenant only. The "Alpine base image shared across all customers" optimization is post-GA.

---

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of OCI Adapter Contract.**
