---
id: "RB-WEBHOOK-DLQ-REPLAY"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "stripe", "webhook", "dlq", "replay", "billing", "r-prep"]
---

# RB-WEBHOOK-DLQ-REPLAY — Stripe Webhook Dead-Letter Replay

**Scope.** Operator runbook for the `stripe_webhook_events_dlq` D1
table (migration `0045_stripe_webhook_dlq.sql`) — when an alert fires
(non-zero DLQ depth OR oldest-row age > 6h), how to triage, decide,
replay or abandon, and verify outcome. Companion to
`specs/_audits/2026-05-15-webhook-retry-dlq.md`.

**Audience.** On-call billing engineer + secondary approver for the
dual-approval replay step (admin API surface; today this is two
ops engineers running paired `wrangler d1 execute` commands).

**Severity bands.**

- **SEV-3 (warning, Slack).** DLQ depth ≥ 1 (someone needs to look
  within next business day).
- **SEV-2 (page, PagerDuty).** Oldest row age > 6 h (revenue events
  are stuck; tier mutations are deferred).
- **SEV-1 (page + war room).** DLQ depth > 20 OR oldest row age > 24 h
  OR any row of type `invoice.paid` or `customer.subscription.deleted`
  present (high-business-impact event types).

## 1. Symptom

One of the following fires:

| Alert | Source | Threshold |
|-------|--------|-----------|
| `corelink_stripe_webhook_dlq_depth > 0` | Prometheus rule (canonical metric per `corelink_stripe_real::dlq`) | SEV-3 |
| `corelink_stripe_webhook_dlq_oldest_age_seconds > 21600` (6 h) | Prometheus rule | SEV-2 |
| Manual: customer reports missing invoice / tier downgrade not applied | Customer support ticket | SEV-2 |

## 2. Triage (do this FIRST — 15 min budget)

### 2.1 Confirm the DLQ contents

```bash
# List active (un-replayed, un-expired) DLQ rows.
wrangler d1 execute corelink-prod --command "
  SELECT event_id, event_type, attempt_count, first_seen_at_ms,
         last_seen_at_ms, last_error
  FROM stripe_webhook_events_dlq
  WHERE replay_outcome IS NULL
    AND expires_at_ms > strftime('%s', 'now') * 1000
  ORDER BY first_seen_at_ms ASC
  LIMIT 50;
"
```

### 2.2 Group by event_type to identify the failure shape

```bash
wrangler d1 execute corelink-prod --command "
  SELECT event_type, COUNT(*) AS n, MIN(first_seen_at_ms) AS oldest
  FROM stripe_webhook_events_dlq
  WHERE replay_outcome IS NULL
    AND expires_at_ms > strftime('%s', 'now') * 1000
  GROUP BY event_type
  ORDER BY n DESC;
"
```

### 2.3 Pull the audit chain for one representative event

```bash
# For event_id evt_XYZ, pull every audit row in correlation_id chain.
wrangler d1 execute corelink-prod --command "
  SELECT * FROM audit_chain
  WHERE correlation_id IN (
    SELECT correlation_id FROM stripe_webhook_events_dlq
    WHERE event_id = 'evt_XYZ'
  )
  ORDER BY ts_ms ASC;
"
```

The chain will show the `processing_failed` row with the error
detail. NEVER paste customer email / payment-method tokens into
Slack — `last_error` is already redacted by the route, but the
audit chain may carry tenant identifiers.

### 2.4 Identify tenant impact

```bash
# Most Stripe webhook events carry a Stripe customer id; map to tenant.
wrangler d1 execute corelink-prod --command "
  SELECT t.tenant_id, t.tier, dlq.event_id, dlq.event_type
  FROM stripe_webhook_events_dlq dlq
  LEFT JOIN tenants t
    ON t.stripe_customer_id =
       json_extract(dlq.raw_body_hex, '\$.data.object.customer')
  WHERE dlq.replay_outcome IS NULL;
"
```
> Note: `raw_body_hex` is hex-encoded; the production admin API will
> decode + `json_extract` natively. Until the admin API lands, the
> operator can decode in a local shell.

## 3. Decision tree

