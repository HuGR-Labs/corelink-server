---
id: "STATUSPAGE-SUBSCRIBER-IMPORT"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "CS Lead + DevOps (dual)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "marketing/launch/STATUS-PAGE-SPEC.md"
tags:
  - "marketing"
  - "launch"
  - "status-page"
  - "subscribers"
  - "lighthouse"
  - "lgpd"
  - "gdpr"
  - "wt-r8-1"
---

# Statuspage.io — Lighthouse Subscriber Import

> **Purpose:** bulk-add the 3 lighthouse customers (multiple contacts each) plus internal mandatory subscribers to the Statuspage.io page, with **opt-in confirmation** so we do not violate LGPD Art. 8 (Brazilian consent) or GDPR Art. 6(1)(a) (EU consent) by mass-emailing people who did not consent.
> **Audience:** CS Lead (executor) + DevOps (API caller) + VPSec (consent-evidence reviewer).
> **Cross-references:** `STATUS-PAGE-SPEC.md` §4 (subscriber policy), `STATUSPAGE-INIT.md` §7 (UI toggles), `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` (the contract that promised this), `docs/legal/privacy-policy.md`.
> **Hard rule:** **opt-in evidence must exist BEFORE the API call adds an email**. The lighthouse-customer contract (`CUSTOMER-PLAYBOOK.md` Day-1 step) covers this, but each individual contact email needs an audit-trail row.

---

## 1. Scope

### 1.1 Lighthouse customers (3 customers × 2–3 contacts each)

Per `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` Day-1, each lighthouse customer designates:

- **Primary business contact** (decision-maker, billing).
- **Primary SRE / oncall contact** (technical, paged during incidents).
- (Optional) **Compliance / security contact** for SOC 2 / audit signals.

Target counts at GA:

| Customer | Primary biz | Primary SRE | Compliance | Total contacts |
|---|---|---|---|---|
| Lighthouse #1 | 1 | 1 | 1 | 3 |
| Lighthouse #2 | 1 | 1 | 0 | 2 |
| Lighthouse #3 | 1 | 1 | 1 | 3 |
| **Subtotal** | | | | **8** |

### 1.2 Internal mandatory subscribers

Per `STATUS-PAGE-SPEC.md` §4.1:

| Address | Reason | Per-component filter? |
|---|---|---|
| `oncall+statuspage@corelink.pagerduty.com` | PagerDuty service email (mirror of SP back into PD for visibility) | All components |
| `security@corelink.dev` | Security alerting bridge | All components |
| `support@corelink.dev` | CS-OC visibility | All components |
| `compliance@corelink.dev` | SOC 2 / audit trail | C5 (Audit) + C4 (BYOK) only |
| `ceo@corelink.dev` | Executive visibility | SEV1 only (filtered downstream by mail rule) |
| `sre-lead@corelink.dev` | SRE leadership | All components |
| `vpsec@corelink.dev` | Security leadership | All components |

**Internal-mandatory total: 7 addresses.**

### 1.3 Grand totals at GA

- **Email subscribers** to bulk-import: **15** (8 lighthouse + 7 internal).
- **SMS subscribers** to bulk-import: **3** (one SRE contact per lighthouse customer, SEV1-only, explicit phone consent on file).
- **Webhook subscribers**: **0 at T-0** (customers can self-serve post-launch via `status.corelink.dev`).

---

## 2. Consent / opt-in evidence

### 2.1 What counts as evidence

For **each** email or phone we bulk-import, the audit trail must contain one of:

1. **Lighthouse contract addendum** signed via DocuSign — Section 4 ("Communications") explicitly lists each contact + check-marks "Status page notifications opt-in (LGPD Art. 8 / GDPR Art. 6(1)(a))". DocuSign envelope ID + page reference recorded.
2. **Email opt-in confirmation** with a clickable link in a confirmation email; click event recorded with timestamp + IP.
3. **Internal mandatory** (the 7 addresses in §1.2): consent inferred from employment / role; documented in `docs/legal/internal-data-processing-record.md`.

### 2.2 Where evidence is stored

| Source | Stored at | Retention |
|---|---|---|
| DocuSign envelope | `specs/_audits/LIGHTHOUSE-OPTIN-<customer-slug>-<date>.pdf` | 7 years (SOC 2 + LGPD Art. 16) |
| Email confirmation click | Statuspage audit log → monthly S3 export `corelink-soc2-evidence-prod/statuspage-audit/` | 7 years |
| Internal mandatory | `docs/legal/internal-data-processing-record.md` row | Tied to employment record retention |

### 2.3 Hard rule: dual-evidence for SMS

SMS = phone numbers. LGPD Art. 11 (Brazilian) and TCPA (US) both impose **stricter** consent rules for SMS than email. For the 3 SMS subscribers:

- DocuSign opt-in **plus** an inbound confirmation reply ("YES" via SMS to the Atlassian Statuspage SMS short-code) → both archived.
- If inbound confirmation reply is missing 7 days after the DocuSign opt-in: that contact stays **email-only** at GA. CS Lead follows up post-T+7d.

