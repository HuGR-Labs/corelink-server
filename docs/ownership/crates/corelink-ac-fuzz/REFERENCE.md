---
schema: corelink-ownership/1.1
document: reference
package: corelink-ac-fuzz
manifest: crates/corelink-ac/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
integration_baseline: 10cce309b9fc642d357ea9cd7a30a2fcde0a2758
profile: S
state: draft
evidence_set: ac-fuzz-source-20260921
---

# corelink-ac-fuzz — ownership reference

[Identity](#r01) · [Boundaries](#r02) · [Implementation](#r03) ·
[Contracts](#r04) · [State/invariants](#r05) · [Targets](#r06) ·
[Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and role

`corelink-ac-fuzz` is an independent, unpublished Cargo package with one
`cargo-fuzz` binary. It tests HKDF-SHA256 properties using direct registry
dependencies; it does not depend on `corelink-ac` by path and does not invoke a
TDK provider, worker, R2, KMS, or production handler. The source is pinned at
`1177dad2ca2a9f21c29b5a118aa7944b77147798`; no Cargo command or fuzz run was
performed.

| Field | Verified value |
|---|---|
| Package / manifest | `corelink-ac-fuzz` / `crates/corelink-ac/fuzz/Cargo.toml` |
| Target | `hkdf_expand` → `fuzz_targets/hkdf_expand.rs` |
| Metadata | `0.0.0`, edition 2021, `publish=false`, `UNLICENSED`, `cargo-fuzz=true` |
| Workspace | Root excludes this path; nested manifest declares its own `[workspace]` |
| Direct dependencies | `libfuzzer-sys 0.4`, `hkdf 0.12`, `sha2 0.10`; all registry declarations |
| Runtime / production reachability | Unknown; not built, run, deployed, or observed |

<a id="r02"></a>
## R02 — Boundaries and ownership

| Surface | Implementation owner | Contract owner | Composition / operation / review |
|---|---|---|---|
| Input decoder and assertions | This fuzz target | Harness oracle only | LibFuzzer selects bytes; operator unknown |
| HKDF production contract | Not implemented here | Public `corelink_ac::sig` (private implementation module `ac_core`) and ADR-0021 | Worker/CAS consumers compose the production API; `corelink-ac` is implementation/contract owner, while worker/CAS are composition owners; reviewer/operator assignment unknown |
| Workflow selection | YAML/script configuration | Repository fuzz process | `fuzz-nightly.yml` / `fuzz-all.sh`; execution is unverified |
| Acceptance | Not the author | Four independent cold reviewers required | No reviewer identity or approval is claimed |

The harness generalizes production inputs: it varies one-byte IKM/salt/info
lengths and arbitrary `info`, while production derives a 32-byte key from a TDK,
`sig_key_id.to_le_bytes()`, and fixed `b"ac-sig"`. A matching HKDF primitive is
not an end-to-end signer/verifier proof. The [CAS/AC OKF cluster](../../../knowledge/crates/cas-ac-core.md)
and [ADR-0021](../../../knowledge/adr/adr-0021-hkdf-vs-ed25519-ac-signing.md)
are canonical references, not duplicated policy.

<a id="r03"></a>
## R03 — Implementation map

| Unit | Behavior | Boundary | Evidence |
|---|---|---|---|
| `Cargo.toml` | Defines an independent fuzz package, lints, 3 registry dependencies, and one disabled-test/doc/bench bin | Build/package metadata | `S01` |
| `hkdf_expand.rs` | Decodes `[ikm_len, salt_len, info_len, L]`, slices input after a saturating length check, derives twice, and checks properties | One LibFuzzer closure; no external effect | `S02` |
| `corelink_ac::sig::{HkdfSigner,HkdfVerifier}` | Production `HKDF-Extract/Expand` + BLAKE3 keyed MAC with fixed `ac-sig` info and TDK/key-id policy | Public contract is re-exported by `corelink-ac`; private implementation is under `ac_core/sig` and is not a dependency of this package | `S03–S04` |
| Workflow/script entries | Select `corelink-ac:hkdf_expand` from the parent crate directory with bounded time | Configuration, not execution | `S05–S06` |

The target has no library, build script, tests, examples, FFI, telemetry, SQL,
schema, or application route. `#![no_main]` and `fuzz_target!` are harness entry
mechanics. The manifest denies `panic`, `unwrap_used`, and `indexing_slicing`
for the package, while this target intentionally uses assertions/`expect` as
finding boundaries. That is a declared policy conflict to resolve in an
authorized Cargo/clippy check; no lint/build result was observed and this
document does not call it clean.

<a id="r04"></a>
## R04 — Fuzz oracle contracts

These are harness contracts, not public production APIs. A LibFuzzer panic or
assertion is a finding against this local primitive composition.

**Contract index:** [API-001](#api-001) · [API-002](#api-002) ·
[API-003](#api-003) · [API-004](#api-004).

<a id="api-001"></a>
### API-001 — Input admission and HKDF invocation

**Receiver/signature:** the `fuzz_target!` closure receives `data: &[u8]`; it
constructs `Hkdf::<Sha256>::new(Option<&[u8]>, &[u8])` and calls
`expand(&self, &[u8], &mut [u8]) -> Result<(), InvalidLength>`.
**Precondition:** four header bytes plus all declared slices; `L` is 0..=255.
**Postcondition:** short input returns; otherwise expand returns `Ok` or `Err`.
**Effects/errors:** no external effect; assertion/panic is the finding boundary.
**Compatibility:** this input layout is harness-local, not the production wire
format, so changing it invalidates corpus interpretation. **Links:** INV-001/002;
REL-001/002. **Evidence:** `S02`.

[Contract index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Deterministic length and salt-sensitivity oracle

**Receiver/signature:** the closure owns `out1/out2/out3: Vec<u8>`;
`expand` returns `Result<(), InvalidLength>`; assertions return `()` or panic.
**Symbols:** output-length and repeated-output equality assertions plus the
conditional flipped-salt inequality assertion. **Precondition:** API-001 and a
successful first expand; salt sensitivity additionally needs non-empty salt and
`L >= 16`. **Postcondition:** requested length, determinism, and conditional
salt sensitivity hold. **Effects/errors:** assertion failure is local; no audit,
storage, network, or signer effect. **Compatibility:** this does not promise
32-byte `ac-sig`. **Links:** INV-002–004; REL-002/005. **Evidence:** `S02–S04`.

[Contract index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Public production signer/verifier boundary

**Sign:** `corelink_ac::sig::HkdfSigner::sign(&self, Uuid, u32, &[u8])
-> Result<[u8; AC_ENVELOPE_SIG_LEN], SigError>`. **Verify:**
`HkdfVerifier::verify(&self, Uuid, u32, &[u8], &[u8]) -> Result<(), SigError>`.
**Preconditions:** sign requires nonzero ID and TDK fetch; no current-ID
equality check. Verify requires a 32-byte signature, nonzero ID in
`accepted_key_ids`, and TDK fetch. **Errors:** sign has reserved/backend/
derivation errors; verify additionally has length, unknown-ID, and mismatch.
**Effects/compatibility:** callers may audit failures; canonical bytes, rotation,
and `b"ac-sig"` are production contracts, not fuzz guarantees.
**Links:** INV-005; REL-005–008/011–014. **Evidence:** `S03–S04`, `S09–S10`.

[Contract index](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Sealed AC audit taxonomy boundary

**Receiver/signature:** handler `make_record` supplies `event_type` and
`reason`; `AcEventType::as_str/is_sev1` supplies event taxonomy. **Precondition:**
GET emits `GetSigInvalid` with `canonical_drift` or `sig_mismatch`; UPDATE emits
`UpdateSigInvalid` with `sig_sign_failed` (except reserved-key short-circuit).
The worker-local `SigError` has no `audit_code`; its adapter preserves canonical
code prefixes in local backend messages, but the handler does not copy them into
`AcAuditRecord.reason`. **Postcondition:** fields stay distinct; this harness
emits neither. **Effects:** mapping changes affect dashboards.
**Compatibility:** sealed S-04 is coordination reference, not execution proof.
**Links:** INV-005; REL-015/018. **Evidence:** `S09–S11`.

[Contract index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State, flows, and falsifiable invariants

| State | Owner / lifetime | Effects |
|---|---|---|
| LibFuzzer input | Runner; one closure invocation | Arbitrary bytes only; short/incomplete cases return |
| Parsed IKM/salt/info | Target slices over input | Borrowed for one invocation; no secret provenance |
| `out1/out2/out3` | Target `Vec<u8>` | At most 255 bytes per output from the one-byte `L`; dropped after input |
| HKDF result | `Ok(())` or `Err(_)` | No persistence, network, telemetry, or production signer call |

**Invariant index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005).

<a id="inv-001"></a>
### INV-001 — Admission never slices incomplete input

**Predicate:** if `data.len() < 4` or `< 4 + ikm_len + salt_len + info_len`, no
HKDF slice/expand is reached. **Enforcement:** the two early returns before
`data[4..]` slices. **Violation:** removing/changing either guard can panic on
short input. **Verification:** source inspection of `hkdf_expand.rs`; no run.

[Invariant index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Successful expansion preserves requested length

**Predicate:** when the first `expand` returns `Ok(())`, `out1.len() == L`.
**Enforcement:** `vec![0; l]` and `assert_eq!`. **Violation:** a successful
library call with a differently sized output would panic the oracle. **Verification:**
source inspection; no fuzz execution.

[Invariant index](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Repeated derivation is deterministic

**Predicate:** for identical `(ikm, salt, info, L)` and successful second expand,
`out1 == out2`. **Enforcement:** second `Hkdf` construction and `assert_eq!`.
**Violation:** differing bytes are a harness finding. **Verification:** target
source; no runtime claim.

[Invariant index](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Salt sensitivity is conditional and bounded

**Predicate:** when `salt != empty` and `L >= 16`, flipping bit 0 of salt and a
successful third expand produces `out3 != out1`. **Enforcement:** conditional
branch and `assert_ne!`; shorter outputs deliberately skip it. **Violation:**
equal output is treated as a high-severity cryptographic finding by the target.
**Verification:** source inspection; no observation.

[Invariant index](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Production signature and audit identifiers remain distinct

**Predicate:** this package never claims production signing/audit; the worker
keeps event types `GetSigInvalid`/`UpdateSigInvalid` separate from handler
reason literals `canonical_drift`, `sig_mismatch`, and `sig_sign_failed`.
**Enforcement:** adapter mapping and handler emission are outside this package;
worker-local `SigError` has no `audit_code`, and canonical prefixes remain only
in local error messages. Sources are `S03–S04` and `S09–S11`. **Violation:**
treating a canonical code as `AcAuditRecord.reason`, conflating fields, or
treating the oracle as production coverage.
**Verification:** static cross-package review only; no runtime delivery.
**Links:** API-003/004; REL-005/006/015/018.

[Invariant index](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuration, targets, and features

| Surface | Selection | Effect / unknown |
|---|---|---|
| Target | `hkdf_expand`; `test=false`, `doc=false`, `bench=false` | Host fuzz bin; no build result observed |
| Features | No package feature table or feature overrides | Resolved dependency features unknown |
| Input size | Four one-byte headers; `L <= 255`; `need` uses saturating addition | Harness output allocation is bounded by `L`; runner input/resource budget remains external |
| Root/workspace | Root excludes path; nested `[workspace]` | Standalone resolver; no `cargo metadata` run |
| CI | Manual-only `fuzz-nightly` matrix entry; `scripts/fuzz-all.sh` entry | Configuration proves intent, not execution; self-hosted Mac/nightly details unverified locally |
| Lint policy | `panic`, `unwrap_used`, `indexing_slicing`, `todo`, `unimplemented` denied in manifest; target contains `expect` and direct indexing | Feature/env selection and whether cargo-fuzz target linting applies are unresolved; authorized clippy gate required |

The target does not implement production's fixed 32-byte `ac-sig` output. Its
`Err` branch is retained for the generic HKDF API but `L <= 255` is below the
HKDF-SHA256 maximum 8160 bytes, so that particular output-length error is not
expected from this input encoding.

<a id="r07"></a>
## R07 — Failures and observability

| Signal | Local cause | State / diagnostic limit |
|---|---|---|
| Early return | Header or declared slices do not fit | No derivation; not evidence of a pass or production behavior |
| `Err(_)` | Generic HKDF rejection | Target returns; exact resolved error/version unknown |
| Assertion/expect panic | Length, determinism, or conditional salt property fails | LibFuzzer finding; no application telemetry or remote side effect |
| Timeout/resource failure | Runner/toolchain/corpus budget or host issue | Requires run-specific evidence; workflow YAML is insufficient |

No metrics, logs, database, event, FFI, or deployment path is implemented in
this package. Production AC invalid-signature metrics and alerts belong to the
`corelink-ac`/worker composition and cannot be inferred from this harness.

<a id="r08"></a>
## R08 — Verification and evidence

| ID | Source / blob | Method | Result / limit |
|---|---|---|---|
| S01 | `crates/corelink-ac/fuzz/Cargo.toml:1-34` / `d84afdbf77e3ffdc31d5c58b72ed40f2b45951c7` | Static read at pin | Identity, workspace, target, lint policy, and 3 direct declarations; no resolver |
| S02 | `fuzz_targets/hkdf_expand.rs` / `3c2fde4864636eb77954cf9ae6c1a5e7760dfde1` | Full source read | Input layout and all four oracle branches; not executed |
| S03 | `crates/corelink-ac/src/ac_core/sig.rs` / `0a14115450ac4f931c7a913733590ce256dac245` | Static contract read | Exports/fixed `ac-sig`/sentinel; production not called here |
| S04 | `crates/corelink-ac/src/ac_core/sig/hkdf_signer.rs` / `cd4ae9c3eef8cfe4a792d11fef27f71fafab5c11` | Static implementation read | TDK/key-id/HKDF/BLAKE3 composition; no runtime proof |
| S05 | `.github/workflows/fuzz-nightly.yml` / `61b86f724660f3ba318c6a1c03b7b986a9934ee9` | Workflow source read | Matrix declares `corelink-ac::hkdf_expand`; no run inspected |
| S06 | `scripts/fuzz-all.sh` / `e4dd689534c76c84554f54019f588fd440c11e59` | Script source read | Target list declares same pair; no execution |
| S07 | `Cargo.toml` / `801ee986c93374461d468ed6e643345e8c2ed8f1` | Root manifest read | Package path excluded from outer workspace |
| S08 | ADR-0021 / `6e7657f74772f6c1d9975aa735a3b2b2e68dec7b` | Canonical OKF-linked ADR read | Production algorithm context; not duplicated as ownership policy |
| S09 | `crates/corelink-ac/tests/ac_core_canonical_vectors_sig.rs` / `f78ef9d5cd50ca8bd874e2349c3471d474bcd534` | Static integration-test read | Public `corelink_ac::sig` imports, exact signer/verifier construction and 32-byte/`ac-sig` constants; no execution |
| S10 | `crates/corelink-ac/tests/ac_core_key_rotation_sig.rs`, `ac_core_prop_sig.rs`, `ac_core_timing_sig.rs` / `144cdff8d1584a2a00939f6b1afaec16d22de674`, `ed0c0c091a550b1563dbffe918b1b7779eb516eb`, `fe8b6db23e763fee22d77e6ce643d21d32a7cd71` | Static test-target census | Rotation, property and timing suites are declared; no result or feature-resolved selection |
| S11 | `crates/corelink-worker/src/reapi/ac/sig.rs` / `d8f4d277975f37c17ec22e01e8f74cc204b7d633`; `crates/corelink-worker/src/reapi/ac/handler/methods.rs` / `0fc5aa9d053a90a3f5b343e8f337957f4979f96b`; `crates/corelink-worker/src/reapi/ac/handler/handler_impl.rs` / `4069bde4c36b6a58c6f9e9c54f64ecf55e328eb0`; `crates/corelink-worker/src/reapi/ac/audit.rs` / `24caac47c608f9c622b7e4b6ad20e57d57c5d721`; `specs/04_sprints/_sealed/S04/PRR-S04.md` / `f344785c99dbe3f60fe668985e8b1d58df427d00` | Static adapter, handler, `make_record`, audit taxonomy, and sealed S-04 read | Adapter preserves canonical code prefixes in local errors; `make_record` copies event/reason fields; handler emits separate literals; sealed coordination boundary; not harness execution or audit delivery |

**Unknowns:** current remote pointer, resolved registry versions/features,
toolchain support, corpus/artifacts, workflow history, runtime reachability,
TDK/KMS provenance, provider state, named owners, and independent reviewers.
The source pin is requested and fixed above; the manifest/test lint conflict is
unresolved until an authorized clippy gate; approval is pending.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
