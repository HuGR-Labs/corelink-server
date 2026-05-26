# Adapter Contract — pip (Python)

**Wave:** 34 · **Crate target:** `corelink-adapter-pip` · **Owner:** Gustavo Schneiter · **Authored:** 2026-05-26

## 1. Headline + scope

CoreLink becomes a **caching PyPI proxy** implementing PEP 503 (Simple Repository API) + PEP 691 (JSON Simple Index). `pip install` (and `uv` / `poetry` / `pdm` — all consume PEP 503 + 691) consults CoreLink as a transparent caching mirror in front of `pypi.org`. Wheels (`.whl`) and source dists (`.tar.gz`) stored in CoreLink CAS; index JSON cached in KV with TTL.

**In scope:** package index + wheel/sdist caching. Read path (`install`). Tenant-scoped namespaces. PEP 691 (JSON) preferred; PEP 503 (HTML) as compatibility fallback.
**Out of scope:** upload (`twine`), private package indexes, package signing verification (PEP 458), TUF.

## 2. Upstream protocol summary

PEP 691 (JSON Simple Index): https://peps.python.org/pep-0691/

- `GET /simple/<project>/` with `Accept: application/vnd.pypi.simple.v1+json` → JSON listing of all versions + files.
- `GET /<file_url_from_index>` → wheel or sdist tarball (binary).

PEP 503 (HTML — legacy/fallback): https://peps.python.org/pep-0503/

- `GET /simple/<project>/` returns HTML `<a href>` listing if JSON not negotiated.

Clients send no auth for public PyPI; for private indexes they use `--index-url https://user:pass@host/simple/`. We use `Authorization: Bearer hugr-pat_<token>` for tenant resolution.

Wheel/sdist URLs in the index carry an attached `#sha256=<hex>` fragment that the client verifies post-download.

## 3. Mapping to CoreLink

| pip wire | CoreLink call |
|---|---|
| `GET /simple/<project>/` (JSON) | `metadata_kv.get(tenant, "pkg:" + project)` → miss: upstream + put; hit: serve |
| `GET /simple/<project>/` (HTML, fallback) | Same as JSON but with HTML serialization step (re-encode from cached JSON) |
| `GET /<wheel_url>` | `CasStore::get(tenant, Digest::from_hex(sha256_from_url))`; miss: upstream + put + audit |

**Wheel/sdist storage trick:** PyPI URLs embed the SHA256 (`#sha256=...` fragment). That hex hash IS valid `Digest` after `Digest::from_hex(...)`. Zero translation overhead, like the cargo adapter — alignment by accident, but exploitable.

The `#sha256=` fragment serves as our pre-store integrity check: if upstream download's actual SHA256 doesn't match the fragment, the adapter REJECTS the store (audit emit `pip.wheel.integrity_mismatch`).

## 4. Crate structure

```
crates/corelink-adapter-pip/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # ~50 LOC re-exports
│   ├── server.rs               # axum router + index/wheel/sdist endpoints (~260 LOC)
│   ├── index.rs                # GET /simple/<project>/ JSON+HTML logic (~200 LOC)
│   ├── wheel.rs                # wheel/sdist CAS-backed cache (~150 LOC)
│   ├── upstream.rs             # reqwest client for pypi.org (~110 LOC)
│   ├── pep503_html.rs          # JSON ↔ PEP 503 HTML re-encoder (~120 LOC)
│   ├── auth.rs                 # PAT → tenant (~100 LOC)
│   ├── audit.rs                # audit emit (~80 LOC)
│   ├── config.rs               # config (HUGR_PIP_ADAPTER_*) (~80 LOC)
│   ├── error.rs                # ~60 LOC
│   └── tests/
│       ├── smoke.rs            # pip install run against adapter (~160 LOC)
│       ├── prop_index_parse.rs # PEP 691 ↔ 503 conversion property (~100 LOC)
│       └── adversarial.rs      # integrity mismatch, oversize, forged auth (~140 LOC)
```

Estimated: ~1280 LOC. Largest: `server.rs` (260). All ≤500 per L2.10.

### Dep graph

```
corelink-adapter-pip
  ├── corelink-cas
  ├── corelink-auth
  ├── corelink-audit
  ├── corelink-core
  ├── corelink-telemetry
  └── corelink-adapters-cloud   (KV for index; reqwest)
```

## 5. Trait interface

