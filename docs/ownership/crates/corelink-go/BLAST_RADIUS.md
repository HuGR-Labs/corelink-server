---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-go
manifest: tools/sdks/go/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: candidate
evidence_set: corelink-go-source-static-3feae2b
---

# corelink-go — blast radius

Each item is one bounded SOURCE arrow at the pinned commit. None proves
resolution, compilation, library loading, ABI compatibility, Go/cgo use,
client traffic, release, network, runtime, or deployment.

[Manifest](#b01) · [Root](#b02) · [FFI](#b03) · [Lifecycle](#b04) · [Digest](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Manifest → root relation

`tools/sdks/go/Cargo.toml` → `src/lib.rs`: the manifest declares the package
crate types and dependency with `ffi`, while the root imports that dependency
and locally denies unsafe code. A changed manifest declaration or root import
can change the static binding seam. **Evidence:** `Cargo.toml:1-34`;
`src/lib.rs:1-28`. **Failure boundary:** no selected feature, produced library,
or linker result is observed.

<a id="b02"></a>
## B02 — Root → bridge relation

`src/lib.rs` → `src/go_bridge.rs`: the root declares `pub mod go_bridge` and
re-exports five named bridge entries. A changed declaration or re-export can
change a source-visible Rust path. **Evidence:** `src/lib.rs:23-28`;
`src/go_bridge.rs:1-259`. **Failure boundary:** no C symbol table, generated
header, or foreign import is observed.

<a id="b03"></a>
## B03 — Individually directed FFI relations

### B03.1 — Root → dependency FFI re-export

`src/lib.rs` → `corelink-client-verify::ffi`: the root wildcard-re-exports
the dependency FFI module. A changed re-export can alter this source-visible
Rust seam. **Evidence:** `Cargo.toml:32-34`; `src/lib.rs:19-21`.
**Failure boundary:** no symbol emission, header, ABI compatibility, or
foreign import is observed.

### B03.2 — Constructor → default-on constructor

`corelink_go_client_new` → `corelink_verifier_new_default_on`: the nonzero
`client_verify` branch calls this imported dependency constructor. A changed
import, condition, or call can alter this static delegation seam. **Evidence:**
`src/go_bridge.rs:24-28,105-109`. **Failure boundary:** no verifier allocation,
branch execution, or FFI use is observed.

### B03.3 — Constructor → disabled constructor

`corelink_go_client_new` → `corelink_verifier_new_disabled`: the zero
`client_verify` branch calls this imported dependency constructor with `1`.
A changed import, condition, argument, or call can alter this static delegation
seam. **Evidence:** `src/go_bridge.rs:24-28,110-119`. **Failure boundary:** no
verifier allocation, branch execution, log delivery, or FFI use is observed.

### B03.4 — Free entry → dependency free

`corelink_go_client_free` → `corelink_verifier_free`: after its null branch,
the entry passes the handle's verifier field to the imported dependency free
function. A changed field access or call can alter this static seam.
**Evidence:** `src/go_bridge.rs:24-27,130-146`. **Failure boundary:** no
pointer validity, deallocation, or FFI use is observed.

### B03.5 — Verify entry → dependency verify

`corelink_go_client_verify_get` → `corelink_verifier_verify`: after its null
branch, the entry delegates the verifier, input fields, and null output-code
pointer to the imported function. A changed argument or call can alter this
static seam. **Evidence:** `src/go_bridge.rs:24-27,236-259`.
**Failure boundary:** no ABI use, verification result, or caller handling is
observed.

<a id="b04"></a>
## B04 — Individually directed handle relations

### B04.1 — Constructor → verifier handle field

`corelink_go_client_new` → `CorelinkGoClient.verifier`: construction assigns
the selected dependency constructor result to the handle's verifier field. A
changed assignment can alter this static storage seam. **Evidence:**
`src/go_bridge.rs:35-44,105-127`. **Failure boundary:** no allocation,
pointer validity, or caller ownership is observed.

### B04.2 — Constructor → verification-enabled field

`corelink_go_client_new` → `CorelinkGoClient.client_verify_enabled`:
construction stores `client_verify != 0` in the handle boolean. A changed
expression can alter this static storage seam. **Evidence:**
`src/go_bridge.rs:35-44,121-127`. **Failure boundary:** no Go-level default,
caller argument, or behavior is observed.

### B04.3 — Handle verifier field → free entry

`CorelinkGoClient.verifier` → `corelink_go_client_free`: the free entry reads
that field as the argument to the dependency free call. A changed field access
can alter this static lifecycle seam. **Evidence:** `src/go_bridge.rs:35-44,130-146`.
**Failure boundary:** no ownership discipline, deallocation, or double-free
avoidance is observed.

### B04.4 — Handle boolean field → accessor entry

`CorelinkGoClient.client_verify_enabled` →
`corelink_go_client_is_verify_enabled`: the accessor reads the field and
converts it to `u8`. A changed field access or conversion can alter this static
observation seam. **Evidence:** `src/go_bridge.rs:35-44,149-167`.
**Failure boundary:** no caller handle validity, cgo call, or returned-value
use is observed.

<a id="b05"></a>
## B05 — Individually directed digest and verify relations

### B05.1 — Digest entry → Digest type

`corelink_go_client_put` → `Digest`: after local pointer/length guards, the
entry calls `Digest::compute` and copies 64 hex bytes. A changed guard, type
call, or copy can alter this static digest seam. **Evidence:**
`src/go_bridge.rs:28,169-217`. **Failure boundary:** no buffer validity,
digest correctness, returned-value use, or upload is observed.

### B05.2 — Verify entry → verifier handle field

`corelink_go_client_verify_get` → `CorelinkGoClient.verifier`: after its null
branch, the entry reads the handle's verifier field as the delegated verifier
argument. A changed field access can alter this static verify seam.
**Evidence:** `src/go_bridge.rs:35-44,236-259`. **Failure boundary:** no
pointer validity, verification result, download, or client behavior is
observed.

<a id="b06"></a>
## B06 — Go wrapper call relations and unknowns

`corelink-go/corelink.go` → C declarations and calls: `NewClient` calls
`corelink_go_client_new`, `Close` calls `_free`, the accessor calls
`_is_verify_enabled`, `Put` calls `_put`, and `Get` calls `_verify_get`.
`Stat` returns local fixed metadata without crossing cgo. **Evidence:**
`corelink-go/corelink.go:32-58,108-132,141-154,160-170,186-206,233-242`.
Changing a declaration or call site changes this static wrapper seam.

**Failure boundary:** these call sites do not prove generated cgo bindings,
exported symbols, pointer safety, linker/ABI compatibility, or runtime calls.
The [SDK reference](../../../knowledge/ops/sdk-reference.md) is a verified
canonical OKF route only, not a relation and not revalidated here. Unknown:
reverse consumers, feature resolution, compilation/artifacts, clients, release,
telemetry, network, runtime, deployment, and independent review.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to manifest](#b01)