```
                 DLQ row inspected
                        │
                        ▼
              Is this a known bug?
                  /         \
                YES         NO
                /              \
       Fix deployed?      Tampered payload?
        /        \          /           \
      YES        NO       YES           NO
      │          │         │             │
      ▼          ▼         ▼             ▼
   REPLAY    WAIT for    REJECT       Is event still
              fix        + page         relevant?
                       SECURITY        /        \
                                     YES         NO
                                     │            │
                                     ▼            ▼
                                  REPLAY      ABANDON
                                              + audit
```

### 3.1 REPLAY — known bug, fix deployed

Pre-condition: the bug that produced `last_error` has a merged + deployed
fix on `main`. Verify via:

```bash
git log --oneline --grep="fix.*tier-ledger" -n 5
```

### 3.2 REJECT (security escalation)

Trigger: `last_error` contains "signature_invalid" OR an unusual
`event_type` that does not appear in the canonical seven (see
`apps/server/src/webhook.rs::CanonicalEventType`). **This should be
impossible** — the route rejects signature-invalid events at step 2
**before** they reach the DLQ. If a `signature_invalid` row appears
in the DLQ, treat it as a P0 security incident:

1. Capture full row + audit chain.
2. Page security on-call (PagerDuty service `security-oncall`).
3. Rotate `STRIPE_WEBHOOK_SECRET` per
   `RB-SYSTEM-CMK-ROTATION` (analogous flow).
4. Do **NOT** replay.

### 3.3 ABANDON — event no longer relevant

Trigger: customer churned + tenant is already in deletion grace
period; the event would mutate a tenant that no longer exists.
Mark the row abandoned + emit audit (NEVER delete the DLQ row —
keep it for forensics until TTL).

## 4. Replay execution (dual-approval, manual until admin API lands)

### 4.1 Operator A — prepare the replay request

```bash
# Generate UUIDv7 for the replay request id.
REPLAY_REQ_ID="$(uuidgen | tr 'A-Z' 'a-z')"
echo "Replay request id: $REPLAY_REQ_ID"

# Mark intent to replay (NULL outcome means "in-flight"; the row
# becomes locked from concurrent replay by enforcing
# `replay_request_id IS NULL` in the UPDATE WHERE clause).
wrangler d1 execute corelink-prod --command "
  UPDATE stripe_webhook_events_dlq
  SET replay_request_id = '$REPLAY_REQ_ID',
      replayed_by       = 'ops_alice'
  WHERE event_id = 'evt_XYZ'
    AND replay_request_id IS NULL;
"
```

### 4.2 Operator B — independent approval

Operator B reviews the audit chain row + the proposed `event_id`,
then:

```bash
# Approve by appending B's identity to the replay record.
wrangler d1 execute corelink-prod --command "
  UPDATE stripe_webhook_events_dlq
  SET replayed_by = 'ops_alice+ops_bob'
  WHERE event_id = 'evt_XYZ'
    AND replay_request_id = '$REPLAY_REQ_ID';
"
```

### 4.3 Execute the replay

Until the admin API lands, the replay is executed by re-POSTing the
quarantined raw body bytes to `/v1/billing/stripe-webhook` from a
trusted operator-only network ingress with the **original**
`Stripe-Signature` header recovered from the audit chain
(`headers_redacted` slot if captured) OR by directly invoking the
`SubscriptionStateHandler` dispatch in `apps/server` via a one-shot
admin CLI.

**Canonical path (one-shot CLI)**:

```bash
cargo run -p corelink-server --bin admin-replay-webhook -- \
  --event-id evt_XYZ \
  --replay-request-id "$REPLAY_REQ_ID" \
  --approved-by "ops_alice+ops_bob"
```

> NOTE: the `admin-replay-webhook` binary lands in S-R3.x admin API
> sprint. Until then, the replay is executed by hand-running the
> dispatch via a `wrangler d1` direct UPDATE on the tier-selection
> ledger + recording the synthetic audit row in the chain. The
> dual-approval audit trail above is the canonical fallback.

### 4.4 Record outcome

After the replay (success or failure), set `replay_outcome`:

