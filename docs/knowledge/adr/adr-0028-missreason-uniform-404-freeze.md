---
type: "ADR"
title: "ADR-0028 — MissReason → uniform HTTP 404 freeze (410 deferred)"
description: "Why every CAS read miss returns an identical 404 regardless of MissReason, so the status code itself cannot become a cross-tenant existence oracle or leak prior existence."
source_files:
  - "specs/03_architecture/adrs/ADR-0028-missreason-uniform-404-freeze.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "missreason", "404", "side-channel", "tenant-isolation", "reapi", "s02"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0028 — MissReason → uniform HTTP 404 freeze (410 deferred)

A CAS read can miss for several distinct reasons — the digest never existed, it exists only in another tenant, or it was soft-deleted by GC — and an earlier design mapped those to different HTTP status codes. This ADR freezes them all to one indistinguishable 404, because a differentiated status code is itself an enumeration oracle even with no timing analysis. It is the status-layer decision whose timing-layer companion is ADR-0023.

# Context

The original spec mapped `NotFound`→404, `CrossTenantMasked`→403, and `Tombstoned`→404-or-410, which created two oracles plus a conformance break: a 403 reveals that a digest exists in another tenant, a 410 reveals prior existence (leaking customer caching behaviour), and 410-for-missing is non-canonical under the bazel REAPI v2 conformance suite (ADR-0028:47-72).

# Decision

All `MissReason` variants return an identical `HTTP 404` with `error_code = COR_CAS_BLOB_NOT_FOUND` and the same body, so the attacker cannot distinguish `NotFound`/`CrossTenantMasked`/`Tombstoned` via the response; forensic reason is retained out-of-band via per-arm CloudEvents in the S-09 chain, `COR_CAS_TENANT_FORBIDDEN` (403) is reserved for PAT-scope failures and never returned by CAS read handlers, and the 410-Gone semantic is explicitly deferred to the S-06 GC sprint behind its own ADR + privacy review (ADR-0028:76-99).

# Consequences

The existence oracle is closed at the status-code layer, customer behaviour stays private, and REAPI v2 conformance is preserved, traded against reduced forensic visibility for customer support (the tombstone-vs-wrong-digest distinction moves to the admin/audit plane), with the residual timing oracle explicitly handed to the constant-time middleware in [ADR-0023](/adr/adr-0023-constant-time-timing-padding.md) (ADR-0028:101-114). It governs the [native CAS surface](/surfaces/native-cas.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0028-missreason-uniform-404-freeze.md:47-72` — differentiated status codes as enumeration + prior-existence oracles and the REAPI conformance break (Context).
2. `specs/03_architecture/adrs/ADR-0028-missreason-uniform-404-freeze.md:76-99` — uniform 404 + COR_CAS_BLOB_NOT_FOUND; 403 reserved; 410 deferred to S-06 (Decision).
3. `specs/03_architecture/adrs/ADR-0028-missreason-uniform-404-freeze.md:101-114` — oracle/privacy/conformance wins vs forensic-visibility loss; timing deferred to ADR-0023 (Consequences).
