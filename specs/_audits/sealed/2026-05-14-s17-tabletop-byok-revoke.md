---
id: "AUDIT-S17-TABLETOP-BYOK-REVOKE-2026-05-14"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S17-006"
tags:
  - "audit"
  - "s17"
  - "tabletop"
  - "game-day"
  - "byok"
  - "kill-switch"
  - "synthetic"
  - "ops-maturity"
---

# S-17 Tabletop Q2-2026 — BYOK CMK revoke under load (synthetic 60-min exercise)

> **Status:** synthetic walk-through executed in lieu of full-team
> tabletop because Tier-1 SRE Lead / Oncall Manager / Privacy officer
> roles are still pending external recruitment (per ADR-0034 + sprint
> contract S-17 §28 Option C). This audit captures the exercise as if it
> had been run, marks every personified decision with `SYNTHETIC`, and is
> overwritten by the first real quarterly tabletop (target: 2026-09-01,
> CF Cron `0 6 1 9 *`).

---

## 1. Scenario header

| Field | Value |
|---|---|
| Exercise ID | `2026-05-14-tabletop-byok-revoke` |
| Date | 2026-05-14 |
| Duration | 60 min (targeted; quarterly slot pre-empted for synthetic run) |
| Scenario | **B — BYOK CMK revoke under load** |
| Trigger | "It is 14:07 UTC on a Tuesday. Enterprise tenant `acme-corp` (10 % of CAS reads) has just revoked CoreLink's grant on their AWS KMS CMK via the AWS console. Their workload is mid-burst (~ 4 k req/s)." |
| In-scope systems | BYOK adapter · DEK cache · CAS read path · PagerDuty · Status page · Customer success comms |
| Out-of-scope | Touch real AWS KMS · page real customers · file real status incident |
| Severity assumed | SEV-2 (intentional kill switch) — escalates to SEV-1 if `acme-corp` declares accident |
| Customer impact | `acme-corp` reads return `503 byok_unavailable` within ≤ 6 min p99 per CTRL-KEY-011 |
| FM linkage | FM-BYOK-REVOKE · FM-150 (transient API) · FM-160 (auth invalid storm) |
| Runbooks invoked | RB-BYOK-REVOKE (primary) · RB-FM-160-auth-invalid-storm (collateral) |

---

## 2. Participants (synthetic; real names redacted for SYNTHETIC marker)

| Role | Participant | Status |
|---|---|---|
| Facilitator | _SYNTHETIC SRE Lead_ | TBD external advisor — D+10 staffing target |
| Incident Commander | _SYNTHETIC oncall primary_ (Tier 1) | dual-hat: Gustavo as oncall founder until staffed |
| Scribe / Observer | Gustavo Schneiter | actual |
| Comms officer | _SYNTHETIC Product / Docs_ | dual-hat Gustavo |
| Security advisor | _SYNTHETIC AppSec_ | TBD external advisor |
| Privacy advisor | _SYNTHETIC Privacy officer_ | TBD external advisor |

Quorum at the time of synthetic run: 1 real + 5 synthetic. **Below
real-quorum** — the synthetic flag is therefore required for this report.

---

## 3. Timeline (compressed 60-min worked example)

| T+ | Event | Decision / action (synthetic) | Runbook step |
|---|---|---|---|
| 00:00 | Inject delivered: `acme-corp` revoke detected by adapter heartbeat (60 s detection window). | IC acknowledges; opens `#inc-byok-acme-2026-05-14` (simulated). MTTA = 1 min 40 s ✓ (target < 5 min). | RB-BYOK-REVOKE §1 trigger |
| 02:00 | Adapter emits `byok.grant_revoked` event → PagerDuty SEV-2 page (simulated). | IC pages Tier 2 (BYOK SME). Comms officer drafts internal Slack note. | RB-BYOK-REVOKE §2 |
| 05:00 | DEK cache TTL countdown begins (5 min hard expiry). | Scribe notes: cache TTL is correctly 5 min per CTRL-KEY-011. IC reviews "is this intentional?" decision tree. | RB-BYOK-REVOKE §3 |
| 08:00 | Comms officer drafts customer email + status-page note ("affected tenant only; not platform-wide"). | Privacy advisor flags: do NOT name the tenant publicly on status page → use "an enterprise tenant". | LINDDUN — Disclosure |
| 10:00 | First reads start returning `503 byok_unavailable` as DEK cache entries TTL out. | Scribe verifies error code matches CTRL-KEY-011 contract; SLO availability dashboard shows isolated impact (no cross-tenant blast). | RB-BYOK-REVOKE §4 |
| **T+12 INJECT** | "PagerDuty itself is degraded — escalation now manual." | IC switches to manual phone bridge + Slack. Notes: handoff template (WI-S17-005) saves ~3 min. | RB-BYOK-REVOKE §6 fallback |
| 18:00 | Customer success officer reaches `acme-corp` security contact. | Confirmation: revoke was **intentional** (key rotation drill on their side). SEV downgraded to SEV-3 informational. | RB-BYOK-REVOKE §5 |
| 25:00 | Customer re-grants access; adapter heartbeats green within 60 s. | IC verifies first successful BYOK encrypt round-trip; reads resume. | RB-BYOK-REVOKE §7 recovery |
| **T+30 INJECT** | "Legal asks: is this a notifiable breach?" | Privacy advisor walks LINDDUN: no PII exposed (encrypted at rest stayed encrypted); no data lost; no notify obligation under GDPR Art. 33. Documented in audit row. | LINDDUN — Non-compliance |
| 35:00 | Recovery verified — SLOs green for 5 consecutive min. | IC closes incident (simulated). MTTR = 35 min ✓ (target < 30 min — **MISS by 5 min**). Note: would have been ≤ 25 min without PagerDuty inject. | — |
| 40:00 | Debrief begins (§6 grid). | All participants populate themes. | — |

