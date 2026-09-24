---
schema: corelink-ownership/1.1
document: reference
package: corelink-pat
manifest: crates/corelink-pat/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-pat-structural-normalization-20260921
---

# corelink-pat — ownership reference

Static-source reference for PAT primitives. It records source-visible contracts
and declared test targets; it does not prove configuration, signing-key custody,
token-store persistence, timing, authorization wiring, deployment, test
execution, or review.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Invariants](#r05) · [Tests](#r06) · [Failures](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Identity and static scope

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005)

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-pat` / `crates/corelink-pat/Cargo.toml` |
| Declared role | Canonical hybrid PAT format plus Argon2id, HMAC-SHA256, scopes, and verification primitives |
| Source modules | `argon`, `error`, `format`, `mint`, `scopes`, `sig`, `types`, `verify` |
| Declared examples | `mint_and_verify`, `scope_check` |
| Declared tests | `prop_pat`, `adversarial`, `canonical_vectors`, `constant_time` |

The manifest and `src/lib.rs` are the identity evidence. A declared target is
not an execution result.

<a id="r02"></a>
## R02 — Ownership boundary

The crate owns format parsing/composition, locally exposed PAT values, Argon2id
PHC hashing and verification, HMAC computation/verification, scope-bitset
operations, and local verification orchestration. `lib.rs` exposes the eight
modules and their root re-exports.

It does not own loading or rotating a real signing key, persistence/lookup of a
`Pat` row, caller-controlled expiration/revocation decisions, response mapping,
or whether a handler invokes this crate. No secret value is inspected or
recorded by this reference.

<a id="r03"></a>
## R03 — Source map

| Path | Source-visible responsibility |
|---|---|
| `src/argon.rs` | Fixed-parameter Argon2id hasher, PHC verification, and dummy verification primitive |
| `src/error.rs` | `PatError` taxonomy |
| `src/format.rs` | Canonical plaintext segments, parser, constants, and environment parsing |
| `src/mint.rs` | Entropy-backed mint and test-only deterministic mint inputs |
| `src/scopes.rs` | `u64` scope mask, membership, set operations, and canonical names |
| `src/sig.rs` | Truncated HMAC-SHA256 computation and overlap-key-set verification |
| `src/types.rs` | PAT newtypes, environments, IDs, signing-key wrapper, and row-shaped `Pat` |
| `src/verify.rs` | Parse/HMAC/Argon2id orchestration and HMAC-only entry points |

<a id="r04"></a>
## R04 — Public contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006).

<a id="api-001"></a>
### API-001 — Canonical plaintext grammar

`parse_plaintext` accepts the source-defined
`corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` grammar. The environment
set is `pat`, `ci`, or `ro`; the source constants fix a 16-character Crockford
token id, 43-character base64url-without-padding random-secret segment, and
22-character base64url-without-padding signature segment. `mint` composes the
same grammar. Source: `format.rs`, `mint.rs`, `types.rs`.

<a id="api-002"></a>
[↩](#r01)
### API-002 — Minted primitive tuple

`mint` accepts environment, tenant/principal IDs, `PatScopes`, optional TTL,
`PatSigningKey`, and signing-key id, and returns `(PatPlaintext, Pat)` or
`PatError`. Source builds a token id and random-secret bytes with `OsRng`,
computes the HMAC segment, hashes the random-secret text, and fills the local
`Pat` value. `mint_with_entropy` is marked hidden/test-only. Source: `mint.rs`.

<a id="api-003"></a>
[↩](#r01)
### API-003 — Argon2id PHC proof

`hash_random_secret` emits a `PatHash` PHC string using source constants
`m=65536 KiB`, `t=3`, `p=4`, and output length 32. `verify_argon2id` parses the
PHC value, rejects missing/under-floor `m`, `t`, or `p` values and a non-Argon2id
algorithm, then verifies the supplied random-secret text. Source: `argon.rs`.
Whether stored values are persisted or supplied from an authorized store is
unknown.

<a id="api-004"></a>
[↩](#r01)
### API-004 — HMAC fast-fail and local orchestration

`compute_hmac_sig` produces the first 16 bytes of HMAC-SHA256 over the supplied
preimage. `verify_hmac_sig_multi` requires a 16-byte supplied signature, rejects
an empty key set, and folds comparisons across every supplied key. The
`verify_with_hash_multi` source sequence is parse → constant-time token-id
comparison → HMAC verification → Argon2id verification. `verify_hmac_only_multi`
stops after parse and HMAC. Source: `sig.rs`, `verify.rs`.

<a id="api-005"></a>
[↩](#r01)
### API-005 — Types, exposure, and errors

`PatPlaintext` has an explicit consuming `into_string`; its `Debug` output is
redacted and its drop implementation calls `zeroize`. `PatSigningKey::from_bytes`
rejects inputs shorter than 32 bytes and its debug output is redacted.
`PatHash`, identifiers, `PatEnv`, and `Pat` are local public types. `PatError`
has `Malformed`, `InvalidPat`, `HashError`, `EntropyUnavailable`, and
`SigningKeyTooShort`. Source: `types.rs`, `error.rs`.

<a id="api-006"></a>
[↩](#r01)
### API-006 — Scope bitset

`PatScopes` stores a `u64`; `from_u64` masks reserved bits and
`from_u64_strict` returns `None` when any reserved bit is present. `has` masks
the requested bitset before testing containment, while `names` emits the
source-defined canonical labels in bit order. Source: `scopes.rs`. Actual
authorization interpretation and persistence are outside this contract.
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable invariants

<a id="inv-001"></a>

### INV-001 — Mint/parser grammar agreement

For a successful local `mint`, parsing the consumed plaintext must yield the
same environment and token id held by its returned `Pat`; a changed separator,
length, alphabet, or composition is falsifying evidence. Source: `mint.rs`,
`format.rs`, declared `prop_pat` and `canonical_vectors` targets.

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Underprovisioned or non-Argon2id PHC rejection

`verify_argon2id` rejects PHC inputs with absent or below-constant `m`, `t`, or
`p`, and with an algorithm other than `argon2id`; a source path that accepts one
is falsifying evidence. Source: `argon.rs`, declared `adversarial` and
`canonical_vectors` targets.

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — HMAC acceptance requires a supplied match

For a 16-byte signature and nonempty key slice, `verify_hmac_sig_multi` accepts
only if its accumulated constant-time comparison has a match; an empty key set
returns `InvalidPat`. A changed fold or early acceptance without a match
falsifies this invariant. Source: `sig.rs` and its local tests.

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Scope import cannot retain reserved bits

`PatScopes::from_u64(raw).to_u64()` equals `raw & SCOPE_KNOWN_MASK`; strict
construction rejects a raw value with a reserved bit. A retained reserved bit
or accepted strict input falsifies this invariant. Source: `scopes.rs`, declared
`prop_pat` target.

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Dummy primitive always reports invalid

`dummy_verify_for_constant_time` calls local Argon2id verification against its
dummy PHC selection and then returns `Err(PatError::InvalidPat)`. A successful
return falsifies this local API invariant. This is not a measured timing claim.
Source: `argon.rs`, declared `constant_time` and `adversarial` targets.
[↩](#r01)

<a id="r06"></a>
## R06 — Declared examples and test targets

The manifest declares `examples/mint_and_verify.rs` and `examples/scope_check.rs`.
It also declares test targets `tests/prop_pat.rs`, `tests/adversarial.rs`,
`tests/canonical_vectors.rs`, and `tests/constant_time.rs`; nearby test sources
also include `mutation_kills.rs` and `emit_e2e_seed.rs`. These names and static
contents identify intended property, adversarial, canonical-vector, and
constant-time coverage only. No target was run for this artifact.

<a id="r07"></a>
## R07 — Failure and boundary table

| Surface | Source-visible outcome | Not established |
|---|---|---|
| Parse | Noncanonical input returns `Malformed` | Caller/wire behavior or timing |
| HMAC | Wrong-length signature is `Malformed`; no match/empty keys is invalid | Key provisioning or rotation operation |
| Argon2id | Mismatch is invalid; malformed/under-floor PHC is `HashError` | Stored-row integrity or error mapping |
| Mint | Entropy failure returns `EntropyUnavailable` | Entropy-service operation or issuance policy |
| Scopes | Reserved bits are masked/rejected by the selected constructor | Authorization decision or durable representation |

<a id="r08"></a>
## R08 — Evidence limits and unknowns

Static manifest consumers are `corelink-auth`, `corelink-server`
(`crates/corelink-container/Cargo.toml`), `corelink-worker`, and `tools/cli`;
source imports/re-exports were inspected for auth (including
`crates/corelink-auth/src/pat.rs`), container handlers, worker middleware/REAPI
handlers, and CLI parsing. PAT's own declared test targets are source-visible;
unrelated workspace fuzz packages are not asserted as PAT consumers. The
complete reverse graph, selected features/targets, and all transitive consumers
are unknown.

Configuration, real signing-key custody/rotation, token-store persistence and
lookup, actual constant-time response behavior, actual authorization wiring,
runtime reachability, deployment, test execution, and cold review are unknown.
No real secret was used, handled, or recorded.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
