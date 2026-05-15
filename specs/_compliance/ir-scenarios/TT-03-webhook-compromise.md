---
id: "TT-03-WEBHOOK-COMPROMISE"
type: "compliance_scenario"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-5"
parent_wi: "WT-GAP-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "ir", "tabletop", "scenario", "sev1", "stripe", "webhook", "signature-validation", "dlq", "replay", "gap-03", "wt-gap-03"]
---

# TT-03 — Stripe Webhook Compromise (Signature-Validation Breakage After Stripe Rotation)

> **Severity:** SEV1 · **Duration:** 90 min · **Quorum:** IC + Scribe + Comms Lead + Tech Lead + Security Lead (5 roles). Customer Comms Lead joins by T+30 if billing-event customer impact confirmed.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` · **Evidence form:** `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md` · **Schedule:** Q4-2026 (target 2026-10-21).
>
> **FM linkage:** **FM-151** (Stripe API outage / billing event handling) + **FM-156** (dependency compromise, secondary lens) + **FM-400** (retry storm if webhooks replay incorrectly).
> **CTRL coverage:** CTRL-WEBHOOK-001 (signature validation) + CTRL-WEBHOOK-002 (idempotency) + CTRL-WEBHOOK-003 (DLQ + replay protocol) + CTRL-AUDIT-001/002.
> **RB-* invoked:** `RB-FM-151-stripe-outage` (semestral; Stripe-adjacent) · `RB-WEBHOOK-DLQ-REPLAY` (canonical DLQ replay protocol) · `RB-SECURITY-VULNERABILITY-INTAKE` (if compromise confirmed).

## Scenario summary

It is **11:18 UTC on a Monday**. The Stripe billing integration begins emitting `webhook_signature_invalid` errors at the rate of ~140/min, up from 0 over the trailing 24h. Stripe published a notice 30 minutes earlier — they are rotating their **public signing keys** as part of a scheduled annual rotation. CoreLink's webhook handler is configured to accept either the previous or the rotated key during the 60-day overlap window, per `CTRL-WEBHOOK-001`.

The signature failures are concerning because they should **not** be happening — the overlap window is supposed to cover this. Initial hypothesis (Tech Lead): a recent deploy may have inadvertently shipped a stale Stripe public-key fingerprint. **However** (revealed by injects), the actual cause is more subtle: the rotation introduced a new signing-key prefix (`sk_rotation_2026q4`) that our key-verification code path *uppercase*-compares — and Stripe's response header is lowercase. A small parsing bug in `crates/corelink-billing/src/webhook.rs::verify_signature` is rejecting all `sk_rotation_2026q4`-signed events.

Meanwhile, the **Stripe webhook DLQ** is filling up: events that fail signature validation are being routed to DLQ per `CTRL-WEBHOOK-003`, but the DLQ is now at 67% capacity (450 events queued and growing). If DLQ overflows, we lose customer billing events — payment confirmations, subscription updates, refunds.

Adversarial hypothesis (Security Lead must consider): is this an actual compromise — someone replaying old signed events through our webhook endpoint to game billing state? Or is it a benign parsing bug? The team cannot assume either.

The team must:
1. Determine root cause (benign parsing bug vs. genuine compromise) without slowing down event throughput.
2. Decide on DLQ replay strategy once root cause is fixed (idempotency safeguards on replay).
3. Communicate to customers if any billing events are delayed beyond SLA.

## Pre-tabletop preparation

### Facilitator-localised anchors (T-1 week)
- Pull a real recent Stripe webhook event sample from `crates/corelink-billing/fixtures/stripe_webhook_sample.json`.
- Open the actual `webhook.rs::verify_signature` source path; locate the case-sensitivity code path for realism.
- Check current DLQ capacity dashboard (Grafana `stripe-webhook-dlq-depth`).

### Participants briefed (T-24h Slack post template)
> Tabletop **TT-03** runs **Wed 2026-10-21 14:00 UTC** on Zoom `<link>`. Scenario: Stripe webhook signature failures spike after Stripe rotation. Roles: IC=`<name>`, Tech Lead=`<name>`, Security Lead=`<name>`, Comms Lead=`<name>`, Scribe=`<name>`. Customer Comms Lead on standby. **Tabletop only.** Bring webhook RB, DLQ replay RB, FM catalog.

## Injects (timeline)

### Inject 1 — T+00:00 (Detection)
> *"It is 11:18 UTC. PagerDuty paged you SEV-2: `stripe_webhook_signature_failures_per_minute` is at 140/min, up from 0 over the last 24h. Stripe published a notice 30 minutes ago that they're rotating their public signing keys (scheduled rotation, was on the roadmap). The DLQ is at 67% (450 events queued, growing ~140 events/min). Go."*

**Expected:** IC declares SEV1 (revenue + customer-impact risk); opens `#inc-2026-10-21-stripe-webhook`; pages Security Lead (compromise-possible signal); Tech Lead starts root-cause investigation.