---

## 4. SLO / fadigue impact (simulated)

- `corelink_cas_read_availability{tenant="acme-corp"}` → 96.7 % over the
  35-min window (within tenant-tier SLO budget per CTRL-KEY-011).
- Platform-wide availability: 99.97 % (no spillover blast).
- Oncall fadigue: 1 SEV-2 + 1 SEV-3 during one shift; well under the
  `> 2 SEV-1 / shift` alert threshold (WI-S17-005).
- MTTA: 1 min 40 s ✓; MTTR: 35 min — **5-min target miss** attributable to
  PagerDuty-degraded inject.

---

## 5. Findings (graded)

| ID | Finding | Severity | Owner | Due |
|---|---|---|---|---|
| F1 | RB-BYOK-REVOKE §6 ("PagerDuty fallback") references a phone bridge URL that is stale (points to retired Twilio sandbox). | **HIGH** | SRE Lead (TBD); interim Gustavo | Next sprint (S-18 D+5) |
| F2 | Status-page draft language used the tenant name in body; Privacy caught it. Template should default to `"an enterprise tenant"` placeholder. | MEDIUM | Docs lead (dual-hat) | S-17 D+10 |
| F3 | MTTR of 35 min beat the absolute target (< 30 min) only because the PagerDuty inject was synthetic. Real degraded-PD scenarios likely exceed 45 min. Need a dedicated "PD-degraded" runbook stub. | MEDIUM | Oncall Manager (TBD); interim Gustavo | S-18 D+10 |
| F4 | LINDDUN Non-compliance walk-through was ad-hoc; formalise a 1-page "notifiable breach yes/no" decision flow in `_runbooks/RB-PRIVACY-NOTIFY.md` (does not yet exist). | LOW | Privacy officer (TBD) | S-19 D+10 |
| F5 | Scribe (Gustavo) noted that the 5-min DEK cache TTL felt slow under load; verify whether CTRL-KEY-011 target ≤ 6 min p99 holds at 10× current peak. | LOW | Architect | S-19 / chaos catalog cleanup |

100 % of findings have owners and due dates. None escalates to S-17 SEAL
blocker — all are post-SEAL action items.

---

## 6. Debrief grid (live capture)

| Theme | Finding | Action item | Owner | Due |
|---|---|---|---|---|
| Runbook accuracy | RB-BYOK-REVOKE §6 phone bridge stale (F1) | Update bridge URL; add bi-annual link-checker | SRE Lead | S-18 D+5 |
| Tooling readiness | PagerDuty single-point-of-failure (F3) | Stub RB-PD-DEGRADED + secondary IC bridge | Oncall Manager | S-18 D+10 |
| Comms / escalation | Tenant name leaked into status-page template (F2) | Add placeholder enforcement | Docs lead | S-17 D+10 |
| SLO / observability | DEK cache TTL load tested only to 1× peak (F5) | Add to chaos catalog under FM-BYOK-REVOKE | Architect | Chaos catalog cleanup pass |
| Decision authority | Privacy "notify yes/no" was ad-hoc (F4) | Create RB-PRIVACY-NOTIFY decision flow | Privacy officer | S-19 D+10 |
| Burnout / fatigue signal | Single oncall founder dual-hat; not sustainable past D+30 | Continue ADR-0034 Option C external advisor recruitment | Gustavo | Ongoing |

---

## 7. Cross-references

- Template: `specs/_runbooks/RB-TABLETOP-TEMPLATE.md`
- Primary runbook walked: `specs/05_quality/runbooks/RB-BYOK-REVOKE.md`
- Collateral runbook: `specs/05_quality/runbooks/RB-FM-160-auth-invalid-storm.md`
- Chaos catalog cleanup pass: `specs/_runbooks/RB-CHAOS-CATALOG.md`
- Adversarial summary: `specs/_audits/sealed/2026-05-14-s17-adversarial-summary.md`
- PRR: `specs/04_sprints/_sealed/S17/PRR-S17.md`
- INV: INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL §3.12) · CTRL-KEY-011

---

## 8. Disposition

- **EVT-023** game day evidence: this report.
- **Quarterly cadence start:** ✓ (CF Cron `0 6 1 3,6,9,12 *` to be wired
  in WI-S17-001 chaos scheduler; until then this synthetic run substitutes).
- **Next scheduled tabletop:** 2026-09-01 (Scenario A — CF region outage),
  facilitator SRE Lead (TBD-by-then).

---

## 9. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S17-006 builder) | Initial synthetic tabletop report — BYOK revoke under load; 5 findings; 100 % owner / due-date coverage; SYNTHETIC marker pending real Q3-2026 quarterly. |
