---
id: "STATUSPAGE-PRE-LAUNCH-TEST"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "marketing/launch/STATUS-PAGE-SPEC.md"
tags:
  - "marketing"
  - "launch"
  - "status-page"
  - "test-plan"
  - "t-minus-7-days"
  - "wt-r8-1"
---

# Statuspage.io — T-7d Pre-Launch Test Plan

> **Purpose:** the acceptance-test plan we run at **T-7d** against the freshly-provisioned Statuspage.io account, BEFORE we import lighthouse subscribers and BEFORE we announce `status.corelink.humangr.com` publicly. Pass = green-light for §7 subscriber import + T-0 banner flip. Fail = rollback (see §6).
> **Audience:** SRE Lead (executor) + DevOps (operator) + VPSec (test-evidence reviewer).
> **Cross-references:** `STATUS-PAGE-SPEC.md`, `STATUSPAGE-INIT.md`, `STATUSPAGE-SUBSCRIBER-IMPORT.md`, `scripts/statuspage-webhook-receiver.example.yaml`, `LAUNCH-CHECKLIST-V2.md`.
> **Hard rule:** **every test runs against a TEST component**, not against C1..C8, until §5. The test component is created in §1 and deleted in §6 after sign-off.

---

## 1. Pre-conditions

Before any test runs, **all** of these must be true:

- [ ] `STATUSPAGE-INIT.md` §1–§7 complete (page exists, components present, secrets stored).
- [ ] DNS for `status.corelink.humangr.com` resolves; TLS cert valid for ≥ 80 days.
- [ ] Fallback page at `https://humangr.com/corelink/status` returns HTTP 200.
- [ ] Webhook receiver spec reviewed; **native PagerDuty → Statuspage integration is the active path under test** (custom receiver is spec-only for GA).
- [ ] **Test component created:** add a 9th component called `TEST — DO NOT SUBSCRIBE` in group `Operations`. Visibility: **public**, **but** marked with a banner description: "Internal test surface; do not subscribe; will be removed before T-0."
- [ ] **Test PagerDuty service created:** `corelink-statuspage-test-prerelease` with the Statuspage integration attached and pointing at the test component.
- [ ] Roster of 2 internal email addresses for subscriber-notification testing: `sre-lead@humangr.com` and `devops@humangr.com`. Both are §1.2 internal-mandatory subscribers already authorized for the page; they self-consent for the test.
- [ ] **Date of run:** T-7d ± 1 calendar day. Snapshot of all UI screenshots saved to `specs/_audits/STATUSPAGE-PRELAUNCH-<run-date>/`.

---

## 2. Test cases

### Test 1 — Component status transitions (manual)

**Goal:** confirm we can drive a component through all 4 states from the Statuspage UI and that the public page reflects each within 60 sec.

**Steps:**

1. In `https://manage.statuspage.io`, set the TEST component to `Degraded performance`.
2. Open `https://status.corelink.humangr.com` in a fresh incognito tab; refresh.
3. Verify the test component shows yellow / "Degraded performance" within 60 s.
4. Set the TEST component to `Partial outage`; verify orange within 60 s.
5. Set to `Major outage`; verify red within 60 s.
6. Set to `Operational`; verify green within 60 s.

**Pass:** every transition observed within 60 s. **Fail:** any transition > 60 s, or color does not match expected — escalate to Atlassian support; do not proceed.

---

### Test 2 — Incident lifecycle (manual, end-to-end)

**Goal:** confirm we can create an incident, post updates, and close it; that subscribers receive the right notifications at each step.

**Subscribe pre-step:** subscribe `sre-lead@humangr.com` to the TEST component **only** via the public widget on `https://status.corelink.humangr.com`. Click the confirmation link in the email that arrives.

**Steps:**

1. In Statuspage UI → **Incidents → New incident**:
   - Title: `TEST — Investigating issue with TEST component`
   - Status: `Investigating`
   - Affected component: `TEST` → `Major outage`
   - Body: paste `CRISIS-COMMS-TEMPLATES.md` §A.1 SEV1 template (placeholders filled with TEST).
   - Deliver notifications: **enabled**.
   - Click **Create**.
2. Confirm `sre-lead@humangr.com` receives the incident-open email within 5 min. Capture screenshot.
3. Add an update to the incident:
   - Status: `Identified`
   - Body: paste §A.2 of CRISIS-COMMS-TEMPLATES.md.
4. Confirm the update email arrives.
5. Add a `Monitoring` update.
6. Resolve the incident with §A.3 body.
7. Confirm the resolution email arrives.
8. Open `https://status.corelink.humangr.com/history` and confirm the TEST incident appears with all 4 status transitions and a clear timeline.

**Pass:** all 4 notifications received within 5 min each; history page shows full timeline. **Fail:** any missing notification → check `sre-lead@humangr.com` spam folder; if absent there too, ticket Atlassian support; do not proceed.

---

### Test 3 — PagerDuty → Statuspage automation (the auto-publish hook)

**Goal:** confirm the native integration fires a Statuspage incident when a PagerDuty SEV1 incident is opened.

**Steps:**

