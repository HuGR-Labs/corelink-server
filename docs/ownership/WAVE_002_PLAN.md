# Ownership wave 002 — core contracts

This wave uses the frozen artifact contract in [WAVE_001_PLAN.md](WAVE_001_PLAN.md#frozen-artifact-contract): four and only four package artifacts; source/static evidence; no Cargo compilation, remote operation, self-review, shared registry/index edit, or publication.

## Package slots

| WP | Package | Manifest | Initial profile | Owned paths |
|---|---|---|---|---|
| W002-CORE | `corelink-core` | `crates/corelink-core/Cargo.toml` | S | `own-corelink-core`, `crates/corelink-core/` |
| W002-AUTH | `corelink-auth` | `crates/corelink-auth/Cargo.toml` | H | `own-corelink-auth`, `crates/corelink-auth/` |
| W002-CRYPTO | `corelink-crypto` | `crates/corelink-crypto/Cargo.toml` | S | `own-corelink-crypto`, `crates/corelink-crypto/` |
| W002-DPA | `corelink-dpa-acceptance` | `crates/corelink-dpa-acceptance/Cargo.toml` | S | `own-corelink-dpa-acceptance`, `crates/corelink-dpa-acceptance/` |
| W002-DSR | `corelink-dsr` | `crates/corelink-dsr/Cargo.toml` | S | `own-corelink-dsr`, `crates/corelink-dsr/` |
| W002-PAT | `corelink-pat` | `crates/corelink-pat/Cargo.toml` | S | `own-corelink-pat`, `crates/corelink-pat/` |

Each slot is conflict-free with every other slot. The lead assigns the exact
profile and creates the worktree only after its static packet is prepared.

## W002-CORE packet anchor

`corelink-core` is a leaf apex package: no `corelink-*` dependencies. Its
public surface is `TenantId`, `Digest`, `Region`, `SecretWrap`, `CoreError`,
and `Clock`; source files are `src/lib.rs`, `types/{tenant,digest,region,secret}.rs`, `errors/{mod,digest_parse}.rs`, and `time.rs`. Known direct manifest consumers at this static census are `corelink-ac`, `corelink-adapter-host`, `corelink-cas`, and `corelink-server`. Existing scattered legacy types may remain; package presence does not prove migration, deployment, or runtime reachability.

Acceptance is the five W001 predicates (four artifacts pass their declared
checker plus scope-only diff), red before its author starts. The package needs
a fresh cold reviewer after any authored bytes; no package issue may be emitted
from this plan.
