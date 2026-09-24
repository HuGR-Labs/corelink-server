---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-clerk
manifest: crates/corelink-clerk/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: draft
evidence_set: corelink-clerk-structural-normalization-20260921
---

# corelink-clerk — blast radius

Atomic static relations only. A dependency, cfg, import, or source value flow
does not show feature selection, compilation, target compatibility, HTTP
operation, live Clerk/JWKS interaction, identity validation, or runtime use.

[Features](#b01) · [RS256](#b02) · [Cache](#b03) · [Config](#b04) · [Consumers](#b05) · [Tests](#b06)

<a id="b01"></a>
## B01 — Manifest feature to public-path relation

**Dependency / flow / impact:** `jwt-adapter` → optional `jsonwebtoken` and
cfg-gated adapter/environment exports; `http-fetcher` → `jwt-adapter` plus
optional `reqwest` and HTTP-fetcher export; `test-utils` → optional `rsa`/
`rand` and fake/test helpers. A changed feature, cfg, or re-export can change
which source names a consumer can compile against.

**Predicate:** `Cargo.toml` feature entries, optional dependencies, required
example features, or the matching `#[cfg]` changes.

**Evidence:** `Cargo.toml:13-85`; `src/lib.rs:112-153`.

**Unknown/stop:** declarations do not establish a selected/resolved feature
set, native/wasm compatibility, or any build result.

<a id="b02"></a>
## B02 — Header/JWKS to decoder relation

**Dependency / flow / impact:** raw token text → header parse/literal `RS256`
check → typed header check and bounded `kid` → JWK filter (`RSA`, `RS256`,
optional `sig`) → RSA decoding key → `Validation::new(Algorithm::RS256)` →
principal-or-`AuthError` source path. Altering one predicate changes accepted
source shapes or error/counter categorization.

**Predicate:** header/JWK literal, key component construction, validation
configuration, claim check, or `AuthError` mapping changes.

**Evidence:** `src/adapter.rs:238-451,603-671`; `src/jwks.rs:43-101`;
`src/error.rs:9-74`.

**Unknown/stop:** this relation is not an observation of a real JWT, key,
provider, cryptographic outcome, or identity validation.

<a id="b03"></a>
## B03 — Config/cache/refresh relation

**Dependency / flow / impact:** builder URL and audience → instance hash →
`KvJwksCache::{get,set,delete}` key; cached `stored_at` and configured TTL →
freshness predicate; allowed `kid` miss → bounded refresh source route →
trait fetch/set and local counters; absent key after those routes → bounded
negative-cache record. A change can alter cache partitioning, freshness,
refresh attempts, or retry suppression.

**Predicate:** hash input/domain, TTL/leeway bound, cache trait signature,
`is_fresh`, refresh trigger, or negative-cache constants change.

**Evidence:** `src/config.rs:15-232`; `src/jwks_cache.rs:17-85`;
`src/adapter.rs:30-31,270-594`.

**Unknown/stop:** no concrete cache durability/expiry, fetch result, upstream
rotation, concurrency behavior across isolates, or metric delivery is proven.

<a id="b04"></a>
## B04 — Environment/principal/redaction relation

**Dependency / flow / impact:** named environment strings and publishable-key
parser → `ClerkConfig` URL/issuer/audience values; secret string → `SecretKey`;
decoded claim fields → opaque principal fields, email/role mapping, timestamps;
subject string → eight-hex `principal_hash`. A change can alter configuration
parsing, public principal shape, local debug output, or correlation string
format.

**Predicate:** env-name/parser, config validation, ID/email/role conversion,
redaction hash domain/length, or public principal field changes.

**Evidence:** `src/{config,env_config,principal,redact}.rs`.

**Unknown/stop:** source does not disclose actual environment values, prove
secret zeroization/telemetry hygiene at runtime, or prove a principal is a
real identity.

<a id="b05"></a>
## B05 — Static consumer and implementation-seam relation

**Dependency / flow / impact:** root workspace dependency declares
`corelink-clerk` with default features disabled; `corelink-worker` declares an
optional dependency naming `jwt-adapter` and imports principal/error surfaces;
`corelink-clerk-cf` declares `corelink-clerk` with default features disabled
and imports `JwksFetcher`/`KvJwksCache`; `corelink-auth::clerk` re-exports this
package. A change to the cited public trait/type/error/feature path requires
those owners to assess source compatibility.

**Predicate:** public export, trait, error, principal type, feature, or
manifest edge changes.

**Evidence:** root `Cargo.toml:92,564-565`; `crates/corelink-worker/Cargo.toml:33,67,113`;
`crates/corelink-worker/src/middleware/{auth,auth_ctx}.rs`;
`crates/corelink-clerk-cf/Cargo.toml:33` and `src/{cf_fetch,cf_kv}.rs`;
`crates/corelink-auth/src/clerk.rs`.

**Unknown/stop:** this is a selected static census, not a complete reverse
graph, resolved build, caller behavior, or runtime reachability claim.

<a id="b06"></a>
## B06 — Declared test/example/bench relation

**Dependency / flow / impact:** manifest target declarations → checked-in
examples, four explicit `[[test]]` targets (`prop_validate`, `adversarial`,
`rotation`, `http_fetcher`), and explicit `principal_id_clone` bench;
`tests/mutation_kills.rs` is an implicit convention-discovered integration-test
source with no `[[test]]` manifest stanza. Examples name fake fetcher/cache,
the target sources name property/adversarial/rotation/HTTP/mutation areas, and
the bench imports test principal helpers. Changes can invalidate target source
assumptions or required-feature declarations.

**Predicate:** target declaration, feature gate, fake/test helper, or target
source API changes.

**Evidence:** `Cargo.toml:60-115`; `examples/*.rs`;
`tests/{prop_validate,adversarial,rotation,http_fetcher,mutation_kills}.rs`;
`benches/principal_id_clone.rs`.

**Unknown/stop:** no explicit target or convention-discovered source was
executed; no test, benchmark, HTTP-fetcher, or live-service result is asserted.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
