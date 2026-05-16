---
id: "WI-S09-008"
type: "work_item"
doc_status: "SEALED"
work_status: "READY"
audit_status: "ACTIVE"
version: "0.3.0"
created: "2026-05-15"
updated: "2026-05-15"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-09"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "PRIVACY-MODEL"
tags: ["wi", "s09", "audit-export", "customer-facing", "compliance", "soc2", "gdpr-art-15", "high-risk", "wave-17-pd-wired"]
---

# WI-S09-008 — Customer-facing `/v1/audit/export` Endpoint (streaming NDJSON + cryptographic inclusion proofs; SOC 2 CC7.2 / GDPR Art. 15+20 / LGPD Art. 9+18 portability; reads from Wave-15 R2 archive producer; reuses `ChainVerifier` + `verify_inclusion_proof` pure-logic primitives; fail-CLOSED per Lote 10.6bis)

> **doc_status:** SEALED · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-09](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S09-008 |
| Título | Customer-facing audit-export endpoint streaming NDJSON of every audit event in a customer-specified time window, with cryptographic inclusion proof per row; reads from the Wave-15 R2 archive producer chunks (`audit/<YYYY>/<MM>/<DD>/<8-digit-sequence>.ndjson`); reuses `corelink-audit-chain::AuditExporter` + `verify_inclusion_proof` primitives; tenant-scoped via JWT + per-tenant chain partitioning; fail-CLOSED on any verifier mismatch; **lift from WI-R-PREP-AUDIT-EXPORT (the pure-logic exporter primitive); this WI wires the CF Worker endpoint** |
| Parent Sprint | S-09 |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (compliance evidence chain — SOC 2 + GDPR + LGPD) |
| Inherits | OBSERVABILITY-MODEL §audit-chain-architecture; SECURITY-MODEL §authn-zerotrust; INVARIANT-REGISTRY §INV-OBS-AUDIT-CHAIN-INTEGRITY + §INV-TENANT-ISOLATION; PRIVACY-MODEL §portability-rights |

---

## 1. Resumo

The customer-facing `GET /v1/audit/export` endpoint streams an immutable NDJSON dump of every audit event in a customer-specified `[since, until]` time window for a tenant identified by JWT. Each NDJSON line carries:

1. The canonical `AuditEvent` body (CloudEvents 1.0 shape, identical to the on-R2 chunk shape).
2. An `InclusionProof` (BLAKE3 chain-link sibling list from the event to the window's `chain_head_at_export` anchor).
3. The window's `chain_head_at_export` anchor (the running chain head AT the moment the window was sealed; published by the daily-verify cron + signed by the producer's `kms-admin` signing key — see `WI-R-PREP-AUDIT-EXPORT` for the signing-key discipline).

Customers re-verify the export offline via the `corelink audit verify <export>` CLI (a thin wrapper around the `corelink-audit-chain::verify_export_result` pure-logic primitive). The endpoint reads from the Wave-15 R2 chunks produced by `archive_producer::ArchiveProducer` — no per-event R2 GETs; the endpoint streams chunk-by-chunk + skips chunks fully outside the window.

---

## 2. Goals / Non-goals

### 2.1 Goals

- Customer-initiated audit export covering up to the full 7-year retention window.
- Cryptographic inclusion proof per event (no naked NDJSON; the proof is the load-bearing compliance evidence).
- Streaming response (CF Worker `ReadableStream`; no full-window buffering — a 7-year tenant window can be TB-scale).
- Tenant isolation enforced at the JWT + chain-prefix layer (`audit/<tenant_id>/...` historical layout) AND at the Wave-15 chunk layout (`audit/<YYYY>/<MM>/<DD>/<seq>.ndjson` — the per-tenant filter happens at chunk-read time by parsing each NDJSON line's `tenant_id` field + dropping non-matching events).
- Fail-CLOSED on any chain-link mismatch during streaming: the verifier emits a SEV-0 audit + the HTTP response transitions to a terminal `X-CoreLink-Audit-Export-Aborted: chain-break-at-seq=<N>` trailer + 500 status.

### 2.2 Non-goals

- No support for ad-hoc SQL queries over the audit log — that's a separate evidence-store reporting product (`WI-S16-002` or follow-on).
- No deletion / redaction of events via this endpoint — the audit log is append-only per INV-AUDIT-APPEND-ONLY.
- No real-time push / WebSocket — customers poll on-demand.

---

## 3. Invariants enforced

- **INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH) — every exported event's `link_hash` recomputes against the running chain head; mismatch fails-CLOSED with SEV-0 audit emit.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+) — JWT `tenant_id` claim MUST equal every exported event's `tenant_id`; cross-tenant slice surfaces `AuditChainError::TenantIsolationViolation`.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL) — endpoint is read-only; no path that mutates R2 chunks.