### Inject 2 — T+10:00 (Analysis, Security Lead pressure)
> *"Security Lead: you have to decide — is this a parsing bug on our side, OR is someone replaying old signed events through our endpoint? The signal that distinguishes: are the failing events new (timestamp within last 5 min) or old (timestamp >1h)? Also — should we **block** the Stripe webhook endpoint temporarily to halt potential replay, accepting that we'd accumulate even more DLQ events?"*

**Decision point D-1:** Block webhook endpoint (yes/no). If yes: stop the bleeding from a potential attacker but accelerate DLQ fill. If no: keep accepting (let signature validation reject) and hope it's benign.

### Inject 3 — T+22:00 (Analysis, root cause)
> *"Tech Lead just found it: the failing events all have a signature header prefix `sk_rotation_2026q4` (lowercase). Our verifier in `crates/corelink-billing/src/webhook.rs::verify_signature` does an uppercase-comparison against the expected prefix `SK_ROTATION_2026Q4` — and rejects everything. This is a parsing bug, not a compromise. The fix is a 1-line change. But the team needs to decide: hotfix-deploy now (skipping normal review) or wait for the next deploy window (90 min from now per `RB-GA-LAUNCH-ROLLBACK.md` cadence)?"*

**Decision point D-2:** Hotfix-deploy posture — IC must invoke (or not) the emergency-deploy exception per `ROADMAP-TO-GA.md` §6 / `RB-GA-LAUNCH-ROLLBACK.md`. Considerations: blast radius (small — 1 line in webhook verifier); test coverage (unit test exists; can we run CI in <10 min?); customer pressure (DLQ filling at 140/min, ~1.5h until overflow).

### Inject 4 — T+38:00 (DLQ replay strategy)
> *"Assume hotfix deployed at T+35. Webhook accept-rate normalises. Now: the DLQ has 482 events that failed signature validation in the bug window — these are *valid* Stripe events (we now know the signatures are good). DLQ replay is needed per `RB-WEBHOOK-DLQ-REPLAY.md`. Options: (a) automatic replay all 482 in order, idempotency keys protect against duplicate processing; (b) manual review of each event before replay (high effort but safe for high-value events like refunds); (c) selective replay by event type — auto-replay subscription updates, manual-review refunds + chargebacks. Choose."*

**Decision point D-3:** DLQ replay strategy (a / b / c). Each path:
- (a) **fast**, relies fully on idempotency safeguards (CTRL-WEBHOOK-002).
- (b) **safe**, slow, high human effort.
- (c) **balanced**, but introduces complexity (need filter logic correct).

### Inject 5 — T+58:00 (Customer-comms decision)
> *"It is now 12:16 UTC. Hotfix is live; DLQ replay is in progress per chosen strategy. Three enterprise customers' usage-based billing events were delayed by 50–58 minutes — none beyond the contractual 24h billing-data SLA, but two are on auto-paying subscriptions where the delayed event was a `payment_succeeded` notification → their internal accounting was briefly out of sync. Do you (a) proactively email these 3 customers, (b) wait for them to notice and respond reactively, (c) silent recovery (no comms — events are eventually consistent)?"*

**Decision point D-4:** Proactive vs reactive customer comms — Customer Comms Lead + IC decision. Real-incident default per `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` is proactive for any visible delay >15 min.

## Decision points summary

| # | Decision | Owner | NIST phase | Reversibility |
|---|---|---|---|---|
| D-1 | Block webhook endpoint (compromise hypothesis) | IC + Security Lead | Containment | reversible |
| D-2 | Hotfix-deploy now vs next window | IC | Eradication | reversible (can roll back) |
| D-3 | DLQ replay strategy (auto / manual / hybrid) | Tech Lead + IC | Recovery | (a) **partially-irreversible** if idempotency safeguards fail |
| D-4 | Proactive customer comms on delayed events | Customer Comms Lead + IC | Recovery | reversible (text) but **irreversible** once sent |

## Comms templates

