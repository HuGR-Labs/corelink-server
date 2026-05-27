---
id: "BETA-FEEDBACK-TRIAGE"
type: "process"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["beta", "pilot", "feedback", "triage", "release-prep", "wave-23"]
---

# Beta Feedback Triage Harness

**Purpose.** Capture, classify, route, and close pilot-tenant findings during
the CoreLink beta window with a deterministic SLA and a single source of
truth. Outputs feed the engineering wave backlog via
`scripts/beta-feedback-ingest.py`.

**Scope.** Applies to every signal arriving from a pilot tenant — Slack
report, email, web form, in-product feedback widget, support ticket,
incident page — between **first invite-only pilot ship** and **first GA
release tag**. After GA the same rubric applies but reports flow through the
standard support pipeline instead of the beta backlog.

**Non-goals.** This harness does **not** replace the incident-management
path (SEV-1/SEV-2 page first, triage second), the security disclosure
process (`docs/internal/secrets-checklist.md` + ISO27001 incident response),
nor the privacy-incident pipeline (LGPD/GDPR notice templates).

---

## §1. Intake form — canonical fields

Every report MUST land in NDJSON / form-submission shape with the following
fields. Missing fields block ingest (`beta-feedback-ingest.py` will reject
the line and emit a non-zero exit row in the summary).

| Field             | Type     | Required | Example                                     | Notes                                                                |
| ----------------- | -------- | -------- | ------------------------------------------- | -------------------------------------------------------------------- |
| `id`              | string   | yes      | `BFB-2026-05-16-0001`                       | `BFB-YYYY-MM-DD-NNNN` monotonically increasing per day               |
| `received_at`     | RFC3339  | yes      | `2026-05-16T14:23:11Z`                      | UTC; clock of the ingestion edge                                     |
| `tenant_id`       | string   | yes      | `t_pilot_acme`                              | Stable tenant slug; never the human company name in the open record  |
| `reporter`        | string   | yes      | `jane.doe@acme.example`                     | Pilot-side reporter; redacted to `<domain>` in public summaries      |
| `severity`        | enum     | yes      | `P0` \| `P1` \| `P2` \| `P3`                | Triager-assigned; see §3 rubric                                      |
| `surface_area`    | enum     | yes      | `api` \| `cli` \| `dashboard` \| `webhook` \| `auth` \| `billing` \| `audit` \| `docs` \| `infra` \| `other` | Routes via §4                  |
| `title`           | string   | yes      | `503 on bulk import after 10k rows`         | One-line headline                                                    |
| `repro_steps`     | string   | yes      | `1. POST /v1/imports …\n2. wait 30s …`      | Numbered; MUST be reproducible by an engineer with pilot credentials |
| `expected`        | string   | yes      | `200 with import_id; rows queued`           | Expected behavior                                                    |
| `actual`          | string   | yes      | `503 with body {"error":"timeout"}`         | Actual behavior                                                      |
| `attachment_url`  | string   | no       | `s3://hugr-pilot-evidence/2026-05-16/...`   | Screenshot / HAR / log; URL not raw content                          |
| `tenant_impact`   | enum     | yes      | `full_outage` \| `degraded` \| `workable` \| `cosmetic` | Pilot-reported magnitude; feeds §3 severity check          |
| `wave_target`     | string   | no       | `wave-24`                                   | Triager may pre-bind; else dispatcher assigns                        |
| `notes`           | string   | no       | freeform                                    | Triager scratch space; not surfaced in customer-visible summaries    |

**Schema location.** The canonical NDJSON shape is the union of the columns
above. `scripts/beta-feedback-ingest.py --schema` prints the JSON Schema
representation.

---

## §2. Triage SLA

The clock starts at `received_at` (intake field). "Ack" means a triager has
opened the report, assigned a severity, and posted an acknowledgement to the
pilot's shared Slack channel (or replied to the email). "Resolve" means the
issue is closed per §5.

| Severity | Ack          | Resolve      | Pager?              | Customer comms cadence                |
| -------- | ------------ | ------------ | ------------------- | ------------------------------------- |
| **P0**   | **≤ 4 h**    | **≤ 24 h**   | PagerDuty SEV-1     | Hourly status until resolve           |
| **P1**   | **≤ 24 h**   | **≤ 7 d**    | PagerDuty SEV-2     | Daily standup update                  |
| **P2**   | **≤ 7 d**    | **≤ 30 d**   | none                | Weekly Friday digest                  |
| **P3**   | **≤ 30 d**   | next quarter | none                | Monthly retrospective only            |

