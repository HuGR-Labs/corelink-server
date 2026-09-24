---
schema: corelink-ownership/1.1
document: reference
package: corelink-worker-fuzz
manifest: crates/corelink-worker/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: worker-fuzz-source-20260921
---

# corelink-worker-fuzz — ownership reference

[Identity](#r01) · [Boundaries](#r02) · [Implementation](#r03) ·
[Harness contracts](#r04) · [State and invariants](#r05) · [Targets](#r06) ·
[Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and role

`corelink-worker-fuzz` is an independent Cargo package containing two `cargo-fuzz` binaries. Its source constructs the parent crate's R2 adapter over `InMemoryR2`; the R2-like names and assertions do not establish credentials, a Cloudflare binding, remote storage, or production execution. This review reads source pinned at `1177dad2ca2a9f21c29b5a118aa7944b77147798`; no Cargo command or fuzz run was used.

| Field | Verified value |
|---|---|
| Package / manifest | `corelink-worker-fuzz` / `crates/corelink-worker/fuzz/Cargo.toml` |
| Targets | `r2_path`, `r2_put_get_roundtrip`; both binary paths are explicit |
| Package metadata | `0.0.0`, edition 2021, `publish=false`, `UNLICENSED`, `cargo-fuzz=true` |
| Workspace boundary | Root workspace excludes this path; fuzz manifest declares its own `[workspace]` |
| Implementation / target wiring | Present in two source files / two `[[bin]]` entries; PR smoke is active, per-crate nightly is dormant, and root nightly matrix is active |
| Runtime execution / production reachability | Unknown; not exercised or observed in this review |

<a id="r02"></a>
## R02 — Boundaries and authority

| Surface | Implementation owner | Contract owner | Composition / operation / review |
|---|---|---|---|
| Fuzz harness code | Package source; implementation owner not identified | Harness oracle only; it does not own worker storage policy | Local fuzz invocation is configured in CI; operator identity is unknown |
| R2 adapter and key behavior called by the harness | `corelink-worker` source | `corelink-worker` for its adapter; `corelink-hash` and `corelink-tenant-path` for their types | This harness composes only the in-memory backend; production composition is outside this evidence |
| Independent review | Not the document author | Not assigned by inspected source | Four fresh artifact reviews remain required; CODEOWNERS is routing, not independent approval |

This package has no declared library API. It does not implement a Cloudflare R2 binding, credential lookup, production service, scheduler, database, or storage operator. Its path dependencies do not transfer contract ownership.

`.github/CODEOWNERS` routes the default path to `@gmhelmold`, but its own header says it only requests review, the sole owner is usually the author, and branch protection is unavailable. This does not establish independent review or an escalation route.

The parent crate's default feature list is empty. This manifest does not select additional dependency features, and the resolved graph remains unqueried.

**Canonical context:** [CAS/AC core cluster](../../../knowledge/crates/cas-ac-core.md), [R2 CAS bucket](../../../knowledge/storage/r2-cas-bucket.md), and [tenant prefix algorithm](../../../knowledge/adr/adr-0043-hmac-tenant-prefix-algorithm.md). These sources do not turn a local fake into an R2 runtime observation.

<a id="r03"></a>
## R03 — Implementation map

| Target / source | Input and owned behavior | Nature | Source evidence |
|---|---|---|---|
| `r2_path` / `fuzz_targets/r2_path.rs` | For inputs of at least 49 bytes: 32-byte TDK, 16-byte UUID, region byte, then body; drives `R2Writer::put` and checks the first in-memory key's six segments, bucket, HMAC-prefix shape, algorithm, shards, and lowercase digest | Fuzz binary; per input it creates a fresh `InMemoryR2` | [Target](../../../../crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs), `S03` |
| `r2_put_get_roundtrip` / `fuzz_targets/r2_put_get_roundtrip.rs` | For inputs of at least 65 bytes: TDK, two UUIDs, region byte, then body; checks size rejection, Fresh then Duplicate, same-tenant byte equality, and cross-tenant `NotFound` | Fuzz binary; writer and reader share a fresh in-memory fake | [Target](../../../../crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs), `S04` |

The manifest tree contains these two target sources and a package-local lockfile; it declares no library, build script, test target, or other bin. No generated source or FFI boundary was observed in the target files. The `r2_put_get_roundtrip` target copies the entire body into `Bytes` before checking the 5 MiB limit, so a fuzzer input bound is also a memory bound.

<a id="r04"></a>
## R04 — Harness entry and oracle contracts

These are test-oracle records, not product APIs. `libfuzzer_sys::fuzz_target!` exposes each input closure to the fuzz runner; a failed assertion is a finding. Neither oracle certifies a Cloudflare adapter or service path.

**Contract index:** [API-001](#api-001) · [API-002](#api-002).

<a id="api-001"></a>
### API-001 — `r2_path` key-shape oracle

**Symbols:** `fuzz_target!`, `region_from_byte`, `is_url_safe_b64` in `r2_path.rs`.
**Input / precondition:** arbitrary byte slice; fewer than 49 bytes returns without a case.
**Postcondition:** after a key is present, six segments, selected bucket, 16-character URL-safe prefix, `blake3`, two 2-character shards, and 64 lowercase-hex characters must hold.
**Failure / scope:** assertion panic is a fuzz finding; a failed PUT or empty key list returns without proving a key.
**Owner / evidence:** harness oracle here; key semantics belong to `corelink-worker`; `S03`, `S06`.

[Contract index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — `r2_put_get_roundtrip` behavior oracle

**Symbols:** `fuzz_target!`, `R2Writer::put`, `R2Reader::get` calls in `r2_put_get_roundtrip.rs`.
**Input / precondition:** arbitrary byte slice; fewer than 65 bytes returns without a case; digest is computed from the body.
**Postcondition:** oversize PUT returns exact `BlobTooLarge`; otherwise first PUT is Fresh, second is Duplicate, same-tenant GET equals the body, and a distinct UUID sees NotFound.

**Failure / scope:** `expect`, `panic`, or assertion failure is a fuzz finding against `InMemoryR2` composition only.
**Owner / evidence:** harness oracle here; adapter and value contracts remain with their source crates; `S04`, `S05`.

[Contract index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State, flows, and invariants

| State | Key / owner | Lifetime and persistence | Write / read / durability |
|---|---|---|---|
| Fuzz input | LibFuzzer input bytes; local runner | One invocation; runner corpus/artifacts can outlive it | Supplied to a target closure; no service persistence implied |
| R2 objects | `InMemoryR2` `HashMap<String, Bytes>` in `corelink-worker` | Fresh fake per input; process memory only | Target-local PUT/GET; not remote R2 or durable storage |
| TDK, tenant IDs, body, digest | Constructed from fuzz bytes; APIs owned by dependency crates | Per input; TDK bytes are wrapped in `Zeroizing` | No credential source or external tenant record is read |

**Invariant index:** [INV-001](#inv-001) · [INV-002](#inv-002).

<a id="inv-001"></a>
### INV-001 — Key output has the asserted canonical shape

**Rule:** a produced key passes the explicit segment, prefix, algorithm, shard, and hex assertions in `r2_path`.
**Enforcement / violation:** target assertions fail on mismatch; PUT error or no key returns early and is not a successful oracle check.
**Proof / status:** target source inspected at the pin; no fuzz execution in this review.

[Invariant index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Round-trip oracle uses only the local fake

**Rule:** the target creates one `InMemoryR2` shared by writer and reader for the current input; it does not instantiate a Cloudflare binding.
**Enforcement / violation:** source construction sites are the scope; a future backend change must be reviewed as a new boundary.
**Proof / status:** `r2_put_get_roundtrip.rs` and `InMemoryR2` implementation inspected at the pin; no runtime claim.

[Invariant index](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuration, targets, and features

| Name / source | Default | Read phase | Target / condition | Effect / failure |
|---|---|---|---|---|
| `r2_path`, `r2_put_get_roundtrip` | No runtime default; each input is runner supplied | Fuzz invocation | Host fuzz bins; manifest sets `test=false`, `doc=false`, `bench=false` | Assertions or panic produce findings; limits depend on runner arguments |
| Dependency features | No feature table in fuzz manifest; path deps omit feature overrides | Cargo resolution | Exact feature closure not queried; parent worker declares `default=[]` | Do not infer compile selection or successful build |
| CI runner/toolchain | Workflows select self-hosted Mac `corelink-builder`; cargo-fuzz 0.13.1 and nightly are configured | Workflow jobs | PR smoke is active; per-crate nightly is dormant; root matrix is schedule/dispatch-capable | Configuration proves intent, not a successful run |

No fuzz target reads credentials or environment configuration in inspected source. No target feature combination was resolved by Cargo in this review.

<a id="r07"></a>
## R07 — Failures and observability

| Signal | Cause in harness | State after failure | Diagnostic limit |
|---|---|---|---|
| Assertion or explicit panic | Key grammar, size, idempotency, read result, or bytes differ | Local fake is discarded with process/input scope | A finding is evidence about this harness composition only |
| Fuzz timeout or resource exhaustion | Input/runtime exceeds runner budget; round-trip copies body before size rejection | Corpus/artifact state may remain in task or CI storage | The 5 MiB writer cap does not bound pre-check input allocation |
| PUT error or no key in `r2_path` | Adapter returned error or no entry appeared | Target returns without a shape assertion | This path can finish without validating key grammar |

The target source has no metrics or network diagnostics. Manifest lints deny unsafe code and selected panic/indexing patterns; both targets locally allow panic, expect, and indexing because these are oracle failures. This does not characterize transitive dependency internals.

<a id="r08"></a>
## R08 — Verification and evidence

| ID | Source and revision | Method | Result and limit |
|---|---|---|---|
| S01 | [Fuzz manifest](../../../../crates/corelink-worker/fuzz/Cargo.toml), blob `2d3c6cac3b69e9ecb6d29588bfc25028e2a6fcb2` | Static read at pinned commit | 2 bins, 8 direct dependencies, independent workspace declaration; no Cargo resolution |
| S02 | [Root workspace](../../../../Cargo.toml), blob `801ee986c93374461d468ed6e643345e8c2ed8f1` | Static read at pinned commit | Fuzz path excluded from root workspace; no metadata command |
| S03 | [Key target](../../../../crates/corelink-worker/fuzz/fuzz_targets/r2_path.rs), blob `4bda17ca5e7c1fd702d6c4f34843105650e408ec` | Full target source read | Input/oracle described; not executed |
| S04 | [Round-trip target](../../../../crates/corelink-worker/fuzz/fuzz_targets/r2_put_get_roundtrip.rs), blob `2d4dbbc07f9ef2c91082f3693043316bcbead24e` | Full target source read | Input/oracle and allocation order described; not executed |
| S05 | [Worker adapter](../../../../crates/corelink-worker/src/storage/r2.rs), blob `e4e0671b5acd00542d6d1249af83e76a1e19b623` | Read adapter and fake sections | `InMemoryR2` is memory backed; no production binding claim |
| S06 | [Canonical key](../../../../crates/corelink-worker/src/storage/key.rs), blob `226a4ce83d4a0dda7c74e569ab4429709cd6ba5d` | Read constructor and key tests | Grammar source inspected; tests not run |
| S07 | [Worker workflow](../../../../.github/workflows/corelink-worker.yml), blob `f7f5861ef4a6ab7a7a1fbf941cdb630c59aaa600` | Static read of trigger, jobs, and commands | PR smoke is active at 60s; per-crate nightly is declared but its schedule is commented; execution not observed |
| S08 | [Nightly workflow](../../../../.github/workflows/nightly.yml), blob `78362efe867634b7cdd028f98f5b486ffb4943c5` | Static matrix and `on.schedule` read | Both targets are in the active 3600s matrix; execution not observed |
| S09 | [CODEOWNERS](../../../../.github/CODEOWNERS), blob `065bc9f507811887977ad798cdfb30dfbf3679a0` | Static read of default route and caveat | Default review request is visible; independence and operation authority are not established |

**Unknowns:** current remote pointer was not re-read; Cargo-resolved graph/features, corpus state, runtime behavior, credentials, production reachability, actual CI run history, and named owners/reviewer remain unknown. The requested local pin is `1177dad2ca2a9f21c29b5a118aa7944b77147798`; review is pending and author approval is not claimed.

**Continue:** [Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).

[Back to identity](#r01)
