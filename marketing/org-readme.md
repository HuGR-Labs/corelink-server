# HuGR Labs

Build infrastructure for regulated polyglot engineering orgs. We ship
tools that default to BYOK, residency honesty, and verifiable audit logs.

## Active products

### CoreLink — content-addressable cache

[corelink-server](https://github.com/HuGR-Labs/corelink-server) |
[corelink-cli](https://github.com/HuGR-Labs/corelink-cli)

REAPI v2 compatible cache for Bazel, Buck2, Cargo, npm, pip, OCI Docker
layers, and ML model registries. Multi-tenant. Per-tenant BYOK is on the
roadmap, with AWS KMS planned first (GCP Cloud KMS, Azure Key Vault, and
HashiCorp Vault to follow) — not yet shipped. An RFC-6962-style append-only
audit chain — BLAKE3-addressed, with tenant-replayable verification and
Ed25519-signed, Object-Lock-immutable heads — is on the roadmap.

Currently shipping: **launched, self-serve** (GA since 2026-07-10). Free
tier available for organisations on the path to the BYOK + verifiable-audit
roadmap.

## Engineering philosophy

**Specs first.** Every non-trivial subsystem has an invariant document
before implementation begins. Specs live under `specs/` and are committed
with version and seal status.

**TLA+ where it matters.** Cross-tenant isolation and the audit chain
commit protocol are modelled formally. The model runs in CI; a regression
that breaks an invariant breaks the build.

**Property tests over snapshots.** Snapshot tests rot; property tests
state what must always hold. The Rust workspace has proptests covering the
CAS addressing, Merkle chain, and tenant namespace isolation.

**Honest accounting.** BYOK, SOC 2 Type II, and SLA contracts are listed
with their actual status — shipped, in-progress, or planned — in the
ROADMAP and CHANGELOG of each repo. No feature listed as "available" unless
CI is green on it.

**Supply chain locked.** `cargo-deny` with explicit allow-lists on
licenses, crates, and advisories. `#![forbid(unsafe_code)]` across the
workspace.

## Currently shipping

CoreLink launched self-serve on 2026-07-10 and is generally available. See
[ROADMAP-TO-LAUNCH.md](https://github.com/HuGR-Labs/corelink-server/blob/main/ROADMAP-TO-LAUNCH.md)
for the full 8-phase plan and what ships next.

## Community

- [CODE_OF_CONDUCT.md](.github/CODE_OF_CONDUCT.md)
- [CONTRIBUTING.md](.github/CONTRIBUTING.md)
- [SECURITY.md](.github/SECURITY.md)
- Issues and pull requests welcome on public repos

## Contact

gustavo@humangr.com | [corelink-docs.humangr.com](https://corelink-docs.humangr.com)
