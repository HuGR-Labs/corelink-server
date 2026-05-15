---
id: "RB-SURVEY-ABUSE"
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
tags: ["runbook", "r-prep", "retention", "customer-health", "survey", "nps", "csat", "abuse", "rate-limit", "anti-spam", "analyst-review"]
---

# RB-SURVEY-ABUSE — Customer-health survey abuse + spam triage

> **Status:** ACTIVE. Owned by the R-prep retention lane. Pairs with
> the `corelink-survey` crate (HMAC-signed invite tokens + per-`jti`
> replay rejection) and the wave schedule documented in
> `marketing/retention/customer-health/NPS-SURVEY-SCHEDULE.md`.

## 1. Scope

This runbook covers abuse vectors against the customer-health survey
surface — the click-through endpoint that records NPS / CSAT /
free-text / multi-choice responses for an HMAC-signed invite token. It
does **not** cover survey *issuance* abuse (forged email lists; that
belongs to the outbound-email worker's own abuse runbook).

The relevant invariants:

- **Token unforgeability** — `corelink-survey::token::decode_and_verify`
  uses constant-time HMAC-SHA256 over a 32-byte key. A blind forge
  requires breaking SHA-256 or stealing the key. Property test
  `forged_token_with_wrong_key_is_rejected` (256 cases) gates this.
- **Replay rejection** — every `jti` (UUIDv7 in the token) is recorded
  in the `survey_responses.token_jti UNIQUE` column. Application-layer
  in-memory dedup is the fast path; DB UNIQUE is belt-and-suspenders.
- **Audit fail-CLOSED** — `corelink-survey::recorder` emits the
  `survey.response.recorded` event BEFORE the row insert. If audit
  emit fails, no row lands.

## 2. Abuse vectors + mitigations

### 2.1 Token brute-force

**Vector:** attacker hammers `/survey/respond?t=<random-blob>` hoping
to land a valid HMAC.

**Mitigation:**

- HMAC-SHA256 has 2^128 forge cost (truncated to 32 bytes; full
  digest carried for survey tokens since this isn't a hot path).
- Apps/server route wraps the recorder in `corelink-ratelimit`:
  - **Per-IP**: 60 requests / 10 min, token-bucket refill.
  - **Per-IP burst guard**: a per-IP `429` after 10 consecutive
    `InvalidSignature` results, escalating to a 1-hour block via
    `corelink-abuse` after 100 within a 15-minute window.

**Detection:** Prometheus alert
`corelink.survey.record_total{outcome="invalid_signature"}` rate
> 1 req/s for > 5 min pages the on-call (SEV-3, escalates to SEV-2 on
> > 10 req/s for 10 min). The audit chain captures every reject as
> `survey.response.rejected{outcome="invalid_signature"}` so the SIEM
> can pivot on the attacker IP hash.

### 2.2 Replay of a valid token

**Vector:** attacker captures a victim's invite URL (e.g. via email
forwarder, screen-share) and submits multiple responses to skew a
tenant's NPS aggregate.

**Mitigation:**

- `corelink-survey::recorder` rejects the second submission with
  `SurveyError::ReplayRejected` via in-memory `jti` dedup.
- D1 `survey_responses.token_jti UNIQUE` constraint catches any race
  through the in-memory check (e.g. two recorder instances behind a
  load balancer).
- Property test `replay_is_rejected` (256 cases) gates the
  application-layer guarantee.

**Detection:** every replay attempt emits
`survey.response.rejected{outcome="replay"}` with the `jti` field in
the audit envelope. CS analyst weekly review (see §3) filters this
list — a `jti` with replay attempts from > 3 distinct `ip_hash`
values is a strong signal of credential sharing or list compromise.

### 2.3 Per-recipient duplicate-account spam

**Vector:** attacker creates many accounts under different recipient
emails and submits responses to one wave, hoping to dominate the NPS
score for a competitor tenant.

**Mitigation:**

- `recipient_hash` is salted-SHA-256 of the normalized email. Multiple
  surveys to the **same recipient** in the same wave is naturally
  prevented because the issuer mints one invite per `(survey_id,
  recipient_hash)` pair.
- The `survey_invites` issuer table (deferred from this WI; lands
  with the production wiring) carries a UNIQUE `(survey_id,
  recipient_hash)` constraint so a double-issuance is caught at mint
  time.
- The `idx_survey_responses_by_recipient` index supports the
  analyst's per-recipient cross-survey check: a `recipient_hash`
  submitting to > 5 surveys within 7 days is auto-flagged.

**Detection:** weekly D1 query (run by CS analyst per §3):

```sql
SELECT recipient_hash, COUNT(DISTINCT survey_id) AS waves,
       COUNT(*) AS total_responses
FROM survey_responses
WHERE submitted_at_ms >= ?  -- 7d ago
GROUP BY recipient_hash
HAVING waves > 5 OR total_responses > 10
ORDER BY total_responses DESC
LIMIT 50;
```

### 2.4 Free-text payload spam / injection

**Vector:** attacker submits free-text responses containing XSS, SQL
fragments, profanity, or 2 KB of garbage to bloat the analytics
pipeline.

**Mitigation:**

- `corelink-survey::sanitize_free_text`: trims whitespace, rejects
  control chars (except `\n` / `\t`), caps at 2048 bytes UTF-8.
- The `response_value` column is rendered as JSON by the analyst
  dashboard — never as raw HTML. XSS surface is zero.
- Profanity + spam are caught by the **analyst review tier** (§3) —
  there is no automated profanity filter (false-positive risk is too
  high; CS judgement is canonical).

**Detection:** every free-text response is in the weekly analyst
queue. Responses with a Levenshtein distance < 5 to a known spam
template (operator-curated list, lives in
`marketing/retention/customer-health/spam-templates.txt`) are
auto-flagged.

### 2.5 Multi-choice ballot stuffing

**Vector:** attacker submits responses to a multi-choice survey
selecting only the option that benefits them (e.g. "what feature
should we kill next?").

**Mitigation:**

- One submission per `jti` (replay-reject covers this).
- Per-recipient cross-wave query (§2.3) catches the multi-account
  vector.
- The bounded option-index cap (≤ 16 distinct, each ≤ 255) prevents
  payload bloat from a single submission.

## 3. CS analyst weekly review protocol

Every Monday 09:00 UTC, the CS lead runs the weekly survey-abuse
review (a 30-min slot on the team calendar). The protocol:

1. Pull the abuse-flag queue (free-text spam matches + per-recipient
   anomalies + replay attempts grouped by `jti`).
2. Triage each row into one of:
   - **valid** — leave row in `survey_responses`; nothing to do.
   - **low-quality** — set a `quality_flag` in the analytics
     downstream (does NOT mutate `survey_responses`; the raw table is
     append-only / audit-canonical).
   - **abuse** — file a `corelink-abuse` block on the `ip_hash` +
     blacklist the `recipient_hash` for future wave issuance.
3. Emit a weekly report to `marketing/retention/customer-health/
   reports/YYYY-WW-survey-abuse.md` summarizing counts + actions.

The analyst MUST NOT delete rows from `survey_responses`. The table
is append-only by design (audit chain integrity + GDPR DSR-erasure
flow handles deletion via the `corelink-privacy-erasure-worker`
backend canonical to all CoreLink stored data).

## 4. Incident escalation

| Trigger                                                              | Severity | Action                                              |
| -------------------------------------------------------------------- | -------- | --------------------------------------------------- |
| `invalid_signature` rate > 10 req/s for 10 min                       | SEV-2    | Page on-call. Run §2.1 burst-guard escalation.      |
| Single `ip_hash` submits > 100 valid responses in 1h                 | SEV-3    | CS analyst reviews; likely tenant employee surveying |
| `recipient_hash` reused across > 10 waves in 7d                      | SEV-3    | Blacklist recipient; investigate issuance pipeline.  |
| HMAC key suspected leaked (e.g. flagged via `corelink-rotation-*`)   | SEV-1    | Rotate HMAC key; invalidate all outstanding tokens. |

## 5. Cross-links

- **Crate:** `crates/corelink-survey/` — token + recorder primitives.
- **Schedule:** `marketing/retention/customer-health/NPS-SURVEY-SCHEDULE.md` —
  wave cadence (sealed pre-GA).
- **Protocol:** `marketing/retention/customer-health/SURVEY-ANALYSIS-PROTOCOL.md` —
  full CS analyst workflow (this runbook is the abuse subset).
- **Migration:** `migrations/d1/0046_survey_responses.sql` — the table
  this runbook protects.
- **Adjacent runbooks:** `RB-CUSTOMER-SUPPORT-T-90.md` (CS oncall
  escalation), `ONCALL-ESCALATION-MATRIX.md` (SEV ladder).
- **Roadmap:** ROADMAP §6 (retention infra) + §8 (customer-health
  lighthouse-kit integration).
