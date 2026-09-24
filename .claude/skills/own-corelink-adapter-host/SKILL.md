---
name: own-corelink-adapter-host
description: Own source-grounded changes to the hybrid package-manager adapter host without claiming unobserved upstream or runtime behaviour.
metadata:
  evidence-set: adapter-host-static-20260920
  source-commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
  manifest: crates/corelink-adapter-host/Cargo.toml
  package: corelink-adapter-host
  scope: corelink-adapter-host
  evidence: static-source-only
  profile: H
---

# Own corelink-adapter-host

Use this skill for the composition package at `crates/corelink-adapter-host`. It contains five protocol surfaces (Cargo, Brew, npm, OCI, and pip), their local ports and bridges, plus shared overload and upstream-SSRF code.

This is a source-graph procedure only. It does not establish live package-manager compatibility, DNS safety, deployment state, or ownership assignment.

[Baseline](#s01) · [Surface](#s02) · [Contract](#s03) · [Boundary](#s04) · [Change](#s05) · [Evidence](#s06) · [Escalate](#s07)

<a id="s01"></a>
## S01 — Establish the source baseline

**Condition:** beginning work or receiving a diff.

**Action:** identify the manifest, current source revision, edited module family, and whether the change crosses an adapter boundary. Keep the source revision with the review record.

**Evidence:** `crates/corelink-adapter-host/Cargo.toml`, `src/lib.rs`, and the exact changed paths.

**Stop:** the revision, manifest, or target family is unavailable; do not infer it from a binary name or a prior absorption claim.

<a id="s02"></a>
## S02 — Classify the affected protocol surface

**Condition:** a path is under `src/cargo`, `brew`, `npm`, `oci`, or `pip`.

**Action:** classify it as auth, configuration, local port, bridge, audit, route/server, or protocol-specific handler; include OCI digest/pull/push/tags and npm/pip/brew upstream paths when applicable.

**Evidence:** module declarations in `src/lib.rs` and the corresponding family files.

**Stop:** a change spans two families or touches `upstream_ssrf.rs`/`overload.rs`; record every affected surface before implementation.

<a id="s03"></a>
## S03 — Preserve adapter-to-SPI translation

**Condition:** editing a `bridge.rs`, `ports.rs`, or constructor that accepts handler dependencies.

**Action:** trace the local async port to `CasReadHandler`, `CasWriteHandler`, `PatValidator`, or `KvBackend`; retain tenant/key/digest/error mapping and the deliberate `spawn_blocking` boundary where source uses it. For OCI uploads, verify a session is bound to its opening tenant and append/finalize/cancel reject a different tenant; do not infer that from the final CAS write tenant.

**Evidence:** the bridge implementation, local port trait, and handler request construction.

**Stop:** an input cannot be mapped to the SPI request, a `NotFound` mapping changes, an OCI session accepts a tenant-independent UUID, or a generic/object-safety requirement is unclear; obtain the handler contract rather than guessing.

<a id="s04"></a>
## S04 — Treat credentials, outbound HTTP, and audit as material boundaries

**Condition:** changing auth, upstream fetching, redirects, mutation, or an error-to-response mapping.

**Action:** retain constant-time comparison where used; trace SSRF policy application for every changed read-through client; establish whether audit is required before mutation and whether overload maps separately from credential denial.

**Evidence:** `auth.rs`, `upstream_ssrf.rs`, upstream client construction, audit call site, error type, and server mapping.

**Stop:** a proposed change weakens literal-IP redirect handling, converts verifier overload to an auth verdict, or makes audit ordering ambiguous.

<a id="s05"></a>
## S05 — Bound wire-contract changes

**Condition:** changing router paths, headers, digest parsing, package metadata, upload flow, or upstream request construction.

**Action:** identify the exact protocol family and source-level request/response contract; distinguish local handler behaviour from package-manager interoperability. Review cache-key and digest handling for the family.

**Evidence:** family `server.rs`, dispatch/handler files, and protocol helpers such as OCI `digest.rs`, Brew `bottle.rs`, npm `tarball.rs`, or pip index/wheel files.

**Stop:** a claim needs a real Cargo/Brew/npm/OCI/pip client or upstream response; label it unknown and request authorized integration evidence.

<a id="s06"></a>
## S06 — Produce falsifiable review evidence

**Condition:** preparing a change for review.

**Action:** cite paths and symbols, list affected adapter families and direct CoreLink dependencies, and state source-backed invariants that a reviewer can falsify by inspection. Preserve unresolved external assumptions separately.

**Evidence:** manifest dependency list, changed source, and the ownership reference/blast/maintenance artifacts.

**Stop:** evidence is only a comment, an unpinned web description, a runtime assertion, or a test result not actually obtained in the stated context.

<a id="s07"></a>
## S07 — Escalate cross-owner or unobservable work

**Condition:** work reaches handler-CAS, Worker, REAPI, audit, core, container wiring, a package-manager client, DNS resolution, or an upstream registry.

**Action:** send the source path, desired contract, affected tenant/digest/audit flow, and missing authority to the responsible owner or review record. Keep the adapter-host change scoped until a contract is supplied.

**Evidence:** a linkable issue/PR reference or explicit source-contract citation; absence of either remains an unknown.

**Stop:** no authority, reproducible fixture, or source contract is available. Do not certify runtime, deployment, cold review, or live upstream compatibility.
