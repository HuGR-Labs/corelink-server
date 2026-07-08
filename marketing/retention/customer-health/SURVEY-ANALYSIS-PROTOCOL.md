# SURVEY-ANALYSIS-PROTOCOL — CS team weekly review of NPS / CSAT / free-text

> **Status:** ACTIVE. Owned by the CS lead. Pairs with the
> `corelink-survey` crate (token + recorder primitives), the
> `RB-SURVEY-ABUSE` runbook (specs/_runbooks), and the wave schedule
> in NPS-SURVEY-SCHEDULE.md.

## 0. TL;DR

Every Monday 09:00 UTC, the CS lead runs a 90-minute protocol over the
prior week's survey responses:

1. **Aggregate**: pull NPS / CSAT distributions per tenant tier + per
   product surface (chunking / dedup / billing / BYOK / DSR).
2. **Trend**: compare against the prior 4-week rolling mean. Anything
   ≥ 1σ drift is annotated.
3. **Extract**: top 5 free-text themes (manual coding; sample size
   keeps this tractable through the GA → 100-customer window).
4. **Generate**: 3-5 action items per drift signal, filed as GitHub
   issues against the owning surface team.
5. **Report**: weekly report at
   `marketing/retention/customer-health/reports/YYYY-WW-survey-review.md`.

This protocol is the **only** canonical way the CS team reads survey
data pre-GA. Ad-hoc D1 queries are fine for spot-checks but cannot
drive product decisions without the weekly aggregation context.

## 1. Aggregation queries

All queries assume `submitted_at_ms` is in the prior 7-day window
(`now - 7d <= submitted_at_ms < now`).

### 1.1 NPS distribution by tenant tier

```sql
SELECT
    t.tier,
    COUNT(*) AS responses,
    AVG(CAST(JSON_EXTRACT(sr.response_value, '$.score') AS INTEGER)) AS avg_score,
    SUM(CASE WHEN CAST(JSON_EXTRACT(sr.response_value, '$.score') AS INTEGER) >= 9 THEN 1 ELSE 0 END) AS promoters,
    SUM(CASE WHEN CAST(JSON_EXTRACT(sr.response_value, '$.score') AS INTEGER) <= 6 THEN 1 ELSE 0 END) AS detractors
FROM survey_responses sr
JOIN tenants t ON t.id = sr.tenant_id
WHERE sr.response_kind = 'nps'
  AND sr.submitted_at_ms >= ?
  AND sr.submitted_at_ms <  ?
GROUP BY t.tier
ORDER BY t.tier;
```

NPS = (promoters - detractors) / responses × 100. Compute per tier.
Pre-GA target: NPS ≥ +40 across all tiers. NPS < 0 in any tier pages
the CS lead as a SEV-3 (this is a roadmap-rewriting signal, not an
on-call page).

### 1.2 CSAT distribution by product surface

CSAT surveys are tagged with `survey_id` like
`csat-<surface>-<wave>` (e.g. `csat-byok-onboarding-2026q2`). The
slug carries the surface so the analyst can pivot without a separate
join:

```sql
SELECT
    SUBSTR(sr.survey_id, INSTR(sr.survey_id, '-') + 1,
           INSTR(SUBSTR(sr.survey_id, INSTR(sr.survey_id, '-') + 1), '-') - 1)
        AS surface,
    AVG(CAST(JSON_EXTRACT(sr.response_value, '$.score') AS REAL)) AS avg_csat,
    COUNT(*) AS responses
FROM survey_responses sr
WHERE sr.response_kind = 'csat'
  AND sr.submitted_at_ms >= ?
GROUP BY surface
HAVING responses >= 5
ORDER BY avg_csat ASC;
```

Pre-GA target: CSAT ≥ 4.0/5 on every surface with ≥ 5 responses.
Surface < 3.5 escalates to a roadmap review in the same week.

### 1.3 Free-text top themes

Free-text is small enough (pre-GA target: ≤ 200 free-text responses
per week) to **manually code** in ~30 min:

1. Export the week's free-text rows:
   ```sql
   SELECT submitted_at_ms,
          JSON_EXTRACT(response_value, '$.text') AS text,
          survey_id
   FROM survey_responses
   WHERE response_kind = 'free_text'
     AND submitted_at_ms >= ?
   ORDER BY submitted_at_ms DESC;
   ```
