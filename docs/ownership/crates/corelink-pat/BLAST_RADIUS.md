---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-pat
manifest: crates/corelink-pat/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-pat-structural-normalization-20260921
---

# corelink-pat — blast radius

Static relation map for PAT primitives. Each relation is atomic and backed by
source or manifests; neither import nor dependency presence proves a request
path, key use, persistence, authorization, timing, deployment, or test result.

[Primitive flow](#b01) · [Local contracts](#b02) · [Direct consumers](#b03) · [Handler surfaces](#b04) · [Compatibility candidates](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Primitive flow relations

Record index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-014](#rel-014)

<a id="rel-001"></a>
### REL-001 — Mint composition

**Flow:** `mint` → random token id/random-secret bytes → HMAC preimage/signature
→ Argon2id `PatHash` → `PatPlaintext` and `Pat`.
**Impact:** changing a segment, entropy conversion, signature input, or hash
input can make local mint/parser/verify contracts disagree.
**Evidence:** `src/mint.rs`, `src/format.rs`, `src/argon.rs`, `src/sig.rs`.
**Unknown:** externally stored or delivered token compatibility. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Verify ordering

**Flow:** `verify_with_hash_multi` → parse → token-id constant-time comparison
→ multi-key HMAC verification → Argon2id verification.
**Impact:** changing a stage, input, or error result changes the local
verification contract and can affect callers using the root API.
**Evidence:** `src/verify.rs`, `src/format.rs`, `src/sig.rs`, `src/argon.rs`.
**Unknown:** whether a caller has already looked up an authorized stored hash. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Cold-path primitive

**Flow:** caller-selected plaintext → `dummy_verify_for_constant_time` → dummy
PHC selection → local `verify_argon2id` → `InvalidPat`.
**Impact:** changing this primitive can alter callers that explicitly use it for
their own padding strategy.
**Evidence:** `src/argon.rs`; static imports in container/worker PAT code.
**Unknown:** actual invocation completeness and observed response timing. [Relation index](#b03)

<a id="b02"></a>
## B02 — Local contract relations

<a id="rel-004"></a>

### REL-004 — Format to typed token id

**Dependency:** `format` → `types::PatTokenId` and `PatEnv`.
**Flow:** parser validates the source-defined segment and produces typed values.
**Impact:** changing token-id alphabet/length or environment vocabulary changes
parse and mint compatibility.
**Evidence:** `src/format.rs`, `src/types.rs`, `src/mint.rs`.
**Unknown:** all persisted or external token readers. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Signing key to HMAC result

**Dependency:** `sig` → `PatSigningKey` and signature-length constant.
**Flow:** the HMAC result is truncated to `PAT_HMAC_SIG_RAW_LEN`; multi-key
verification folds every supplied key comparison.
**Impact:** a key type, truncation, preimage, or fold change changes locally
accepted values.
**Evidence:** `src/sig.rs`, `src/types.rs`, `src/format.rs`.
**Unknown:** real key source, key identifiers, custody, and rotation policy. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Scope construction to names/membership

**Dependency:** `PatScopes` constructors → `SCOPE_KNOWN_MASK` → `has` and `names`.
**Flow:** constructors mask or reject reserved bits; membership and emitted names
operate on the retained mask.
**Impact:** a bit/value/name/mask change can alter local serialized diagnostics
or caller-visible set semantics.
**Evidence:** `src/scopes.rs`, `src/lib.rs` re-exports.
**Unknown:** actual scope-to-authorization mapping or durable scope storage. [Relation index](#b03)

<a id="b03"></a>
## B03 — Direct manifest consumer relations

<a id="rel-007"></a>

### REL-007 — Auth re-export edge

**Dependency:** `corelink-auth` → `corelink-pat`.
**Flow:** `crates/corelink-auth/src/pat.rs` re-exports `corelink_pat::*`.
**Impact:** a root public API change can affect that re-export surface.
**Evidence:** `crates/corelink-auth/Cargo.toml`, `crates/corelink-auth/src/pat.rs`.
**Unknown:** selected feature, downstream use, or runtime authentication path. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Corelink-server edge

**Dependency:** `corelink-server` (`crates/corelink-container/`) → `corelink-pat`.
**Flow:** static source imports PAT types/primitives in adapter and route files.
**Impact:** changing PAT API, grammar, or scope semantics can require
corelink-server source compatibility assessment.
**Evidence:** `crates/corelink-container/Cargo.toml`, `src/adapter_pat_verifier.rs`,
`src/native_pat_gate.rs`, and `src/routes/` PAT import sites.
**Unknown:** mounted handlers, configuration, stored rows, and request behavior. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Worker edge

**Dependency:** `corelink-worker` → optional/workspace `corelink-pat` entries.
**Flow:** middleware, revocation, and REAPI handler source import PAT values and
scope constants.
**Impact:** root API, scope, or error changes can require worker source
compatibility assessment.
**Evidence:** `crates/corelink-worker/Cargo.toml`, `src/middleware/auth.rs`,
`src/auth/revocation/`, `src/reapi/`.
**Unknown:** feature selection and handler execution. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — CLI edge

**Dependency:** `tools/cli` → `corelink-pat`.
**Flow:** CLI authentication source calls `corelink_pat::parse_plaintext`.
**Impact:** parser grammar/type changes can affect the static CLI consumer.
**Evidence:** `tools/cli/Cargo.toml`, `tools/cli/src/auth.rs`.
**Unknown:** command execution, configuration, or user token handling. [Relation index](#b03)

<a id="b04"></a>
## B04 — Handler and middleware source relations

<a id="rel-011"></a>

### REL-011 — Container adapter verification relation

**Flow:** container adapter source → PAT parsing/HMAC/Argon2id/dummy primitive.
**Impact:** a primitive contract change can require adaptation at the static
container verification boundary.
**Evidence:** `crates/corelink-container/src/adapter_pat_verifier.rs` and parts;
`src/native_pat_gate.rs`.
**Unknown:** route attachment, token lookup, authorization result, and latency. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Worker middleware and REAPI scope relation

**Flow:** worker auth-context/middleware and REAPI handler source → `PatScopes`
and PAT identity values.
**Impact:** a changed scope constant or PAT type can require static handler
compatibility review.
**Evidence:** `crates/corelink-worker/src/middleware/`, `src/reapi/ac/`, and
`src/reapi/cas/` import sites.
**Unknown:** endpoint reachability and authorization enforcement. [Relation index](#b03)

<a id="b05"></a>
## B05 — Declared test-target relation

<a id="rel-014"></a>

### REL-014 — Declared property and adversarial target relation

**Dependency:** parser/verification change → property, adversarial, canonical,
and constant-time target assessment.
**Flow:** the PAT manifest declares four named test targets: `prop_pat`,
`adversarial`, `canonical_vectors`, and `constant_time`.
**Impact:** changed acceptance grammar or error behavior needs target selection
before coverage is claimed.
**Evidence:** `crates/corelink-pat/Cargo.toml`, `crates/corelink-pat/tests/`.
**Unknown:** target execution and measured results. Unrelated workspace fuzz
packages are not asserted as PAT consumers. [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and closure limit

This map covers eight source modules, four direct manifest consumers, direct
container/worker/CLI/auth source relations, and PAT's four declared test
targets. It does not prove a complete Cargo graph, features, targets, consumers,
token store, key custody, timing, authorization wiring, runtime behavior,
deployment, execution, or review. Obtain fresh evidence before making a
compatibility or security claim.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
