# Ownership wave 008 — rate, REAPI and observability

Source/static-only four-artifact contract. Use verified OKF concepts as
references; do not fork or duplicate policy.

| Package | Manifest | Profile |
|---|---|---|
| `corelink-rate-headers` | `crates/corelink-rate-headers/Cargo.toml` | S |
| `corelink-ratelimit` | `crates/corelink-ratelimit/Cargo.toml` | S |
| `corelink-reapi` | `crates/corelink-reapi/Cargo.toml` | H |
| `corelink-slo` | `crates/corelink-slo/Cargo.toml` | S |
| `corelink-telemetry` | `crates/corelink-telemetry/Cargo.toml` | H |
| `corelink-tracing` | `crates/corelink-tracing/Cargo.toml` | S |

Authors own only their four package artifacts. REAPI's gRPC/CAS/storage graph,
telemetry's absorbed modules/re-exports, and every rate/SLO/tracing provider
claim require exact source evidence. Do not infer HTTP, PagerDuty, OTLP,
Prometheus, D1/DO, R2, deployment or runtime behavior from declarations.
