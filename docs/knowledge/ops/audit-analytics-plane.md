---
type: "Runbook"
title: "The audit export + analytics plane and customer re-verification"
description: "How a tenant exports its tamper-evident audit chain, re-verifies it with the CLI, and queries analytics — all per-tenant isolated and fail-closed."
source_files:
  - "crates/corelink-container/src/routes/audit_analytics.rs"
  - "crates/corelink-container/src/routes/audit_export.rs"
  - "docs/cli/audit-export.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "audit", "export", "analytics", "compliance", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# The audit export + analytics plane and customer re-verification

The audit plane lets a tenant pull its own append-only, Merkle-chained audit log as a streaming NDJSON
envelope, re-verify the whole chain offline or off-the-wire with the `corelink` CLI, and run bounded
analytics queries over it. The trust property is end-to-end: the server emits a chain-head anchor and,
if it detects a break mid-stream, an abort trailer that the CLI surfaces as a distinct exit code so
SIEM/Drata wrappers can tell a data-integrity event from a generic failure. Every route is per-tenant
isolated and the audit emit is fail-closed. Related: [the audit-analytics crate cluster](/crates/audit-analytics.md).

# Role

It is the compliance read-and-prove surface: customers (and their evidence-collection automation) get
a cryptographically re-verifiable export plus rate-limited analytics, without ever being able to read
another tenant's rows.

# How it works

- The export route's public surface (router + state) is re-exported from the `state` submodule `crates/corelink-container/src/routes/audit_export.rs:146-148`.
- The export audit emit is routed through `emit_or_503`, aborting with 503 on sink error `crates/corelink-container/src/routes/audit_export.rs:145`.
- Mid-stream chain-break detection emits the abort trailer via `mid_stream_abort_trailer_value` `crates/corelink-container/src/routes/audit_export.rs:149-152`.
- The trailer + chain-head anchor header names are canonical constants `crates/corelink-container/src/routes/audit_export.rs:153-156`.
- The CLI offline mode re-verifies a downloaded NDJSON file against a chain-head anchor `docs/cli/audit-export.md:10-31`.
- The CLI HTTP-aware mode streams directly off the wire and watches for the abort trailer `docs/cli/audit-export.md:33-55`.
- Analytics routes (event-count, timeline) ship their router + per-tenant rate-limit config `crates/corelink-container/src/routes/audit_analytics.rs:102`.
- Each analytics handler runs the native PAT possession gate before any data access `crates/corelink-container/src/routes/audit_analytics.rs:118`.

# Invariants

- The export audit row is emitted fail-CLOSED — a sink `Err` aborts with 503 before streaming `crates/corelink-container/src/routes/audit_export.rs:145`.
- Analytics data access is gated per-tenant: the PAT gate rejects forged/wrong-tenant (401) or verifier fault (503) before reads `crates/corelink-container/src/routes/audit_analytics.rs:118`.
- A mid-stream abort surfaces as sysexits DATAERR (65) on the CLI, distinct from generic exit 1 `docs/cli/audit-export.md:79-89`.
- The bearer token is never logged, printed, or surfaced in error messages (CTRL-CRED-001) `docs/cli/audit-export.md:91-99`.

# Gotchas

- The HTTP-fetch path caps the response body at 64 MiB so a malicious server cannot drain CLI memory `docs/cli/audit-export.md:100-102`.
- If both the `--chain-head-anchor` flag and the response header are present they MUST match constant-time — a mismatch is an error, not a warning `docs/cli/audit-export.md:51-55`.

# Citations

1. `crates/corelink-container/src/routes/audit_export.rs:145` — `emit_or_503` fail-closed export audit.
2. `crates/corelink-container/src/routes/audit_export.rs:146-148` — export router + state public surface.
3. `crates/corelink-container/src/routes/audit_export.rs:149-152` — mid-stream abort-trailer builder.
4. `crates/corelink-container/src/routes/audit_export.rs:153-156` — abort-trailer + chain-head-anchor header constants.
5. `crates/corelink-container/src/routes/audit_analytics.rs:102` — analytics router + per-tenant rate-limit config.
6. `crates/corelink-container/src/routes/audit_analytics.rs:118` — per-handler native PAT possession gate.
7. `docs/cli/audit-export.md:10-31` — CLI offline (file) re-verify mode.
8. `docs/cli/audit-export.md:33-55` — CLI HTTP-aware re-verify + anchor cross-check.
9. `docs/cli/audit-export.md:79-89` — exit-65 sysexits DATAERR on mid-stream abort.
10. `docs/cli/audit-export.md:91-99` — token never logged (CTRL-CRED-001) + constant-time anchor compare.
11. `docs/cli/audit-export.md:100-102` — 64 MiB response-body cap.