### Internal Slack post (T+5)
```
@channel — SEV1 incident
Channel: #inc-2026-10-21-stripe-webhook
Symptom: Stripe webhook signature failures spike to 140/min after Stripe key rotation. DLQ at 67% and growing.
IC: <name> / Tech Lead: <name> / Security Lead: <name> / Comms Lead: <name> / Scribe: <name>
Hypothesis: parsing bug vs compromise (under investigation).
Customer-visible: TBD (billing events queued; no failures yet).
Status page: holding (internal-only signal so far).
Update cadence: every 15 min.
```

### Status-page paragraph (T+30, yellow — only published if customer-visible impact confirmed)
```
[INVESTIGATING] Stripe billing webhook events are experiencing a temporary processing delay due to a scheduled Stripe key rotation. No billing data is lost — events are safely queued and will be processed as soon as the integration is updated. Estimated recovery: <NN> minutes.
```

### Customer email (T+70, to 3 customers with usage-billing delays)
```
Subject: [INFO] Brief delay in your CoreLink billing event delivery
Body:
Hi <name>,

Between 11:18 and 12:16 UTC today, your usage-billing events from Stripe experienced a delay of approximately 50–58 minutes due to a Stripe-side signing-key rotation that interacted with a parsing edge case in our integration.

Impact: <event_type> events for your account were delayed, but all events have now been delivered and your billing state is fully consistent. There is no charge discrepancy; no action is needed on your side.

We are publishing a postmortem within 7 days. Reach out if you need a debrief.

— CoreLink Billing Lead (<name>)
```

### Internal write-up of compromise rule-out (Security Lead deliverable)
```
Subject: TT-03 dry-run — compromise hypothesis rule-out (T+22)
Body:
The Stripe webhook signature failures observed 2026-10-21 11:18–11:53 UTC were caused by a case-sensitive prefix comparison in crates/corelink-billing/src/webhook.rs::verify_signature.
Compromise hypothesis ruled out at T+22 based on:
- Failing events' Stripe-Signature headers contained lowercase `sk_rotation_2026q4` prefix consistent with Stripe's rotation announcement.
- Failed events were all dated within the last 5 minutes of receipt (no replay-attack signature pattern of stale timestamps).
- Stripe IP-allowlist source verification matched (CTRL-WEBHOOK-001 layer 2).
- No anomalous endpoint-traffic patterns observed (no requests to /v1/webhooks/stripe from non-Stripe IPs).
Hotfix applied at <UTC>. Postmortem to follow within 7 days.
```

## Post-incident artefacts list

- [ ] Webhook-verifier hotfix PR link (placeholder)
- [ ] DLQ replay manifest (event IDs, replay timestamps, idempotency-key reuse confirmation)
- [ ] Compromise rule-out write-up (Security Lead)
- [ ] Status-page text (if published)
- [ ] Customer emails sent (count + content)
- [ ] Action items into Linear epic `IR-TT-2026-Q4-01`
- [ ] Updates to `RB-WEBHOOK-DLQ-REPLAY.md` if gap surfaced
- [ ] Updates to webhook signature-verification unit test (case-insensitivity regression test)

## Success criteria

1. **SEV1 declared** within 5 min.
2. **Compromise hypothesis explicitly evaluated** by Security Lead (not implicitly dismissed) and ruled in/out with named evidence.
3. **Hotfix-deploy exception** invoked or rejected with explicit rationale.
4. **DLQ replay strategy** chosen with idempotency safeguard reference (CTRL-WEBHOOK-002).
5. **Customer comms** posture decided (proactive/reactive/silent) with documented rationale.
6. **At least 3 action items** captured (one must be: case-insensitivity unit test regression).

## Failure modes during exercise

- Team jumps to "parsing bug" without Security Lead's compromise rule-out → facilitator notes major gap; action item: harden compromise-hypothesis discipline in webhook RBs.
- Team chooses (a) auto-replay all 482 without confirming idempotency-key reuse path is tested for the specific event types → facilitator inserts complication: "two refund events double-processed; customer sees duplicate refund attempt; CS escalation".
- Team skips status-page entirely and surprises customers → action item: tighten `customer-visible threshold` in `RB-WEBHOOK-DLQ-REPLAY.md`.

## Cross-references

- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`.
- `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md` (R-prep DLQ audit follow-up).
- `specs/05_quality/runbooks/RB-FM-151-stripe-outage.md`.
- `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md`.
- `specs/03_architecture/failure_modes.md` FM-151, FM-156, FM-400.

## Controls evidenced

CC7.3 · CC7.4 · CC7.5 · plus CTRL-WEBHOOK-001 (signature validation), CTRL-WEBHOOK-002 (idempotency), CTRL-WEBHOOK-003 (DLQ + replay), CTRL-AUDIT-001/002.
