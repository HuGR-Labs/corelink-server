---
schema: corelink-ownership/1.1
document: reference
package: corelink-clerk
manifest: crates/corelink-clerk/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: draft
evidence_set: corelink-clerk-structural-normalization-20260921
---

# corelink-clerk — ownership reference

[R01](#r01) · [R02](#r02) · [R03](#r03) · [R04](#r04) · [R05](#r05) · [R06](#r06) · [R07](#r07) · [R08](#r08)

This H-profile reference is limited to checked-in manifest and source at
`1c99a5b8ce1db7b622db7811512432a5d070bbbe`. It establishes source contracts,
not a selected feature set, compilation, target compatibility, HTTP operation,
live Clerk/JWKS behavior, identity validation, deployment, secret custody, or
cold review.

[Identity](#r01) · [Surface](#r02) · [Features](#r03) · [RS256](#r04) · [Cache](#r05) · [Config and principal](#r06) · [Declared targets](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Package identity and local ownership

`crates/corelink-clerk/Cargo.toml` names package `corelink-clerk`; `src/lib.rs`
forbids unsafe code and declares local modules for configuration, errors, JWKS,
cache, principals, and redaction, plus cfg-gated adapter, environment, fake,
and HTTP-fetcher paths. The package owns those checked-in interfaces; it does
not own a concrete production cache or Worker fetch implementation merely
because it defines the traits. Falsifier: a manifest dependency, module
declaration, or public re-export changes. Evidence: `Cargo.toml:1-85`,
`src/lib.rs:101-153`.

<a id="r02"></a>
## R02 — Public source surface

| Source path | Static responsibility | Boundary |
|---|---|---|
| `adapter` | `ClerkAdapter`, `validate_session`, refresh/counter source paths | gated local orchestration over supplied traits |
| `config`, `env_config` | immutable builder config and environment loaders | configuration parsing only |
| `jwks`, `jwks_cache` | document/key model plus `JwksFetcher` and `KvJwksCache` futures/traits | backend behavior is external |
| `principal`, `redact`, `error` | opaque principal fields, short hash helper, and error taxonomy | no transport mapping here |
| `fakes` | in-memory cache/fetchers and cfg-gated test-key helpers | test support, not a production backend |
| `http_fetcher` | `reqwest`-backed source implementation | cfg-gated; no request was performed for this reference |

`lib.rs` conditionally re-exports adapter/environment and HTTP symbols and
unconditionally re-exports config, JWKS/cache, principal, error, and redaction
symbols. A public source path does not establish that any caller selects or
executes it. Evidence: `src/lib.rs:112-153`, `src/{adapter,config,env_config,error,fakes,http_fetcher,jwks,jwks_cache,principal,redact}.rs`.

<a id="r03"></a>
## R03 — Declared feature-to-source routes

| Declared feature | Manifest relation | Source route |
|---|---|---|
| `jwt-adapter` | default list names it; enables optional `jsonwebtoken` | gates `adapter`, `env_config`, re-exports, and principal constructors |
| `http-fetcher` | names `jwt-adapter` and optional `reqwest` | gates `http_fetcher` and `HttpJwksFetcher` re-export |
| `test-utils` | enables optional `rsa` and `rand` | admits `fakes`; gates `fakes::test_keys` and principal test helpers |

The manifest gives the three relations above; examples require
`jwt-adapter,test-utils`, while dev-dependency self-reference names all three
for integration targets. These are declarations, not evidence that features
were selected, resolved, built, or usable on any target. Evidence:
`Cargo.toml:13-85`, `src/lib.rs:112-153`, `src/{adapter,env_config,fakes,http_fetcher,principal}.rs`.

<a id="r04"></a>
## R04 — RS256 source contract

The adapter's source first reads header JSON and rejects an `alg` string other than literal `RS256`; it then uses `decode_header` and rejects a non-`RS256` enum. `Jwks::parse` retains only keys with `alg == "RS256"`, `kty == "RSA"`, optional `use == "sig"`, and nonempty `kid`, modulus, and exponent. Before decoder use, the adapter constructs a key from those RSA components and creates `Validation::new(Algorithm::RS256)`; claim-time checks are implemented in the adapter using its

clock after decoder use. `AuthError` distinguishes `AlgNotAllowed`, malformed input, signature, time, issuer, audience, JWKS, and `kid` outcomes.

Falsifiable invariant: changing any of the literal checks, accepted JWK
predicate, `Validation` construction, or error map changes the source-level
algorithm/error contract. This does not prove an accepted token represents a
real identity or that cryptographic, library, or provider behavior was
observed. Evidence: `src/adapter.rs:238-451,603-671`, `src/jwks.rs:43-101`,
`src/error.rs:9-74`.

<a id="r05"></a>
## R05 — Cache, refresh, and `kid` source contract

`KvJwksCache` declares `get`, `set`, and `delete` keyed by `instance_hash`; a
cache implementation is required by its documentation to treat expiry as a
miss. Independently, `is_fresh` returns true only when `stored_at + ttl > now`.
The builder default TTL is `JWKS_TTL_SECS = 86_400`; the source rejects a TTL
above 172,800 seconds. `instance_hash` is the first eight SHA-256 bytes over a
domain separator, URL, and audience.

On a syntactically allowed `kid`, adapter source consults fresh cached JWKS, otherwise fetches and sets through the traits. A cold miss can lead to a scheduled refresh then one `KidMiss` refresh; a warm miss gets one `KidMiss` refresh. Not-found values are held in a process-local negative cache for 60 seconds, bounded at 1,024 entries. `refresh_jwks` names the manual trigger; counters are incremented in source after the corresponding successful fetch-and-cache path.

This is a code-path contract, not proof of a cache backend, a fetch, cache coherence, upstream rotation, or metrics delivery. Evidence: `src/{adapter,config,jwks_cache}.rs`, especially `adapter.rs:30-31,270-594`; `src/fakes.rs:20-247`.

<a id="r06"></a>
## R06 — Configuration, principal, and redaction source contracts

`ClerkConfigBuilder::build` requires nonempty HTTPS JWKS URL, audience, and
issuer allowlist; it trims issuer trailing slashes, requires HTTPS issuers,
defaults leeway to 60 seconds, and rejects leeway above 120 seconds.
`ClerkConfig::from_env` reads `CLERK_PUBLISHABLE_KEY` and `CLERK_AUDIENCE`,
uses optional `CLERK_JWKS_URL` and comma-separated `CLERK_JWT_ISSUER`, and
derives defaults from a parsed publishable-key host. `ClerkEnvSecrets::from_env`
separately reads `CLERK_SECRET_KEY`; `SecretKey` has redacted `Debug` and a
source `Drop` implementation that overwrites its byte buffer.

`ClerkPrincipal` holds opaque user/org/session IDs, email, role, and timestamps.
The ID wrappers store `Arc<str>` and redact `Debug`; `Email::parse` enforces a
small local shape check, and unknown role strings map to `Guest`. `principal_hash`
returns the first four bytes of a domain-separated SHA-256 digest as eight hex
characters. None of these local transformations proves secret erasure in a
running process, PII-free telemetry, or an identity result. Evidence:
`src/config.rs:15-232`, `src/env_config.rs:21-216`,
`src/principal.rs:18-287`, `src/redact.rs:13-32`.

<a id="r07"></a>
## R07 — Declared examples, tests, and bench

The manifest names examples `basic`, `multi_issuer`, and `rotation`; each requires `jwt-adapter,test-utils`. Its four explicit `[[test]]` stanzas name `prop_validate`, `adversarial`, `rotation`, and `http_fetcher`. `tests/mutation_kills.rs` is instead an implicit convention-discovered test source: it exists in the integration-test directory but has no `[[test]]` stanza in this manifest. The explicit `[[bench]]` stanza names Criterion bench `principal_id_clone`. These checked-in sources describe mocked/fake, adversarial, rotation, HTTP-fetcher, mutation, property, and principal-clone coverage areas. No target was run

and no test name is treated as an execution result. Evidence: `Cargo.toml:60-115`, `examples/*.rs`, `tests/{prop_validate,adversarial,rotation,http_fetcher,mutation_kills}.rs`, `benches/principal_id_clone.rs`.

<a id="r08"></a>
## R08 — Explicit unknowns and limits

Unknown from this snapshot: feature selection/resolution; native/wasm
compatibility; all compilation and test/bench results; real `reqwest` or
Worker HTTP operation; live Clerk configuration, JWKS response, rotation,
token/claim behavior, and identity validation; cache durability, expiry,
coherence, and metrics export; environment contents and secret lifecycle;
complete reverse dependencies and caller compatibility; transport status
mapping; deployment, publication, runtime reachability, and cold review.

[Ownership guide](../../../../.claude/skills/own-corelink-clerk/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