---

## 4. API shape

```http
GET /v1/audit/export?since=<rfc3339>&until=<rfc3339>
Authorization: Bearer <jwt>
Accept: application/x-ndjson
```

Response:

```http
200 OK
Content-Type: application/x-ndjson
X-CoreLink-Audit-Export-Window-Since: <rfc3339>
X-CoreLink-Audit-Export-Window-Until: <rfc3339>
X-CoreLink-Audit-Export-Chain-Head-Anchor: <64-hex>
X-CoreLink-Audit-Export-Manifest-Sig: <base64>
Transfer-Encoding: chunked

{"event": <AuditEvent>, "proof": <InclusionProof>}
{"event": <AuditEvent>, "proof": <InclusionProof>}
...
```

Error responses (fail-CLOSED canonical):

- `400 Bad Request` — malformed since/until / since >= until.
- `401 Unauthorized` — JWT missing / invalid.
- `403 Forbidden` — JWT tenant scope mismatch.
- `500 Internal Server Error` + `X-CoreLink-Audit-Export-Aborted` trailer — chain-break detected mid-stream (SEV-0 page).

---

## 5. Implementation surface

| Layer | Crate / module | Surface |
|---|---|---|
| Pure-logic exporter | `corelink-audit-chain::exporter` | `AuditExporter` trait + `ExportResult` + `InclusionProof` (already shipped in `WI-R-PREP-AUDIT-EXPORT`) |
| R2 archive reader | `corelink-audit-chain::archive_producer` | NEW — `ArchiveProducer` produces chunks; this WI adds an inverse reader that streams chunks in a date-range + filters by tenant |
| CF Worker endpoint | `apps/corelink-worker::handlers::audit_export` | NEW — binds JWT + R2 + streaming NDJSON response |
| CLI verifier | `corelink-audit-chain::bin::verifier` (Wave-15) | Reused for customer-side offline re-verify |

---

## 6. Acceptance criteria