---

## 3. Bulk-add API call

### 3.1 Endpoint shape

Statuspage REST API exposes per-page subscriber creation. There is no native batch endpoint (Atlassian docs current as of 2026-05); we loop with rate limiting.

```http
POST https://api.statuspage.io/v1/pages/${STATUSPAGE_PAGE_ID}/subscribers
Authorization: OAuth ${STATUSPAGE_API_KEY}
Content-Type: application/json

{
  "subscriber": {
    "email": "alice@lighthouse-1.example",
    "skip_confirmation_notification": false,
    "component_ids": [
      "${STATUSPAGE_COMPONENT_API}",
      "${STATUSPAGE_COMPONENT_CAS_READ}",
      "${STATUSPAGE_COMPONENT_CAS_WRITE}",
      "${STATUSPAGE_COMPONENT_BYOK}",
      "${STATUSPAGE_COMPONENT_AUDIT}",
      "${STATUSPAGE_COMPONENT_BILLING}",
      "${STATUSPAGE_COMPONENT_DOCS}",
      "${STATUSPAGE_COMPONENT_ADMIN}"
    ]
  }
}
```

**Key field:** `"skip_confirmation_notification": false` — Statuspage will email the recipient a **"please confirm your subscription"** link. The recipient must click it for their subscription to activate. This is our consent-loop closing event for audit.

### 3.2 SMS subscriber API call

```http
POST https://api.statuspage.io/v1/pages/${STATUSPAGE_PAGE_ID}/subscribers
Authorization: OAuth ${STATUSPAGE_API_KEY}
Content-Type: application/json

{
  "subscriber": {
    "phone_country": "BR",
    "phone_number": "+5511999990000",
    "skip_confirmation_notification": false,
    "component_ids": [
      "${STATUSPAGE_COMPONENT_API}",
      "${STATUSPAGE_COMPONENT_CAS_READ}",
      "${STATUSPAGE_COMPONENT_CAS_WRITE}",
      "${STATUSPAGE_COMPONENT_BYOK}"
    ]
  }
}
```

Statuspage sends an SMS with a confirmation code; recipient texts back the code to confirm. This is the inbound confirmation captured under §2.3.

### 3.3 Rate-limit etiquette

- Statuspage API limit: **1 req/sec per page** (Atlassian docs).
- The bulk-import script sleeps 1.5s between calls (50% headroom).
- For 15 email + 3 SMS = 18 calls, total wall time ≈ 27 seconds + confirmation latency.

### 3.4 Implementation sketch

```bash
# scripts/statuspage-bulk-subscribe.example.sh — NOT YET COMMITTED (separate WI)
# Reads from a CSV at marketing/launch/STATUSPAGE-SUBSCRIBER-IMPORT-roster.csv
# (gitignored — contains PII). Each row: contact_name,email_or_phone,kind,components,evidence_envelope_id

while IFS=, read -r name target kind components evidence; do
  if [ -z "$evidence" ]; then
    echo "SKIP $name — missing evidence envelope id" >&2; continue
  fi
  if [ "$kind" = "email" ]; then
    curl -sS -X POST "https://api.statuspage.io/v1/pages/${STATUSPAGE_PAGE_ID}/subscribers" \
      -H "Authorization: OAuth ${STATUSPAGE_API_KEY}" \
      -H "Content-Type: application/json" \
      -d "$(jq -n --arg e "$target" --argjson c "$components" \
            '{subscriber:{email:$e,skip_confirmation_notification:false,component_ids:$c}}')"
  elif [ "$kind" = "sms" ]; then
    country="${target%%:*}"; number="${target#*:}"
    curl -sS -X POST "https://api.statuspage.io/v1/pages/${STATUSPAGE_PAGE_ID}/subscribers" \
      -H "Authorization: OAuth ${STATUSPAGE_API_KEY}" \
      -H "Content-Type: application/json" \
      -d "$(jq -n --arg cc "$country" --arg ph "$number" --argjson c "$components" \
            '{subscriber:{phone_country:$cc,phone_number:$ph,skip_confirmation_notification:false,component_ids:$c}}')"
  fi
  sleep 1.5
done < marketing/launch/STATUSPAGE-SUBSCRIBER-IMPORT-roster.csv
```

The CSV itself is **not committed** to git — it contains PII. Its location:

- Development / staging: `~/.corelink-secrets/statuspage-roster-staging.csv`.
- Production: only on the DevOps engineer's machine during the import session; deleted after import per `docs/legal/data-minimization-policy.md` §3.

The roster CSV's **schema** is committed at `marketing/launch/STATUSPAGE-SUBSCRIBER-IMPORT-roster.schema.json` (header only — no values) as a side-companion if needed; for GA, the schema lives in §4.1 below and that suffices.

---

## 4. Opt-in confirmation flow (end-to-end)

### 4.1 Roster CSV shape

