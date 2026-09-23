---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-handler-cas
manifest: crates/corelink-handler-cas/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-cas-structural-normalization-20260921
---

# corelink-handler-cas — blast radius

These are static manifest/source relations. They identify a dependency,
re-export, or source-visible use; none proves compilation, target selection,
runtime reachability, transport, or deployment.

[Surface](#b01) · [Bazel bridge](#b02) · [CAS facade](#b03) ·
[Container](#b04) · [Collaborators](#b05) · [Unknowns](#b06).

<a id="b01"></a>
## B01 — Public surface impact

Changes to exported handler traits, `CasHandlerError`, request/response
envelopes, audit/SLI interfaces, or `DigestAlgo` can affect source consumers
that compile against these names. The public root is `src/lib.rs`; the
corresponding local contracts are in `src/{handler,error,request,audit,observer,digest_algo}.rs`.
This relation does not enumerate every consumer or establish compatibility.

<a id="b02"></a>
## B02 — Bazel bridge relation

`corelink-bazel-bridge/Cargo.toml` directly depends on this package. Its
`adapter.rs` and `find_missing.rs` import read/write traits, requests, errors,
and `DigestAlgo::Sha256`; source tests also use the in-memory fake. Therefore a
trait, request, error, or SHA-256-tag change requires coordination with that
bridge. The bridge's REAPI parsing, transport, client compatibility, and any
provider behavior are outside this package. Evidence:
`crates/corelink-bazel-bridge/{Cargo.toml,src/adapter.rs,src/find_missing.rs}`.

<a id="b03"></a>
## B03 — CAS facade relation

`corelink-cas/Cargo.toml` directly depends on this package, and
`crates/corelink-cas/src/handler.rs` re-exports `corelink_handler_cas::*`.
Changes to a public symbol can therefore alter a facade-visible import path.
The re-export does not transfer handler implementation ownership, prove callers
have migrated, or identify a concrete handler.

<a id="b04"></a>
## B04 — Container and adapter-host relations

Shared fingerprint `repo:1232040291:relation:corelink-handler-cas-to-corelink-adapter-host-manifest` covers the handler-CAS ↔ adapter-host manifest edge; the adapter-host record is REL-018.

`corelink-container/Cargo.toml` and `corelink-adapter-host/Cargo.toml` each
declare direct dependencies. Container source imports the traits/envelopes and
contains its own route and R2/S3-related code; adapter-host source names CAS
handler traits in its bridges. Impact of an API change crosses those package
boundaries. This does not make routes, HTTP, R2/S3/KV, authentication, quota,
or adapters local behavior of `corelink-handler-cas`. Evidence:
`crates/corelink-container/{Cargo.toml,src/routes/cas/foundation_core.rs}` and
`crates/corelink-adapter-host/{Cargo.toml,src/lib.rs}`.

<a id="b05"></a>
## B05 — Collaborator/dependency relations

This package directly declares `corelink-slo` and re-exports its `Sli` type
through `observer.rs`; an SLI type/tag change therefore affects that local
seam. It also declares `sha2` and `hex` for the SHA-256 helper, and `thiserror`
for the error derive. The absence of `corelink-audit` and of provider/HTTP
dependencies supports the local-seam boundary but does not prove an external
adapter is absent from the wider workspace. Evidence: `Cargo.toml`,
`src/{observer,error,handler}.rs`.

<a id="b06"></a>
## B06 — Coverage limits and unknowns

Known direct manifest relations are Bazel bridge, CAS facade, container, and
adapter host. Unknown: a complete reverse dependency graph, all trait
implementations, all feature/target selections, application composition,
HTTP/REAPI behavior, wasm execution, remote storage, audit/telemetry delivery,
production data, traffic, deployment, and caller compatibility. A source match
or manifest edge is not evidence of any unknown being absent.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) ·
[Ownership guide](../../../../.claude/skills/own-corelink-handler-cas/SKILL.md#s01).
