# corelink-clerk

CoreLink Clerk SSO adapter — JWT RS256 validation + JWKS cache.

Implements [WI-S03-001](../../specs/04_sprints/S03/work_items/WI-S03-001-clerk-adapter.md)
(HIGH_RISK lane; FF-HR-002 / FF-HR-005 / FF-HR-009).

## Surface

- `ClerkAdapter::validate(raw_jwt) -> Result<ClerkPrincipal, AuthError>`
- `ClerkAdapter::refresh_jwks() -> Result<(), AuthError>` — manual JWKS refresh hook.
- `JwksFetcher` + `KvJwksCache` traits — production wiring is layered in
  `corelink-worker` (CF Workers `worker::Fetch` + `worker::kv::Store`),
  tests use the in-memory fakes in [`fakes`](src/fakes.rs).

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
