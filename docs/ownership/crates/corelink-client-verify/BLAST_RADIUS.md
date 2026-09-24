---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-client-verify
manifest: crates/corelink-client-verify/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: candidate
evidence_set: client-verify-source-static-16d9f0303
---

# corelink-client-verify — blast radius

Each relation is one bounded SOURCE arrow at the pinned commit. It is not proof
of dependency resolution, compilation, ABI use, client behavior, I/O, runtime,
or deployment.

[Root/config](#b01) · [Digest](#b02) · [Verifier/error](#b03) · [Stream](#b04) · [FFI](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Root → configuration relation

`src/lib.rs` → `src/config.rs`: the root exposes `config` and re-exports
`VerifyConfig`. A change to that module declaration or re-export can change a
source-visible Rust path. **Evidence:** `src/lib.rs:54-55,74`; `src/config.rs:22-108`.
**Failure boundary:** no selected config or caller construction is observed.

<a id="b02"></a>
## B02 — Digest module → corelink-hash relation

`src/digest.rs` → `corelink-hash`: `Digest`, `ParseError`, `DIGEST_LEN`, and
the mismatch constant are re-exported/imported from the dependency. A change to
that named import can alter the local type/constant seam. **Evidence:**
`Cargo.toml:51-55`; `src/digest.rs:11-19`. **Failure boundary:** no hash
calculation or dependency compatibility is observed. **Shared relation identity:** `repo:1232040291:boundary:hash-client-ffi-001`; hash-side record: [corelink-hash REL-043](../corelink-hash/BLAST_RADIUS.md#rel-043). This identity covers the Rust public aliases; the adjacent C width declarations do not establish ABI use or compatibility.

<a id="b03"></a>
## B03 — Verifier → config/error/digest relation

`src/verifier.rs` → `{config,digest,error}`: the verifier reads `VerifyConfig`,
uses `Digest`, and returns `VerifyError`. A changed input, branch, or result
type can alter this local source seam. **Evidence:** `src/verifier.rs:4-8,52-119`.
**Failure boundary:** no body, result handling, warning delivery, or counter
observation is established.

<a id="b04"></a>
## B04 — Stream feature → stream relation

`Cargo.toml` `stream` → `src/lib.rs` gate → `src/stream.rs`: the named feature
declares optional dependencies, gates the module/re-exports, and the stream
module imports verifier/error/digest types. A feature or gate edit can alter the
compile-time stream surface. **Evidence:** `Cargo.toml:20-28,57-65`;
`src/lib.rs:63-65,79-83`; `src/stream.rs:29-41,43-101`. **Failure boundary:**
no feature resolution, async read, chunk transfer, or EOF result is observed.

<a id="b05"></a>
## B05 — FFI feature → FFI relation

`Cargo.toml` `ffi` → `src/lib.rs` gate → `src/ffi.rs`: the named feature gates
the C-facing module, which imports config/digest/error/verifier and maps its
local verifier result to integer tags. A gate, input validation, or tag edit can
alter this static surface. **Evidence:** `Cargo.toml:20-28`; `src/lib.rs:67-72`;
`src/ffi.rs:48-95,225-318`. **Failure boundary:** no library is built/loaded,
no pointer is supplied, and no ABI consumer is observed.

<a id="b06"></a>
## B06 — Relation closure and unknowns

Coverage ends at the manifest, root, and direct local-module arrows B01–B05.
The [SDK reference](../../../knowledge/ops/sdk-reference.md) is the verified
canonical OKF route only—not a relation and not revalidated here. Unknown:
reverse consumers, resolved graph/features, compilation, tests, SDK/client
integration, ABI compatibility/use, I/O, telemetry, runtime, deployment, and
independent review.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to root/config](#b01)
