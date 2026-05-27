---
id: "SOC2-ROADMAP-TYPE1-2026-05-14"
type: "compliance_roadmap"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
sprint: "S-20"
parent_wi: "WI-S20-003"
owner: "Gustavo Schneiter"
tags: ["soc2", "type-i", "roadmap", "drata", "schellman", "a-lign", "type-ii-prep"]
---

# SOC 2 Type I — 6-Month Roadmap (Post-GA)

> **doc_status:** FROZEN · **scope:** SOC 2 Type I engagement pós-GA T+0..T+6 meses + Type II observation window opens T+6m.
>
> **Anchor:** WI-S20-003 D4 + spec contract §10 anti-scope (Type I deferred 6m pós-GA).
>
> **Companion:** `specs/_compliance/SOC2-GAP-ANALYSIS.md` (33 GAPs · 1 blocking-GA closing · 9 major · 23 minor) + `specs/_compliance/vendor-shortlist-soc2.md` (Drata selected) + `specs/_audits/sealed/2026-05-14-soc2-readiness-score.md`.

## Phase overview

| Phase | Window | Owner | Output |
|---|---|---|---|
| **M0 — Scope freeze** | T-1m .. T+0 (pre-GA finish) | Compliance + Owner | scope statement, system boundaries, in-scope criteria list |
| **M0-M1 — Auditor RFP & selection** | T+0 .. T+1m | Owner + Finance + Compliance | engagement letter (Schellman / A-LIGN / Prescient) |
| **M1-M3 — Evidence collection automation** | T+1m .. T+3m | SRE + Compliance | Drata agents fully connected; > 95% controls green sustained |
| **M3-M4 — Walkthrough sessions** | T+3m .. T+4m | Engineering + Compliance | auditor walkthrough notes; control owners interviewed |
| **M4-M5 — Audit fieldwork** | T+4m .. T+5m | Compliance + all control owners | sample evidence pulled; auditor PBC list closed |
| **M5-M6 — Management response & report** | T+5m .. T+6m | Owner + Compliance + Legal | management assertion signed; Type I report delivered |
| **M6+ — Type II observation window opens** | T+6m .. T+18m | Compliance + Drata continuous | rolling 6-12m observation; Type II fieldwork T+15m..T+18m |

## M0 — Scope freeze (T-1m .. T+0)

### Activities

1. **Scope statement** — system boundary covers CoreLink production: Cloudflare Workers + Pages + R2 + D1 + DO + KV + Clerk + Stripe; excludes corporate IT / SaaS HR / personal devices.
2. **TSC selection** — Common Criteria (CC1..CC9) **mandatory** + Availability + Confidentiality + Processing Integrity + Privacy (selected per customer-facing requirements). All 5 trust principles in scope.
3. **System description (SOC 2 §B)** — draft system description following AICPA DC 200; sections: services, principal service commitments, components (infra/software/people/procedures/data), boundaries, complementary user-entity controls (CUECs), complementary sub-service organization controls (CSOCs), risks.
4. **CUECs / CSOCs** — enumerate customer responsibilities (e.g., MFA enforcement on their Clerk tenants) + sub-processor responsibilities (Cloudflare, AWS, GCP, Azure SOC 2 reports referenced).
5. **Owner sign-off** — Final Approver signs scope per HIGH_RISK lane.

### Exit criteria

- Scope statement committed em `specs/_compliance/SOC2-SCOPE.md` (TBD T-1m doc).
- All 33 GAPs categorized in-scope / out-of-scope.
- Drata Trust Center / Vanta system-description scaffold populated.

## M0-M1 — Auditor RFP & selection (T+0 .. T+1m)

### Candidates

