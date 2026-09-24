# Ownership wave 011 — service integrations and worker boundary

Source/static-only four-artifact contract. The verified OKF remains canonical:
apply its concepts as routing, without copying, revalidating or redefining it.

| Package | Manifest | Profile |
|---|---|---|
| `corelink-stripe-real` | `crates/corelink-stripe-real/Cargo.toml` | S |
| `corelink-tier-selection` | `crates/corelink-tier-selection/Cargo.toml` | S |
| `corelink-transparency-log` | `crates/corelink-transparency-log/Cargo.toml` | S |
| `corelink-turbo-bridge` | `crates/corelink-turbo-bridge/Cargo.toml` | S |
| `corelink-worker` | `crates/corelink-worker/Cargo.toml` | H |
| `corelink-client-verify` | `crates/corelink-client-verify/Cargo.toml` | S |

External-provider names, SDK routes and the worker's broad source layout are
not evidence of payment, request, tenant, storage, deployment or runtime
behavior. Worker is H because its source inventory is 78 Rust files; all
other profile decisions remain source-observable, not operational claims.
