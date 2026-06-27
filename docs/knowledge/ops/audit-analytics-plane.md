---
type: "Runbook"
title: "The audit export + analytics plane and customer re-verification"
description: "How a tenant exports its tamper-evident audit chain, re-verifies it with the CLI, and queries analytics — all per-tenant isolated and fail-closed."
source_files:
  - "crates/corelink-container/src/routes/audit_analytics.rs"
  - "crates/corelink-container/src/routes/audit_analytics/state.rs"
  - "crates/corelink-container/src/routes/audit_analytics/rate_limit.rs"
  - "crates/corelink-container/src/routes/audit_export.rs"
  - "crates/corelink-container/src/routes/audit_export/audit_sink.rs"
  - "crates/corelink-container/src/routes/audit_export/stream.rs"
  - "crates/corelink-container/src/routes/audit_export/state.rs"
  - "crates/corelink-container/src/routes/audit_export/types.rs"
  - "tools/cli/src/main.rs"
  - "tools/cli/src/commands/verify_ndjson_http.rs"
  - "docs/cli/audit-export.md"
  - "crates/corelink-container/src/routes/dsr/audit.rs"
  - "apps/analytics-worker/src/ingest.ts"
checkpoint_sha: "d0e4f8bd669cb1e982f7511de9a895c602a5ee45"
provenance: "AUTHORED"
tags: ["ops", "audit", "export", "analytics", "compliance", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# The audit export + analytics plane and customer re-verification

The audit plane lets a tenant pull its own audit log as a streaming NDJSON envelope, re-verify it
offline or off-the-wire with the `corelink` CLI, and run bounded analytics queries over it. **Carve-out
(load-bearing): "append-only, Merkle-chained ... re-verify the whole chain" describes the DESIGNED
plane, not the deployed trail.** The chain machinery (the [audit-analytics crate cluster](/crates/audit-analytics.md))
has no live producer — nothing in the container calls `HashChainBuilder::append` outside tests, and the
live DSR audit sink appends plain *unchained* CloudEvents to D1 `audit_outbox` with `digest=NULL`,
`emitted_at=NULL` and no `prev_hash`/`sequence_number` (`crates/corelink-container/src/routes/dsr/audit.rs:7-8`,
`crates/corelink-container/src/routes/dsr/audit.rs:82-84`). The chain-sealing drain that would link those
rows (and give the CLI a real chain to re-verify) is the unbuilt **WI-S09-007** — so today the exported
rows are unsealed/unchained and there is no live chain to re-verify offline. **More than missing — tamperable (red-team-confirmed):** because nothing ever `SET`s `emitted_at`, seals a digest, or applies R2 Object-Lock, and no compliance reader validates a chain, `audit_outbox` is a plain MUTABLE D1 table — the app's own D1 credential can `DELETE`/`UPDATE` any audit row **undetectably**, so an exported log proves nothing an operator couldn't have rewritten post-hoc. "Edit/deletion detectable" is **FALSE on the deployed plane** until sealing is wired. The trust property below is
end-to-end *once that producer is wired*: the server emits a chain-head anchor and,
if it detects a break mid-stream, an abort trailer that the CLI surfaces as a distinct exit code so
SIEM/Drata wrappers can tell a data-integrity event from a generic failure. Every route is per-tenant
isolated and the audit emit is fail-closed. Related: [the audit-analytics crate cluster](/crates/audit-analytics.md).

# Role

It is the compliance read-and-prove surface: customers (and their evidence-collection automation) get
a cryptographically re-verifiable export plus rate-limited analytics, without ever being able to read
another tenant's rows.

# How it works

- The export route's public surface (router + state) lives in the `state` submodule (`crates/corelink-container/src/routes/audit_export/state.rs:85`, `crates/corelink-container/src/routes/audit_export/state.rs:154`), re-exported verbatim through the barrel `crates/corelink-container/src/routes/audit_export.rs:146-148`.
- The export audit emit is routed through `emit_or_503`, aborting with 503 on sink error `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105`.
- Mid-stream chain-break detection emits the abort trailer via `mid_stream_abort_trailer_value` `crates/corelink-container/src/routes/audit_export/stream.rs:412-417`.
- The trailer + chain-head anchor header names are canonical constants `crates/corelink-container/src/routes/audit_export/types.rs:42-57`.
- The CLI offline mode re-verifies a downloaded NDJSON file against a chain-head anchor `docs/cli/audit-export.md:10-31`.
- The CLI HTTP-aware mode streams directly off the wire and watches for the abort trailer `docs/cli/audit-export.md:33-55`.
- Analytics routes (event-count, timeline) ship their router (`crates/corelink-container/src/routes/audit_analytics/state.rs:80`) + per-tenant rate-limit config (`crates/corelink-container/src/routes/audit_analytics/state.rs:63`) enforced by `rate_limit_check` (`crates/corelink-container/src/routes/audit_analytics/rate_limit.rs:99`).
- Each analytics handler runs the native PAT possession gate before any data access `crates/corelink-container/src/routes/audit_analytics.rs:118`.
- The product-analytics EDGE collector (`POST /v1/event` on the standalone analytics-worker) is the ingest tap that feeds this plane's D1 store; it authenticates each request two ways — a CORS `Origin` allow-list for browsers, or a constant-time-compared `X-Corelink-Ingest-Key` for trusted servers — and rejects anything matching neither with 403 (`apps/analytics-worker/src/ingest.ts:114-136`, `apps/analytics-worker/src/ingest.ts:127-136`).
- The ingest validator enforces a HARD privacy gate at the edge so no PII reaches the analytics store: a forbidden `email`/`ip`/`ip_address`/`remote_addr` key anywhere in an event's `properties` is rejected, the `event_name` must be in a closed allow-list, and a single bad event never poisons the batch (`apps/analytics-worker/src/ingest.ts:85-112`, `apps/analytics-worker/src/ingest.ts:104-107`).

# Invariants

- The export audit row is emitted fail-CLOSED — a sink `Err` aborts with 503 before streaming `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105`.
- Analytics data access is gated per-tenant: the PAT gate rejects forged/wrong-tenant (401) or verifier fault (503) before reads `crates/corelink-container/src/routes/audit_analytics.rs:118`.
- The edge ingest tap is authenticated, never anonymous: an event with neither an allow-listed `Origin` nor a correct constant-time-matched `X-Corelink-Ingest-Key` is rejected 403 before any D1 write, and the key compare is length-checked + XOR-folded so a wrong key cannot be timing-probed (`apps/analytics-worker/src/ingest.ts:127-136`, `apps/analytics-worker/src/ingest.ts:71-78`).
- PII can never land in the analytics store: the edge validator hard-rejects any event whose `properties` carries an `email`/`ip`/`ip_address`/`remote_addr` field, so the privacy rule is enforced at the gate, not left to each caller (`apps/analytics-worker/src/ingest.ts:104-107`).
- A mid-stream abort surfaces as sysexits DATAERR (65) on the CLI, distinct from generic exit 1 — the `EXIT_DATAERR = 65` constant and the `AbortedMidStream => EXIT_DATAERR` mapping (`tools/cli/src/commands/verify_ndjson_http.rs:80`, `tools/cli/src/commands/verify_ndjson_http.rs:141`), wired in `main` (`tools/cli/src/main.rs:410-411`).
- The bearer token is never logged, printed, or surfaced in `Display`/error messages (CTRL-CRED-001):
  the `HttpVerifyOutcome` `Display` impl is "intentionally human-readable and never includes the
  Bearer" per its doc-comment `tools/cli/src/commands/verify_ndjson_http.rs:102-104`. (The Bearer IS,
  correctly, attached to the outbound request itself — `Authorization` is set on the request builder at
  `tools/cli/src/commands/verify_ndjson_http.rs:219-229`; the invariant is about output surfaces, not the wire.)

