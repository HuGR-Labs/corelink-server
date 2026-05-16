# CoreLink Customer Success Playbook (Pilot Phase)

> **Audience:** CS team, on-call SRE rotation, product, and the
> founding team during the wave-23+ pilot window.
>
> **Status:** living document. Edit freely; PRs welcome.
> Updates DO NOT require spec validation (this is `docs/internal/`,
> not `specs/`).
>
> **Companion docs:**
> - `docs/internal/pilot-comms-templates.md` — canned emails per
>   touchpoint.
> - `docs/internal/pilot-dashboard-checklist.md` — the 10 metrics
>   our pilot dashboard MUST surface.
> - `docs/internal/admin-plane.md` — tenant lifecycle reference
>   (signup, suspend, offboard).
> - `docs/internal/legal-review-process.md` — NDA / DPA / BAA flow.
>
> **Rule of thumb:** a pilot is not a product trial. It is a
> *bilateral* commitment — we promise to make their first 30 days
> succeed; they promise to give us a post-pilot retro. Treat every
> pilot like a P1 outage waiting to happen, because that is the
> failure mode that matters most at GA.

---

## §1 Pilot Tenant Target Profile

### Customer-zero: Forge (HuGR internal)

Forge is the canonical first customer. It is not technically a
"pilot" in the same legal sense as an external tenant — it is the
internal dogfooding workload that drives our P0 priorities. Treat
Forge's onboarding as a forcing function: every friction point Forge
hits is a friction point the next 10 pilots will hit silently before
they churn. Forge's tenant ID is reserved at signup and is exempt
from the 100 GB cap (it sits on the internal STANDARD tier already).

### External pilots: 5–10 tenants

The pilot cohort target is **5 minimum, 10 maximum**. Below 5 we lack
statistical signal for GA decisions; above 10 we cannot personally
shepherd onboarding within the bandwidth of a 2-person CS function.

**Pilot ICP (ideal customer profile):**

| Dimension | Must-have | Nice-to-have |
|---|---|---|
| Workload | Build, package, Docker layer, or ML artifact cache | Multi-tenant SaaS reselling CoreLink under their brand (deferred to GA) |
| Scale | ≥ 10 GB warm working set; ≥ 1k blob writes / day | ≥ 50 GB; ≥ 10k writes / day |
| Org size | 20–500 engineers | 100–500 (sweet spot for support density) |
| Geo | Residency: US, EU, or APAC (we have all three at GA) | Single-region preferred for pilot |
| Compliance posture | SOC 2 Type II *required* or *in progress* | ISO 27001, HIPAA-adjacent, FedRAMP-adjacent |
| Tech maturity | Has CI/CD already; can wire S3-compatible client in < 1 day | Already runs a self-hosted cache they want to retire |
| Decision velocity | Single sign-off authority (CTO, VP Eng, Head of Platform) | Pre-existing NDA with HuGR or HuGR portfolio |

**Anti-profile (politely decline):**

- Workloads requiring PHI without a signed BAA before signup.
- Tenants under 5 engineers (support-cost ratio kills the pilot).
- Tenants that want exclusive product features before GA.
- Tenants in jurisdictions we don't yet serve (LATAM, MENA pending
  in wave-25+).
- Tenants asking to use us as a primary, non-cache durable store
  (we are *content-addressable cache* — we DO NOT replace S3).

---

## §2 Pre-Onboarding Checklist

All items MUST be complete and recorded in the pilot tracker before
the activation link is issued.

- [ ] **NDA executed** (mutual; standard HuGR template; legal
      review per `docs/internal/legal-review-process.md`)
- [ ] **DPA executed** (GDPR Art. 28 processor terms; mandatory for
      EU residency, recommended for US/APAC)
- [ ] **BAA executed if PHI in scope** (HIPAA; defaults to OFF —
      pilot tenants MUST explicitly opt in to PHI handling and
      we MUST verify our PHI flag is wired before activation)
- [ ] **Residency selected** (`US-east-1`, `EU-west-1`, `APAC-southeast-1`)
- [ ] **Tier selected** (PILOT free tier; auto-graduates to STANDARD
      at GA — see §8)
