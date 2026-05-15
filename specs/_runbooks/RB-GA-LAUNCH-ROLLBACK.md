---
id: "RB-GA-LAUNCH-ROLLBACK"
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
parent: "GA-GATE-CRITERIA"
tags: ["runbook", "p0", "ga", "launch", "rollback", "private-preview", "r7", "r8", "decision-tree", "comms"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-GA-LAUNCH-ROLLBACK — GA Launch Rollback to Private Preview

> **Severity floor:** **P0** — invoking this runbook means GA launch is being **reverted** (or threatened with revert) to a private-preview / closed-beta posture. This is the highest-stakes operational decision short of SEV-0 data loss.
> **Detect:** ≤ 5 min (auto-alerts) · **Acknowledge:** ≤ 10 min · **Engage CEO/CTO/VPSec/VPProduct:** ≤ 30 min · **Decision deadline:** ≤ 4 h from trigger detection
> **State-machine impact:** flips `public_status` from `GA` back to `PRIVATE_PREVIEW`; locks new-customer signup; preserves all existing customer infra (no service interruption); pauses marketing comms; triggers re-attestation cycle.
>
> **Companion docs.** `specs/_compliance/GA-GATE-CRITERIA.md` (the 59-criteria checklist that may need re-validation) · `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` §4 (forward-references this runbook for D+1/D+7/D+30 triggers) · `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` (paging escalation if SEV-1 concurrent).
>
> **Parent:** `GA-GATE-CRITERIA.md` waiver register §7 + `GA-GATE-GO-NOGO-TEMPLATE.md` §4 rollback trigger.

---

## 1. Trigger conditions

This runbook fires when **any** of the following are detected in the launch window:

### 1.1 D+1 triggers (T+0 .. T+24 h post-launch)

| # | Trigger | Source / metric | Auto-page? |
|---|---|---|---|
| T1-1 | **Any SEV-1 (P0) incident** in first 24 h | PagerDuty incident severity = SEV-1 | YES |
| T1-2 | **Error budget burn ≥ 10× normal for ≥ 1 h** | Grafana SLO multi-burn-rate alert | YES |
| T1-3 | **Public reputational hit** — viral negative coverage (HN front-page negative thread > 50 upvotes, OR > 100 negative-sentiment retweets/posts in 24 h tracked by social listening, OR > 3 press articles questioning launch) | Brand monitoring (Mention.com or equivalent) + manual flag | NO (manual page) |
| T1-4 | **Customer mass-churn** — ≥ 1 lighthouse customer formally withdrawing within 24 h, OR ≥ 5 paying signups churn refund-requested within 24 h | `corelink-lighthouse-tracker` state-machine: `Observing → Withdrawn`; `corelink-billing` refund webhook | YES (lighthouse) |
| T1-5 | **CRITICAL security finding** disclosed publicly during launch window (CVE assigned, public PoC) | Security advisor manual page; HackerOne triage | YES |
| T1-6 | **CRITICAL invariant violation** — any of INV-CRITICAL (10) flips `status: VIOLATED` in live production | `corelink_invariant_violation_total{severity=CRITICAL}` > 0 | YES |
| T1-7 | **Audit chain integrity break** | `corelink_audit_chain_integrity_violation_total` > 0 (INV-AUDIT-APPEND-ONLY breach) | YES |

### 1.2 D+7 triggers (T+1 d .. T+7 d)

| # | Trigger | Source / metric | Auto-page? |
|---|---|---|---|
| T7-1 | **P1 cluster** — ≥ 3 unrelated P1 incidents in 7 d | PagerDuty incident count | NO (daily review) |
| T7-2 | **SLO breach sustained > 24 h** on any GA-GATE-E04 perf budget | Grafana sustained-breach alert | YES |
| T7-3 | **> 2 lighthouse customer escalations** in 7 d (any severity above L3 routine support) | RB-LIGHTHOUSE-CUSTOMER-INCIDENT log | NO (daily review) |
| T7-4 | **Pentest finding regression** — any HIGH/CRITICAL finding from initial pentest re-opened (e.g. via dependency upgrade reintroducing CVE) | `cargo audit` weekly diff vs pentest baseline | NO (weekly review) |
| T7-5 | **DPA violation** by sub-processor (e.g. Cloudflare incident, Stripe data event affecting customer scope) | Privacy Officer manual flag | YES |
| T7-6 | **Compliance regression** — Drata SOC 2 readiness score drops ≥ 10 points in 7 d | Drata weekly digest | NO (weekly review) |

### 1.3 D+30 triggers (T+8 d .. T+30 d)

| # | Trigger | Source / metric | Auto-page? |
|---|---|---|---|
| T30-1 | **Any expired waiver still open as P0** | `GA-GATE-CRITERIA.md` §7 waiver expiry sweep cron | NO (weekly Owner review) |
| T30-2 | **Cumulative SLA credits issued > $X** (TBD per lighthouse contract; canonical floor $50k aggregate) | Billing dashboard | NO (monthly review) |
| T30-3 | **SOC 2 readiness regression** > 5 points over 30 d cumulative | Drata monthly digest | NO (monthly review) |
| T30-4 | **Customer churn rate > 5%** in 30 d (signups - cancellations among paying tier) | Stripe + `corelink-tier-selection` analytics | NO (monthly review) |
| T30-5 | **D+30 retrospective** mandatory review meeting reveals systemic gap | Quarterly OKR review | NO (scheduled) |

---

## 2. Detection + Initial response (first 30 min)

### 2.1 Detection

Auto-page triggers fire to PagerDuty SEV-1 rotation per ONCALL-ESCALATION-MATRIX.md. Manual triggers (T1-3, T7-1, T30-*) require Owner / SRE Lead / Security Lead / Privacy Officer / Compliance Officer manual paging.

### 2.2 First 5 min — Acknowledge

1. On-call SRE acks PagerDuty incident; sets severity = **P0** in incident.io / Slack `#incident-active`.
2. On-call SRE confirms **trigger ID** matches §1 table; logs to incident timeline.
3. On-call SRE pages **CEO + CTO + VPSec + VPProduct** simultaneously (PagerDuty escalation tier-3 broadcast).

### 2.3 First 15 min — Engage 4 launch signers

1. Open dedicated war-room Zoom (pre-staged URL in `marketing/launch/COORDINATION/EMERGENCY-WAR-ROOM-URL.md`).
2. Open `#ga-launch-rollback` Slack channel (pre-created at T-0; archived at T+30 d if unused).
3. CEO confirms initial triage cadence: status updates every 15 min until decision.
4. SRE Lead captures + summarizes the trigger evidence (alert URL, dashboard screenshot, incident timeline).

### 2.4 First 30 min — Convene emergency Go/No-Go

Use `GA-GATE-GO-NOGO-TEMPLATE.md` §5 emergency re-convene format (30 min compressed agenda). Decision options (next section).

---

## 3. Decision tree

### 3.1 Three decisions

| Decision | When to choose | Effect | Reversible? |
|---|---|---|---|
| **HOLD** | Trigger root-caused as transient (e.g. third-party outage recovered, one-off P0 fixed forward, vibe-check noise) AND fix confidently deployable within next 24 h | Continue GA posture; document trigger as L1 incident with remediation; do NOT roll back marketing/pricing/positioning | YES (low cost) |
| **REVERT** | Trigger root-caused as structural (e.g. invariant violation in production, sustained SLO breach, lighthouse customer withdrawal, public security finding) AND fix requires > 24 h | Roll back per §4 steps; flip `public_status` to `PRIVATE_PREVIEW`; pause new signups; preserve existing customers | YES (high cost — re-attestation cycle, public messaging hit) |
| **ESCALATE** | Trigger is **catastrophic** (data loss, breach, audit-chain integrity break, criminal investigation, lighthouse customer in regulatory crisis) | Declare SEV-0; legal counsel engaged ≤ 1 h; breach notification clock starts (72 h GDPR); public communication via PR firm + legal-vetted statement | NO (this is a permanent posture change requiring full board review) |

### 3.2 Decision matrix

| Trigger class | Default decision | Override possible? | Quorum required |
|---|---|---|---|
| T1-1..T1-2 (technical SEV-1, error budget burn) | **HOLD** (fix-forward if < 4 h to fix); **REVERT** if > 24 h | YES (CTO + SRE Lead) | 4 of 4 signers if REVERT |
| T1-3 (public reputational hit) | **HOLD** + comms response | YES (CEO override only) | CEO + VPProduct minimum |
| T1-4 (customer mass-churn) | **REVERT** | YES (CEO override only — but rarely justified) | 4 of 4 signers |
| T1-5 (CRITICAL security finding public) | **REVERT** OR **ESCALATE** | YES (VPSec lead) | 4 of 4 signers; Legal Counsel + Security Advisor consulted |
| T1-6 (CRITICAL invariant violation prod) | **ESCALATE** | NO | 4 of 4 + Legal + DPO |
| T1-7 (audit chain break) | **ESCALATE** | NO | 4 of 4 + Legal + DPO |
| T7-1..T7-3 (P1 cluster, SLO sustained breach, lighthouse escalations) | **HOLD** with remediation plan; **REVERT** if pattern indicates wider issue | YES (joint CTO + SRE Lead) | 3 of 4 minimum |
| T7-4 (pentest regression) | **HOLD** with patch deployment; **REVERT** if HIGH/CRITICAL unfixable < 7 d | YES (VPSec lead) | 3 of 4 minimum |
| T7-5 (DPA / sub-processor violation) | **ESCALATE** | NO | VPSec + Legal Counsel + DPO required |
| T7-6 (SOC 2 regression > 10) | **HOLD** + remediation roadmap | YES (Compliance Officer lead) | 2 of 4 minimum (CEO + VPSec) |
| T30-1 (expired waiver still P0) | **HOLD** with waiver extension OR feature flag revert; **REVERT** if cumulative ≥ 2 expired waivers | YES (joint CEO + CTO) | 3 of 4 minimum |
| T30-2 (SLA credits > $X) | **HOLD** + product pricing review; consider partial **REVERT** of affected feature | YES | 3 of 4 minimum |
| T30-3 (SOC 2 regression > 5) | **HOLD** + Compliance Officer remediation | YES | 2 of 4 (CEO + VPSec) |
| T30-4 (churn > 5%) | **HOLD** + Product Lead review | YES | 2 of 4 (CEO + VPProduct) |
| T30-5 (D+30 retrospective systemic gap) | **HOLD** + roadmap update; consider **REVERT** only if gap is invariant-class | YES | 4 of 4 signers if REVERT |

### 3.3 Decision logging

Each emergency decision is logged to `specs/_audits/2026-MM-DD-ga-rollback-decision-N.md` (`type: audit`) within 24 h of decision; signed-sealed by the quorum signers via DocuSign.

---

## 4. REVERT steps (full rollback to private preview)

If decision = REVERT, execute the following sequence. Target completion: **≤ 4 h** from decision time.

### 4.1 Step 1 — Lock public surface (≤ 15 min)

```bash
# 4.1.1 Flip public_status feature flag in CF KV
wrangler kv key put --namespace-id <PROD_FF_KV> "public_status" "PRIVATE_PREVIEW"

# 4.1.2 Verify the flag propagated
curl https://api.corelink.dev/__health/feature-flags | jq '.public_status'
# expected: "PRIVATE_PREVIEW"

# 4.1.3 Lock new-customer signup in admin-ui
wrangler kv key put --namespace-id <PROD_FF_KV> "new_signup_open" "false"

# 4.1.4 Update pricing page banner via CF Pages preview-promote
# (banner content lives in apps/docs/components/PrivatePreviewBanner.tsx behind FF)
pnpm --filter docs run deploy:rollback-banner
```

**Verification.** New signups to `https://corelink.dev/signup` show "Private preview — invite-only" page; existing customers see no change.

### 4.2 Step 2 — Pause marketing comms (≤ 15 min)

```bash
# 4.2.1 Pull launch blog posts from front page (keep at permalink for SEO continuity)
cd marketing/launch/COORDINATION
./scripts/unpublish-launch-posts.sh   # moves to /archive subpath

# 4.2.2 Replace launch blog with deferral statement (pre-staged at T-0)
cp marketing/launch/COMMS/T-0-DEFER-DRAFT.md apps/docs/blog/2026-MM-DD-private-preview-extension.md

# 4.2.3 Pause scheduled social posts
# Buffer / Hootsuite / native API — see marketing/launch/COORDINATION/SOCIAL-SCHEDULE.md
./scripts/pause-scheduled-social.sh

# 4.2.4 Cancel pending press release (BusinessWire)
# Manual: email PR firm or press-wire account contact ASAP
```

**Verification.** Front page no longer shows GA messaging; LinkedIn / X scheduled posts paused.

### 4.3 Step 3 — Customer + investor communication (≤ 1 h)

Run the following comms in parallel:

| Audience | Channel | Template | Owner |
|---|---|---|---|
| Lighthouse customers (3) | Phone call + signed email | `marketing/launch/COMMS/LIGHTHOUSE-ROLLBACK-OUTREACH.md` (apologetic, factual, what's-next-with-timeline) | Owner / VPProduct |
| Paying customers (early-access) | Email (Postmark / SES) | `marketing/launch/COMMS/CUSTOMER-EMAIL-ROLLBACK.md` (reassurance, no service interruption, refund offer if applicable) | VPProduct |
| Status page (public) | Statuspage.io incident | `marketing/launch/COMMS/STATUS-PAGE-ROLLBACK-INCIDENT.md` (factual, transparent about return to private preview, no fault-attribution) | SRE Lead |
| Press / public | Blog post + tweet thread | `marketing/launch/COMMS/PUBLIC-DEFERRAL-STATEMENT.md` (owns the decision, shows discipline, sets re-attestation timeline) | Owner / CEO |
| Investors / advisors | Direct DM/email | `marketing/launch/COMMS/INVESTOR-ROLLBACK-NOTE.md` (root cause + mitigation + timeline + ask) | Owner / CEO |
| Internal team | Slack `#all-hands` + standup | Live verbal + Slack write-up | Owner |

> **Sub-processor breach scope.** If trigger is T1-5 (security finding public) or T7-5 (DPA violation), the **breach notification clock starts** per `legal/breach-notification/`: 72 h GDPR (Art. 33) + LGPD ANPD notice + state-level US notice (per jurisdiction). Legal Counsel + DPO drive this in parallel; the marketing comms above must NOT preempt the legal-required notification format.

### 4.4 Step 4 — Engineering containment (≤ 2 h)

| Action | Owner | Notes |
|---|---|---|
| Root-cause analysis kickoff | SRE Lead + CTO | RB-POSTMORTEM-PROCESS started; target draft within 48 h |
| Production hotfix deployed (if applicable) | Engineer Lead | Per S-13 canary rollout 5% → 25% → 100% with auto-rollback |
| Feature-flag revert (alternative to code rollback) | Engineer Lead | If trigger is feature-specific, disable feature via FF KV instead of rolling back entire deployment |
| Customer-facing API stays UP | SRE Lead | Existing customers MUST experience zero service interruption — REVERT affects only **public posture**, not infra |
| Audit chain integrity verification | Security Lead | INV-AUDIT-APPEND-ONLY TLA+ re-run; chain head SHA recorded |
| Snapshot D1 + R2 state | SRE Lead | `wrangler d1 export` + R2 inventory CSV for forensic baseline |

### 4.5 Step 5 — Documentation + audit (≤ 24 h post-decision)

1. Seal `specs/_audits/2026-MM-DD-ga-rollback-decision-N.md` with 4-signer signatures.
2. Update `PRR-S20-GA.md` `work_status`: `APPROVED → CONDITIONALLY_APPROVED` + add waiver row referencing this rollback.
3. Update `GA-GATE-CRITERIA.md` §8 readiness summary with regressed criteria flipped to `BLOCKED`.
4. Open `gh issue` with label `ga-rollback-N` per regressed criterion + root cause.
5. Schedule re-attestation Go/No-Go meeting per §6 below.
6. Trigger postmortem doc (RB-POSTMORTEM-PROCESS) — published within 14 d as public retrospective.

---

## 5. HOLD steps (no full rollback; targeted mitigation)

If decision = HOLD, the launch posture stays `GA` but the trigger is documented + mitigated:

1. **Document the trigger** in `specs/_audits/2026-MM-DD-ga-hold-decision-N.md` (`type: audit`).
2. **Deploy mitigation** (hotfix / feature flag / rate limit increase / cache warmup) within 24 h.
3. **Customer comms scoped** to affected customers only (no public deferral statement); status page incident with `investigating → identified → monitoring → resolved` lifecycle.
4. **No GA-GATE-CRITERIA.md regression**: criteria stay `READY` but a waiver row is added in §7 with expiry ≤ 30 d.
5. **Trigger re-evaluation at next D+ window** — if the same trigger fires twice in the same window, **auto-escalate to REVERT**.

---

## 6. ESCALATE steps (catastrophic / SEV-0)

If decision = ESCALATE (T1-5, T1-6, T1-7, T7-5):

1. Declare SEV-0 in incident.io.
2. Engage **Legal Counsel within 1 h** (Cooley / DLA Piper / Bird & Bird) — breach notification clock starts.
3. Engage **external Security Advisor** (Schellman / Bishop Fox retainer) for incident response support.
4. Suspend all marketing (per §4.2) + cancel scheduled comms.
5. Public statement only after Legal Counsel review — no premature attribution.
6. Per-jurisdiction breach notification: GDPR 72 h (Art. 33), LGPD ANPD, CCPA AG, state-level US (per `legal/breach-notification/`).
7. **Lighthouse customers notified within 1 h** by phone — they're contractually entitled to early notice (lighthouse SLA §X.Y).
8. **Board / investor emergency meeting within 24 h.**
9. Schedule post-incident review (PIR) within 14 d with external auditor present.
10. `work_status` of PRR-S20-GA flips to `REJECTED`; full re-attestation cycle required before next GA attempt (re-engagement of pentest, possibly new SOC 2 audit).

---

## 7. Re-attestation criteria (next GA attempt)

Before scheduling the next Go/No-Go meeting per `GA-GATE-GO-NOGO-TEMPLATE.md`, the following must be true:

| # | Re-attestation criterion | Owner | Evidence |
|---|---|---|---|
| RA-1 | **Root cause of rollback trigger documented + closed** with code fix / process fix / contract fix | SRE Lead + relevant track owner | Postmortem doc sealed + linked PR merged |
| RA-2 | **All BLOCKED GA-GATE criteria flipped back to READY** | per-track owner | GA-GATE-CRITERIA.md §8 sweep |
| RA-3 | **30 d new sustained-staging window** if trigger involved infra/SLO regression (resets GA-GATE-O01) | SRE Lead | new daily-staging-evidence series |
| RA-4 | **Pentest re-engagement** if trigger involved security regression (re-runs GA-GATE-S01) | Security Lead | new pentest letter |
| RA-5 | **Lighthouse customer re-confirmation** — explicit re-affirmation of attestation by lighthouse customers (they have right to withdraw post-rollback) | Product Lead | new signed attestation docs |
| RA-6 | **Public comms reset** — new launch messaging that acknowledges (or transparently does not mention) the rollback per CEO judgment | Owner | new draft blog + press kit |
| RA-7 | **Waiver register cleaned** — all expired waivers from prior GA attempt resolved | Owner | GA-GATE-CRITERIA.md §7 sweep |
| RA-8 | **Re-execution of GA-GATE-GO-NOGO meeting** with quorum + recording + sealed audit | Owner | `2026-MM-DD-ga-go-nogo-attempt-{N+1}.md` |

**Minimum gap between attempts.** No re-attempt within **14 d** of a REVERT decision; no re-attempt within **90 d** of an ESCALATE decision. This is non-negotiable per ADR-0034 + GA-GATE-CRITERIA.md §9.

---

## 8. Verification + drill cadence

This runbook is **non-DR-drilled** in normal cadence (you cannot rehearse a real rollback without disrupting customers). However:

- **Tabletop exercise** quarterly: SRE Lead + Owner walk through §3 decision tree against a hypothetical trigger; document in `specs/_audits/2026-QQ-ga-rollback-tabletop.md`.
- **Comms templates dry-run**: every 6 months, the 6 comms templates in §4.3 are smoke-tested (rendered, reviewed, but NOT sent) — sealed audit doc per dry-run.
- **CF KV flag flip dry-run**: every 6 months, the `public_status` flip is rehearsed on a **staging KV namespace** (never on prod KV) — verified that admin-ui + apps/docs respond correctly.
- **Pre-GA dress rehearsal**: ≤ T-7 d before the GA Go/No-Go meeting, a full tabletop walkthrough of this runbook with all 4 signers + SRE Lead + Legal Counsel.

---

## 9. References

- `specs/_compliance/GA-GATE-CRITERIA.md` — criteria that get re-validated in re-attestation cycle.
- `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` §4 + §5 — forward-refs this runbook; §5 is the emergency Go/No-Go format used here.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — SEV-1 paging escalation tiers.
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — postmortem template kicked off in §4.5.
- `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` — lighthouse-specific incident response (consumed in §3 T1-4 + T7-3).
- `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md` — pentest finding handling (consumed in §3 T1-5 + T7-4).
- `legal/breach-notification/` — breach notification templates (consumed in §6 ESCALATE).
- `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md` — sister runbook for T-7..T+7 launch sequence.
- `marketing/launch/COMMS/` — pre-staged comms templates per §4.3.
- `ROADMAP-TO-GA.md` §8 — Wave R-8 launch context.
- ADR-0034 — solo-tier emergency provisions.

---

## 10. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Sonnet R-7 builder, worktree `wt/r7-2-ga-gate`) | Initial GA launch rollback runbook — D+1/D+7/D+30 trigger conditions (18 triggers across 3 windows); 3-decision tree (HOLD / REVERT / ESCALATE) with per-trigger default + quorum; full REVERT step sequence (5 steps; ≤ 4 h target); customer + investor + press comms templates referenced; SEV-0 ESCALATE path with breach notification 72 h GDPR; re-attestation criteria (8 items) with minimum 14 d / 90 d gap; tabletop + dry-run cadence. |

---

**Status:** ACTIVE. Tabletop quarterly; dress rehearsal ≤ T-7 d pre-GA per §8.
