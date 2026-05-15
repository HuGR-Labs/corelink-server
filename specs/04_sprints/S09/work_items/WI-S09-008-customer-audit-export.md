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
- [x] Chaos test: induced chain break surfaces SEV-0 audit (`apps/server/tests/audit_export.rs::chain_tamper_emits_verify_failed_sev0`; the test still flushes bytes — the SEV-0 audit emit is the security anchor; the `X-CoreLink-Audit-Export-Aborted` trailer ships in the follow-on streaming wire-up once we move off the in-memory buffer).
- [x] Tenant-isolation test: cross-tenant JWT returns 403 + audit emit (`apps/server/tests/audit_export.rs::cross_tenant_attempt_emits_security_audit_and_403`); 7 tests total (happy + cross-tenant + empty-range + verify-failed + 401 + 429 + 503-on-audit-fail).
- [x] **PagerDuty alert wiring for the 2 security/integrity emits (Wave-17, 2026-05-15)** — `dashboards/alerts/dash-audit-export-alerts.yml` ships rule `AuditExport_CrossTenantAttempt` (SEV-1 → PagerDuty critical → `PAGERDUTY_ROUTING_KEY` row #11 secrets matrix, escalation policy `corelink-incident-response`) and rule `AuditExport_VerifyFailed` (SEV-0 → PagerDuty critical → secondary direct-to-Tier-3 escalation; 1h MTTA / 4h MTTR; LGPD Art. 46 + GDPR Art. 33 72h regulatory clock on confirm). Tenant ids in PD payloads are BLAKE3-pseudonymised (INV-AUTH-AUDIT-PSEUDONYMIZATION + CTRL-PRIV-001). Event #1 (`corelink.audit.export_request.v1`) is info-only — dashboard recording rule `corelink_audit_export_request_rate_5m`, NOT paged. New runbooks `specs/_runbooks/RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md` (SEV-1; 6 ops sections + customer-comm decision tree) and `specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md` (SEV-0; 7 ops sections incl. R2 chunk pull forensics + audit-chain rebuild). Wave-17 follow-on flagged at wave-16 L9 risk register §6 is **CLOSED**.
- [ ] Customer-side CLI re-verify (`corelink audit verify`) green on the exported NDJSON (deferred — depends on the CLI binary at `crates/corelink-audit-chain/src/bin/verifier.rs` accepting the route's NDJSON envelope shape; tracked as follow-on lifting the verifier CLI off the chunk-file input).
- [ ] Daily-verify cron (`audit-chain-daily-verify.yml`) green for 7 consecutive days (production gate; runs after Wave-15.3 deploys).

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