- [ ] **Compliance attestations exchanged** (their SOC 2 / ISO; our
      SOC 2 Type II + pentest summary)
- [ ] **Primary technical contact identified** (named individual
      with on-call pager; not a shared inbox)
- [ ] **Primary business contact identified** (signs renewals; not
      the same person as technical contact)
- [ ] **Pilot success criteria written down** (their words, in
      the tracker — what does THEIR 30-day success look like?)
- [ ] **Communication channel created** (shared Slack Connect channel
      or equivalent; email-only is acceptable but discouraged)
- [ ] **Kickoff call scheduled** (45 min within 5 business days of
      contract execution)

If any item is "pending legal," do NOT issue the activation link.
Activating before paperwork is the single most common source of
mid-pilot blow-ups.

---

## §3 Onboarding Sequence

The onboarding sequence is a strict 24-hour window from activation
to first CAS write. We measure this. It is the single best leading
indicator of pilot success.

### T-0: Signup link issued

- CS sends activation link (single-use, 72-hour expiry) via the
  pilot welcome email (see comms templates §1).
- Link delivers: tenant ID, API endpoint, residency-specific URL,
  initial admin credentials (rotate-on-first-use), and the
  quickstart guide URL.
- Slack Connect channel goes live; CS posts the welcome message
  and pins it.

### T+0h to T+24h: Activation window

The 24-hour activation window is when we earn or lose the pilot.
The CS owner runs a watch:

- T+1h: confirm activation link was clicked.
- T+4h: if no click, send onboarding nudge (comms template §2)
  via Slack first, then email.
- T+12h: if no first auth, escalate to Tier 1 support (see §5)
  and offer a screen-share session.
- T+24h: if no first CAS write, this is a YELLOW status — pilot
  is at risk. CS owner schedules a sync call within 48h.

### T+24h to T+72h: First CAS write

- Goal: tenant lands their first PUT to CAS via their own CI/CD
  or workload, NOT via curl.
- Success signal: `cas.write.success` event with `tenant_id` in
  audit log; visible in dashboard (per `pilot-dashboard-checklist.md`).

### T+72h to T+7d: First audit export

- Goal: tenant successfully pulls an audit log export via the
  admin API.
- This proves their compliance flow works end-to-end; the audit
  trail is the single most common pre-GA blocker we will hit.

### T+7d to T+14d: SLO baseline capture

- CS captures baseline SLOs for this tenant: p50/p95/p99 latency,
  availability, error rate, audit emit lag.
- Baseline becomes the floor for the conversion criteria (§6).
- If baseline is materially worse than our published SLOs, this
  is a RED status — the tenant has a misconfiguration or a
  network path issue and we need SRE involvement before day 15.

---

## §4 First-30-Days Success Metrics

> **Post-GA cross-ref:** during the first 30 days **after GA cutover**, the
> per-tenant metrics below are consumed by `specs/_runbooks/RB-POST-GA-CONTINUITY.md`
> §1.2 (initial pilot ack) + §2.2 (first onboarding ack) + §3.1.4 (weekly summary
> cross-publish) + §4.1 (pilot-to-GA conversion). The continuity runbook is the
> canonical orchestrator of the T+0..T+30d window; this §4 remains the
> per-pilot daily review template.

These are the metrics CS reviews **daily** during the pilot. All
are surfaced by the pilot dashboard (`pilot-dashboard-checklist.md`).

| Metric | Definition | Target | Yellow | Red |
|---|---|---|---|---|
| Activation rate | % of issued links activated within 24h | ≥ 90% | 70–89% | < 70% |
| Time-to-first-blob | Hours from activation to first successful CAS write | ≤ 24h | 24–72h | > 72h |
| Time-to-first-audit-export | Hours from activation to first successful audit export | ≤ 168h (7d) | 7–14d | > 14d |
| Support ticket volume | Tickets per tenant per week | ≤ 3 | 4–7 | > 7 |
| SLO breaches | Count of breaches against per-tenant SLO baseline | 0 | 1–2 | ≥ 3 |
| Activation-to-meaningful-use | Hours from activation to ≥ 100 blobs OR ≥ 1 GB stored | ≤ 168h | 7–14d | > 14d |
| NPS (lightweight, day 15) | Single-question survey | ≥ 8/10 | 6–7 | < 6 |
| Documentation-friction tickets | Tickets explicitly tagged "docs unclear" | ≤ 1 / week | 2–3 | > 3 |