- [x] Pure-logic exporter primitive shipped (WI-R-PREP-AUDIT-EXPORT lift).
- [x] Wave-15 archive producer + chunk layout shipped (this wave, see `crates/corelink-audit-chain/src/archive_producer.rs`).
- [x] CF Worker `GET /v1/audit/export` endpoint wired (Wave-15.3; `apps/server/src/routes/audit_export.rs`; native-target trait-object wire-up against `InMemoryAuditExporter` + `corelink-ratelimit::InMemoryTokenBucketRateLimiter`; wasm32 CF-Worker slot pinned via `#[cfg(target_arch = "wasm32")]` compile-error per the cas/admin trait-abstraction-defer pattern).
- [x] Integration test: in-memory chain + window export, full proof verification round-trip (`apps/server/tests/audit_export.rs::happy_path_tenant_exports_own_audit_logs`; 5-event window; manifest footer + chain-head anchor header asserted).
- [x] Chaos test: induced chain break surfaces SEV-0 audit (`apps/server/tests/audit_export.rs::chain_tamper_emits_verify_failed_sev0`; the SEV-0 audit emit is the security anchor). **Streaming wire-up + abort trailer DONE wave-18** — `apps/server/src/routes/audit_export.rs` now wires `axum::body::Body::new(http_body_util::StreamBody::new(...))` with a per-row `verify_inclusion_proof` re-check against the manifest anchor; on the first chain-break detected mid-stream the route (a) emits the SEV-0 `corelink.audit.export_verify_failed.v1` audit row carrying `{break_at_seq, break_at_chunk, observed, expected}` BEFORE pushing (b) the `http_body::Frame::trailers` frame setting `X-CoreLink-Audit-Export-Aborted` to the canonical JSON payload — the body stream then closes immediately. The `Trailer:` response header advertises the abort trailer upfront per RFC 7230 §4.4. Wave-18 ships 3 new server integration tests (`streaming_response_does_not_buffer`, `abort_trailer_emitted_on_mid_stream_chain_break`, `customer_cli_handles_abort_trailer_gracefully`) on top of the wave-16 7 + 1 new CLI test (`streaming_wire_shape_round_trips_via_line_parser`); the wave-17 CLI parser remains byte-equivalent on the streaming wire shape. Commit `wave-18 / wt/r-prep-audit-export-streaming` (see `git log`).
- [x] Tenant-isolation test: cross-tenant JWT returns 403 + audit emit (`apps/server/tests/audit_export.rs::cross_tenant_attempt_emits_security_audit_and_403`); 7 tests total (happy + cross-tenant + empty-range + verify-failed + 401 + 429 + 503-on-audit-fail).
- [x] **PagerDuty alert wiring for the 2 security/integrity emits (Wave-17, 2026-05-15)** — `dashboards/alerts/dash-audit-export-alerts.yml` ships rule `AuditExport_CrossTenantAttempt` (SEV-1 → PagerDuty critical → `PAGERDUTY_ROUTING_KEY` row #11 secrets matrix, escalation policy `corelink-incident-response`) and rule `AuditExport_VerifyFailed` (SEV-0 → PagerDuty critical → secondary direct-to-Tier-3 escalation; 1h MTTA / 4h MTTR; LGPD Art. 46 + GDPR Art. 33 72h regulatory clock on confirm). Tenant ids in PD payloads are BLAKE3-pseudonymised (INV-AUTH-AUDIT-PSEUDONYMIZATION + CTRL-PRIV-001). Event #1 (`corelink.audit.export_request.v1`) is info-only — dashboard recording rule `corelink_audit_export_request_rate_5m`, NOT paged. New runbooks `specs/_runbooks/RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md` (SEV-1; 6 ops sections + customer-comm decision tree) and `specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md` (SEV-0; 7 ops sections incl. R2 chunk pull forensics + audit-chain rebuild). Wave-17 follow-on flagged at wave-16 L9 risk register §6 is **CLOSED**.
- [x] Customer-side CLI re-verify (`corelink audit verify-ndjson --ndjson <file> --chain-head-anchor <hex>`) green on the exported NDJSON (Wave 17; `crates/corelink-cli/src/commands/verify_ndjson.rs`; 6 unit tests — happy path 10-event round-trip, tampered chunk #3, wrong anchor, empty NDJSON, malformed proof JSON, mismatched chain-head; structured chain-break diagnostic with line + observed vs expected hash + kind; constant-time hash compare via `corelink_audit_chain::hashes_eq_ct`).
- [x] Daily-verify cron (`audit-chain-daily-verify.yml`) extended to a 7-day rolling matrix (Wave 17; today + today-1..today-6; SEV-0 marker `AUDIT_CHAIN_7DAY_BREAK_DETECTED::<date>::<chunk-key>` routes to PagerDuty via `PAGERDUTY_ROUTING_KEY`; production R2 list path behind `AUDIT_R2_BUCKET` + `CF_API_TOKEN` per branch `wt/r-prep-r2-list-cf-token` commit `275281e`; public CI smoke harness fixture-only; all `uses:` SHA-pinned per HIGH_RISK lane FF-HR-005). Wave-18 follow-on: integrate the paginated CF API v4 R2-list step inside each matrix-day job (current workflow has the 7-day matrix structure adopted; the per-day production R2 list lift remains tracked as `scripts/r2-audit-list-and-download.sh` placeholder).

### Wave-19 lift — true async page-by-page generator (2026-05-15)

Wave-18 shipped `axum::body::Body::new(StreamBody::new(...))` over a **pre-materialized** `Vec<Frame<Bytes>>` plan built up-front by `build_audit_export_stream_frames`. The wire shape was streaming-compliant (no `Content-Length`; per-row `Frame::data`) but the whole frame plan lived in memory before the first byte hit the socket — fine for tens of rows, doesn't scale to large export windows.

Wave-19 replaces the synchronous frame plan with `build_audit_export_async_stream` returning `impl Stream<Item = Result<Frame<Bytes>, Infallible>>` driven by `async_stream::stream!`. The generator pulls one page of rows at a time from a new [`R2ListPager`] trait (per-page key budget = [`R2_LIST_PAGE_SIZE`] = 1000; default mirrors the wave-17/18 CF API v4 `per_page=1000` walk), runs the per-row inclusion-proof re-verify inside the page loop, and yields one `Frame::data` per row. The trailing manifest frame fires only after `R2ListPager::next_page` returns `None`. On a mid-stream chain break the generator emits the SEV-0 `corelink.audit.export_verify_failed.v1` audit row via `emit_mid_stream_break_audit` **BEFORE** yielding the `Frame::trailers` carrying `X-CoreLink-Audit-Export-Aborted` — the audit-anchor-BEFORE-trailer invariant is now pinned by a **10 000-iter property test** (`audit_anchor_emits_before_trailer_under_random_breaks` randomizes row count `n ∈ [1, 16]`, page size `p ∈ [1, 8]`, and tenant seed).

Memory bound: at any point one page of rows + one row's serialized buffer are live; the generator parks on each `yield` so the consumer (axum body-flow) controls when the next page is fetched (back-pressure-respecting; no buffer-ahead of multiple pages). The per-row buffer capacity hint is tunable via `EXPORT_ROW_BUFFER_BYTES` env var (default 65 536 bytes via `DEFAULT_EXPORT_ROW_BUFFER_BYTES`; rows above the hint grow naturally via `Vec::with_capacity` semantics). The [`R2ListPager`] trait is `Send + Sync + async_trait` + `Debug`; the in-memory [`InMemoryR2ListPager`] fake slices a `Vec<ExportedAuditEvent>` into `page_size`-bounded pages and powers every wave-19 unit + integration test (no real R2 binding required). The production CF Worker wire-up slots a real adapter implementing `next_page` against the CF API v4 `GET /accounts/{account_id}/r2/buckets/{bucket}/objects?prefix=...&cursor=...` walk lifted from the wave-17/18 daily-verify cron pattern.

Tests net-new (wave-19): +9 server unit (`r2_list_page_size_matches_cf_api_v4_default`, `default_export_row_buffer_bytes_is_64kib`, `in_memory_pager_returns_pages_then_none`, `in_memory_pager_empty_rows_returns_one_empty_page_then_none`, `in_memory_pager_clamps_zero_page_size_to_default`, `async_stream_yields_rows_across_pages_then_manifest`, `async_stream_audit_anchor_emits_before_trailer_on_mid_page_break`, `async_stream_empty_range_yields_only_manifest_frame`, `audit_anchor_emits_before_trailer_under_random_breaks` proptest @ 10 000 iter) + 1 server integration (`streaming_response_yields_all_rows_across_multiple_pages` — 9 rows + `pager_page_size=3` → 3 pages, asserts no `Content-Length`, `Trailer:` advertised, 9 row lines + 1 manifest line preserved at the wire level). All 10 wave-18 integration tests + 14 wave-18 unit tests preserved bit-for-bit (24/10/7 baseline → 23 unit + 11 integration + same 7 wave-16 CLI / audit-export tests in `corelink-audit-chain`). Branch `wt/r-prep-audit-export-async-pages`; commit see `git log -- apps/server/src/routes/audit_export.rs`.

---

## 7. Open questions

1. Does the export proof anchor get co-signed by a KMS-managed signing key (for stronger non-repudiation) or rely on R2 bucket immutability alone? Default: rely on R2 Object Lock for the storage-layer immutability + add a `manifest_sig` HMAC over the export header for transport integrity (cheap; no KMS dependency).
2. Rate-limit the endpoint to 1 concurrent export per tenant? Default: yes; the workflow takes minutes for large windows + concurrent runs would thrash R2 list throughput.

---

## 8. Cross-references

- `WI-S09-004` — parent CloudEvents emitter + daily verifier (lift inheritance).
- `WI-R-PREP-AUDIT-EXPORT` — pure-logic exporter primitive (lift inheritance).
- `specs/_audits/2026-05-15-audit-chain-retention.md` — 7-year retention mechanism + property test stub + Wave-15.3 endpoint wire-up reference.
- `specs/_runbooks/RB-AUDIT-EXPORT-INTEGRITY.md` — operator response for SEV-0 chain-break-mid-export.
- `.github/workflows/audit-chain-daily-verify.yml` — Wave-15 daily verifier cron.
- `crates/corelink-audit-chain/src/archive_producer.rs` — Wave-15 R2 NDJSON archive producer.
- `crates/corelink-audit-chain/src/bin/verifier.rs` — Wave-15 daily-verify CLI.
- `apps/server/src/routes/audit_export.rs` — **Wave-15.3** customer-facing endpoint wire-up (this commit).
- `apps/server/tests/audit_export.rs` — **Wave-15.3** integration test suite (7 tests).

---

## 9. Wave-15.3 wiring summary (2026-05-15)

The `GET /v1/audit/export?from=&to=&tenant=` route landed in `apps/server/src/routes/audit_export.rs` with:

- **Auth boundary** — production middleware injects the JWT-validated tenant id via the `X-Tenant-Id` request header (mirrors the cas + admin wire-up patterns); the route rejects missing / malformed headers with `401 Unauthorized`. The optional `tenant` query parameter is the cross-tenant attempt surface: a mismatch with the authenticated principal emits the SEV-1 security audit row `corelink.security.audit_export_cross_tenant_attempt.v1` BEFORE the `403 Forbidden` (fail-CLOSED ordering); tenant compare via `subtle::ConstantTimeEq`.
- **Rate limit** — bound to the existing `corelink-ratelimit::InMemoryTokenBucketRateLimiter` framework (WI-S08-001); bucket key `BucketKey::per_tenant_per_endpoint(tenant_id, "audit.export")`; burst = 1, refill = 1 tps, `Retry-After` floor = 60s. The second request inside the same minute receives `429 Too Many Requests` + a populated `Retry-After` header + an audit row with `exit_status="rate_limited"`.
- **Audit emit boundary (3 events)** — `corelink.audit.export_request.v1` (every request; carries tenant + from + to + bytes_written + events_written + exit_status); `corelink.security.audit_export_cross_tenant_attempt.v1` (SEV-1); `corelink.audit.export_verify_failed.v1` (SEV-0; emitted alongside delivering the bytes — caller decides what to trust). Audit-sink failure on the request emit fails CLOSED (`503 Service Unavailable` — never serve bytes without the audit row, per INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER).
- **NDJSON envelope** — `Content-Type: application/x-ndjson`; one row per line `{"event": <AuditEvent>, "proof": <InclusionProof>}`; trailing manifest line `{"manifest": <ExportManifest>}` carrying `chain_head_at_export`. The chain head also surfaces as the `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header (64-char BLAKE3 hex).
- **Empty range semantics** — a `[from, to)` window with zero matching events returns `200 OK` with the manifest-only body (empty audit history is a legitimate state; NOT `404`).
- **Verify pipeline** — every export runs through `corelink_audit_chain::verify_export_result` BEFORE the bytes are flushed. A `ChainBreak` surfaces the SEV-0 audit row + the bytes still ship so the customer-side CLI can attempt independent verification.

Quality gates closed:

| Gate | Status |
|---|---|
| `cargo build -p corelink-server` | green |
| `cargo test -p corelink-server --test audit_export` | 7 tests pass |
| `cargo test -p corelink-server --lib` | 47 tests pass |
| `cargo clippy -p corelink-server --tests -- -D warnings` | green |
| `python3 scripts/validate_specs.py` | green |

---

## 10. Wave-17 PagerDuty wiring closure (2026-05-15)

The wave-16 commit `9eaacd8` (audit-export endpoint) introduced 3
audit event types but deferred the PagerDuty alert wiring to wave-17
(flagged in the wave-16 L9 risk register §6). This wave-17 closure
ships:

| Audit event type | Severity | Routing | Rule | Runbook |
|---|---|---|---|---|
| `corelink.audit.export_request.v1` | info | dashboard only | recording rule `corelink_audit_export_request_rate_5m` | n/a (info) |
| `corelink.security.audit_export_cross_tenant_attempt.v1` | SEV-1 | PagerDuty critical via `PAGERDUTY_ROUTING_KEY` (row #11 secrets matrix); escalation policy `corelink-incident-response` | `AuditExport_CrossTenantAttempt` in `dashboards/alerts/dash-audit-export-alerts.yml` | `specs/_runbooks/RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md` |
| `corelink.audit.export_verify_failed.v1` | SEV-0 | PagerDuty critical via `PAGERDUTY_ROUTING_KEY` + secondary `corelink-incident-response-tier-3-direct` escalation; 1h MTTA / 4h MTTR; LGPD Art. 46 + GDPR Art. 33 72h regulatory clock starts on confirm | `AuditExport_VerifyFailed` in `dashboards/alerts/dash-audit-export-alerts.yml` | `specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md` |

**Privacy guarantee:** PagerDuty payloads use the BLAKE3-pseudonymised
`tenant_id_hex8` (first 8 hex of BLAKE3(tenant_id)) for the metric
label per INV-AUTH-AUDIT-PSEUDONYMIZATION + CTRL-PRIV-001 +
INV-OBS-CARDINALITY-BUDGET. The raw tenant_id is resolvable
only from the admin audit-viewer with dual-approval (`WI-S16-005`).

**Secrets matrix:** no new secrets — `PAGERDUTY_ROUTING_KEY` at row
#11 (`docs/internal/secrets-checklist.md`) is reused unchanged.

**Quality gates (wave-17):**

| Gate | Status |
|---|---|
| `python3 scripts/validate_specs.py` (2 new runbooks parse with frontmatter) | green |
| `python3 scripts/validate_references.py` (no new dangling refs) | green |
| `python3 scripts/validate_secrets_matrix.py` (row #11 unchanged) | green |
| `cargo build` / `clippy` / `test` | n/a (no Rust changes — alert rules are dashboard-side; the route already emits the 3 events from wave-16) |

## 11. Wave-17 closure summary — CLI verify-ndjson + 7-day daily-verify cron (2026-05-15)

Wave 17 closes the two open ACs from Wave-15.3.

**Customer-CLI NDJSON re-verify (`crates/corelink-cli/src/commands/verify_ndjson.rs`):**

- New subcommand `corelink audit verify-ndjson --ndjson <file> --chain-head-anchor <hex>`.
- Reads the NDJSON envelope emitted by `GET /v1/audit/export` (one
  `{event, proof}` line per audit row + a trailing
  `{"manifest": <ExportManifest>}` line).
- For each row: recomputes the BLAKE3 link via
  `corelink_audit_chain::link_chain_hash(event.prev_hash, event)`,
  compares constant-time against the claimed `proof.link_hash`.
- Walks chain continuity forward: row N+1's `event.prev_hash` MUST
  equal row N's recomputed link hash.
- Final assertions: the recomputed final hash MUST equal both the
  manifest `chain_head_at_export` AND the customer-supplied
  `--chain-head-anchor` (the value the server published in the
  `X-CoreLink-Audit-Export-Chain-Head-Anchor` response header).
- Constant-time hash compare via `corelink_audit_chain::hashes_eq_ct`
  (no short-circuit on the first differing byte).
- Exit 0 on full chain integrity; exit 1 with a structured chain-break
  diagnostic carrying `{file, line, observed, expected, kind}`
  (`kind=link_recompute` or `kind=continuity`). Error messages are
  informative + actionable per WI charter.
- 6 unit tests: happy path (10 events round-trip), tampered chunk #3
  one-byte flip, wrong anchor, empty NDJSON, malformed proof JSON,
  mismatched chain-head in manifest.
- README "Offline audit-chain verify" section documents the usage.

**7-day daily-verify cron (`.github/workflows/audit-chain-daily-verify.yml`):**

- Extends Wave-15 cron with a `seven-day-verify` job iterating today,
  today-1, ..., today-6 (default `DEFAULT_WINDOW_DAYS=7`; tunable via
  `workflow_dispatch` input `window_days` in `[1, 30]`).
- For each day in the window: lists `audit/<YYYY>/<MM>/<DD>/*.ndjson`
  R2 keys (production: wrangler r2 list under `AUDIT_R2_BUCKET` +
  `CF_API_TOKEN`; public CI: fixture-only smoke), downloads each
  chunk, invokes the verifier binary on the day's chunk paths, greps
  for `AUDIT_CHAIN_BREAK_DETECTED`.
- SEV-0 alert marker `AUDIT_CHAIN_7DAY_BREAK_DETECTED::<date>::<chunk-key>`
  routes to PagerDuty via `PAGERDUTY_ROUTING_KEY` (per RB-AUDIT-CHAIN-001 §3).

**Wave-18 lift (2026-05-15) — paginated CF API v4 inside each matrix day:**

- Lifted the paginated Cloudflare API v4 R2 list step from
  `wt/r-prep-r2-list-cf-token` (commit `275281e`) INSIDE the
  `seven-day-verify` job's per-day loop. Each of the 7 matrix days now
  performs the full production list/GET/verify/page cycle independently:
  - `GET /accounts/{account_id}/r2/buckets/{bucket}/objects?prefix=audit/<YYYY>/<MM>/<DD>/`
    paginated (per_page=1000, `result_info.cursor` walk, cap 100 pages
    = 100k keys / fanout-guard).
  - Per-object `GET /accounts/{account_id}/r2/buckets/{bucket}/objects/{key}`
    download to `./.audit-verify-staging/<date>/chunks/<r2-key>`.
  - Verifier on argv; on `AUDIT_CHAIN_BREAK_DETECTED` emit the
    `AUDIT_CHAIN_7DAY_BREAK_DETECTED::<date>::<chunk-key>` marker AND
    dispatch PagerDuty Events API v2 trigger (dedup
    `audit-chain-break-<date>-<chunk>` — per-day dedup so N bad days
    surface as N distinct PD pages).
- **Per-day fail-CLOSED + continue matrix** contract: list 5xx/auth/JSON
  parse/cursor errors, per-object GET non-2xx, and verifier non-zero
  exit all mark the day BROKEN, page PD, and continue to the next day
  (one bad day does NOT short-circuit the remaining 6). Job exits
  non-zero IFF ≥1 day surfaced a break.
- SHA-pin audit: 528 `uses:` lines pinned, no new actions introduced
  (CF + PD dispatch are pure curl).
- `permissions: contents: read` preserved (no `id-token: write` —
  production R2 access stays on `CF_API_TOKEN`).
- `concurrency: audit-chain-daily-verify` group added to prevent
  overlapping cron + workflow_dispatch runs.
- `permissions: contents: read` only (no `id-token: write` — production
  R2 access uses `CF_API_TOKEN` secret, not OIDC).
- All `uses:` SHA-pinned (40-char) per HIGH_RISK lane FF-HR-005;
  smoke job runs as Job 1 (cron-safe zero-input no-op), 7-day job
  runs as Job 2 with `needs: smoke-verify` dependency.
- Workflow YAML parses cleanly via `python3 -c "import yaml; yaml.safe_load(...)"`.

Quality gates closed (Wave 17):

| Gate | Status |
|---|---|
| `cargo build -p corelink-cli` | green |
| `cargo test -p corelink-cli` (commands::verify_ndjson 6 tests) | green |
| `cargo clippy -p corelink-cli --tests -- -D warnings` | green |
| `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/audit-chain-daily-verify.yml'))"` | green |
| `python3 scripts/verify-action-sha-pinning.py` (528 uses pinned) | green |
| `python3 scripts/validate_specs.py` | green |