1. In PagerDuty, trigger a synthetic incident on the test service:
   ```bash
   curl -sS -H "Authorization: Token token=${PD_API_TOKEN}" \
     -H "Content-Type: application/json" \
     -H "From: sre-lead@humangr.com" \
     -X POST https://api.pagerduty.com/incidents \
     -d '{
       "incident": {
         "type": "incident",
         "title": "TEST — synthetic SEV1 to exercise Statuspage bridge",
         "service": { "id": "${PD_TEST_SERVICE_ID}", "type": "service_reference" },
         "urgency": "high"
       }
     }'
   ```
2. Within 60 s, expect a new Statuspage incident on the TEST component with status `Investigating` and the SEV1 auto-publish template body.
3. Acknowledge the PD incident → expect the SP incident status to advance to `Identified` within 60 s.
4. Resolve the PD incident → expect the SP component to flip back to `Operational` after the 15-min recovery window (per `STATUS-PAGE-SPEC.md` §9). **For test speed**, the recovery window can be overridden to 60 s in staging via the failure-injection env var; do **not** override in prod.
5. Confirm `sre-lead@humangr.com` received the SEV1 email AND (if SMS opt-in) an SMS.

**Pass:** all 3 transitions auto-driven by PD; 1 email + 1 SMS delivered. **Fail:** any transition manual → re-check PD integration severity mapping in `STATUSPAGE-INIT.md` §6.1.

---

### Test 4 — Subscriber confirmation flow (consent-loop)

**Goal:** confirm the email + SMS confirmation flow Atlassian sends fires correctly and that the API reports the subscriber state transitions.

**Steps:**

1. Use the bulk-add API call shape from `STATUSPAGE-SUBSCRIBER-IMPORT.md` §3.1 to add `devops@humangr.com` to the TEST component.
2. Within 5 min, expect a confirmation email at that address. Capture screenshot.
3. Click the confirmation link.
4. Query `GET /subscribers` and confirm the row's `state` transitions from `quarantined` to `active`.
5. Repeat with a TEST SMS subscriber (an internal phone number, **NOT** a lighthouse customer phone).
6. Reply with the SMS confirmation code.
7. Query `GET /subscribers` and confirm SMS row state transitions to `active`.

**Pass:** both email and SMS subscriber rows are `active` within 10 min of API call. **Fail:** if email confirmation arrives but click does not activate → check Atlassian outbound link domain (sometimes blocked by corporate firewalls); whitelist if needed. If SMS confirmation never arrives → cost-control may have throttled; check Atlassian usage dashboard.

---

### Test 5 — Webhook subscriber payload (for power-user customers)

**Goal:** confirm Statuspage outbound webhooks deliver a signed payload to a customer-provided URL.

**Steps:**

1. Stand up a temporary `webhook.site` URL (or local `ngrok` tunnel).
2. Add a webhook subscriber via the API targeting that URL, scoped to TEST component.
3. Open + update + resolve a TEST incident (re-use Test 2 procedure).
4. Confirm 3 POST requests arrive at the webhook URL with correct JSON shape and a valid `X-Statuspage-Signature` header that verifies against `STATUSPAGE_WEBHOOK_SECRET` using HMAC-SHA256.

**Pass:** 3 webhooks arrive within 5 min total; all signatures verify. **Fail:** any signature mismatch → rotate `STATUSPAGE_WEBHOOK_SECRET` and re-run; if persistent, ticket Atlassian.

---

### Test 6 — Retry + SEV2 fallback (failure injection)

**Goal:** confirm the receiver's retry-then-page-SRE behavior. **Note:** this test exercises the **custom receiver** (`scripts/statuspage-webhook-receiver.example.yaml`) **only if** the receiver is deployed to staging by T-7d. If the receiver remains spec-only at GA (per `STATUSPAGE-INIT.md` §6.2), **skip Test 6** and rely on the native integration's built-in retry semantics; flag this skip in the test-evidence summary.

**Steps (only if receiver deployed):**

1. Set `BRIDGE_FAIL_NEXT_REQUEST=3` on the staging receiver Worker.
2. Trigger a PD incident as in Test 3.
3. Confirm the receiver retries 3× with the documented backoff (`5s / 15s / 45s`) — visible in logs.
4. After the 3rd failure, confirm the receiver emits a `corelink-sre-internal-staging` SEV2 PagerDuty incident titled "Statuspage publish failed".
5. Clear `BRIDGE_FAIL_NEXT_REQUEST`; retrigger the original incident manually via Statuspage UI.

**Pass:** retries match documented backoff; SEV2 fires within 90 s of 3rd failure. **Fail or skip:** documented in §7 evidence summary.

---

### Test 7 — Public-page degradation under simulated DDoS (light)

**Goal:** confirm Atlassian's CDN sustains the page under elevated load — we will not actually DDoS Atlassian, just send a small burst.

**Steps:**

1. From a single load-test box, send 100 requests/sec for 30 sec to `https://status.corelink.humangr.com`.
2. Confirm response time stays < 500 ms p99 throughout.
3. Confirm zero 5xx responses.

