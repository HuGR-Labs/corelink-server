---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-adapter-host
manifest: crates/corelink-adapter-host/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: H
state: draft
evidence_set: adapter-host-static-20260920
---

# corelink-adapter-host — maintenance guide

Source-only maintenance modes for this hybrid adapter package. These procedures
do not authorize network access, package publication, deployment, runtime probes,
or claims that Cargo tests were run.

[Baseline](#m01) · [Classify](#m02) · [Bridge](#m03) · [Boundary](#m04) · [Wire](#m05) · [Recovery](#m06)

Procedure index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003).

<a id="m01"></a>
## M01 — Baseline and scope

**Mode:** `READ_ONLY`.

**Predicate:** the manifest revision and changed paths are known.

**Action:** record `Cargo.toml`, source revision, and the affected public module/family before review.

**Evidence:** `git rev-parse HEAD`, the manifest, `src/lib.rs`, and the diff.

**Stop/recovery:** stop when the baseline or worktree scope is ambiguous; restore certainty by obtaining the intended revision and a clean, attributable diff, not by resetting unrelated work.

<a id="m02"></a>
## M02 — Family classification

**Mode:** `READ_ONLY`.

**Predicate:** a changed path is in an adapter family or shared module.

**Action:** classify Cargo/Brew/npm/OCI/pip work as auth, config, ports, bridge, audit, server, or helper; classify `upstream_ssrf.rs` and `overload.rs` as shared. Include OCI dispatch/digest/pull/push/tags where relevant.

**Evidence:** path inventory and `src/lib.rs`.

**Stop/recovery:** stop on cross-family/shared edits until all affected families are listed; recover by extending the review set rather than treating the shared file as local.

<a id="m03"></a>
## M03 — Bridge and storage translation

**Mode:** `READ_ONLY`.

**Predicate:** a local port, bridge, handler request, tenant id, digest/key, bytes, or mapped error changes.

**Action:** trace local async trait → bridge → handler-CAS/Worker/KV request/result. Check `spawn_blocking`, `NotFound`, and generic KV constraints where the source uses them. For OCI uploads, trace opening tenant through append, finalize, and cancel rather than relying on the final CAS write tenant.

**Evidence:** affected `ports.rs` and `bridge.rs`, plus handler request symbols. `src/oci/bridge.rs:95-164` is a baseline defect: sessions hold only `Vec<u8>` by UUID and discard the tenant in append/finalize/cancel.

**Stop/recovery:** stop OCI upload changes until the session is explicitly bound to its opening tenant and cross-tenant append/finalize/cancel are rejected. Recover by changing the session representation and adding source-backed contract evidence; handler semantics or storage durability still require the owning SPI contract.

<a id="m04"></a>
## M04 — Security and audit boundary

**Mode:** `READ_ONLY`.

**Predicate:** auth, token comparison, tenant resolution, audit, mutation, upstream fetch, redirect policy, or overload response changes.

**Action:** inspect the exact auth/error/server and audit call path; verify use of the shared SSRF policy for changed read-through clients (including npm `upstream.rs:9,79`) and distinguish overload from credential denial where source provides the variant.

**Evidence:** family `auth.rs`, `error.rs`, `server.rs`, `audit.rs`, `upstream.rs`, and `upstream_ssrf.rs`.

**Stop/recovery:** stop if audit ordering or redirect posture cannot be shown from the call site; recover by narrowing the change or obtaining an explicit source contract. Do not claim live SSRF resistance or audit delivery.

<a id="m05"></a>
## M05 — Wire/digest compatibility assessment

**Mode:** `READ_ONLY`.

**Predicate:** router path, header, body limit, metadata, digest, cache key, upload session, tag, index, or upstream request construction changes.

**Action:** identify one protocol family and trace the source helper through its route/handler. Record the proposed before/after grammar, representation, and storage edge.

**Evidence:** Cargo translate; Brew bottle; npm metadata/tarball; pip index/PEP-503/wheel; or OCI digest/dispatch/pull/push/tags source, as applicable.

**Stop/recovery:** stop when compatibility requires a real client or registry response; recover with an authorized, revision-pinned integration fixture. Static review alone cannot certify wire compatibility.

<a id="m06"></a>
## M06 — Escalation, recovery, and record

**Mode:** `READ_ONLY`.

**Predicate:** the change crosses into handler-CAS, Worker, REAPI, audit, core, container wiring, live upstreams, or a persistent-storage recovery decision.

**Action:** record affected paths, protocol family, tenant/digest/audit flow, requested contract, and unknown. Route it to the dependency owner or review process. Preserve the source evidence and do not broaden this package's claim.

**Evidence:** issue/PR reference or cited owner contract, plus `REFERENCE.md`/`BLAST_RADIUS.md` identifiers.

**Stop/recovery:** stop without authority or a falsifiable contract; recover only when the owner supplies it. A code revert alone is not proof that an external write, cache, registry interaction, or deployed configuration was undone.

<a id="proc-001"></a>
### PROC-001 — Trace one family bridge change
**Objective/trigger:** source change to one adapter port, bridge, or SPI request. **Mode:** `READ_ONLY`; **state:** reviewed-not-executed. **Preconditions/environment:** pinned package source and scoped diff; no credentials/network. **Permissions/inputs:** repository read only; family path and affected operation.

**Steps:** (1) read family port signature; (2) trace bridge request, blocking boundary and result/error mapping; (3) compare exact SPI contract and REL/API record; (4) record downstream effects/unknowns. **Expected predicate:** each input, result, effect and failure maps to one cited source branch. **Stop/recovery:** stop when behavior depends on SPI/provider/runtime; request owner evidence, preserve UNKNOWN. **Evidence:** source pin, paths, symbols, relation IDs and diff. [Procedure index](#m02)

<a id="proc-002"></a>
### PROC-002 — Review OCI upload-session tenant boundary
**Objective/trigger:** change to `open_upload`, `append_chunk`, `finalize_upload`, or `cancel_upload`. **Mode:** `READ_ONLY`; **state:** reviewed-not-executed. **Preconditions/environment:** source pin and `REL-007`; no real upload. **Permissions/inputs:** repository read only; session and tenant call paths.

**Steps:** (1) trace session creation; (2) verify session state retains opener tenant; (3) trace tenant comparison on each subsequent operation; (4) verify mismatch fails before bytes/state change. **Expected predicate:** all four operations enforce same tenant binding. **Stop/recovery:** current source does not meet predicate; do not claim tenant isolation until code and review evidence change. **Evidence:** method lines, storage shape, result branches and checker output. [Procedure index](#m02)

<a id="proc-003"></a>
### PROC-003 — Assess protocol-family compatibility
**Objective/trigger:** change to route, header, digest/key, metadata, upstream request, or response. **Mode:** `READ_ONLY`; **state:** reviewed-not-executed. **Preconditions/environment:** pinned family source and identified consumer; no live registry/client. **Permissions/inputs:** source access only; family and changed symbols.

**Steps:** (1) trace helper to route/bridge; (2) list exact before/after grammar or key shape; (3) inspect a checked-in consumer/fixture; (4) name absent integration evidence. **Expected predicate:** static impact and unknown interoperability are separate. **Stop/recovery:** stop if conclusion requires external client/upstream; seek authorized fixture and owner. **Evidence:** source paths, API/REL IDs, fixture or explicit absence. [Procedure index](#m02)

[Reference](REFERENCE.md#r01) · [Impact relations](BLAST_RADIUS.md#b01) · [Start](#m01)
