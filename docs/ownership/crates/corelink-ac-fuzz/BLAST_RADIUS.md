---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-ac-fuzz
manifest: crates/corelink-ac/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
integration_baseline: 10cce309b9fc642d357ea9cd7a30a2fcde0a2758
profile: S
state: draft
evidence_set: ac-fuzz-source-20260921
---

# corelink-ac-fuzz — blast radius

The package is one standalone HKDF property harness. It has no Cargo path edge
to `corelink-ac`, but its oracle is semantically compared with that production
contract and is selected by two repository workflow surfaces. No relation below
proves a build, runtime call, deployment, or provider operation.

[Scope](#b01) · [Method](#b02) · [Direct relations](#b03) ·
[Propagation](#b04) · [Change impact](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and priority risks

Highest risks are weakening the no-panic/admission oracle, confusing generalized
HKDF properties with fixed production `b"ac-sig"`/32-byte behavior, changing
the root/standalone workspace boundary, or changing CI time/runner/artifact
policy without evidence. The harness does not load TDKs, call `corelink-ac`,
invoke workers, or touch R2.

**Source pin:** `1177dad2ca2a9f21c29b5a118aa7944b77147798`.
**Integration baseline:** `10cce309b9fc642d357ea9cd7a30a2fcde0a2758`.
No Cargo resolution, build, fuzz run, workflow dispatch, network, or runtime
observation was performed.

<a id="b02"></a>
## B02 — Census and method

| Population | Static selection | Found / documented | Limit |
|---|---|---|---|
| Package identity | `Cargo.toml:1-12,29-34` at pin; root exclude | 1 package, 1 target, 1 nested workspace | No `cargo metadata` |
| Direct dependencies | Fuzz manifest `Cargo.toml:24-27` | 3 declarations: `libfuzzer-sys`, `hkdf`, `sha2` | Resolved versions/features and inverse graph unknown |
| Production contract | `corelink-ac/src/ac_core/sig.rs`, signer/error modules, ADR-0021 | HKDF salt/info/output, TDK/key-ID, typed error boundaries | Harness does not call production code |
| Contract test population | `corelink-ac/tests/ac_core_*_sig.rs` | Canonical, rotation, property, timing suites declared in manifest and source | No test execution or selected-feature proof |
| Workflow selectors | `fuzz-nightly.yml:86-93`, `scripts/fuzz-all.sh:38-44` | 2 explicit `corelink-ac:hkdf_expand` selectors | Declarations are not run evidence |
| Semantic consumers | Pinned literal search for `HkdfSigner`, public `corelink_ac::sig`, target name, AC event names, and audit selectors | AC tests; worker bridge; CAS manifest signer; AC audit taxonomy; sealed S-04 contract | Dynamic/generated links and historical run state unknown |

`corelink-ac` owns the production contract; this is an independent harness.
Re-export or semantic reference does not transfer ownership. The out-of-Cargo
census is literal-bounded and cannot prove generated names or runtime dispatch.
The consumer endpoint is `corelink_ac::sig`, implemented under private
`ac_core/sig`. The current `corelink-ac` peer document has no reciprocal stable
relation, so endpoint/owner reconciliation is **UNRESOLVED**. This candidate
must not claim a shared fingerprint or reconciled peer fact.

<a id="b03"></a>
## B03 — Direct relationships

Local IDs are anchors. Cross-package identity uses the qualified form
`repo:1232040291:boundary:ac-fuzz-<name>-001` only where a reciprocal peer
record exists. REL-006, REL-015, and REL-018 have no such records and remain
candidate-local with cross-package identity **UNRESOLVED**. Dependency
direction is consumer → provider; data and impact directions are stated
separately.

| ID | Type / boundary | Dependency direction | Data direction | Impact direction |
|---|---|---|---|---|
| [REL-001](#rel-001) | fuzz engine | fuzz → libfuzzer-sys | runner bytes → closure | engine/macro → target execution |
| [REL-002](#rel-002) | HKDF primitive | fuzz → hkdf | IKM/salt/info → expand | API/algorithm → all oracle branches |
| [REL-003](#rel-003) | SHA-256 digest | fuzz → sha2 | `Sha256` type → HKDF | digest implementation → derived bytes |
| [REL-004](#rel-004) | workspace selection | root excludes; nested workspace owns fuzz | manifest → resolver scope | workspace change → target selection |
| [REL-005](#rel-005) | production HKDF contract | harness oracle ↔ corelink-ac contract (not Cargo) | generalized bytes → property comparison | contract drift → false confidence or findings |
| [REL-006](#rel-006) | public signer contract | worker/CAS callers → `corelink_ac::sig` | canonical bytes + key ID → signature/verification result | public path/signature/error changes → callers |
| [REL-007](#rel-007) | worker adapter | worker → corelink-ac | envelope → sign/verify bridge | production contract → worker result path |
| [REL-008](#rel-008) | CAS manifest signer | CAS → corelink-ac | TDK/key ID → manifest signature domain | API/domain change → CAS tests/verification |
| [REL-009](#rel-009) | fuzz-all selector | script → target | generated input → local job | list/command change → coverage/budget |
| [REL-010](#rel-010) | nightly target selector | workflow matrix → target | `crate,target` pair → selected job | matrix/list change → target coverage |
| [REL-011](#rel-011) | AC contract tests | tests → corelink-ac | vectors/properties → assertions | signer/oracle change → test outcomes |
| [REL-012](#rel-012) | signer HKDF primitive | corelink-ac → hkdf/sha2 | TDK + key ID → derived key | primitive/version change → signature bytes |
| [REL-013](#rel-013) | TDK/key-ID boundary | signer → `TdkHandle`/key policy | tenant + key ID → TDK lookup/salt | provider/rotation change → sign/verify errors |
| [REL-014](#rel-014) | keyed-MAC output | signer → BLAKE3/subtle | derived key + canonical bytes → tag/compare | MAC/compare change → acceptance/timing |
| [REL-015](#rel-015) | signature error adapter | `corelink_ac::sig` → worker-local `SigError` | canonical error → local variant/message | mapping change → worker error behavior |
| [REL-016](#rel-016) | nightly environment isolation | workflow → runner env | run ID/crate/target → `CARGO_HOME`/`CARGO_TARGET_DIR` | shared-state change → contamination risk |
| [REL-017](#rel-017) | corpus/artifact lifecycle | workflow → corpus/artifact stores | target → cached corpus/crash artifacts | retention/path change → recovery evidence |
| [REL-018](#rel-018) | handler audit emission | worker handler → `AcAuditRecord` | event type + handler literal → audit fields | mapping/order change → outbox/alerts |

<a id="rel-001"></a>
### REL-001 — LibFuzzer entry
**Identity:** `repo:1232040291:boundary:ac-fuzz-libfuzzer-001`.
**Dependency/data/impact:** fuzz→libfuzzer-sys; runner bytes→`fuzz_target!`;
macro/runner changes→closure invocation. **Surface/activation:** manifest
`0.4`, target macro. **Contract:** arbitrary bytes either return or reach
assertions. **Failure/containment:** panic is a finding in the current run.
**Validation:** manifest/target read; no runner. **Evidence:** `S01–S02`. [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — HKDF primitive
**Identity:** `repo:1232040291:boundary:ac-fuzz-hkdf-001`.
**Dependency/data/impact:** fuzz→hkdf; slices→Extract/Expand; API or algorithm
change→length, determinism, and salt assertions. **Surface:** `Hkdf::<Sha256>`
and `expand`; all admitted inputs. **Failure:** `Err` returns, assertion fails,
or expect panics. **Validation:** source only. **Coordination:** resolved
registry version/feature set is unknown. **Evidence:** `S02`. [Relation index](#b03)


<a id="rel-003"></a>
### REL-003 — SHA-256 selection
**Identity:** `repo:1232040291:boundary:ac-fuzz-sha2-001`.
**Dependency/data/impact:** fuzz→sha2; `Sha256` type→HKDF generic; digest
selection changes→all derived outputs. **Surface:** manifest `0.10` and target
type parameter. **Failure:** build/behavior may change. **Validation:** static
read; no Cargo resolution. **Evidence:** `S01–S02`. [Relation index](#b03)


<a id="rel-004"></a>
### REL-004 — Independent workspace boundary
**Identity:** `repo:1232040291:boundary:ac-fuzz-workspace-001`.
**Dependency/data/impact:** root exclusion + nested workspace→standalone
resolver; manifest→Cargo scope; workspace edits→selection/build semantics.
**Surface:** root `exclude` and fuzz `[workspace]`. **Failure:** accidental
outer-workspace inclusion or omitted target. **Validation:** manifest read only.
**Evidence:** `S01`, `S07`. [Relation index](#b03)


<a id="rel-005"></a>
### REL-005 — Generalized oracle versus production contract
**Identity:** `repo:1232040291:boundary:ac-fuzz-contract-001`.
**Dependency/data/impact:** harness oracle↔`corelink-ac` contract; random
IKM/salt/info/L→properties; production drift→false confidence or finding.
**Surface:** `HKDF_INFO_AC_SIG`, `derive_sig_key`, ADR-0021. **Boundary:** no
Cargo/runtime call from fuzz to production. **Validation:** compare source and
fixed vectors in authorized gates. **Evidence:** `S03–S04`, ADR-0021. [Relation index](#b03)


<a id="rel-006"></a>
### REL-006 — Public signer/verifier contract
**Identity:** candidate-local relation only; the `corelink-ac` peer has no
reciprocal stable relation in its current draft, so cross-package identity is
**UNRESOLVED**.
**Endpoints/direction:** worker/CAS callers → public `corelink_ac::sig`; source
implementation is private `ac_core/sig`. **Data/impact:** canonical bytes,
tenant, and key ID → signature/result; public path, trait, or `SigError` change
→ caller compatibility/mapping. **Failure:** source break or changed mapping;
no fuzz edge reaches this API. **Validation:** reconcile peer records and run
authorized package tests; no run here. **Evidence:** `S03–S04`, `S09–S10`. [Relation index](#b03)


<a id="rel-007"></a>
### REL-007 — Worker AC bridge
**Identity:** `repo:1232040291:boundary:ac-fuzz-worker-bridge-001`.
**Dependency/data/impact:** worker→corelink-ac; envelope bytes→sign/verify
adapter; API/domain change→worker AC result path. **Surface:**
`crates/corelink-worker/src/reapi/ac/sig.rs`. **Failure:** worker maps canonical
errors; no fuzz edge reaches it. **Validation:** source/test gate requires an
authorized package owner. **Evidence:** `S04`, worker source blob
`d8f4d277975f37c17ec22e01e8f74cc204b7d633`. [Relation index](#b03)


<a id="rel-008"></a>
### REL-008 — CAS manifest signer
**Identity:** `repo:1232040291:boundary:ac-fuzz-cas-signer-001`.
**Dependency/data/impact:** CAS→corelink-ac; manifest TDK/key ID→sibling
signature domain; corelink-ac API/domain changes→CAS verification/tests.
**Surface:** `crates/corelink-cas/src/manifest/sig.rs` and tests. **Failure:**
no fuzz invocation; manifest uses distinct info. **Validation:** CAS test/build
when authorized. **Evidence:** `S04`, blob `77f9139e891a6e7d586f64b26d08afcc36544d71`. [Relation index](#b03)


<a id="rel-009"></a>
### REL-009 — Local fuzz-all selection
**Identity:** `repo:1232040291:boundary:ac-fuzz-script-001`.
**Dependency/data/impact:** script→`corelink-ac:hkdf_expand`; bytes→local fuzz
run; list/duration/toolchain changes→coverage and resource use. **Surface:**
`scripts/fuzz-all.sh`. **Failure:** nonzero target marks the batch failed.
**Validation:** shell review; no execution. **Evidence:** `S06`. [Relation index](#b03)


<a id="rel-010"></a>
### REL-010 — Nightly target selector
**Identity:** `repo:1232040291:boundary:ac-fuzz-nightly-selector-001`.
**Dependency/data/impact:** workflow matrix→`corelink-ac:hkdf_expand`; matrix
`crate,target`→selected job; matrix/list edits→target coverage. **Surface:**
`.github/workflows/fuzz-nightly.yml:86-93`. **Failure:** a stale pair can skip
the target or select the wrong package; YAML is not run evidence. **Validation:**
compare the matrix pair with `scripts/fuzz-all.sh`; dispatch outcome unknown.
**Evidence:** `S05–S06`. [Relation index](#b03)


<a id="rel-011"></a>
### REL-011 — AC production contract tests
**Identity:** `repo:1232040291:boundary:ac-fuzz-contract-tests-001`.
**Dependency/data/impact:** AC tests→`corelink-ac`; vectors/properties→signer
assertions; production or domain drift→test results. **Surface:**
`ac_core_canonical_vectors_sig`, `ac_core_prop_sig`, key-rotation/timing tests.
**Failure:** tests may fail; no run here. **Validation:** authorized corelink-ac
test gate, not this harness. **Evidence:** `S03–S04` and pinned test tree. [Relation index](#b03)


<a id="rel-012"></a>
### REL-012 — Signer HKDF primitive
**Identity:** `repo:1232040291:boundary:ac-signature-hkdf-001`.
**Dependency/data/impact:** `corelink-ac::sig`→`hkdf`/`sha2`; TDK and
`sig_key_id.to_le_bytes()`→HKDF output; version/algorithm/feature changes→
signature bytes and canonical vectors. **Surface:** `Hkdf::<Sha256>`,
`HKDF_INFO_AC_SIG`, fixed 32-byte expansion. **Failure:** typed derivation error
or incompatible signature; this harness uses a separate registry edge. **Validation:**
manifest/source/test reconciliation and authorized vector tests. **Evidence:**
`S03–S04`, `S09–S10`. [Relation index](#b03)


<a id="rel-013"></a>
### REL-013 — TDK and key-ID boundary
**Identity:** `repo:1232040291:boundary:ac-signature-tdk-001`.
**Dependency/data/impact:** signer→`TdkHandle` and accepted-key policy; tenant
and key ID→TDK lookup plus salt; provider/rotation changes→reserved/unknown/
backend errors and verification compatibility. **Surface:** `TdkHandle`,
`accepted_key_ids`, `RESERVED_SIG_KEY_ID`. **Failure:** missing provider or
rotation mismatch; no provider is reached by this package. **Validation:**
key-rotation tests and owner review, not run here. **Evidence:** `S04`, `S09–S10`. [Relation index](#b03)


<a id="rel-014"></a>
### REL-014 — Keyed-MAC and constant-time compare
**Identity:** `repo:1232040291:boundary:ac-signature-mac-001`.
**Dependency/data/impact:** signer→BLAKE3 keyed MAC and `subtle` compare;
derived key + canonical bytes→32-byte tag/verification result; MAC or compare
changes→acceptance and timing properties. **Surface:** `keyed_mac_with_info`,
`compute_signature`, `SignatureVerifier::verify`. **Failure:** invalid signature
or timing/compatibility regression; no fuzz path invokes it. **Validation:**
canonical/property/timing suites in an authorized environment. **Evidence:**
`S04`, `S09–S10`. [Relation index](#b03)


<a id="rel-015"></a>
### REL-015 — Canonical signature error adapter
**Identity:** `repo:1232040291:boundary:ac-worker-sig-adapter-001` (candidate-local;
no reciprocal worker record). **Endpoint/direction:** public
`corelink_ac::sig::SigError` → worker-local `reapi::ac::sig::SigError`.
**Mapping:** `Invalid`→`Mismatch`, `KeyIdReserved` stays reserved, and other
canonical variants → `Backend` messages retaining code prefixes. **Impact:**
adapter drift changes worker error behavior; no direct audit-record write.
**Failure:** lost variant/context or unknown-variant fallback. **Validation:**
adapter tests and source review; no run. **Evidence:** `S11`. [Relation index](#b03)


<a id="rel-018"></a>
### REL-018 — Handler audit emission
**Identity:** `repo:1232040291:boundary:ac-worker-audit-001` (candidate-local;
no reciprocal worker record). **Endpoint/direction:** worker AC handler →
`AcAuditRecord`; `AcEventType` → `event_type`, handler literals
`canonical_drift`/`sig_mismatch`/`sig_sign_failed` → `reason`. **Impact:** field
or ordering drift → outbox, SIEM, dashboard, and alert consumers. **Failure:**
reserved-key update is intentionally not emitted; this harness emits nothing.
**Validation:** handler/audit tests and sealed-review coordination; no run.
**Evidence:** `S11`. [Relation index](#b03)


<a id="rel-016"></a>
### REL-016 — Nightly environment isolation
**Identity:** `repo:1232040291:boundary:ac-fuzz-nightly-env-001`.
**Dependency/data/impact:** workflow→self-hosted runner environment; run ID,
crate, and target→per-leg `CARGO_HOME`/`CARGO_TARGET_DIR`; isolation edits→
cross-run registry/build contamination risk. **Surface:** workflow lines
114–121 and nightly toolchain env. **Failure:** shared state can corrupt or
confound concurrent jobs. **Validation:** workflow review; no dispatch. **Evidence:**
`S05`. [Relation index](#b03)


<a id="rel-017"></a>
### REL-017 — Corpus and artifact lifecycle
**Identity:** `repo:1232040291:boundary:ac-fuzz-nightly-artifacts-001`.
**Dependency/data/impact:** workflow→target corpus/artifact stores; target→
cached corpus and failure artifacts; path/key/retention edits→reproducibility
and recovery evidence. **Surface:** workflow lines 158–184; failure artifacts
retain 14 days. **Failure:** missing or expired evidence blocks triage.
**Validation:** inspect paths/keys/retention and read back run artifacts; no run.
**Evidence:** `S05`. [Relation index](#b03)


<a id="b04"></a>
## B04 — Transitive propagation

| Destination | Witness path | Condition | Causal effect | Containment / validation |
|---|---|---|---|---|
| Harness oracle | `REL-001–003` | Target receives complete input | Registry primitive/API changes alter derived bytes or assertions | Isolated target run with bounds; no run observed |
| Production AC signer | `REL-005–006`, `REL-012–014` | Production HKDF, key, MAC, or public contract changes | Harness may miss or report generalized primitive drift; production sign/verify changes separately | Compare fixed contract and run corelink-ac tests |
| Worker/CAS consumers | `REL-006–008`, `REL-015`, `REL-018` | Public `corelink_ac::sig`, adapter, or audit mapping changes | Worker and manifest verification/build/audit paths can fail or change | Reconcile inverse consumers and package-specific tests |
| Fuzz job resources | `REL-009–010`, `REL-016–017` | Script/workflow trigger or isolation/retention change | Runner time, corpus, artifacts, and failures change | Inspect run-specific evidence; do not infer from YAML |

No path establishes a fuzz→TDK/KMS/R2/production edge. Re-export, manifest
dependency, workspace presence, or dashboard text is not runtime proof.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | Records | Impact | Validation | Coordination / recovery |
|---|---|---|---|---|
| Input parser or assertion | API-001/002, INV-001–004, REL-001/002/005 | Findings and false-negative/positive profile | Bounded target run plus source review | Preserve old corpus/finding bytes; independent cold review |
| HKDF/SHA dependency or feature | REL-002/003/012 | Derived output and production comparison | Manifest/resolution review, target gate, corelink-ac contract tests | No unbounded install; revert task-owned change only |
| Root/standalone workspace | REL-004 | Package selection and lock/resolution boundary | Compare manifests and resolved graph in authorized environment | Restore exact exclusion and record lockfile state |
| Production `ac-sig`/TDK/key rotation | REL-006–008/011–015/018 | Sign/verify, adapter, audit compatibility and consumers | Corelink-ac, worker, CAS tests and contract review | Coordinate canonical owner; fuzz alone cannot approve |
| Script/workflow/time/cache | REL-009/010, REL-016/017 | Runner load, isolation, corpus, artifact retention | YAML/script review and one authorized run if needed | Preserve run ID/artifacts; operator owns cancellation/recovery |

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Discovered | Documented | Excluded / reason | Unknown |
|---|---:|---:|---|---|
| Package targets | 1 | 1 | 0 | Cargo-expanded selection |
| Direct declarations | 3 | 3 | 0 | Resolved versions/features/inverse graph |
| Atomic relations | 18 | 18 | No direct Cargo edge to production; REL-006/015/018 cross-package identities and peer reciprocity are **UNRESOLVED** | Owner reconciliation, selected features, and runtime reachability |
| Workflow selectors | 2 | 2 | Historical text/report paths are not execution evidence | Run history/corpus/artifacts |
| Production/provider paths | 0 observed | 0 | Harness has no such code | TDK/KMS, deployment, R2, telemetry |

Static search covered AC tests, worker bridge, CAS signer, AC audit taxonomy,
and sealed S-04. Public endpoint facts are source-backed, but reciprocal peer
relation fingerprints/owner records are unresolved. Generated names, dynamic
dispatch, external repositories, remote state, and runtime calls remain
unknown. No Cargo or runtime operation occurred. Four cold reviews remain
required; draft only.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01)