# Gotchas

- The HTTP-fetch path caps the response body at 64 MiB so a malicious server cannot drain CLI memory — `const MAX_BYTES: usize = 64 * 1024 * 1024` (`tools/cli/src/commands/verify_ndjson_http.rs:370`, `tools/cli/src/commands/verify_ndjson_http.rs:378`).
- If both the `--chain-head-anchor` flag and the response header are present they MUST match constant-time — a mismatch is an error, not a warning. The enforcer is `ascii_eq_ct(flag.trim(), header.trim())` at the resolve site `tools/cli/src/commands/verify_ndjson_http.rs:294` (refuses on disagreement), backed by the constant-time helper `fn ascii_eq_ct` at `tools/cli/src/commands/verify_ndjson_http.rs:348` (`docs/cli/audit-export.md:51-55`).

# Citations

1. `crates/corelink-container/src/routes/audit_export/audit_sink.rs:100-105` — `emit_or_503` fail-closed export audit (the enforcing impl).
2. `crates/corelink-container/src/routes/audit_export/state.rs:85`, `crates/corelink-container/src/routes/audit_export/state.rs:154` — export `build_state` + `router` impl; `crates/corelink-container/src/routes/audit_export.rs:146-148` — barrel re-export of that public surface.
3. `crates/corelink-container/src/routes/audit_export/stream.rs:412-417` — `mid_stream_abort_trailer_value` builder.
4. `crates/corelink-container/src/routes/audit_export/types.rs:42-57` — chain-head-anchor + abort header constants.
5. `crates/corelink-container/src/routes/audit_analytics/state.rs:80` — analytics `router`; `crates/corelink-container/src/routes/audit_analytics/state.rs:63` + `crates/corelink-container/src/routes/audit_analytics/rate_limit.rs:99` — per-tenant rate-limit config + `rate_limit_check`.
6. `crates/corelink-container/src/routes/audit_analytics.rs:118` — per-handler native PAT possession gate (in-barrel impl).
7. `docs/cli/audit-export.md:10-31` — CLI offline (file) re-verify mode.
8. `docs/cli/audit-export.md:33-55` — CLI HTTP-aware re-verify + anchor cross-check.
9. `docs/cli/audit-export.md:79-89` — exit-65 sysexits DATAERR on mid-stream abort (spec).
9b. `tools/cli/src/commands/verify_ndjson_http.rs:80`, `tools/cli/src/commands/verify_ndjson_http.rs:141`, `tools/cli/src/main.rs:410-411` — `EXIT_DATAERR = 65` + `AbortedMidStream => EXIT_DATAERR` mapping + `main` wiring (the enforcer).
10. `docs/cli/audit-export.md:91-99` — token never logged (CTRL-CRED-001) + constant-time anchor compare (spec).
10b. `tools/cli/src/commands/verify_ndjson_http.rs:102-104` — the `HttpVerifyOutcome` `Display` doc-comment guaranteeing the Display impl never includes the Bearer (the output-surface enforcer). NOTE: `:104` is a doc-comment, NOT a request builder — the outbound request DOES set `Authorization` (`:219-229`); the never-include guarantee is about Display/error output, not the wire.
10c. `tools/cli/src/commands/verify_ndjson_http.rs:294` / `:348` — `ascii_eq_ct(flag.trim(), header.trim())` constant-time anchor flag-vs-header cross-check (refuses on disagreement) + the `fn ascii_eq_ct` constant-time helper (the enforcer of the constant-time-anchor invariant).
11. `docs/cli/audit-export.md:100-102` — 64 MiB response-body cap (spec).
11b. `tools/cli/src/commands/verify_ndjson_http.rs:370`, `tools/cli/src/commands/verify_ndjson_http.rs:378` — `MAX_BYTES = 64 MiB` body cap (the enforcer).
12. `crates/corelink-container/src/routes/dsr/audit.rs:7-8`, `crates/corelink-container/src/routes/dsr/audit.rs:82-84` — the live audit sink writes *unchained* CloudEvents to `audit_outbox` (`digest=NULL`, `emitted_at=NULL`, no `prev_hash`/`sequence_number`); nothing ever seals those columns, so the deployed trail is a mutable D1 table the app's own credential can rewrite undetectably; chain-sealing is the unbuilt WI-S09-007, so the exported rows are unsealed — there is no live chain to re-verify yet.
13. `apps/analytics-worker/src/ingest.ts:114-136` — the edge `POST /v1/event` collector entry (CORS-origin OR ingest-key auth, 403 otherwise); the dual-auth 403 gate: `apps/analytics-worker/src/ingest.ts:127-136`; the constant-time key compare `constantTimeEqual`: `apps/analytics-worker/src/ingest.ts:71-78`.
14. `apps/analytics-worker/src/ingest.ts:85-112` — the `validate()` event gate (closed `event_name` allow-list + size bounds); the hard PII privacy gate forbidding `email`/`ip`/`ip_address`/`remote_addr` in `properties`: `apps/analytics-worker/src/ingest.ts:104-107`.
