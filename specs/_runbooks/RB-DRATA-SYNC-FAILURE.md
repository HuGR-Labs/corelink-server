---
id: "RB-DRATA-SYNC-FAILURE"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-R5P-SOC2-DRATA"
tags: ["runbook", "r5p", "soc2", "drata", "evidence-sync", "fail-open", "sla-24h"]
---

# RB-DRATA-SYNC-FAILURE — Drata evidence sync failure response

> **Owned by:** WI-R5P-SOC2-DRATA. **Triggered by:** any of (a) the CF Cron Worker `drata-evidence-sync` reports non-zero `failed` count for ≥ 1 consecutive run; (b) Prometheus alert `corelink_drata_evidence_backlog_seconds > 86400` (24h SLA breach); (c) Drata Trust Center shows "stale evidence" for any of the six streams.
>
> **SLA summary:** end-to-end Drata evidence backlog MUST stay < 24h. If the backlog exceeds 24h, manual evidence upload through the Drata web UI is the canonical fallback (§4). Type I audit fieldwork tolerates one fallback per 30-day window; more triggers re-evaluation of the Drata integration.

---

## 1. Trigger detection

A Drata sync failure response is triggered by any of:

1. CF Cron Worker `drata-evidence-sync` (schedule `0 3 * * *` UTC) emits a structured log line `outcome={sent,skipped,failed,total} stream=*` where `failed > 0` for ≥ 1 daily run.
2. Prometheus alert `DrataEvidenceBacklogSlaBreach` (gauge `corelink_drata_evidence_backlog_seconds`) fires past 86400 (24h).
3. Drata Trust Center → Continuous Monitoring tab shows a red dot on any of the six CoreLink streams (Audit Logs / Access Reviews / Credential Management / Change Management / Incident Response / Vulnerability Management).
4. Auditor (Schellman / A-LIGN) flags during PBC review that an evidence sample is missing for an in-scope date.

If ANY of (1..4) occurs, this runbook applies.

---

## 2. Roles

| Role | Responsibilities |
|---|---|
| On-call SRE | First responder. Triages the failure mode (auth / transport / Drata edge / D1 ledger) and decides whether to retry, manual upload, or freeze the cron. |
| Compliance Officer | Validates the manual-upload fallback content; signs off on the `out-of-band evidence` log entry in the audit chain. |
| Owner (Gustavo Schneiter) | Approves any Drata API key rotation; signs the post-incident review. |
| Auditor (read-only) | Receives a courtesy Slack message via `#corelink-compliance-auditor` when the fallback fires during fieldwork. |

---

## 3. Diagnosis

```
# 1. Inspect the last cron run audit envelope (canonical truth).
$ wrangler tail drata-evidence-sync --format json \
  | jq 'select(.event_type | startswith("corelink.compliance.drata_evidence"))'

# 2. Backlog age per stream (D1 view).
$ wrangler d1 execute corelink-prod --command \
  'SELECT stream, sent_count_24h, oldest_sent_at_ms FROM drata_evidence_sent_backlog_24h;'

# 3. Drata edge probe.
$ curl -sSf -H "Authorization: Bearer $DRATA_API_KEY" \
  https://api.drata.com/v1/evidence/audit-logs/health
```

Failure modes from the audit-chain `reason` field:

| Reason pattern | Cause | Goto |
|---|---|---|
| `status 401` / `status 403` | API key rotated or revoked | §4.1 |
| `status 4xx` (other) | Schema drift on Drata side OR malformed metadata | §4.2 |
| `status 5xx` after `attempts=4` | Drata edge degraded | §4.3 |
| `transport: ...` | Network egress from CF Worker degraded | §4.3 |
| `idempotent: ledger hit` (mass-skipped) | Ledger corrupted (false hit) | §4.4 |

---

## 4. Response procedures

### 4.1 API key rotation (auth failure)

1. Confirm rotation cause: check Drata Admin → Settings → API Keys for "last rotated" timestamp.
2. Generate fresh key in Drata UI; copy.
3. Update Cloudflare secret: `wrangler secret put DRATA_API_KEY --env production`.
4. Manually re-run the cron: `wrangler cron trigger drata-evidence-sync --env production`.
5. Confirm next audit envelope shows `corelink.compliance.drata_evidence_sent` with the new redacted key suffix.
6. Append to chain: emit `corelink.security.secret_rotation` with payload `{secret: "drata_api_key", reason: "rotation_response", actor: "<sre-slug>"}`.
7. SLA: complete steps 1-5 within **2 hours** of trigger.

### 4.2 Schema drift / 4xx (other than 401/403)

1. Capture the rejected payload from the chain: `record_sha256` → `wrangler d1 execute corelink-prod --command "SELECT * FROM audit_outbox WHERE sha256 = '<hash>'"`.
2. Diff against the canonical [`EvidenceRecord`](../../crates/corelink-drata-sync/src/record.rs) schema; identify the offending field (typical cause: new metadata key in `audit_outbox` not yet allowlisted in the source-to-record mapper).
3. Patch the mapper; ship a hotfix release through STANDARD lane (no rollback needed — the failed record stays in the source table and replays on next cron after the fix is deployed).
4. Manual upload fallback (§4.5) covers the gap while the patch ships if the auditor calendar is < 24h away.
5. SLA: hotfix merged within **8 hours**; backlog < 24h after redeploy.