2. Tag each row with 1-3 theme labels from the canonical taxonomy:
   - `feature-request` — explicit ask for a feature CoreLink doesn't ship.
   - `bug-report` — reported behavior the analyst can reproduce.
   - `praise` — positive sentiment, no actionable ask.
   - `friction` — workflow / docs / pricing pain.
   - `competitor-mention` — name-drop of a competitor product.
   - `pricing-feedback` — comment on tier limits or cost.
3. Group + count themes. Top 5 by frequency feed §4 action items.

Themes are stored in the **report file**, not back into the
`survey_responses` table (the table is append-only and audit-canonical).

### 1.4 Multi-choice cross-tabs

For multi-choice surveys (e.g. "which 2-3 features matter most for
your team?"), generate the option-frequency histogram + a co-occurrence
matrix. The CS lead's spreadsheet template lives at
`marketing/retention/customer-health/templates/multi-choice-cross-tab.xlsx`.

## 2. Trend extraction

Compare the current week's aggregates against the 4-week rolling
mean for the same metric:

- |z-score| ≥ 1.0 — note it in the report under "drift watch".
- |z-score| ≥ 2.0 — escalate to a weekly product-review item; the
  surface team owner has 7 days to file a remediation issue or
  acknowledge the signal as noise.

The CS lead maintains a sparkline grid in
`marketing/retention/customer-health/dashboards/sparklines.md`
updated each week (manual paste of the SVG snippets from the
analyst's Excel).

## 3. Per-tenant cohort cuts

For lighthouse customers (the 3 named in `corelink-lighthouse-tracker`),
the analyst additionally pulls a **per-tenant** view every week:

```sql
SELECT sr.tenant_id, sr.survey_id, sr.response_kind,
       sr.response_value, sr.submitted_at_ms
FROM survey_responses sr
WHERE sr.tenant_id IN (?, ?, ?)
  AND sr.submitted_at_ms >= ?
ORDER BY sr.submitted_at_ms DESC;
```

A lighthouse with NPS dropping below +30 in 2 consecutive weeks
triggers a "save call" — the CS lead schedules a 30-min check-in
with the customer within 48h.

## 4. Action item generation

Each "drift watch" + each top-5 free-text theme yields 3-5 action
items, filed as GitHub issues under the appropriate org repo:

- **Engineering** issues: `HumanGuardrail/corelink-server` with label
  `customer-feedback`.
- **Docs** issues: `HumanGuardrail/corelink-docs` with label
  `cs-week-<YYYY-WW>`.
- **Marketing / positioning** issues: `HumanGuardrail/corelink-website`
  with label `cs-feedback`.

Each issue carries the source rows (with tenant_id redacted to
`recipient_hash` for privacy) so the engineering / docs team can
trace back without re-querying the analyst.

## 5. Weekly report template

The weekly report at
`marketing/retention/customer-health/reports/YYYY-WW-survey-review.md`
follows this shape (mirrors the SCSV — Customer Success Standard
Vocabulary the CS lead documents in the lighthouse-kit):

```markdown
# CS week YYYY-WW survey review

## Aggregates
- NPS (Solo / Team / Business / Enterprise): _ / _ / _ / _
- CSAT by surface: chunking _, dedup _, billing _, byok _, dsr _
- Response volume: _ NPS, _ CSAT, _ free-text, _ multi-choice

## Drift watch
- (list of metrics with |z| ≥ 1)

## Top 5 free-text themes
1. (theme) — N occurrences — sample quote
2. ...

## Lighthouse pulse
- (per-lighthouse one-liner)

## Action items
- [ ] (issue link) — owner — due
- ...
```

## 6. Privacy + audit posture

- Reports MUST NOT include raw recipient identifiers. The CS team
  works only with `recipient_hash` (64-char hex).
- Any export beyond the team boundary (e.g. board deck, investor
  update) goes through the redaction lint at
  `tools/audit_pii_lint/` (deferred — runs against report files
  pre-publish once it lands).
- Every aggregation query is audit-logged via the same outbox the
  recorder uses — the analyst's read path appears in the audit chain
  alongside the write path.

## 7. Cross-links

- **Schedule:** `NPS-SURVEY-SCHEDULE.md` — when each wave goes out.
- **Abuse runbook:** `specs/_runbooks/RB-SURVEY-ABUSE.md`.
- **Landing page:** `apps/docs/docs/how-to/survey-response.mdx` —
  what the recipient sees on click-through.
- **Crate:** `crates/corelink-survey/` — token + recorder primitives.
- **Lighthouse:** `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` —
  customer-success cadence the per-tenant cuts feed into.
- **Roadmap:** ROADMAP §6 (retention infra) + §8 (lighthouse-kit
  customer-health integration).