| Firm | Strengths | Weaknesses | Indicative cost |
|---|---|---|---|
| **Schellman** (primary) | SOC 2 specialist; SaaS-heavy book; ISO 27001 + HITRUST capable; pre-IPO grade | premium pricing; calendar tight Q4 | $45-65k Type I |
| **A-LIGN** (secondary) | broad coverage; FedRAMP capable; PCI DSS option | larger shop, more variable analyst quality | $35-55k Type I |
| **Prescient Assurance** | startup-friendly; fast turnaround; tight Drata integration | smaller brand recognition with EU customers | $25-40k Type I |
| **Sensiba LLP** (contingency) | West Coast SaaS focus | smaller bandwidth | $30-45k Type I |

### Process

1. **RFP issued** T+0 to 3 firms (Schellman + A-LIGN + Prescient); include scope statement + GAP analysis sanitized + Drata dashboard snapshot.
2. **Bid responses** by T+0.5m.
3. **Reference checks** (≥ 2 SaaS startup references each).
4. **Selection** by T+1m; engagement letter signed; access provisioning for auditor team (read-only Drata + GitHub audit).
5. **Kickoff** — engagement plan, weekly cadence calls, PBC list shared.

### Budget envelope

- Type I engagement: **$30-60k** (target $40-50k Schellman).
- Drata annual subscription: **$5-15k** (committed; see `specs/_compliance/vendor-shortlist-soc2.md`).
- Internal engineering time: ~120h (Compliance Officer + SRE Lead + Architect).
- Legal review of report: $3-5k (counsel of record).
- **Total Type I phase:** $40-85k (target $55k median).

### Exit criteria

- Engagement letter signed + filed em `legal/auditor-engagement/`.
- Auditor team identified + onboarded.
- PBC list shared with control owners.

## M1-M3 — Evidence collection automation (T+1m .. T+3m)

### Drata integrations to harden

- **Cloudflare** — DNS / WAF / Access logs + Workers deploy events.
- **GitHub** — branch protection + signed commits + PR approvals + Dependabot.
- **Clerk** — user list + MFA enforcement + role assignments.
- **Stripe** — billing access + admin actions.
- **AWS / GCP / Azure** — IAM policies, KMS key usage, CloudTrail / Cloud Audit Logs / Activity Logs.
- **PagerDuty** (post WI-S20-006) — on-call + incident records.
- **Sentry / observability** — alert routing + retention.

### Evidence catalog (target 100% control coverage)

- Access provisioning tickets + RBAC snapshots.
- Quarterly access review attestations (closes GAP-01 + GAP-26).
- BYOK FIPS attestation letters (closes GAP-02).
- Change-management approvals (Cosign + Rekor + PR approvals).
- IR tabletop transcripts (closes GAP-03).
- Cold restore drill results (closes GAP-15).
- Sub-processor SOC 2 reports refreshed (closes GAP-09).
- Vendor risk register completed (closes GAP-14).
- Vulnerability mgmt SLA (closes GAP-29).
- DR drill quarterly (closes GAP-13).
- Vendor breach notification dry-run with lighthouses (closes GAP-06).

### Drata dashboard sustained ≥ 95%

- D+30 snapshot ≥ 95% (96.4% current per gap analysis §summary).
- Weekly compliance review (closes GAP-08) verifies drift < 1% week-over-week.

### Exit criteria

- All 9 major GAPs closed by T+3m.
- ≥ 95% controls green sustained 8 weeks.
- Auditor sample-evidence pull dry-run successful.

## M3-M4 — Walkthrough sessions (T+3m .. T+4m)

### Sessions (one per TSC area)

1. CC1+CC2+CC3 — governance + risk (Owner + Compliance).
2. CC4+CC5 — monitoring + control activities (Compliance + SRE).
3. CC6 — logical access (Architect + Security + SRE).
4. CC7+CC8 — operations + change mgmt (SRE + Engineering).
5. CC9 — vendor risk (Compliance + Legal).
6. A1 — availability (SRE).
7. C1 — confidentiality + BYOK (Architect).
8. PI1 — processing integrity (Engineering).
9. Privacy — DSR + consent + breach (Privacy Officer + Legal).

### Outputs

- Auditor walkthrough notes (auditor-owned).
- Control descriptions reviewed + signed off.
- PBC list version 2 issued (refined).

