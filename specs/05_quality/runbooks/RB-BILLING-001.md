---
id: "RB-BILLING-001"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "billing", "audit-replay", "forensic", "compliance"]
---

# RB-BILLING-001 — Audit-Grade Invoice Replay (Forensic Reconstruction from R2 Events)

> **INV:** INV-BILLING-REPLAYABLE-FROM-EVENTS HIGH | **SLA:** reconstruction ≤ 5 min; comparison report ≤ 30 min

## Pré-condições

- S-10 billing pipeline live com R2 events bucket Object Lock 7y.
- Replay endpoint `POST /v1/billing/replay` ativo + role `billing_admin` provisioned.
- D1 schema billing tables populated.
- Stripe customer + invoice IDs disponíveis.

## Quando usar

- **Auditor request**: SOC 2 Type I review; auditor solicita reconstruction de invoice específico.
- **Customer dispute**: customer questiona line item em invoice; need source-of-truth verification.
- **Internal compliance review**: monthly/quarterly internal audit verifies pipeline integrity.
- **Finance close-of-month**: verification antes de financial close.
- **Drift investigation**: 3-layer reconciliation flag drift; replay confirms source-of-truth.

## Procedure

### Step 1: Authenticate + Authorize (≤ 1 min)
1. Operator authenticates com PAT scope `billing:admin` (S-03 R-S03-3).
2. MFA re-auth required (CTRL-AUTH-010 freshness 30 min).
3. Audit emit `corelink.billing.replay_initiated` event com `actor_id, invoice_id, requested_at, reason`.

### Step 2: Identify scope (≤ 2 min)
1. Identify `invoice_id` + `tenant_id` + `period_start` + `period_end`.
2. Cross-reference Stripe `invoice_id` ↔ CoreLink internal `invoice_line_item_id`.

### Step 3: Replay execution (≤ 5 min)
```bash
# dry_run=true (default) to preview without side effects
curl -X POST https://api.corelink.dev/v1/billing/replay \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "X-MFA-Token: $MFA" \
  -d '{
    "invoice_id": "in_1A2B3C",
    "tenant_id": "uuid-...",
    "period_start": "2026-04-01T00:00:00Z",
    "period_end": "2026-04-30T23:59:59Z",
    "dry_run": true
  }'
```

Endpoint:
- Reads R2 `billing-events` bucket per tenant + ts range.
- Reconstructs invoice line items deterministically via:
  - Aggregation per SKU.
  - Pricing rules from D1 `plan` snapshot at period_end.
  - Hash chain integrity verification (S-09).
- Returns reconstructed JSON.

### Step 4: Comparison (≤ 30 min)
1. Fetch Stripe invoice via Stripe API.
2. Compare reconstructed JSON vs Stripe invoice JSON (modulo Stripe metadata: ts, IDs internal Stripe).
3. Diff report:
   - **Diff < 0.01%**: accept; document em audit log.
   - **0.01% ≤ Diff < 0.1%**: SEV-3 monitor; investigate before close-of-month.
   - **Diff ≥ 0.1%**: SEV-2 reconciliation; do not close month.
   - **Diff ≥ 1%**: SEV-1 + invoice freeze + Finance lead + Legal.

### Step 5: Document outcome (≤ 1h)
1. Audit emit `corelink.billing.replay_completed` com diff report attached.
2. Stamp into compliance audit log (S-09 R2 audit bucket 7y retention).
3. Customer/auditor notification se needed.

## Outputs

- Reconstructed invoice JSON (downloadable artifact).
- Stripe invoice JSON (reference).
- Diff report (HTML + JSON).
- Audit emission `corelink.billing.replay_executed`.

## Acceptance criteria

- **Reconstructed = Stripe**: ≤ 0.01% diff (only Stripe metadata) → ✅ PASS.
- **Diff > 0.01%**: investigate; potentially recompute + corrective billing event emit.

## Escalation

- **Diff ≥ 0.1%**: page Finance lead + SRE.
- **Diff ≥ 1%**: page Finance + Legal + Compliance officer; freeze close-of-month.
- **Replay endpoint failure**: SEV-2; engage S-10 billing engineer.
- **Hash chain integrity break detected during replay**: CRITICAL — escalate via S-09 INV-OBS-AUDIT-CHAIN-INTEGRITY violation runbook (planned Lote 9.5); SEV-1.

## Recovery / Rollback

- Replay endpoint é read-only por default (dry_run=true). Se executed com `dry_run=false`:
  - Generates corrective billing event em R2 events bucket.
  - Audit chain extended (não rewrite).
  - Stripe API call only se explicit `apply_correction=true` flag (additional MFA + dual-approval S-13).

## Post-incident (se diff > threshold)

- Post-mortem within 7d.
- 5-Why focused on:
  - Layer 1 (events R2 ↔ counters D1) drift?
  - Layer 2 (counters D1 ↔ invoice_line_item D1) drift?
  - Layer 3 (D1 ↔ Stripe) drift?
- Reconciliation worker review.
- SOC 2 audit log update.

## Evidence

- Replay request payload.
- Replay response JSON (reconstructed).
- Stripe invoice JSON.
- Diff report.
- Audit log emissions.
- Engagement timeline.

## References

- `invariant_registry.md` INV-BILLING-REPLAYABLE-FROM-EVENTS (§3.12), INV-BILLING-NO-LOSS, INV-BILLING-NO-DUP.
- `specs/04_sprints/S10/_spec_contract.md` (CAP-BILLING-007 replay endpoint).
- `specs/03_architecture/error_taxonomy.md` `COR_BILLING_REPLAY_FORBIDDEN`.
- SOC 2 CC1.4 (financial integrity).
- GAAP ASC 606 revenue recognition.
- Stripe API: <https://stripe.com/docs/api/invoices>.