```rust
#[non_exhaustive]
pub struct PipAdapterConfig {
    pub bind_addr: std::net::SocketAddr,
    pub upstream_pypi: url::Url,           // default https://pypi.org
    pub index_ttl_seconds: u64,            // default 300
    pub wheel_size_limit_bytes: u64,       // default 1 GiB (largest scientific wheels)
    pub prefer_json_index: bool,           // default true (PEP 691)
    pub cas: std::sync::Arc<dyn corelink_cas::CasStore>,
    pub metadata_kv: std::sync::Arc<dyn corelink_adapters_cloud::cf::kv::KvStore>,
    pub tenant_resolver: std::sync::Arc<dyn corelink_auth::TenantResolver>,
    pub auditor: std::sync::Arc<dyn corelink_audit::ports::AuditEmitter>,
}

pub async fn run_pip_adapter(config: PipAdapterConfig) -> Result<(), PipAdapterError>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PipAdapterError {
    #[error("bind: {0}")]
    Bind(std::io::Error),
    #[error("auth: {0}")]
    Auth(String),
    #[error("cas: {0}")]
    Cas(String),
    #[error("upstream: {0}")]
    Upstream(String),
    #[error("integrity mismatch: expected sha256 {expected}, got {actual}")]
    IntegrityMismatch { expected: String, actual: String },
    #[error("wheel exceeds limit: {0} bytes")]
    WheelOversized(u64),
    #[error("audit: {0}")]
    Audit(String),
}
```

## 6. Auth + multi-tenancy

Same model as npm adapter. PAT → tenant; per-tenant CAS namespace; no anonymous reads at v1.

PAT scope: `pip:read`. Same scope-catalog pattern as npm/cargo.

`pip install --index-url http://<adapter>/simple/` with the PAT either as Bearer header (via pip's `--keyring-provider import` or env `PIP_INDEX_URL` with basic auth user:pass synth where user=`hugr` pass=`<pat>`). Adapter accepts both forms.

## 7. Cache invalidation

| Cache type | Invalidation |
|---|---|
| **Wheel/sdist (CAS)** | Never. Content-addressable by SHA256. PyPI enforces filename uniqueness; wheels are immutable. |
| **Index JSON (KV)** | TTL 300s default. After: refresh from upstream. |

Edge case: PyPI yanks (PEP 592). Yanked versions still exist but are marked. The index JSON includes yank status; cached index may show stale yank info (acceptable — pip warns user; doesn't block install).

## 8. Tests

| Tier | Tests | Acceptance |
|---|---|---|
| **Smoke (e2e)** | Adapter on localhost. `pip install --index-url http://localhost:<port>/simple/ requests`. Re-run install with `--no-deps --force-reinstall` → cache hit. | 2 runs succeed; 2nd has zero upstream traffic |
| **Property** | PEP 691 JSON ↔ PEP 503 HTML roundtrip. JSON `<a href>` extraction. Yank status propagation. | 1k cases pass |
| **Adversarial** | (1) Upstream tampered wheel (bytes don't match `#sha256=` URL fragment) → reject + audit. (2) Oversize wheel (>1 GiB) → 413 + audit. (3) Forged PAT → 401 + audit. (4) Tenant A blocked from tenant B's cache. (5) Index injection (upstream returns malformed JSON with extra URLs) → parse-rejected; fail-CLOSED. | All 5 pass |

## 9. Acceptance criteria

- [ ] `cargo build -p corelink-adapter-pip`: green
- [ ] `cargo test -p corelink-adapter-pip`: ≥12 tests passing
- [ ] `cargo clippy -p corelink-adapter-pip --all-targets -- -D warnings`: clean
- [ ] wasm32 workspace stays green
- [ ] `#[non_exhaustive]` on every public type
- [ ] `#![forbid(unsafe_code)]`
- [ ] Zero `unwrap/expect/panic` in `src/`
- [ ] **Integrity check MANDATORY pre-CAS-store** (wheel bytes match `#sha256=` URL fragment); fail-CLOSED with audit emit
- [ ] State-mutation audit emit before return
- [ ] PAT `SecretString` + constant-time compare
- [ ] L2.10: no file >500 LOC; sweet-spot ≤200
- [ ] SEAL audit `specs/_audits/2026-05-2X-w34-adapter-pip.md`
- [ ] Smoke test §8 row 1 passes
- [ ] DCO + Co-Authored-By

## 10. Out of scope / explicit deferrals

- **Publishing (`twine`).** Read-only mirror.
- **Private indexes** (`--extra-index-url` chaining). Future iteration.
- **PEP 458 / TUF (signed packages).** Out of v1; pip doesn't enforce by default.
- **Cross-tenant dedup.** Per-tenant only.
- **Anonymous reads / public namespace.** v1 requires PAT.
- **Yank metadata propagation under TTL.** Acceptable staleness; documented.
- **Devpi / Nexus protocol parity.** Only PEP 503/691; not Nexus's extended schema.
- **PEP 660 editable installs.** Local dev paths, not registry-relevant.

---

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of pip Adapter Contract.**
