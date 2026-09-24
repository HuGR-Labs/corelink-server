# corelink-rate-headers

[![crates.io](https://img.shields.io/crates/v/corelink-rate-headers.svg)](https://crates.io/crates/corelink-rate-headers)
[![docs.rs](https://docs.rs/corelink-rate-headers/badge.svg)](https://docs.rs/corelink-rate-headers)

Client-side rate-limit header primitives for [CoreLink](https://github.com/HumanGuardrail/corelink).

This crate exposes the client-facing **wire contract** an SDK or HTTP client
needs to interpret CoreLink rate-limit responses. Its header surface provides
canonical header shapes and the response-code taxonomy. The crate also
includes a separate in-memory global circuit-breaker model; it owns
process-local state but does not imply a durable provider or server wiring.

## What it provides

- **`RateLimitHeaderBuilder`** — composes canonical [RFC 9331](https://www.rfc-editor.org/rfc/rfc9331)
  `RateLimit: limit=N, remaining=M, reset=S` + `RateLimit-Policy: <limit>;w=<window>`
  headers (IETF-stable; supersedes legacy `X-RateLimit-*`).
- **`XRateLimitTypeKind`** — a `#[non_exhaustive]` 5-arm enum mapping the
  per-layer response-code taxonomy (`tenant_quota` / `per_ip` / `per_pat` /
  `over_quota` / `global_circuit_open`).
- **`Retry-After`** ([RFC 6585](https://www.rfc-editor.org/rfc/rfc6585)) emission,
  always present on a rejection.

## Why it's open

CoreLink opens the primitives a developer must *trust* and *reproduce*, and
closes the server that runs the service. A client cannot back off correctly
against a black-box header format — so this contract is published. The
abuse-prevention heuristics that decide *when* to emit a `429` stay in the
closed server. See [`docs/OSS_STRATEGY.md`](https://github.com/HuGR-dev/corelink-server/blob/main/docs/OSS_STRATEGY.md).

Maintainers: see the [local ownership reference](../../docs/ownership/crates/corelink-rate-headers/REFERENCE.md)
for this crate's source boundaries and evidence limits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE-2.0) at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this crate by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