```
contact_name,target,kind,components,evidence_envelope_id,evidence_pdf_path
"Alice Builder","alice@lighthouse-1.example",email,"C1,C2,C3,C4,C5,C6,C7,C8","DS-LIGHTHOUSE1-2026-05-01","specs/_audits/LIGHTHOUSE-OPTIN-acme-2026-05-01.pdf"
"Alice Builder","BR:+5511999990000",sms,"C1,C2,C3,C4","DS-LIGHTHOUSE1-2026-05-01","specs/_audits/LIGHTHOUSE-OPTIN-acme-2026-05-01.pdf"
... (etc)
```

### 4.2 Flow

```
            +-------------------------+
            | CS Lead receives signed |
            | lighthouse contract     |
            |  addendum (DocuSign)    |
            +-----------+-------------+
                        |
                        v
       +-------------------------------+
       | CS Lead drops the signed PDF  |
       | into specs/_audits/ + emails  |
       | DevOps with envelope_id + the |
       | contact list                  |
       +-------------------------------+
                        |
                        v
       +-------------------------------+
       | DevOps appends rows to the    |
       | local roster CSV (NOT git)    |
       +-------------------------------+
                        |
                        v
       +-------------------------------+
       | DevOps runs bulk-subscribe    |
       | script during T-7d window     |
       +-------------------------------+
                        |
            +-----------+-----------+
            |                       |
            v                       v
  Statuspage emails        Statuspage SMS-es
  confirmation link        confirmation code
            |                       |
            v                       v
  Subscriber clicks        Subscriber texts
  "Confirm" link           code back
            |                       |
            v                       v
  Statuspage activates     Statuspage activates
  subscription             subscription
            |                       |
            +-----------+-----------+
                        |
                        v
       +-------------------------------+
       | DevOps pulls subscriber list  |
       | from /subscribers endpoint    |
       | and verifies all expected     |
       | rows are state=quarantined or |
       | state=active                  |
       +-------------------------------+
                        |
                        v
       +-------------------------------+
       | CS Lead chases any subscriber |
       | still in state=quarantined at |
       | T-72h (re-send the link)      |
       +-------------------------------+
```

### 4.3 SLA / acceptance bars

- **Pre-T-7d:** ≥ 80% of subscribers expected to confirm within 24h of the bulk add (covers timezone variance).
- **T-72h cutoff:** all 15 email + 3 SMS expected to be `state=active`. If not:
  - **Email failures** → re-send confirmation link via Statuspage UI; if 2 retries fail, that contact is **not subscribed at GA**. Escalate to CS Lead → CS Lead asks the customer to designate an alternate contact.
  - **SMS failures** → fall back to email-only for that person (per §2.3 hard rule). Do **not** mark "subscribed" without inbound confirmation.

---

## 5. Audit + reconciliation

### 5.1 Daily reconciliation (T-7d through T-0)

Run nightly at 02:00 PT:

```bash
curl -sS -H "Authorization: OAuth ${STATUSPAGE_API_KEY}" \
  "https://api.statuspage.io/v1/pages/${STATUSPAGE_PAGE_ID}/subscribers" \
  | jq -r '.[] | [.id,.email // (.phone_country + ":" + .phone_number),.state,.created_at] | @csv' \
  > /tmp/statuspage-subscribers-$(date -u +%F).csv
```

Diff against the roster CSV. Any row in roster but missing from API = follow-up. Any row in API but missing from roster = **immediate red flag** (someone subscribed by mistake or an attacker added a row). Page DevOps with SEV2.

### 5.2 Final snapshot at T-1h

Captured into the war room log per `RB-LAUNCH-WAR-ROOM-COORDINATION.md` §3 step 3:

- Total active email subscribers: **15** (expected baseline).
- Total active SMS subscribers: **≤ 3** (depends on confirmation).
- Any subscriber in non-active state at T-1h is logged but **not blocking** for launch — the page launches; the contact can self-subscribe at T-0 by clicking the public "Subscribe" widget.

---

## 6. Post-launch growth

Once `status.corelink.dev` is publicly announced at T-0 (per `LAUNCH-CHECKLIST-V2.md` row L22):

- Public visitors can self-subscribe via the Statuspage native widget (already enabled in `STATUSPAGE-INIT.md` §7.1).
- No further bulk imports planned for the launch window.
- Post-launch enterprise customer onboarding (R10 / post-GA work) extends the lighthouse-style bulk add to new design partners; the same playbook applies.

---

## 7. Cross-references

- `STATUS-PAGE-SPEC.md` §4 — canonical subscriber policy.
- `STATUSPAGE-INIT.md` §7 — UI toggles enabling email / SMS / webhook / RSS / Slack.
- `STATUSPAGE-PRE-LAUNCH-TEST.md` §4 — pre-launch test that exercises confirmation flow against staging page.
- `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` — the contract that established the consent.
- `docs/legal/privacy-policy.md` + `docs/internal/auth-event-taxonomy.md` — consent-log event types.
- `RB-LAUNCH-WAR-ROOM-COORDINATION.md` §3 — war room baseline snapshot.