A pilot that sits in YELLOW for 7 consecutive days OR drops to RED
on any metric for 48 consecutive hours triggers the escalation tree
(§5) without waiting for a scheduled review.

---

## §5 Escalation Tree

Escalation paths exist so CS does not absorb engineering load and
so engineering does not absorb account load. Use them.

```
                           Pilot tenant issue
                                   │
                                   ▼
                  ┌──────────────────────────────┐
                  │ Tier 1 Support (CS owner)    │
                  │ — auth, quotas, onboarding   │
                  │ — SLA: 4 business hours      │
                  └──────────────────────────────┘
                                   │
                       Cannot resolve in 4h?
                                   │
                                   ▼
                  ┌──────────────────────────────┐
                  │ Tier 2 Support (Eng on-call) │
                  │ — config, integration bugs   │
                  │ — SLA: 4 hours 24/7          │
                  └──────────────────────────────┘
                                   │
                  P0/P1 incident or data integrity?
                                   │
                                   ▼
                  ┌──────────────────────────────┐
                  │ On-call SRE (PagerDuty)      │
                  │ — outage, SLO breach, data   │
                  │ — SLA: 15 min ack 24/7       │
                  └──────────────────────────────┘
                                   │
                Cross-cutting product decision needed?
                                   │
                                   ▼
                  ┌──────────────────────────────┐
                  │ Product Lead                 │
                  │ — feature gaps, scope cuts   │
                  │ — SLA: 1 business day        │
                  └──────────────────────────────┘
                                   │
              Contract / churn risk / strategic exception?
                                   │
                                   ▼
                  ┌──────────────────────────────┐
                  │ CEO                          │
                  │ — final escalation; no SLA   │
                  │   (used sparingly, expected  │
                  │   never to be > 1×/pilot)    │
                  └──────────────────────────────┘
```

**Rules:**

- Skipping tiers is allowed for true emergencies (data loss,
  security incident, public outage) but each skip MUST be
  retroed within 72h to confirm we did not skip due to laziness.
- A pilot that escalates to CEO twice within 30 days is
  automatically a §7 termination candidate — the relationship
  has structurally broken down.
- Every escalation logs to the pilot tracker with: tier, time,
  resolver, outcome, MTTR.

---

## §6 Pilot-to-GA Conversion Criteria

A pilot graduates to GA-paying status only if **all** of the
following are true at the day-30 review. Failing any one is grounds
for an extension call (one 30-day extension permitted) or
termination per §7.

| Criterion | Threshold |
|---|---|
| P0 incidents (CoreLink-side) attributable to this tenant's traffic | 0 |
| SEV-1 audit log discrepancies | 0 |
| Availability over the 30-day window (CoreLink SLO) | ≥ 99.5% |
| CAS storage utilization | ≥ 50 GB |
| Audit events emitted | ≥ 10,000 |
| Tenant has executed at least one successful audit export | yes |
| Tenant has rotated at least one BYOK key (if BYOK enabled) | yes |
| Tenant has passed our day-15 NPS at ≥ 8/10 | yes |
| Tenant signed the GA conversion order form | yes |
| Tenant's primary technical contact has attended the day-25 review | yes |

The conversion call (held between day 25 and day 30) walks through
each criterion live with the tenant. The CS owner produces a
one-page "conversion brief" attached to the pilot tracker entry.

**Extension policy:** if 8 of 10 criteria are met and the tenant
genuinely wants to continue, a single 30-day extension is allowed.
Two consecutive failed extensions = termination (§7).

---

## §7 Pilot Termination Protocol

Pilots end. Most end well (conversion to GA). Some end early; that
is a *feature*, not a bug. Better to off-board a pilot at day 30
than to drag a misfit relationship through GA churn.

### Triggers