**Holiday / weekend handling.** P0 SLA does **not** stop. P1+ SLA pauses
between Saturday 00:00 UTC and Sunday 23:59 UTC and on company-observed
holidays (`docs/internal/oncall-24-7-readiness.md` calendar). P0 ack times
out of band on weekends ring the secondary on-call (`SLO p99: 4h end-to-end
ack including escalation`).

**SLA breach.** Any SLA breach auto-generates a row in the Friday weekly
summary (§6) and a `sla_breach=true` flag in the wave backlog dispatch.

---

## §3. Classification rubric

A triager picks the **highest applicable** row. When in doubt, escalate one
level (`P2 → P1`, never the other direction).

### P0 — drop everything

- Data loss observable to any pilot tenant (rows missing, durable writes
  rejected, audit chain truncated).
- Data exposure across tenant boundary (cross-tenant read / write, RLS
  bypass, BYOK plaintext leak, audit-chain replay across tenants).
- Full pilot outage: HTTP 5xx error budget > 50 % for any 5-minute window
  on the affected pilot's primary surface (`api` or `dashboard`).
- Security breach: any signal that aligns with the security-incident
  playbook (`docs/internal/auth-event-taxonomy.md` SEV-1 nodes).
- Billing miscalculation that overcharges or undercharges any pilot
  tenant by ≥ 10 % of expected.
- Any signal flagged by the pilot themselves as P0 — accept the
  classification, downgrade later if evidence warrants.

### P1 — significant impairment

- SLO breach: documented SLO target (`specs/slo-targets.md` if present;
  else `docs/internal/PERFORMANCE-PLAYBOOK.md`) missed for ≥ 30 minutes.
- One pilot's primary workflow broken end-to-end but with a documented
  workaround (workaround MUST be communicated in ack).
- Any auth, billing, audit, or webhook failure that does not meet P0 but
  has measurable customer impact (e.g. 1 % of requests failing on a
  non-critical surface).
- BYOK / KMS misbehaviour that is not yet a leak but blocks key rotation
  or revoke flow.

### P2 — annoying but workable

- UX papercut with a clean workaround (wrong copy, slow but not timing
  out, dashboard layout glitch on supported browsers).
- Documentation error that misleads but does not cause production
  damage.
- Lints, warnings, or low-volume background errors that the on-call sees
  but pilots don't.

### P3 — cosmetic

- Typo, spacing, color, icon, copy polish.
- "Nice to have" feature ask the pilot is volunteering rather than
  blocked on.

**Rubric edge cases.**

- If `tenant_impact = full_outage` but the only affected tenant is a
  non-pilot (e.g. internal test tenant), the floor is **P1**, not P0.
- If the report is a **regression** of previously closed P0/P1 work, the
  floor is the original severity (it doesn't downgrade just because the
  blast radius is smaller this time).
- If the report straddles two surfaces (e.g. audit-chain + billing), the
  triager assigns both surfaces (comma-separated) and routes to the
  higher-severity owner per §4.

---

## §4. Routing matrix

| `surface_area` | Primary owner       | Secondary (escalation) | Backlog label             |
| -------------- | ------------------- | ---------------------- | ------------------------- |
| `api`          | API platform        | TechLead               | `area/api`                |
| `cli`          | CLI / DX            | API platform           | `area/cli`                |
| `dashboard`    | Admin Plane / UX    | TechLead               | `area/dashboard`          |
| `webhook`      | Webhooks & DLQ      | API platform           | `area/webhook`            |
| `auth`         | Identity & AuthZ    | TechLead (Security)    | `area/auth`               |
| `billing`      | Billing / Stripe    | TechLead               | `area/billing`            |
| `audit`        | Audit chain & Neon  | TechLead (Security)    | `area/audit`              |
| `docs`         | DocsOps             | Author of the doc      | `area/docs`               |
| `infra`        | Platform / SRE      | TechLead               | `area/infra`              |
| `other`        | TechLead (triage)   | —                      | `area/triage`             |

