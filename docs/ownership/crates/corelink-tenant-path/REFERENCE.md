---
schema: corelink-ownership/1.1
document: reference
package: corelink-tenant-path
manifest: crates/tenant-path/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: w007-tenant-path-static-source-20260920
---

# corelink-tenant-path — reference

SOURCE-only record of the package manifest and local Rust source. It does not establish a tenant runtime, secret source, storage access, cache deployment, path use, migration, rollback, or production behavior.

[Identity](#r01) · [Surface](#r02) · [Key](#r03) · [Prefix](#r04) · [Derivation](#r05) · [Cache](#r06) · [Tests](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Package identity

The manifest package name is `corelink-tenant-path`; its repository path is `crates/tenant-path/Cargo.toml`. `src/lib.rs` forbids unsafe code, exposes `error`, keeps `cache` and `prefix` private modules, and re-exports the stated public types, cache API, error type, and derivation function.

**INV-TP-01:** A changed package identity or re-export list is falsifiable by reading the manifest and `src/lib.rs`; directory naming alone is not package identity.

Evidence: `crates/tenant-path/{Cargo.toml,src/lib.rs}`.

<a id="r02"></a>
## R02 — Public surface boundary

The source re-exports `derive_prefix`, `TenantDerivationKey`, `TenantPrefix`, `TENANT_PREFIX_LEN`, `TdkVersion`, `TenantPrefixCache`, `CACHE_CAPACITY`, and `DeriveError`. `DeriveError` has one non-exhaustive `HmacOutputSize(usize)` variant, while `derive_prefix` returns `TenantPrefix` directly rather than `Result`.

**INV-TP-02:** The listed names and direct return shape are falsifiable by the two `pub use` statements and their declarations. This is not a promise of caller compatibility or execution.

Evidence: `src/lib.rs`; `src/prefix.rs`; `src/cache.rs`; `src/error.rs`.

<a id="r03"></a>
## R03 — Derivation-key boundary

`TenantDerivationKey` wraps a private `[u8; 32]`, derives `Clone`, `Zeroize`, and `ZeroizeOnDrop`, and is constructed by `from_bytes(Zeroizing<[u8; 32]>)`. Its raw-byte accessor is `pub(crate)`. Its `Debug` implementation writes the literal `TenantDerivationKey(REDACTED)`.

**INV-TP-03:** External code cannot call an inherent public raw-byte accessor or initialize the private tuple field through this source API; this is falsifiable by visibility changes. It does not prove key provenance, custody, wiping on a target, or absence of logs elsewhere.

Evidence: `src/prefix.rs` (`TDK_LEN`, `TenantDerivationKey`, `from_bytes`, `as_bytes`, `Debug`).

<a id="r04"></a>
## R04 — Prefix-value boundary

`TENANT_PREFIX_LEN` is `16`. `TenantPrefix` is a private-field `[u8; TENANT_PREFIX_LEN]` newtype with public `as_str`, `Display`, and `Debug`; construction occurs in `derive_prefix`. `as_str` uses UTF-8 conversion over the internally created byte array.

**INV-TP-04:** A prefix returned by the current derivation function is represented by a 16-byte `TenantPrefix`, and the public source surface exposes string formatting rather than a public raw-byte accessor. This is falsifiable by its constant, field visibility, or methods; it does not establish any persisted-path format.

Evidence: `src/prefix.rs` (`TENANT_PREFIX_LEN`, `TenantPrefix`, `as_str`, formatting implementations).

<a id="r05"></a>
## R05 — Derivation sequence

`derive_prefix` initializes `Hmac<Sha256>` from the key's crate-private bytes, updates it with `tenant_id.as_bytes()`, finalizes it, URL-safe-base64-no-pad encodes the digest into a 43-byte buffer, and copies its first 16 bytes into `TenantPrefix`. The normal dependencies declare `hmac`, `sha2`, `base64`, `uuid`, `zeroize`, and `thiserror`.

**INV-TP-05:** For typed inputs, the source-visible operation order and truncation are exactly the calls and copy shown above; a reordered update, encoding engine, or slice width falsifies this record. It is not a cryptographic certification, collision measurement, or authorization claim.

Evidence: `Cargo.toml`; `src/prefix.rs` (`HMAC_B64_LEN`, `derive_prefix`).

<a id="r06"></a>
## R06 — Cache contract

`TenantPrefixCache` stores `HashMap<(TdkVersion, Uuid), TenantPrefix>` behind `RwLock`; `TdkVersion` wraps `u32`, and `CACHE_CAPACITY` is `4096`. `get_or_derive` attempts a read lookup, otherwise calls `derive_prefix`, then attempts insertion with iterator-first eviction when at capacity. Counters use relaxed `AtomicU64` loads and increments.

**INV-TP-06:** The cache key contains both `TdkVersion` and `Uuid`, and a read/write-lock failure path returns the directly derived value in the current source. This is falsifiable by the map key or control flow; it does not prove rotation execution, capacity in a process, isolation, locking performance, or cache deployment.

Evidence: `src/cache.rs` (`TdkVersion`, `CACHE_CAPACITY`, `TenantPrefixCache`, `get_or_derive`).

<a id="r07"></a>
## R07 — Test and vector sources

The manifest declares three benches and dev dependencies including `proptest`, `serde_json`, and `criterion`. `tests/prop_tenant_path.rs` imports the public derivation surface and contains fixed and property assertions. `tests/edge_parity_vectors.rs` reads `worker/tests/vectors/tenant_prefix_vectors.json`, derives prefixes, and constructs local comparison strings.

**INV-TP-07:** These named test sources and declared bench targets are present at the baseline; removal or changed target path falsifies the inventory. No test, benchmark, fuzz target, vector, or cross-language comparison was executed for this record.

Evidence: `Cargo.toml`; `tests/prop_tenant_path.rs`; `tests/edge_parity_vectors.rs`.

<a id="r08"></a>
## R08 — Evidence limit and unknowns

SOURCE evidence does not establish the full consumer graph, feature resolution, source or rotation of actual keys, caller inputs, wire or stored-value compatibility, path construction by consumers, data migration, rollback, storage writes, cache residency, tenant isolation in an environment, timing, deployment, publication, or runtime reachability.

**INV-TP-08:** This record remains invalid as evidence for any listed execution or runtime claim unless separately selected source, resolved-graph, execution, provider, or runtime evidence is added. Route such requests to the owning integration, storage, security, or operational boundary.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-tenant-path/SKILL.md#s01)