### Exit criteria

- All 9 walkthroughs completed.
- No auditor red flags un-addressed.
- Sample population for fieldwork agreed.

## M4-M5 — Audit fieldwork (T+4m .. T+5m)

### Activities

- Auditor pulls samples per agreed population (typically 25 access provisioning events, 25 change events, 25 incidents if applicable).
- Auditor tests design of controls (Type I = design only; Type II = design + operating effectiveness).
- Weekly cadence call; PBC tracker updated daily.
- Owner final-approver review of findings.

### Exit criteria

- All samples returned + evaluated.
- Draft management letter received.
- No material weaknesses outstanding.

## M5-M6 — Management response & report finalization (T+5m .. T+6m)

### Activities

1. **Management assertion** — Owner signs SOC 2 §B management assertion attesting controls fairly presented + suitably designed.
2. **Management response** to any deviations — remediation timeline + compensating controls documented.
3. **Auditor independence + opinion** — auditor issues unqualified opinion (target) OR qualified with reservations + management response.
4. **Report formats** — full SOC 2 report (customer-NDA distribution) + sanitized SOC 3 trust-services public report (optional T+8m).
5. **Distribution** — Drata Trust Center; customer NDAs; sales enablement.

### Exit criteria

- Type I report delivered.
- Unqualified opinion preferred; qualified acceptable if narrow scope.
- Type II observation window starts T+6m.

## M6+ — Type II observation window opens (T+6m .. T+18m)

### Activities

- Continuous Drata monitoring sustained.
- Quarterly internal audit cadence (Compliance Officer + Owner).
- Type II target fieldwork T+15m..T+18m.
- Annual penetration test (closes GAP-20) before Type II fieldwork.
- Annual policy review attestations (closes GAP-31).

### Type II budget

- $50-80k engagement (longer observation, more sample testing).
- Drata renewal $5-15k.
- Internal time ~160h.
- **Total Type II phase:** $60-100k (target $75k median).

## Risk register (roadmap-specific)

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| RR-1 | Auditor calendar miss (Q4 holidays) | medium | high | engage by T+1m; pre-book fieldwork window |
| RR-2 | Drata agent gap on Cloudflare Workers (no agent runtime) | medium | medium | document compensating control (CC log streaming); closes GAP-18 |
| RR-3 | Blocking-GA gap re-opens during fieldwork | low | high | weekly compliance review; Drata drift alerts |
| RR-4 | Lighthouse customer NDA blocks Type I distribution | low | low | sanitized SOC 3 fallback |
| RR-5 | Budget overrun > $85k | medium | medium | competitive bids; Prescient/Sensiba fallback |
| RR-6 | Type II observation insufficient by T+15m | medium | medium | start Type II planning T+9m |

## Cumulative dependencies

- **Drata subscription** ($5-15k/yr) — committed pre-GA.
- **Auditor engagement** ($30-60k) — funded from compliance budget Q4 pós-GA.
- **WI-S20-006** (IR PagerDuty 24/7 + synthetic page) — closes GAP-03.
- **WI-S20-004** (3 lighthouse customers + 1 enterprise BYOK 30d SLA) — provides operational evidence for Type II observation.
- **WI-S20-007** (30d staging + TLA+ 4 specs + runbooks 90d SBOM) — observation-window baseline.

## Acceptance per WI-S20-003 §6.1.4

- ✅ Roadmap Type I 6m pós-GA committed em `specs/_compliance/SOC2-ROADMAP.md`.
- ✅ Firm bid + selection + SOW timeline documented (T+0 .. T+1m).
- ✅ Budget estimate $30-60k Type I + $5-15k Drata = $35-75k total documented.
- ✅ Type II target T+12m..T+18m documented.

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Opus 4.7 builder) | Initial roadmap WI-S20-003 D4; 6-phase plan T-1m..T+6m+; Schellman primary; Type II window T+6m..T+18m. |

---

**Fim SOC2-ROADMAP.**
