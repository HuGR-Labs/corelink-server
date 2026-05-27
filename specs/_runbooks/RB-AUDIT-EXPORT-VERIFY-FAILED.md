---
id: "RB-AUDIT-EXPORT-VERIFY-FAILED"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "S-17"
parent_wi: "WI-S09-008"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-AUDIT-EXPORT-INTEGRITY"
  - "ONCALL-ESCALATION-MATRIX"
  - "security_model"
tags: ["runbook", "audit", "audit-export", "data-integrity", "sev-0", "soc2", "cc7.2", "gdpr", "lgpd", "art-46", "art-33", "chain-integrity", "fail-closed", "wave-17"]
---

# RB-AUDIT-EXPORT-VERIFY-FAILED — Server-side audit-export chain-break (SEV-0)

> **Status:** DRAFT. Owner: Gustavo Schneiter.
>
> **Scope:** operator response when the PagerDuty rule
> `AuditExport_VerifyFailed` fires. The rule counts emits of
> `corelink.audit.export_verify_failed.v1` from the customer
> audit-export endpoint (`apps/server/src/routes/audit_export.rs`
> §6 server-side verify pipeline).
>
> **Severity:** **SEV-0**. The route still ships the bytes (caller
> decides what to trust — `audit_export.rs` doc §"Fail-CLOSED
> ordering"); this runbook is the **server-side detection anchor**.
> Treat every emit as a confirmed chain break OR tampering signal
> until proven otherwise.
>
> **MTTA target:** **≤ 1 hour** (per task contract; tighter than the
> 5-min ack default because the regulatory clock starts on SEV-0
> confirm).
>
> **MTTR target:** **≤ 4 hours** end-to-end.
>
> **Regulatory clock:** **LGPD Art. 46 / GDPR Art. 33 — 72-hour
> notification clock starts at SEV-0 confirm** (§5). Do NOT delay §5
> while waiting for §4 cryptographic forensics — start the clock the
> moment §2 triage classifies as `chain_break` or `tampering`.
>
> **Companion docs / refs:**
> - `dashboards/alerts/dash-audit-export-alerts.yml` (the PD alert rule)
> - `apps/server/src/routes/audit_export.rs` §6-7 (verify pipeline + emit)
> - `crates/corelink-audit-chain/src/exporter.rs` (`verify_export_result` semantics)
> - `crates/corelink-audit-chain/src/archive_producer.rs` (R2 chunk shape)
> - `specs/_audits/sealed/2026-05-15-audit-chain-retention.md` (retention mechanism + R2 Object Lock)
> - `specs/_runbooks/RB-AUDIT-EXPORT-INTEGRITY.md` (customer-reported sibling; reuse §2 chain-break triage)
> - `specs/_runbooks/RB-AUDIT-EXPORT-CROSS-TENANT-ATTEMPT.md` (sibling SEV-1; if both fire concurrently → coordinated attack)
> - `RB-AUDIT-CHAIN-001` (daily-verifier chain-break runbook)
> - `specs/03_architecture/security_model.md` §Merkle proofs

---

## 1. Detect (≤ 1h MTTA)

The alert rule `AuditExport_VerifyFailed` fires on any non-zero
observation:

```promql
sum by (tenant_id_hex8) (
  increase(corelink_audit_export_verify_failed_total[5m])
) > 0
```

The PagerDuty incident body carries the **pseudonymised tenant id**
(`tenant_id_hex8` = first 8 hex chars of BLAKE3(tenant_id); per
INV-AUTH-AUDIT-PSEUDONYMIZATION + CTRL-PRIV-001).

**Step 1.1 — Acknowledge within ≤ 1h.** Tier-1 oncall ack via
PagerDuty mobile. If unack within 30 min, the
`corelink-incident-response` escalation policy auto-pages Tier-2
(5-min delay per `infra/pagerduty/schedule.yaml`); the secondary
`corelink-incident-response-tier-3-direct` policy fans out to
Tier-3 (Architect + Security Lead) IMMEDIATELY with no delay (SEV-0
spec; per the alert rule `escalation_policy_secondary` label).

**Step 1.2 — Pull the offending event-id chain.** Query the
audit-chain D1 mirror for the last 15 minutes of verify-failed
emits PLUS the surrounding `export_request.v1` row:

```bash
wrangler d1 execute corelink-audit \
  --command "SELECT
    event_id,
    event_type,
    occurred_at,
    authenticated_tenant_blake3_hex8,
    from_ms,
    to_ms,
    bytes_written,
    events_written,
    exit_status,
    correlation_id
  FROM audit_events_index
  WHERE event_type IN (
      'corelink.audit.export_verify_failed.v1',
      'corelink.audit.export_request.v1'
    )
    AND occurred_at >= datetime('now', '-15 minutes')
  ORDER BY occurred_at DESC;"
```

The verify-failed row will be IMMEDIATELY AFTER the corresponding
export_request row (route §7 ordering). Capture the
`correlation_id`, `from_ms`, `to_ms`, `authenticated_tenant_blake3_hex8`
— these drive every downstream step.

---

## 2. Triage — chain break vs tampering vs ingestion bug (decision tree)

The verify failure has 3 root causes; the triage decision tree
selects which §3 + §4 path to run. The decision must complete within
**30 minutes of ack** (regulatory clock implication — see §5).

| Symptom | Probable root cause | Severity decision | Next |
|---|---|---|---|
| Verify failure reproduces from a fresh re-export of the same window AND the daily-verifier `last_verified_hash` for the same window MISMATCHES the export's `chain_head_at_export` | **Real chain break in R2** | **SEV-0 confirmed**; clock starts | §3 freeze + §4 forensics |
| Verify failure reproduces AND daily-verifier hash MATCHES export's `chain_head_at_export` BUT the per-row Merkle proofs mismatch at a specific `at_sequence` | **Tampering of a single archived row** (extremely rare; Object Lock should prevent — escalate) | **SEV-0 confirmed**; clock starts | §3 freeze + §4 forensics |
| Verify failure does NOT reproduce on re-export AND no daily-verifier discrepancy | **Ingestion bug — transient mid-write inconsistency** (e.g. chunk-rotation race) | SEV-1 downgrade after 2 independent re-exports succeed | §6 ingestion bug path; do NOT trigger §5 |

### 2.1 Re-export to triage

```bash
# Operator re-runs the same window. Compare verify outcomes.
corelink audit export \
  --tenant <TENANT_ID> \
  --since-ms <FROM_MS> \
  --until-ms <TO_MS> \
  --format json-ld \
  --include-merkle-proofs \
  --verify \
  --output ./triage-reexport/

# Pull the daily-verifier checkpoint for the SAME window.
wrangler d1 execute corelink-audit \
  --command "SELECT
      last_verified_hash,
      last_verified_sequence,
      verified_at
    FROM audit_chain_verify_checkpoints
    WHERE tenant_id = '<TENANT_ID>'
      AND last_verified_until_ms >= <TO_MS>
    ORDER BY verified_at DESC LIMIT 1;"
```

Compare `triage-reexport/manifest.chain_head_at_export` (hex) against
`last_verified_hash`:

- **MATCH** → re-export verify passes → ingestion bug branch (§6).
- **MATCH** → re-export verify STILL fails → tampering of a row
  inside the window; §4 forensics with chunk pull at the divergent
  sequence.
- **MISMATCH** → chain head drift between daily-verifier and the
  export pipeline → **real chain break**; §3 freeze + §4 forensics.

### 2.2 Decision criterion — when to start the regulatory clock

The §5 LGPD/GDPR clock starts the moment §2 classifies as
`chain_break` or `tampering` — **NOT** when §4 forensics complete.
Document the decision time in the incident memo (§7 §closure).
Triage downgrade to SEV-1 (ingestion bug) requires ≥ 2 independent
clean re-exports + Tier-3 sign-off; the regulatory clock does NOT
start on the downgrade path.

---

## 3. Immediate freeze of audit-chain writes for the impacted tenant

This step runs the moment §2 classifies as `chain_break` or
`tampering`. Goal: stop the audit-chain mutator from appending any
new rows for the affected tenant until §4 forensics + §6 rebuild
complete.

### 3.1 Disable the audit-export endpoint per-tenant

```bash
corelink admin tenant feature-flag set \
  --tenant-id <TENANT_ID> \
  --flag audit_export_disabled \
  --value true \
  --reason "RB-AUDIT-EXPORT-VERIFY-FAILED §3.1; chain break confirmed at $(date -u +%FT%TZ)"
```

The flag is checked by the route at request boundary (`apps/server/src/routes/audit_export.rs` step 1 auth → step 1.5 feature-flag gate; production wiring). Subsequent export requests return `503 Service Unavailable` + the audit emit `corelink.audit.export_disabled.v1`.

### 3.2 Pause new audit-chain appends for the tenant (NOT a global pause)

```bash
# Pause emits for the specific tenant. Other tenants continue to
# write — global pause would be a SEV-0 on its own (audit chain MUST
# remain append-only for non-impacted tenants).
corelink admin audit-chain pause-tenant \
  --tenant-id <TENANT_ID> \
  --reason "RB-AUDIT-EXPORT-VERIFY-FAILED §3.2; forensic hold"

# The audit emitter starts queueing the tenant's events in a
# DLQ-style holding buffer (corelink-audit-chain::HoldingBuffer);
# the buffer is drained only after §6 rebuild + Tier-3 sign-off.
```

**Escalation trigger:** if the pause command fails (e.g. the audit
emitter rejects the pause), do NOT retry — page the Architect +
Security Lead via the secondary escalation policy and reach for the
incident commander.

### 3.3 Notify the on-call privacy officer + legal counsel

Required for the 72h regulatory clock orchestration (§5). Page via
`ONCALL-ESCALATION-MATRIX.md` Tier-3 + bridge to
`#sev-0-audit-integrity` Slack channel.

---

## 4. Cryptographic forensics — R2 chunk pull + chain replay

Runs in parallel with §3 (freeze). The cf-bindings + R2 chunk layout
is documented in `crates/corelink-audit-chain/src/archive_producer.rs`.

### 4.1 Pull the R2 chunk that contains the divergent sequence

The verify error embeds `at_sequence` (the first offending row's
canonical sequence). Compute the chunk that owns it (chunks are date-
sharded; layout `audit/{tenant_id}/{date}/{seq:08}.cloudevent.ndjson`
per `RB-AUDIT-EXPORT-INTEGRITY` §2.1):

```bash
TENANT_ID=<TENANT_ID>
SEQ=<AT_SEQUENCE_FROM_VERIFY_ERROR>
# Discover the date that owns the sequence via the index.
DATE=$(wrangler d1 execute corelink-audit \
  --command "SELECT date FROM audit_chunk_index
             WHERE tenant_id = '${TENANT_ID}'
               AND first_sequence <= ${SEQ}
               AND last_sequence  >= ${SEQ};" \
  --json | jq -r '.[0].results[0].date')

# Pull the chunk via cf-bindings R2 binding (corelink-audit-archive).
wrangler r2 object get \
  corelink-audit-${REGION} \
  "audit/${TENANT_ID}/${DATE}/$(printf '%08d' ${SEQ}).cloudevent.ndjson" \
  --file ./forensics/${TENANT_ID}-${DATE}-${SEQ}.ndjson
```

### 4.2 Chain replay at the divergent sequence

```bash
# Replay the chain from the previous chunk's chain_head_after through
# the divergent sequence. The verifier prints the first mismatch.
corelink audit-chain replay \
  --tenant ${TENANT_ID} \
  --from-sequence $((SEQ - 1)) \
  --to-sequence   $((SEQ + 10)) \
  --r2-bucket corelink-audit-${REGION} \
  --report ./forensics/replay-${TENANT_ID}-${SEQ}.json
```

The report shape per `crates/corelink-audit-chain/src/exporter.rs`:

- `replay_match` → the chunk content is INTACT; the export pipeline
  computed a wrong proof → **export pipeline bug** (rare; §6 path
  with code-fix + regression test).
- `replay_mismatch_byte_diff` → the chunk content differs from the
  daily-verifier's stored hash by ≥ 1 byte → **tampering or storage
  corruption** (R2 Object Lock should make this 0% probability;
  escalate to Cloudflare support per WI-S20-006).
- `replay_mismatch_missing_object` → the chunk is missing from R2 →
  **retention violation**; cross-reference
  `specs/_audits/sealed/2026-05-15-audit-chain-retention.md` §2.2 monotonic
  chunk-count check and trigger the §3.7 audit-retention runbook
  (the daily-verifier should have caught this — investigate the
  verifier's gap).

### 4.3 Capture forensic artifacts (write-once)

```bash
# Bundle the chunk + replay report + manifest + verify error into a
# tamper-evident artifact for the incident memo.
corelink admin forensic-bundle create \
  --incident-id <PD_INCIDENT_ID> \
  --inputs ./forensics/ \
  --output ./forensic-bundles/audit-export-verify-failed-$(date -u +%FT%TZ).zip
```

The bundle output is content-addressed (BLAKE3 filename suffix);
attach to the SOC2 evidence channel via the Drata API.

---

## 5. Customer notification — LGPD Art. 46 / GDPR Art. 33 72h clock

**Clock start:** the moment §2 classifies as `chain_break` or
`tampering`. Document the start timestamp in the incident memo.

### 5.1 Internal sequencing (within 4h MTTR)

1. Privacy Officer + Legal Counsel reach SEV-0 bridge (§3.3 already
   paged them).
2. DPO drafts the customer notification using the template below.
3. Legal counsel reviews + signs off.
4. CEO / Founder approves before send (Gustavo Schneiter as final
   approver per the WI-S09-008 owner contract).

### 5.2 Template — customer notification (SEV-0)

> **Subject:** `[Critical] CoreLink audit-log integrity alert affecting your tenant`
>
> Hi `<customer name>`,
>
> On `<UTC timestamp>` our server-side audit-chain verifier detected
> a chain-integrity failure within an audit-export response delivered
> to your tenant. We are writing to you within the 72-hour notification
> window required by GDPR Article 33 (and LGPD Article 46, where
> applicable) because this incident may constitute a personal-data
> integrity breach affecting your audit log.
>
> **What happened.** The verifier surfaced a Merkle-chain mismatch at
> audit sequence `<SEQ>` within the window `<FROM>`–`<TO>` you (or
> your client) requested via `GET /v1/audit/export`. The bytes WERE
> delivered to your client; we cannot vouch for their server-side
> integrity at the moment of delivery.
>
> **What we did.**
> 1. Disabled the audit-export endpoint for your tenant pending
>    investigation (you will see HTTP 503 until we clear the freeze).
> 2. Paused new audit-chain appends for your tenant (events queue in
>    a holding buffer; nothing is lost).
> 3. Captured a tamper-evident forensic bundle of the affected R2
>    archive chunks.
> 4. Triggered cryptographic replay against the daily-verifier
>    checkpoint store; results indicate `<classification>` (chain
>    break / tampering / pending).
>
> **What you should do.**
> 1. Treat any export of the affected window taken from your archive
>    as **not independently verifiable** until we ship the rebuilt
>    chain (§6).
> 2. If you have shared the export with a downstream auditor, notify
>    them of this hold.
> 3. Confirm receipt of this notice within 24h.
>
> **What we will do next.**
> We will rebuild the audit chain from the R2 archive (§6) and ship
> a fresh, re-verified export. We will publish a post-mortem within
> 5 business days at
> `https://corelink.humangr.com/incidents/<incident-id>`.
>
> Your dedicated incident channel: `<slack/email>`. The Data
> Protection Officer is in the loop.
>
> — Gustavo Schneiter, Security Lead, CoreLink

### 5.3 Regulator notification

If the affected tenant is in the EU (GDPR jurisdiction) OR Brazil
(LGPD jurisdiction):

1. Legal Counsel files with the relevant Supervisory Authority
   (GDPR Art. 33 — within 72h of confirm); CNPD/ANPD for LGPD Art.
   46.
2. The filing template lives at `legal/regulatory/templates/breach-art33.md`.
3. File a copy in the incident memo + Drata evidence channel.

---

## 6. Post-incident — audit-chain rebuild from R2 archive

Goal: rebuild a verifiable chain head from the R2 NDJSON archive
(canonical source of truth per the wave-15 archive producer +
retention audit). The R2 chunks ARE the chain in canonical form.

### 6.1 Rebuild procedure (Tier-3 supervised)

```bash
# 1. Sanity check: confirm every chunk in the affected window is
#    present in R2 (negative-delta = SEV-0 retention violation;
#    see RB-AUDIT-CHAIN-RETENTION-VIOLATION).
corelink audit-chain inventory \
  --tenant <TENANT_ID> \
  --since <FROM_DATE> \
  --until <TO_DATE> \
  --report ./rebuild/inventory.json

# 2. Rebuild the chain head by re-running the producer's hash chain
#    over every chunk in lexicographic order. The output is a fresh
#    chain_head + per-row Merkle proofs.
corelink audit-chain rebuild \
  --tenant <TENANT_ID> \
  --since <FROM_DATE> \
  --until <TO_DATE> \
  --output ./rebuild/chain.bin

# 3. Cross-check the rebuilt chain head against the LAST KNOWN-GOOD
#    daily-verifier checkpoint (prior to the divergence).
corelink audit-chain verify-against-checkpoint \
  --rebuilt ./rebuild/chain.bin \
  --checkpoint-before <FROM_MS>
```

### 6.2 Drain the holding buffer (§3.2 reversal)

After Tier-3 signs off on the rebuilt chain:

```bash
# Drain queued events back into the live chain. Each drained event
# emits `corelink.audit_chain.drained_from_hold.v1` for transparency.
corelink admin audit-chain drain-tenant \
  --tenant <TENANT_ID> \
  --order chronological
```

### 6.3 Unfreeze the export endpoint

```bash
corelink admin tenant feature-flag set \
  --tenant-id <TENANT_ID> \
  --flag audit_export_disabled \
  --value false \
  --reason "RB-AUDIT-EXPORT-VERIFY-FAILED §6.3; rebuild signed off at $(date -u +%FT%TZ)"
```

### 6.4 Ship a fresh export to the customer

Re-export the affected window using the operator-side path documented
in `RB-AUDIT-EXPORT-INTEGRITY.md` §4; attach to the customer's Drata
evidence channel referencing the original incident id.

---

## 7. MTTR target + closure

**MTTA target ≤ 1h; MTTR target ≤ 4h** end-to-end. Closure
checklist:

- [ ] §1 page acknowledged within ≤ 1h.
- [ ] §2 triage classification documented; clock-start timestamp recorded.
- [ ] §3 freeze applied; impacted tenant's audit-export endpoint disabled + holding buffer engaged.
- [ ] §4 forensic bundle captured (BLAKE3-addressed; attached to Drata).
- [ ] §5 customer notification sent within 72h of clock start.
- [ ] §5.3 regulator filing complete (where applicable).
- [ ] §6 rebuild signed off by Tier-3; holding buffer drained.
- [ ] §6.4 fresh export shipped.
- [ ] Post-mortem within 5 business days at
      `specs/_post_mortems/PM-AUDIT-EXPORT-VERIFY-FAILED-<YYYY-MM-DD>-<ticket>.md`.

---

## 8. Fitness function

This runbook MUST be drilled quarterly via the
`corelink admin chaos audit-export-chain-break` synthetic fixture
which injects a single-row hash drift into a staging tenant's audit
chain and confirms (a) the SEV-0 page fires within 30 seconds of the
verify-failed emit, (b) the operator can complete §1–§4 within 4
hours, (c) the rebuild §6 produces a clean chain head from the
staging archive. Drift > 2× MTTR (8h) triggers FM-202 review per
`corelink-runbook-tracker`.
