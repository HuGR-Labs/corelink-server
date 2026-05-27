# corelink-clerk

CoreLink Clerk SSO adapter — JWT RS256 validation + JWKS cache.

Implements [WI-S03-001](../../specs/04_sprints/_sealed/S03/work_items/WI-S03-001-clerk-adapter.md)
(HIGH_RISK lane; FF-HR-002 / FF-HR-005 / FF-HR-009).

## Surface

- `ClerkAdapter::validate(raw_jwt) -> Result<ClerkPrincipal, AuthError>`
- `ClerkAdapter::refresh_jwks() -> Result<(), AuthError>` — manual JWKS refresh hook.
- `validate_session(jwt, config, fetcher, cache)` — free-function convenience surface for cold-path / one-shot validation.
- `JwksFetcher` + `KvJwksCache` traits — production fetcher implementations coexist:
  - **Native** (R2-2): `HttpJwksFetcher` (feature `http-fetcher` → `reqwest` + `rustls-tls`).
  - **CF Workers**: `corelink-clerk-cf::CfJwksFetcher` (`worker::Fetch::Url` on the wasm32 target).
  - Tests use the in-memory fakes in [`fakes`](src/fakes.rs).
- `ClerkConfig::from_env()` — load config from the canonical env-var matrix (`CLERK_PUBLISHABLE_KEY`, `CLERK_SECRET_KEY`, `CLERK_AUDIENCE`, optional `CLERK_JWKS_URL`, `CLERK_JWT_ISSUER`).

## Feature flags

| Feature | Pulls | Use case |
|---|---|---|
| `jwt-adapter` (default) | `jsonwebtoken` (ring) | Native JWT validate path. Required for `ClerkAdapter`. Incompatible with `wasm32-unknown-unknown`. |
| `http-fetcher` (opt-in) | `reqwest` + `rustls-tls` | `HttpJwksFetcher` for native targets. Implies `jwt-adapter`. |
| `test-utils` (opt-in) | `rsa` + `rand` | `TestRsaKey` keypair generator for integration tests. NEVER enable in production. |

## Canonical bounds (WI §9)

| Field | Value | Source |
|---|---|---|
| Algorithm allowlist | `RS256` only | WI §1, §9.1 |
| Clock skew leeway | 60 s | WI §9.3 / RFC 7519 §4.1.4 |
| JWKS cache TTL | 24 h | WI §9.2 |
| Issuer matching | exact-string allowlist | WI §9.4 |
| KID rotation | lazy refresh on miss + 1 retry | WI §9.5 |

## Test pyramid

- Property tests (`tests/prop_validate.rs`) — 10k iter PR per
  invariant (wrong sig / expired / iss / aud / random no-panic).
- Adversarial regressions (`tests/adversarial.rs`) — five canonical
  CVE classes (alg=none, RS↔HS confusion, exp bypass, aud spoof,
  iss spoof) + KID rotation + manual refresh.
- Rotation chaos (`tests/rotation.rs`) — outage, rotation storm,
  clock-skew boundary, JWKS fetch failure.

## Reuse pattern (S-04 PAT signing key, S-13 admin plane)

The `JwksFetcher` + `KvJwksCache` trait split is intentionally
generic: any future asset class with a "lazy-refresh on KID miss"
JWKS-shaped key store can reuse the cache + outcome-counter
plumbing without forking.