A pilot enters termination protocol if any of the following holds:

- Day-30 review fails ≥ 3 conversion criteria AND no extension
  agreed.
- Two consecutive failed extensions (60 total days, no GA fit).
- Tenant requests termination unilaterally.
- HuGR terminates for cause (compliance violation, abusive load,
  payment fraud, ToS breach).
- CEO escalation triggered twice within 30 days (per §5).

### Sequence (all steps complete within 14 calendar days of trigger)

1. **Termination notice** sent (comms template §8). Includes:
   off-boarding date (14 days out), data export window, contact
   for off-boarding questions.
2. **Data export** offered: tenant can request a full CAS bundle
   export and audit log export at no cost. Default retention
   after termination: 30 days, then hard-delete per residency
   policy.
3. **Tenant offboarding** executed via the admin plane (suspend →
   delete; see `docs/internal/admin-plane.md`). All BYOK material
   is destroyed per the BYOK runbook.
4. **Compliance retention** preserved: audit logs retained per
   regulatory minimums (default 7 years for financial; 6 years
   HIPAA where BAA was in scope) in cold storage.
5. **Post-mortem retro** held within 7 days of off-boarding. Two
   formats:
   - *External retro:* 30-minute call with the tenant; what worked,
     what didn't, would they recommend us. Optional but offered to
     every terminated pilot.
   - *Internal retro:* mandatory; CS + Eng + Product + on-call SRE.
     Blameless. Output: a written retro doc in
     `docs/internal/retros/pilots/<tenant-id>-YYYY-MM-DD.md` with
     at minimum: what we promised, what we delivered, what broke,
     two action items with owners.
6. **Pilot tracker** updated: status `TERMINATED`, reason code,
   retro doc link.

---

## §8 Pricing / Billing During Pilot

Pilots are free for the 30-day window subject to a hard 100 GB CAS
cap per tenant. This is enforced at the storage-quota layer (see
`specs/storage/quotas.md`) — it is NOT a billing softlimit.

| Aspect | Pilot | GA (STANDARD) |
|---|---|---|
| Monetary cost to tenant | $0 | Per published price list |
| Storage cap | 100 GB hard | Per tier |
| Request rate cap | Pilot tier rate limits | STANDARD tier rate limits |
| BYOK | Allowed (no surcharge) | Per price list |
| Audit retention | 90 days | Per tier (default 365d STANDARD) |
| Support | Tier 1 + Tier 2 (business hours) + SRE on-call (24/7 for SEV-1) | Per support contract |
| SLA credits | Not offered (pilot has no SLA, only SLOs) | Per MSA |

**Auto-graduation to STANDARD at GA:**

- 7 days before pilot end, CS sends the GA conversion offer
  (comms template §6) including the order form.
- On day 30, if conversion is signed, the tenant flips to STANDARD
  at midnight UTC of day 31. The tenant ID, residency, and BYOK
  posture are preserved. The 100 GB cap is removed.
- If conversion is not signed by day 30, the tenant enters a
  3-day grace state (read-only) before termination protocol fires.

**Pilot abuse guardrails:**

- A tenant repeatedly hitting the 100 GB cap and signaling intent
  to "stay pilot forever" is a §7 termination candidate.
- A tenant generating > 1M requests / day with < 10 GB stored is
  a workload mismatch — we are a cache, not a queue. Refer to
  product to discuss fit.

---

## Appendix A: Pilot tracker fields

The pilot tracker (whichever tool we are on — Linear, Notion,
spreadsheet — does not matter, but the fields do) MUST capture:

- Tenant ID
- Tenant legal name
- Primary technical contact (name, email, pager)
- Primary business contact (name, email)
- Residency
- Tier (PILOT until GA conversion)
- BYOK enabled (y/n)
- PHI in scope (y/n; requires BAA)
- Signup date
- Activation date
- First-CAS-write timestamp
- First-audit-export timestamp
- Day-15 NPS score
- Day-30 conversion decision (CONVERT / EXTEND / TERMINATE)
- Termination reason code (if applicable)
- Retro doc link

Missing fields are a CS process bug, not a tenant problem.