### 4.3 Drata edge degraded / transport exhausted

1. Check Drata status page: https://status.drata.com.
2. If Drata-side incident is confirmed, file a support ticket referencing every failed `record_sha256` from the chain. Drata SLA: 4h response, 24h resolution.
3. Engage manual-upload fallback (§4.5) preemptively if the auditor calendar is < 48h away.
4. The cron is safe to re-trigger every 30 min while Drata is degraded — idempotency guarantees no duplicates. Run via `wrangler cron trigger drata-evidence-sync` until the failure count clears.
5. SLA: drain backlog within **24h** of Drata recovery.

### 4.4 Ledger corruption (false idempotency hit)

1. Confirm: pick three random records from the last 24h whose audit envelope shows `Skipped` and check the Drata UI shows no corresponding receipt id.
2. Mark the affected rows for replay: `wrangler d1 execute corelink-prod --command "DELETE FROM drata_evidence_sent WHERE sent_at_ms < <epoch_ms_threshold> AND receipt_id NOT LIKE 'rcp_%';"` (paranoia DELETE — re-pushes are safe because Drata's own `Idempotency-Key` header deduplicates on its side too).
3. Manually re-run the cron; verify the next batch shows `sent > 0` for previously-skipped records.
4. File a post-incident review (`specs/_postmortems/`) referencing the affected hash ranges. Root cause: D1 replication lag or schema migration mid-flight.
5. SLA: ledger reconciled within **4 hours**; the audit chain remains the canonical source for any external auditor question.

### 4.5 Manual evidence upload fallback

Used when the automated path cannot drain the backlog within 24h.

1. Export the affected source rows to CSV:
   - audit_outbox → `audit-logs.csv`
   - tenants + access_review_events → `access-reviews.csv`
   - pat + pat_revocations → `credential-management.csv`
   - github_events → `change-management.csv`
   - pagerduty_incidents → `incident-response.csv`
   - pentest_findings → `vulnerability-management.csv`
2. Strip PII per CTRL-PRIV-001 (drop tenant emails, customer names, raw PAT strings; replace with `tenant_hash` and `pat_id` opaque ids).
3. Upload via Drata UI → Evidence Library → Manual Upload → select stream → drag CSV → tag with sprint id + date range.
4. After upload, append one row per CSV to the audit chain emitting `corelink.compliance.drata_evidence_out_of_band` with payload `{stream, row_count, sha256_of_csv, uploaded_by, upload_reason}`.
5. After manual upload completes, write the affected `record_sha256` values into `drata_evidence_sent` with `receipt_id = "manual-upload-<ticket-id>"` so the cron does not re-push the same content on the next tick.

---

## 5. Post-incident review

A post-incident review is REQUIRED whenever:

- The manual-upload fallback fires AND the cause was NOT a Drata-side incident (RB applies §4.5 plus its own causation).
- A single Drata sync run reports `failed > 50` (mass-failure threshold).
- The Type I audit fieldwork window opens within the next 30 days AND any fallback fires.

Template: `specs/_postmortems/<YYYY-MM-DD>-drata-sync-<short-cause>.md`. Sections: timeline, root cause, impact (records affected; audit gap window; auditor notified yes/no), what worked, what didn't, action items (target SEAL within 14 days).

---

## 6. Quarterly drill

Once per quarter, the on-call SRE runs a tabletop:

1. Inject a 503 response from Drata via wiremock-style proxy in staging.
2. Time the response from alert-fired → backlog-drained.
3. Document `specs/_audits/<YYYY-MM-DD>-drata-sync-tabletop.md`.
4. Compare against the 24h SLA; if margin < 6h, file an action item.

Drill calendar: `specs/_governance/compliance-review-cadence.md` Q1/Q2/Q3/Q4.

---

## 7. Related

- Crate: [`crates/corelink-drata-sync/`](../../crates/corelink-drata-sync) — implementation.
- Coverage: [`specs/_compliance/DRATA-INTEGRATION-COVERAGE.md`](../_compliance/DRATA-INTEGRATION-COVERAGE.md) — per-TSC coverage matrix.
- Gap analysis: [`specs/_compliance/SOC2-GAP-ANALYSIS.md`](../_compliance/SOC2-GAP-ANALYSIS.md) — esp. GAP-08 (weekly compliance review), GAP-18 (Drata Workers agent), GAP-26 (continuous access review).
- Audit event types: `corelink.compliance.drata_evidence_sent`, `.failed`, `.skipped`, `.out_of_band`.
- D1: migration `migrations/d1/0044_drata_evidence_sent.sql`.

---

**Fim RB-DRATA-SYNC-FAILURE.**
