# Ownership wave 009 — metadata, operations and replication

Source/static-only four-artifact contract. OKF remains the verified canonical
reference: apply its concepts without copying, revalidating or redefining its
policy.

| Package | Manifest | Profile |
|---|---|---|
| `corelink-meta` | `crates/corelink-meta/Cargo.toml` | S |
| `corelink-ops` | `crates/corelink-ops/Cargo.toml` | H |
| `corelink-replica-worker` | `crates/corelink-replica-worker/Cargo.toml` | S |
| `corelink-replication` | `crates/corelink-replication/Cargo.toml` | S |
| `corelink-replication-coordinator` | `crates/corelink-replication-coordinator/Cargo.toml` | S |
| `corelink-rotation-adapters` | `crates/corelink-rotation-adapters/Cargo.toml` | S |

Authors own only their four package files. `corelink-ops` has many local
modules and test/bin targets, hence H, but declarations and dry-run targets do
not prove PagerDuty, deploy, migrations, D1, backups or production operations.
Replication and rotation docs must retain the distinction between source-level
coordination/adapters and a completed external replication or key operation.