**Pass:** p99 < 500 ms, zero 5xx. **Fail:** ticket Atlassian; if Atlassian's CDN can't handle 100 rps from one box, our launch traffic is at risk and we need to escalate.

> **Hard rule:** do **not** scale beyond 100 rps. Atlassian terms forbid load-testing their infrastructure; 100 rps is a smoke test, not a stress test.

---

## 3. Notification matrix verification

After Test 2 + Test 3 complete, verify the notification policy from `STATUS-PAGE-SPEC.md` §4.3 is enforced. Capture screenshots of the inbox for each.

| Severity / event | Subscriber config | Expected delivery | Observed? |
|---|---|---|---|
| SEV1 + Major Outage | Email + SMS opt-in + RSS + Slack | All 4 channels | ☐ |
| SEV2 + Partial Outage | Per-component email + RSS + Slack | 3 channels | ☐ |
| SEV3 + Degraded | (no email blast) | RSS only | ☐ |
| Scheduled maintenance | Per-component email + RSS | 2 channels | ☐ |

If any row diverges, fix the Statuspage page-level notification settings in §7.1 of `STATUSPAGE-INIT.md` and re-run.

---

## 4. Evidence collection

For each test:

1. Screenshot the Statuspage UI before / after.
2. Screenshot the destination inbox / SMS log.
3. Save the JSON response of API calls to `specs/_audits/STATUSPAGE-PRELAUNCH-<run-date>/`.
4. Append a per-test row to `specs/_audits/STATUSPAGE-PRELAUNCH-<run-date>/RESULTS.md`:
   ```
   | Test | Started (UTC) | Finished (UTC) | Outcome | Evidence path | Notes |
   |---|---|---|---|---|---|
   | T1 | 14:02 | 14:08 | PASS | ./test-1/ | All transitions ≤ 30 s. |
   ...
   ```

The full evidence directory is the artifact attached to the `wt-r8-1` SEAL gate.

---

## 5. Gate to production usage

Test outcome → next action:

| All 7 tests PASS | Most pass, ≤ 1 minor anomaly | Any test FAIL |
|---|---|---|
| Proceed to `STATUSPAGE-SUBSCRIBER-IMPORT.md` §4 (bulk import). | SRE Lead + VPSec sign off in writing; document anomaly with a follow-up WI; proceed. | **STOP.** Execute §6 rollback. Do not import subscribers. Do not advance to T-0. |

---

## 6. Rollback (if any test reveals a configuration issue)

| Issue surface | Rollback action |
|---|---|
| Custom domain TLS misissued / DNS misconfigured | (a) Revert CNAME at Cloudflare to a 5-min TTL placeholder; (b) keep using `corelink.statuspage.io` (Atlassian default); (c) re-issue cert; re-run §1. |
| Subscriber confirmation flow broken | (a) Disable bulk-import API token (Atlassian → revoke `STATUSPAGE_API_KEY`); (b) re-create token with same scope; (c) re-run Test 4. |
| PagerDuty integration severity mapping wrong | (a) Revert integration severity-to-status mapping in PagerDuty UI; (b) re-trigger synthetic PD incident; (c) re-run Test 3. |
| Page-level notification policy wrong (e.g., SEV3 sending emails) | (a) Set page to **"Maintenance window"** (Statuspage native) so subscribers don't get spam; (b) fix toggle in `STATUSPAGE-INIT.md` §7.1; (c) re-run §3 matrix. |
| Atlassian itself appears unreliable (test failures point to Atlassian, not us) | (a) Open Atlassian support ticket with collected evidence; (b) escalate to Atlassian account exec via VPSec; (c) **DEFER launch** per `CRISIS-COMMS-TEMPLATES.md` §F if the SLA risk is unresolved within 48h. |
| All recoverable issues fixed | (a) Delete the TEST component (Statuspage UI → Components → TEST → Delete); (b) delete `corelink-statuspage-test-prerelease` PD service; (c) commit evidence to `specs/_audits/`. |

**Hard rule:** the TEST component **must be deleted** before T-24h. A leftover public TEST component is a launch-day reputational risk.

---

## 7. Sign-off

SRE Lead + DevOps + VPSec all sign the `RESULTS.md` summary at the bottom of the run. The signed file is the artifact for `LAUNCH-CHECKLIST-V2.md` row L8 (T-12h status page set to "Preparing for launch").

---

## 8. Cross-references

- `STATUS-PAGE-SPEC.md` §4.3 (notification matrix), §7 (auto-publish), §9 (recovery confirmation).
- `STATUSPAGE-INIT.md` §6 (PD integration), §7 (subscription toggles).
- `STATUSPAGE-SUBSCRIBER-IMPORT.md` §3 (bulk-add API call), §4 (confirmation flow).
- `scripts/statuspage-webhook-receiver.example.yaml` (custom receiver, used in Test 6 only if deployed).
- `CRISIS-COMMS-TEMPLATES.md` §A (template bodies used as incident text).
- `LAUNCH-CHECKLIST-V2.md` rows L8 (T-12h "Preparing"), L22 (T-0 "Operational").