```bash
# On success:
wrangler d1 execute corelink-prod --command "
  UPDATE stripe_webhook_events_dlq
  SET replay_outcome  = 'succeeded',
      replayed_at_ms  = strftime('%s', 'now') * 1000
  WHERE event_id = 'evt_XYZ'
    AND replay_request_id = '$REPLAY_REQ_ID';
"

# On failure:
wrangler d1 execute corelink-prod --command "
  UPDATE stripe_webhook_events_dlq
  SET replay_outcome  = 'failed',
      replayed_at_ms  = strftime('%s', 'now') * 1000,
      last_error      = 'replay attempt: <REDACTED ERROR>',
      attempt_count   = attempt_count + 1
  WHERE event_id = 'evt_XYZ'
    AND replay_request_id = '$REPLAY_REQ_ID';
"
```

### 4.5 Expected audit entries

Every replay MUST produce three audit-chain rows:

| Audit event type | When | `event_data` |
|------------------|------|--------------|
| `corelink.billing.webhook.dlq_replay_requested` | After 4.1 | `{replay_request_id, requested_by, event_id}` |
| `corelink.billing.webhook.dlq_replay_approved` | After 4.2 | `{replay_request_id, approved_by, event_id}` |
| `corelink.billing.webhook.dlq_replay_outcome` | After 4.4 | `{replay_request_id, outcome, event_id}` |

These event names are reserved as canonical `WebhookAuditEventType`
extensions for the admin API sprint; the runbook documents the
contract today so the audit-chain post-hoc verification is
unambiguous.

## 5. Post-replay verification (5-min budget)

```bash
# Re-query the DLQ — replayed row should have replay_outcome set.
wrangler d1 execute corelink-prod --command "
  SELECT event_id, replay_outcome, replayed_by, replayed_at_ms
  FROM stripe_webhook_events_dlq
  WHERE event_id = 'evt_XYZ';
"

# Confirm depth dropped (Prometheus).
curl -s "$PROM_URL/api/v1/query?query=corelink_stripe_webhook_dlq_depth" \
  | jq '.data.result[].value[1]'

# Confirm tenant state mutated (for subscription.created replay).
wrangler d1 execute corelink-prod --command "
  SELECT tenant_id, tier, last_event_id
  FROM tier_selection_ledger
  WHERE last_event_id = 'evt_XYZ';
"
```

## 6. Abandon path (4.x equivalent for stale events)

```bash
wrangler d1 execute corelink-prod --command "
  UPDATE stripe_webhook_events_dlq
  SET replay_outcome  = 'abandoned',
      replay_request_id = 'abandon-$EVENT_ID',
      replayed_by     = 'ops_alice+ops_bob',
      replayed_at_ms  = strftime('%s', 'now') * 1000
  WHERE event_id = '$EVENT_ID'
    AND replay_outcome IS NULL;
"
```

The audit chain MUST record a `corelink.billing.webhook.dlq_abandoned`
row with a free-text justification field. Recommend pasting the
Slack thread URL.

## 7. Post-incident

After every replay (success, fail, or abandon):

1. Capture cause in the post-mortem template (`RB-POSTMORTEM-PROCESS`).
2. If the root cause is a novel failure mode (not previously in
   `specs/03_architecture/failure_modes.md`), add a new FM row
   per the framework convention.
3. If the failure was caused by a tier-ledger / handler bug, add a
   regression test pinning the recovery.
4. If the failure was caused by a Stripe schema change (new event
   type / field shape), update the `CanonicalEventType` taxonomy +
   the `WebhookEnvelope` deserializer.
5. Verify the DLQ pruning job is still running (TTL = 30 days).

## 8. Related runbooks + invariants

- `specs/_audits/2026-05-15-webhook-retry-dlq.md` — audit doc.
- `RB-FM-SIGNUP-FAILED.md` — upstream sign-up failure runbook
  (FM-150 sibling).
- `RB-POSTMORTEM-PROCESS.md` — canonical post-mortem template.
- `RB-SYSTEM-CMK-ROTATION.md` — secret rotation if the DLQ row was
  caused by a signature-invalid path (security escalation).
- INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH) — every replay step
  emits its audit row BEFORE state mutation.
- INV-OBS-AUDIT-CHAIN-INTEGRITY (S-09 inheritance) — every replay
  is a separate chain event.

## 9. Sign-off

`final_approver: Gustavo Schneiter` — date `2026-05-15`.
