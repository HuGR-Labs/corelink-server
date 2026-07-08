# corelink-rate-headers

[![crates.io](https://img.shields.io/crates/v/corelink-rate-headers.svg)](https://crates.io/crates/corelink-rate-headers)
[![docs.rs](https://docs.rs/corelink-rate-headers/badge.svg)](https://docs.rs/corelink-rate-headers)

Client-side rate-limit header primitives for [CoreLink](https://github.com/HumanGuardrail/corelink).

This crate is the **wire contract** an SDK or HTTP client needs to correctly
handle CoreLink backpressure. It carries no server logic, no secrets, and no
state — only the canonical header shapes and the response-code taxonomy, so
that any client (in any language, via FFI) can parse a `429` the same way the
server emits it.

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
closed server. See [`docs/OSS_STRATEGY.md`](https://github.com/HumanGuardrail/corelink-server/blob/main/docs/OSS_STRATEGY.md).

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE-2.0) at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this crate by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
