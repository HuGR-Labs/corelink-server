# Adapter Contract — Homebrew

**Wave:** 34 · **Crate target:** `corelink-adapter-brew` · **Owner:** Gustavo Schneiter · **Authored:** 2026-05-26

## 1. Headline + scope

CoreLink becomes a **bottle (pre-compiled binary) caching mirror** for Homebrew. `brew install` downloads pre-compiled bottles (`.tar.gz`) — by default from GitHub Packages OCI. This adapter sits between brew and the upstream OCI registry, caching bottles in CoreLink CAS so repeated installs across a CI fleet hit local cache.

**In scope:** bottle (`.tar.gz`) caching via the `HOMEBREW_BOTTLE_DOMAIN` env override. Read path (`install`). Per-tenant namespace.
**Out of scope:** tap source caching, formula generation, casks (`.dmg`/`.pkg`), bottling itself (`brew bottle`), Linux/Apple Silicon arch matrix logic (we cache whatever brew requests; brew handles arch).

## 2. Upstream protocol summary

Homebrew bottles are distributed as OCI artifacts via GitHub Packages by default. The brew client supports a **`HOMEBREW_BOTTLE_DOMAIN`** override (canonical reference: https://docs.brew.sh/Manpage#environment) that points all bottle downloads at an alternative host.

With `HOMEBREW_BOTTLE_DOMAIN=http://corelink-brew-adapter`, brew does:

```
GET <domain>/<bottle-image-name>/<sha256-or-version>
```

Wire is a **plain HTTPS GET** for a `.tar.gz` — no GraphQL, no manifest negotiation, no OCI dance from the client's perspective. The bottle filename embeds the SHA256 by convention.

Canonical bottle layout: `<formula>-<version>.<os>.bottle.tar.gz`. Filename does NOT contain SHA256 explicitly; the integrity hash is in the brew tap's formula DSL (Ruby file), not in the URL. The brew client verifies the downloaded bottle against the formula's `sha256` declaration post-download.

## 3. Mapping to CoreLink

| brew wire | CoreLink call |
|---|---|
| `GET /<domain-path>` | digest-keyed: `CasStore::get(tenant, blake3(canonical_url))`; miss: upstream fetch + CAS put |

Unlike npm/pip, brew doesn't include the upstream hash in the URL. Two implications:

1. **Pre-store integrity check is BEST-EFFORT.** We can't verify against an inline `#sha256=` fragment. The brew client does the verification post-download (against the formula DSL). The adapter trusts the upstream-served bytes and stores; if the upstream gets compromised, brew's downstream check catches it.
2. **No deduplication across formula updates.** When a formula's bottle SHA changes (e.g., security fix), the URL typically changes too (new version), so cache key naturally rotates.

Optional hardening: if the consumer wants, the adapter can fetch the brew tap's formula JSON in parallel and verify the bottle SHA before storing. Defer to v2 (adds latency + complexity for limited benefit since brew client verifies anyway).

## 4. Crate structure

```
crates/corelink-adapter-brew/
├── Cargo.toml
├── src/
│   ├── lib.rs                  # ~50 LOC re-exports
│   ├── server.rs               # axum router; catch-all bottle proxy (~200 LOC)
│   ├── bottle.rs               # bottle URL → CAS digest mapping + caching (~150 LOC)
│   ├── upstream.rs             # reqwest client for upstream OCI (~110 LOC)
│   ├── auth.rs                 # PAT → tenant (~100 LOC)
│   ├── audit.rs                # audit emit (~80 LOC)
│   ├── config.rs               # config (HUGR_BREW_ADAPTER_*) (~70 LOC)
│   ├── error.rs                # ~60 LOC
│   └── tests/
│       ├── smoke.rs            # brew install simulation (HOMEBREW_BOTTLE_DOMAIN flip) (~150 LOC)
│       ├── prop_url_normalize.rs # URL canonicalization (~70 LOC)
│       └── adversarial.rs      # oversize, forged auth, replay (~120 LOC)
```

Estimated: ~1080 LOC. Largest file: `server.rs` (200). All ≤500.

### Dep graph

```
corelink-adapter-brew
  ├── corelink-cas
  ├── corelink-auth
  ├── corelink-audit
  ├── corelink-core
  ├── corelink-telemetry
  └── corelink-adapters-cloud   (reqwest)
```

(No KV — brew adapter has no metadata-cache layer; only binary bottles in CAS.)

## 5. Trait interface

```rust
#[non_exhaustive]
pub struct BrewAdapterConfig {
    pub bind_addr: std::net::SocketAddr,
    pub upstream_domain: url::Url,            // default https://ghcr.io
    pub bottle_size_limit_bytes: u64,         // default 2 GiB (some scientific bottles)
    pub cas: std::sync::Arc<dyn corelink_cas::CasStore>,
    pub tenant_resolver: std::sync::Arc<dyn corelink_auth::TenantResolver>,
    pub auditor: std::sync::Arc<dyn corelink_audit::ports::AuditEmitter>,
}

pub async fn run_brew_adapter(config: BrewAdapterConfig) -> Result<(), BrewAdapterError>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BrewAdapterError {
    #[error("bind: {0}")]
    Bind(std::io::Error),
    #[error("auth: {0}")]
    Auth(String),
    #[error("cas: {0}")]
    Cas(String),
    #[error("upstream: {0}")]
    Upstream(String),
    #[error("bottle exceeds limit: {0} bytes")]
    BottleOversized(u64),
    #[error("audit: {0}")]
    Audit(String),
}
```

## 6. Auth + multi-tenancy

PAT-based, same pattern as cargo/npm/pip. `Authorization: Bearer hugr-pat_<token>`. Brew supports custom auth via `HOMEBREW_BOTTLE_DOMAIN` chained with `HOMEBREW_GITHUB_API_TOKEN` (env var) — we accept the PAT in that env var slot when running brew against the adapter.

PAT scope: `brew:read`.

Per-tenant CAS namespacing via `corelink-auth::tenant_path`.

## 7. Cache invalidation

| Cache type | Invalidation |
|---|---|
| **Bottle (CAS)** | Never. Bottles are immutable upon publishing (new bottle = new URL = new CAS key). |

Operator force-evict: out of scope; admin plane.

## 8. Tests

| Tier | Tests | Acceptance |
|---|---|---|
| **Smoke (e2e)** | Local adapter, mocked upstream OCI (wiremock returning canonical bottle bytes), `brew install` flow simulated by direct `curl <adapter>/<path>` against the adapter. Re-run hits cache. | 2nd request: zero upstream HTTP traffic |
| **Property** | URL canonicalization: trailing slash / case / query-param stripping all produce the same CAS digest. | 1k cases |
| **Adversarial** | (1) Oversize bottle (>2 GiB) → 413 + audit. (2) Forged PAT → 401 + audit. (3) Tenant isolation: A's `wget` bottle bytes are NOT served to B's request. (4) Upstream 5xx → 502 + audit; no half-store in CAS. | All 4 pass |

Real `brew` binary NOT required (it would download too much in CI). Smoke flow uses `curl` to exercise the adapter's HTTP surface against mocked OCI upstream.

## 9. Acceptance criteria

- [ ] `cargo build -p corelink-adapter-brew`: green
- [ ] `cargo test -p corelink-adapter-brew`: ≥10 tests passing
- [ ] `cargo clippy -p corelink-adapter-brew --all-targets -- -D warnings`: clean
- [ ] wasm32 workspace stays green
- [ ] `#[non_exhaustive]` on every public type
- [ ] `#![forbid(unsafe_code)]`
- [ ] Zero `unwrap/expect/panic` in `src/`
- [ ] Audit emit before every CAS put
- [ ] PAT `SecretString` + constant-time compare
- [ ] L2.10: no file >500 LOC; sweet-spot ≤200
- [ ] SEAL audit `specs/_audits/2026-05-2X-w34-adapter-brew.md`
- [ ] Smoke test §8 row 1 passes
- [ ] DCO + Co-Authored-By

## 10. Out of scope / explicit deferrals

- **Pre-store SHA verification against tap formula.** Defer to v2; brew client verifies downstream anyway.
- **Tap source caching.** `brew tap` does git clones — separate domain (`tap-domain`), not bottles. Future iteration.
- **Casks (`brew install --cask`).** GUI app distribution via DMGs/PKGs; different upstream (homebrew/homebrew-cask GitHub releases). Separate adapter.
- **Bottling (`brew bottle`).** Write path; out of scope.
- **Cross-platform bottle matrix logic.** Adapter is platform-agnostic at the URL level; brew picks the right URL.
- **Cross-tenant dedup.** Per-tenant only.
- **Anonymous reads.** v1 PAT required.
- **Brew install dependency resolution.** Brew does that locally; we serve whatever URLs brew requests.

---

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

**End of brew Adapter Contract.**
