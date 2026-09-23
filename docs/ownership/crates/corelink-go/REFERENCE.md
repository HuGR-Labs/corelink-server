---
schema: corelink-ownership/1.1
document: reference
package: corelink-go
manifest: tools/sdks/go/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: candidate
evidence_set: corelink-go-source-static-3feae2b
---

# corelink-go — ownership reference

S-profile SOURCE reference restricted to the manifest and Rust binding source
at the pinned commit. “Invariant” means a source-falsifiable predicate, not a
built library, C ABI result, Go/cgo consumer, client outcome, release, or
runtime guarantee.

[Identity](#r01) · [Root](#r02) · [Construction](#r03) · [Lifecycle](#r04) · [Accessor](#r05) · [Digest](#r06) · [Verify](#r07) · [Boundary](#r08)

<a id="r01"></a>
## R01 — Manifest identity invariant

Each row is a distinct SOURCE predicate; no row proves artifact emission,
linking, publication, feature resolution, or consumer selection.

| Predicate | Falsifier | SOURCE |
|---|---|---|
| Package identity is `corelink-go`. | Change/remove `package.name`. | `Cargo.toml:1-3` |
| Rust publication is disabled with `publish = false`. | Change/remove that field. | `Cargo.toml:4-8` |
| The library name is `corelink_go`. | Change/remove `lib.name`. | `Cargo.toml:10-13` |
| `cdylib` is a declared crate type. | Remove/rename that list item. | `Cargo.toml:10-13` |
| `staticlib` is a declared crate type. | Remove/rename that list item. | `Cargo.toml:10-13` |
| `corelink-client-verify` is a dependency. | Remove/rename its dependency entry. | `Cargo.toml:32-34` |
| That dependency requests feature `ffi`. | Remove/rename its feature list item. | `Cargo.toml:32-34` |
| `tracing` is a dependency. | Remove/rename its dependency entry. | `Cargo.toml:32-34` |

<a id="r02"></a>
## R02 — Root-export invariant

**Predicate:** the root denies `unsafe_code`, re-exports the dependency FFI
surface, exposes `go_bridge`, and re-exports exactly the five named
`corelink_go_client_*` entry points. **Falsifier:** remove/change the deny,
module declaration, wildcard FFI re-export, or named re-export. **SOURCE:**
`tools/sdks/go/src/lib.rs:17-28`. **Unknown:** public symbol emission, header
generation, ABI visibility, or foreign import.

<a id="r03"></a>
## R03 — Constructor-validation axiom

**Predicate:** `corelink_go_client_new` returns null for a nonempty null PAT
pointer, invalid PAT UTF-8, a nonempty null tenant pointer, or invalid tenant
UTF-8 before constructing `CorelinkGoClient`. **Falsifier:** remove/reorder a
named guard or replace either early return. **SOURCE:**
`tools/sdks/go/src/go_bridge.rs:70-103,121-128`. **Unknown:** caller pointer
validity, allocation behavior, or returned-handle use.

<a id="r04"></a>
## R04 — Verification-selection axiom

**Predicate:** nonzero `client_verify` calls
`corelink_verifier_new_default_on`; zero logs `COR_CAS_VERIFY_DISABLED`, calls
`corelink_verifier_new_disabled(1)`, and records the corresponding boolean in
the handle. **Falsifier:** alter the branch condition, either constructor,
warning code, or assigned boolean. **SOURCE:**
`tools/sdks/go/src/go_bridge.rs:105-127`. **Unknown:** log delivery, counter
observation, verifier behavior, or Go-level defaulting.

<a id="r05"></a>
## R05 — Handle-lifecycle and accessor axioms

**Predicate:** `corelink_go_client_free` returns for a null handle; otherwise
it reconstructs the box and passes its verifier to `corelink_verifier_free`.
`corelink_go_client_is_verify_enabled` returns zero for null and otherwise
returns the stored boolean as `u8`. **Falsifier:** remove a null branch,
replace box reconstruction/free call, or alter the boolean conversion.
**SOURCE:** `tools/sdks/go/src/go_bridge.rs:130-167`. **Unknown:** ownership
discipline, double-free avoidance, thread safety, or cgo call behavior.

<a id="r06"></a>
## R06 — Digest-entry axiom

**Predicate:** `corelink_go_client_put` rejects a null output pointer, a body
length above `isize::MAX`, and a nonempty null body pointer before constructing
the body slice; on the remaining path it calls `Digest::compute`, obtains hex,
copies 64 bytes, and returns zero. **Falsifier:** remove/reorder a guard,
replace the digest/copy operation, or alter the success tag. **SOURCE:**
`tools/sdks/go/src/go_bridge.rs:169-217`. **Unknown:** output-buffer size,
pointer validity, digest algorithm behavior, caller consumption, or upload.

<a id="r07"></a>
## R07 — Verify-delegation axiom

**Predicate:** `corelink_go_client_verify_get` returns `3` for a null handle;
otherwise it passes the stored verifier, supplied body/digest fields, and a
null output-code pointer to `corelink_verifier_verify`. **Falsifier:** remove
the null return, replace a passed argument, or replace the delegated call.
**SOURCE:** `tools/sdks/go/src/go_bridge.rs:219-259`. **Unknown:** raw-pointer
validity, result-code interpretation, verification outcome, or Go call.

<a id="r08"></a>
## R08 — Evidence boundary invariant

**Predicate:** this pack describes `Cargo.toml`, Rust binding sources, and
`corelink-go/corelink.go` at the pinned commit. The Go source declares C
prototypes for five `corelink_go_client_*` functions.

`NewClient` calls `corelink_go_client_new`, `Close` calls
`corelink_go_client_free`, `IsClientVerifyEnabled` calls
`corelink_go_client_is_verify_enabled`, `Put` calls `corelink_go_client_put`,
and `Get` calls `corelink_go_client_verify_get`. `Stat` uses fixed local
metadata and makes no C call. **Falsifier:** change a declaration or call at
`corelink-go/corelink.go:32-58,108-132,141-154,160-170,186-206,233-242`.

**Unknown:** cgo generation, symbol emission/visibility, pointer validity,
ownership/thread behavior, linking, ABI compatibility, network/client outcome,
release, runtime, deployment, or review. The verified canonical OKF route is
[SDK reference](../../../knowledge/ops/sdk-reference.md); routed only, not
copied or revalidated.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
