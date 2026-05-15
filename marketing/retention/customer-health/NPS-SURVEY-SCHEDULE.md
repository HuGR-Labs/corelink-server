# NPS-SURVEY-SCHEDULE — Customer-health survey wave cadence

> **Status:** ACTIVE. Owned by the CS lead. Pairs with the
> `corelink-survey` crate (mint + record primitives), the
> `SURVEY-ANALYSIS-PROTOCOL.md` (CS weekly review workflow), and the
> `RB-SURVEY-ABUSE.md` runbook.

## 0. Why this doc exists

This file is the canonical timeline for **when** customer-health
surveys go out. The `corelink-survey` crate gives us the actual
infrastructure (HMAC-signed invite tokens + the recorder); this doc
tells the CS team **what** to send + **when**.

## 1. Six waves pre-GA → GA+90d

| Wave | Trigger                                | Audience                          | Survey kind                   | Cadence                |
| ---- | -------------------------------------- | --------------------------------- | ----------------------------- | ---------------------- |
| 1    | T+0d after first PAT issued            | New tenant primary owner          | CSAT (`csat-onboarding-<wave>`) | Once per tenant       |
| 2    | T+14d after first PAT issued           | New tenant primary owner          | NPS (`nps-w2-<wave>`)         | Once per tenant        |
| 3    | T+30d after first PAT issued           | New tenant primary owner          | CSAT + free-text              | Once per tenant        |
| 4    | T+90d after first PAT issued           | New tenant primary owner          | NPS + free-text               | Once per tenant        |
| 5    | Rolling — every 90d after wave 4       | Returning tenant primary owner    | NPS + multi-choice            | Quarterly             |
| 6    | Triggered (post-incident, post-feature) | Affected tenant's primary owner   | CSAT + free-text              | Event-driven           |

Wave 1-4 are the **lifecycle waves** — every new tenant goes through
them in order. Waves 5-6 are the **steady-state cadence**.

## 2. Per-wave content

### Wave 1 (T+0d, CSAT onboarding)

- **Slug:** `csat-onboarding-<YYYY-Qn>`
- **Kind:** CSAT (1..=5)
- **Question:** "How easy was setting up your first CoreLink tenant?"
- **TTL:** 7 days from issuance.
- **Target response rate:** ≥ 35%.
- **CS action on score ≤ 2:** within 48h, CS lead schedules a
  30-min "first-week check-in" with the tenant.

### Wave 2 (T+14d, NPS post-integration)

- **Slug:** `nps-w2-<YYYY-Qn>`
- **Kind:** NPS (0..=10)
- **Question:** "How likely are you to recommend CoreLink to a peer?"
- **TTL:** 7 days.
- **Target NPS:** ≥ +30.
- **CS action on detractor (≤ 6):** save-call within 5 business days.

### Wave 3 (T+30d, CSAT + free-text)

- **Slug:** `csat-30d-<YYYY-Qn>` (CSAT) +
  `free-text-30d-<YYYY-Qn>` (separate invite, same recipient,
  separate `jti` so replay rejection is per-survey).
- **Kind:** CSAT then FreeText ("What's missing?").
- **TTL:** 14 days.
- **Use:** mid-funnel friction discovery; feeds the weekly free-text
  themes in `SURVEY-ANALYSIS-PROTOCOL.md` §1.3.

### Wave 4 (T+90d, NPS + free-text)

- **Slug:** `nps-w4-<YYYY-Qn>` + `free-text-90d-<YYYY-Qn>`
- **Kind:** NPS then FreeText.
- **TTL:** 14 days.
- **Use:** retention signal; entry condition for lighthouse-customer
  recruitment (NPS ≥ 9 = candidate).

### Wave 5 (every 90d steady-state)

- **Slug:** `nps-steady-<YYYY-Qn>` + `mc-priorities-<YYYY-Qn>`
- **Kind:** NPS + MultiChoice ("Pick 2-3 areas we should invest in
  this quarter: [Performance, Cost, BYOK, Compliance, DX, …]").
- **TTL:** 14 days.

### Wave 6 (event-driven)

- **Slug:** `csat-incident-<incident-id>` or
  `csat-feature-<feature-slug>-<YYYY-Qn>`.
- **Kind:** CSAT + FreeText.
- **TTL:** 7 days.
- **Triggers:**
  - Post-SEV-2 / SEV-1 incident: sent 48h after resolution to every
    tenant in the blast radius.
  - Post-feature launch: sent 14d after a tenant first uses the new
    feature (event detection via the `corelink-analytics` first-use
    signal).

## 3. Per-tenant guardrails

To prevent survey fatigue (which would also be flagged by the abuse
runbook §2.3):

- **At most 1 survey per tenant per 14 days.**
- **At most 4 surveys per tenant per 90 days.**
- Lifecycle waves (1-4) always take priority over steady-state (5)
  and event-driven (6) — if a wave-5 cadence would land within 14d
  of a wave-2 issuance, wave-5 is deferred.

These guardrails are enforced at issuance time by the
`survey_invites` writer (deferred to production wiring) checking the
existing `(recipient_hash, survey_id, submitted_at_ms)` triples.

## 4. Infrastructure cross-links

- **Crate:** `crates/corelink-survey/` — `SurveyLinkSigner::sign_invite`
  mints the invite URL; `SurveyResponseRecorder::record` verifies +
  records the response.
- **Migration:** `migrations/d1/0046_survey_responses.sql` — the
  append-only response table.
- **Landing page:** `apps/docs/docs/how-to/survey-response.mdx` — what
  the recipient sees on click-through.
- **Abuse runbook:** `specs/_runbooks/RB-SURVEY-ABUSE.md`.
- **Analysis protocol:** `SURVEY-ANALYSIS-PROTOCOL.md` (sibling file).
- **Lighthouse playbook:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`
  — feeds the per-lighthouse pulse view.
- **Roadmap:** ROADMAP §6 (retention infra) + §8 (customer-health
  lighthouse integration).
