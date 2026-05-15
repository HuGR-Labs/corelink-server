---
id: "WI-S09-008"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "0.1.0"
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
tags: ["wi", "s09", "audit-export", "customer-facing", "compliance", "soc2", "gdpr-art-15", "high-risk"]
---

# WI-S09-008 — Customer-facing `/v1/audit/export` Endpoint (streaming NDJSON + cryptographic inclusion proofs; SOC 2 CC7.2 / GDPR Art. 15+20 / LGPD Art. 9+18 portability; reads from Wave-15 R2 archive producer; reuses `ChainVerifier` + `verify_inclusion_proof` pure-logic primitives; fail-CLOSED per Lote 10.6bis)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
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
- [ ] CF Worker `GET /v1/audit/export` endpoint wired (this WI).
- [ ] Integration test: 7-day rolling window, 1000 events, full proof verification round-trip.
- [ ] Chaos test: induced chain break mid-stream surfaces SEV-0 audit + `X-CoreLink-Audit-Export-Aborted` trailer.
- [ ] Tenant-isolation test: cross-tenant JWT returns 403; cross-tenant event in window returns 500 with TenantIsolationViolation.
- [ ] Customer-side CLI re-verify (`corelink audit verify`) green on the exported NDJSON.
- [ ] Daily-verify cron (`audit-chain-daily-verify.yml`) green for 7 consecutive days.

---

## 7. Open questions

1. Does the export proof anchor get co-signed by a KMS-managed signing key (for stronger non-repudiation) or rely on R2 bucket immutability alone? Default: rely on R2 Object Lock for the storage-layer immutability + add a `manifest_sig` HMAC over the export header for transport integrity (cheap; no KMS dependency).
2. Rate-limit the endpoint to 1 concurrent export per tenant? Default: yes; the workflow takes minutes for large windows + concurrent runs would thrash R2 list throughput.

---

## 8. Cross-references

- `WI-S09-004` — parent CloudEvents emitter + daily verifier (lift inheritance).
- `WI-R-PREP-AUDIT-EXPORT` — pure-logic exporter primitive (lift inheritance).
- `specs/_audits/2026-05-15-audit-chain-retention.md` — 7-year retention mechanism + property test stub.
- `specs/_runbooks/RB-AUDIT-EXPORT-INTEGRITY.md` — operator response for SEV-0 chain-break-mid-export.
- `.github/workflows/audit-chain-daily-verify.yml` — Wave-15 daily verifier cron.
- `crates/corelink-audit-chain/src/archive_producer.rs` — Wave-15 R2 NDJSON archive producer.
- `crates/corelink-audit-chain/src/bin/verifier.rs` — Wave-15 daily-verify CLI.
