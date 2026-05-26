---
title: Wave 34 — npm Registry Adapter SEAL Audit (v2)
date: 2026-05-26
status: SEAL — all acceptance criteria met
parent_audit: null
dispatch_packet: specs/_proposals/adapters/npm.md
branch: wt/r-prep-w34-adapter-npm-v2
base_commit: 04a48e7d78ea7109b9f43253a31437ebf1928dda
crate_target: crates/corelink-adapter-npm/
---

# §1. Scope as dispatched

Implement `corelink-adapter-npm`, a caching npm registry proxy adapter
implementing the npm registry API subset used by `npm install` (and
`pnpm` / `yarn` / `bun`) as a transparent caching mirror in front of
`registry.npmjs.org`. Package metadata (JSON) is cached in tenant-scoped
KV with TTL (default 300s); tarballs are stored in CoreLink CAS keyed by
SHA256 of the tarball URL (URL is the canonical identity for npm tarballs;
npm enforces tarball immutability). The adapter MUST perform a mandatory
pre-CAS-store integrity check verifying tarball SHA1 against the npm
metadata `dist.shasum` field using `subtle::ConstantTimeEq`, emitting
`corelink.npm.tarball.integrity_mismatch.v1` fail-CLOSED on mismatch.

**v2 re-dispatch note:** v1 packet HALTed pre-mutation on fictional trait
surfaces (`corelink_cas::CasStore` etc. — none exist in the workspace).
v2 was redrafted with the inline-ports mandate (see `specs/_proposals/adapters/npm.md`
§0 and `specs/_proposals/adapters/README.md` §"Architectural pattern — inline-ports").

# §2. Acceptance gate results (per npm.md §9)

| Gate | Result |
|---|---|
| `cargo build -p corelink-adapter-npm` | GREEN |
| `cargo test -p corelink-adapter-npm` (≥12 floor) | **50 passing** (32 unit + 7 adversarial + 6 property + 5 smoke) |
| `cargo clippy -p corelink-adapter-npm --all-targets -- -D warnings` | GREEN |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | GREEN |
| Every public type `#[non_exhaustive]` | applied (`NpmAdapterError`, `NpmAdapterConfig`, `NpmAdapterConfig`, `MetadataResponse`, `TarballResponse`, `AdapterState`, `SearchQuery`, `UpstreamClient`) |
| `#![forbid(unsafe_code)]` | applied at `src/lib.rs` |
| `#[non_exhaustive]` on every pub struct/enum | confirmed (see §5 file list) |
| Zero `unwrap/expect/panic` in `src/` production paths | confirmed by `clippy::unwrap_used = "deny"` + `expect_used = "deny"` + `panic = "deny"` at crate-level lints; test mods have explicit `#[allow]` |
| **Integrity check MANDATORY pre-CAS-store** | enforced in `tarball.rs::serve_tarball` via `verify_sha1` (constant-time `subtle::ConstantTimeEq` compare); fail-CLOSED with `TARBALL_INTEGRITY_MISMATCH` audit before return |
| State-mutation audit BEFORE return | `tarball.rs::serve_tarball` calls `emit_npm_audit(TARBALL_STORED)` BEFORE `cas.put`; `metadata.rs::refresh_from_upstream` calls `emit_npm_audit(METADATA_REFRESHED)` BEFORE `kv.put` |
| PAT `SecretString` + constant-time compare | `auth.rs::extract_pat` returns `secrecy::SecretString`; `FixedTenant` test resolver uses `subtle::ConstantTimeEq`; resolver trait contract documents the same requirement |
| L2.10 (no file >500 LOC; sweet-spot ≤200) | top 3: 372 (`tarball.rs`) / 331 (`server.rs`) / 235 (`metadata.rs`) — every file under 500 cap |
| SEAL audit doc | this file (`specs/_audits/2026-05-26-w34-adapter-npm-v2.md`) |
| DCO + Co-Authored-By | applied at commit time |

# §3. Trait-surface gap and parallel-safety decision (mirrors pip §3)

The v1 dispatch packet's §5 trait interface references three trait names
that do not exist in the codebase at the dispatch baseline (`04a48e7d`):

- `corelink_cas::CasStore`
- `corelink_adapters_cloud::cf::kv::KvStore`
- `corelink_auth::TenantResolver`

A pre-mutation grep confirmed: no `pub trait CasStore`, `pub trait KvStore`,
or `pub trait TenantResolver` is declared anywhere under `crates/`. The
v2 contract (redrafted per the convergent inline-ports decision from the
pip / brew / oci campaigns) mandates adapter-local port traits.

**Decision (consistent with pip/brew/oci §3):** rather than mutating shared
umbrella crates (which would create source-overlap with the parallel cargo
adapter agent), `corelink-adapter-npm` declares its own minimal port surface
in `src/ports.rs`:

- `pub trait CasStore` (`get`, `put`) — async, tenant-scoped.
- `pub trait KvStore` (`get`, `put`) — async, tenant-scoped, returns
  `(value, inserted_at_unix_ms)` so the TTL check in `metadata.rs::is_fresh`
  is pure-logic.
- `pub trait TenantResolver` (`resolve(pat_plaintext) → TenantId`) — async;
  resolver contract documents constant-time PAT compare requirement.

Consolidation into a workspace-level `corelink-adapter-ports` crate (or
extension of the `corelink-cas` umbrella) is deferred to a sequential
follow-up wave once all five adapters (cargo / npm / brew / oci / pip)
have landed and the trait surfaces have stabilised. Production wiring at
boot bridges these local-to-adapter ports to `corelink-cas` /
`corelink-adapters-cloud::cf::kv` surfaces.

# §4. SHA1 integrity check — implementation note

npm's `dist.shasum` is a SHA1 hex digest (40 chars). This crate implements
SHA1 inline in `tarball.rs::sha1_digest` (FIPS PUB 180-4 §6.1) to avoid
pulling an uncategorised `sha1` dep into the workspace. The implementation:

- Uses only safe array access patterns (no `indexing_slicing`).
- Is validated against FIPS PUB 180-4 known vectors:
  - SHA1(`""`) = `da39a3ee5e6b4b0d3255bfef95601890afd80709` ✓
  - SHA1(`"abc"`) = `a9993e364706816aba3e25717850c26c9cd0d89d` ✓
- The `verify_sha1` function uses `subtle::ConstantTimeEq` to prevent
  timing leaks in the SHA1 comparison path (spec mandate).

CAS key derivation uses SHA256 of the tarball URL (not SHA1) because the
`Digest` type requires a 32-byte value. The URL-keyed CAS digest provides
sufficient collision resistance for immutable tarball identity.

# §5. Hard pause triggers — none activated

Evaluated each hard pause trigger:

1. Any `.rs` file >500 LOC — **NOT activated**. Largest: `tarball.rs` at 372 LOC.
2. Crate LOC >1900 (50% over 1300 estimate) — **NOT activated**. `src/` total:
   1594 LOC across 9 files. Within the 1300 estimate + 50% margin (1950 ceiling).
3. wasm32 workspace red — **NOT activated**. `cargo build --target
   wasm32-unknown-unknown -p corelink-clerk-cf`: GREEN (disk space required
   freeing ~900 MiB of incremental wasm build artifacts before the check).
4. `corelink_audit::ports::AuditEmitter` missing — **NOT activated**. Trait
   is present; `InMemoryAuditEmitter` available for tests.
5. Cargo.lock conflicts beyond UNION-resolvable — **NOT activated**. Added
   `corelink-adapter-npm` to `[workspace.members]` and `[workspace.dependencies]`;
   no new version constraints introduced.
6. Test count <12 — **NOT activated**. 50 tests (4.2× the 12-test floor).
7. Tarball integrity check impossible — **NOT activated**. `dist.shasum`
   exists in npm metadata; verified against known vectors.

# §6. Files added (all NEW; zero source overlap with sibling Wave-34 worktrees)

- `crates/corelink-adapter-npm/Cargo.toml`
- `crates/corelink-adapter-npm/src/lib.rs`
- `crates/corelink-adapter-npm/src/error.rs`
- `crates/corelink-adapter-npm/src/ports.rs`
- `crates/corelink-adapter-npm/src/config.rs`
- `crates/corelink-adapter-npm/src/audit.rs`
- `crates/corelink-adapter-npm/src/auth.rs`
- `crates/corelink-adapter-npm/src/upstream.rs`
- `crates/corelink-adapter-npm/src/metadata.rs`
- `crates/corelink-adapter-npm/src/tarball.rs`
- `crates/corelink-adapter-npm/src/server.rs`
- `crates/corelink-adapter-npm/tests/common.rs`
- `crates/corelink-adapter-npm/tests/smoke.rs`
- `crates/corelink-adapter-npm/tests/adversarial.rs`
- `crates/corelink-adapter-npm/tests/prop_metadata.rs`
- `specs/_audits/2026-05-26-w34-adapter-npm-v2.md` (this file)

Shared (UNION-resolvable) workspace edits:

- `Cargo.toml`: added `"crates/corelink-adapter-npm"` to `[workspace.members]`
  (after the `corelink-adapter-oci` entry) + `corelink-adapter-npm = { path = ... }`
  to `[workspace.dependencies]` (after the `corelink-adapter-oci` entry). No
  edits to version constraints; no mutations to any other crate's `Cargo.toml`.

# §7. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

End of npm adapter v2 SEAL audit.