**Primary owner** acks within SLA (§2). **Secondary** is the escalation
target if the primary does not ack within half of the SLA window
(P0 → 2 h, P1 → 12 h, P2 → 3.5 d, P3 → 15 d). Escalation MUST be auto-paged
for P0/P1; manual for P2/P3.

**Wave assignment.** Severity floor maps to the dispatch wave:

| Severity | Earliest wave    |
| -------- | ---------------- |
| P0       | **current wave** (preempt) |
| P1       | next wave        |
| P2       | wave + 2         |
| P3       | wave + 3 or pre-GA hardening backlog |

---

## §5. Closure protocol

A report is **closed** only when **all five** of the following hold:

1. **Root cause documented.** A single sentence root-cause string is
   stored in the closure record (not "fixed a thing"). The root-cause
   string MUST identify the failing layer (e.g.
   `corelink-audit-chain::InMemoryNeonShadowSink lost rows when the
   sink Ok-but-no-write race fired`).
2. **Fix commit referenced.** The closure record carries the SHA of the
   merge commit (or commits) that resolved the issue. For
   non-code closures (doc fixes, infra config), the commit MAY be in a
   sibling repo but MUST be a permalink.
3. **Test / detection added.** Either (a) a regression test that would
   have failed pre-fix, (b) an SLO alert that would have detected the
   issue inside the next ack-SLA window, or (c) a documented waiver in
   `docs/internal/debt-register.md` (or sprint equivalent) with an
   expiry date.
4. **Pilot verifies.** The reporter — or another contact at the pilot
   tenant — confirms the fix in their environment in writing
   (Slack / email screenshot pasted into the closure record). For P0/P1
   pilot-verify is **mandatory**; for P2/P3 pilot-verify MAY be waived
   if the fix is internally observable and the pilot is unresponsive
   for > 7 days.
5. **Wave backlog row removed.** The corresponding `wave-N` backlog
   entry is checked off; the closure timestamp is recorded for the §6
   aggregation.

**Reopens.** If a pilot reports the same issue again within 30 days of
closure, the original row reopens at **one severity higher** (P3 → P2,
P2 → P1, P1 → P0; P0 already at top) and the closure is recorded as
`closure_rejected`.

---

## §6. Aggregation cadence

### Weekly summary (Friday review)

Every Friday by **17:00 UTC**, `scripts/beta-feedback-ingest.py --summary
--week` emits a markdown digest under
`specs/_audits/<YYYY-MM-DD>-beta-feedback-weekly.md` capturing:

- Open + closed counts by severity (this week + delta vs last week).
- Median + p95 ack time and resolve time by severity.
- SLA breaches with named report IDs.
- Wave dispatch summary (which reports landed in which wave).
- Top 3 surfaces by report count.
- Carryovers (P1+ open > 1 SLA window).

The digest is the input to the **Friday review meeting**; meeting notes
live as an audit doc under
`specs/_audits/<YYYY-MM-DD>-beta-feedback-friday-review.md`.

### Monthly retrospective

On the first Monday of every month, the team holds a 30-minute
retrospective on the previous month's beta feedback. Inputs:

- Concatenation of the four Friday digests.
- Trend lines (severity mix, surface mix, SLA-breach rate).
- Pilot satisfaction signals (NPS, churn risk per pilot).

Output: a single audit doc
`specs/_audits/<YYYY-MM-DD>-beta-feedback-monthly-retro.md` with action
items, each scoped to a future wave.

---

## Cross-links

- `scripts/beta-feedback-ingest.py` — CLI tooling that consumes NDJSON
  intake records, emits the markdown summary by severity, and assigns
  rows to the wave-N backlog.
- `tests/beta_feedback_triage_test.py` — rubric / SLA / routing tests.
- `specs/_audits/sealed/2026-05-16-beta-feedback-triage-harness.md` — audit doc
  for this harness and first-week post-pilot-launch process.
- `docs/internal/oncall-24-7-readiness.md` — on-call calendar referenced
  by SLA holiday handling.
- `docs/internal/PERFORMANCE-PLAYBOOK.md` — SLO baselines referenced by
  the P1 rubric.
- `docs/internal/auth-event-taxonomy.md` — security-incident SEV nodes
  referenced by the P0 rubric.
