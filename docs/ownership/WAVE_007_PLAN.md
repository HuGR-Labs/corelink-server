# Ownership wave 007 — storage lifecycle and regions

Source/static-only four-artifact contract. OKF remains verified canonical
reference; apply its concepts without copying or revalidating policy.

| Package | Manifest | Profile |
|---|---|---|
| `corelink-eviction` | `crates/corelink-eviction/Cargo.toml` | S |
| `corelink-failover-router` | `crates/corelink-failover-router/Cargo.toml` | S |
| `corelink-gc` | `crates/corelink-gc/Cargo.toml` | H |
| `corelink-r2-multipart` | `crates/corelink-r2-multipart/Cargo.toml` | S |
| `corelink-region` | `crates/corelink-region/Cargo.toml` | S |
| `corelink-tenant-path` | `crates/tenant-path/Cargo.toml` | S |

Each author owns only `.claude/skills/own-<package>/SKILL.md` and
`docs/ownership/crates/<package>/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`.
Fake/trait/source semantics are not D1/R2/cron/CF/region/runtime evidence.
GC's cron/checkpoint/degrade/manual surfaces and all storage/transport claims
need exact source qualification. Tenant path uses package identity, never
directory basename.
