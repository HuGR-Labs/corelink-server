# Ownership wave 012 — schedulers, inquiry, drift and SDKs

Source/static-only four-artifact contract. Route to the verified canonical OKF
only; do not fork, duplicate or revalidate its policy.

| Package | Manifest | Profile |
|---|---|---|
| `corelink-chaos-scheduler` | `crates/corelink-chaos-scheduler/Cargo.toml` | S |
| `corelink-dsr-statuspage-scheduler` | `crates/corelink-dsr-statuspage-scheduler/Cargo.toml` | S |
| `corelink-enterprise-inquiry` | `crates/corelink-enterprise-inquiry/Cargo.toml` | S |
| `corelink-terraform-drift-consumer` | `crates/corelink-terraform-drift-consumer/Cargo.toml` | S |
| `corelink-go` | `tools/sdks/go/Cargo.toml` | S |
| `corelink-py` | `tools/sdks/python/Cargo.toml` | S |

Schedulers, Terraform, external status/API names and SDK targets are static
surfaces only. They never prove a scheduled run, provider operation, customer
request, release, deployment or cross-language runtime reachability.
