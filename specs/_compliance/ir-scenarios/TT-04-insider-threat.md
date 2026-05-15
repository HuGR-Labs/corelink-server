---
id: "TT-04-INSIDER-THREAT"
type: "compliance_scenario"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-5"
parent_wi: "WT-GAP-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "ir", "tabletop", "scenario", "sev0", "insider-threat", "pat", "dual-approval", "hr", "legal", "gap-03", "wt-gap-03"]
---

# TT-04 — Insider Threat (Bulk PAT Issuance by Privileged Employee)

> **Severity:** SEV0 · **Duration:** 120 min · **Quorum:** IC + Scribe + Comms Lead + Tech Lead + Security Lead + Legal Liaison + HR Representative + Privacy Officer (8 roles). HR + Legal Liaison are mandatory — without them, reschedule.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` · **Evidence form:** `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md` · **Schedule:** Q4-2026 (target 2026-12-02).
>
> **FM linkage:** **FM-258** (insider data exfil via support tool) + **FM-205** (admin mistake, secondary lens) + **FM-302** (billing leak, downstream if PATs used for usage gaming) + **FM-450** (DSR erasure incomplete, downstream).
> **CTRL coverage:** CTRL-AUTH-010 (dual-approval) + CTRL-PRIV-014 + CTRL-PRIV-016 + CTRL-AUDIT-003 (immutable audit) + CTRL-ANTI-FRAUD-001 (abuse signals).
> **RB-* invoked:** `RB-FM-258-insider-exfil` (canonical, annual tabletop cadence per FM catalog) · `RB-BREACH-NOTIF` (if customer data accessed) · `RB-FM-205-admin-mistake`.

## Scenario summary

It is **15:42 UTC on a Wednesday**. Routine PAT-issuance audit report (Tuesday's, processed at 15:00 UTC daily) flags an anomaly: **employee `sre_lead_004`** issued **47 PATs** across the prior 12-hour window — well above the 30-day median of 2/week. The PATs are tied to 47 different customer tenants and have the broadest permission scope available (`scope:tenant.read.all + tenant.audit.read.all`).

The employee in question is a recently-hired Senior SRE (3 months in role) with legitimate access to the support-tool console. Their employment records show: passed background check, no PIP, performance review pending. No HR signal exists.

A second concerning signal arrives at T+8: a PostHog product-analytics dashboard shows `sre_user_id=sre_lead_004` triggered the `support_tool.export_audit_bundle` action 47 times in the last 6 hours — each export bundles 30 days of audit events for the targeted tenant.

The team must navigate a delicate process:
1. Investigate without alerting the suspected insider (per `RB-FM-258-insider-exfil.md` Comunicação §"não envolver o SRE suspeito").
2. Decide whether to revoke the employee's access immediately (containment) vs. observe (forensics).
3. Loop in HR + Legal — handle whistleblower vs accusatory framing carefully.
4. Determine customer-notification posture if data was actually exfiltrated.
5. Preserve evidence chain-of-custody for potential law enforcement / litigation.

This scenario is **explicitly delicate** — the goal is rehearsing the process under emotional pressure and ambiguity, not establishing guilt.

## Pre-tabletop preparation

### Facilitator-localised anchors (T-1 week)
- Confirm `support_tool.export_audit_bundle` is a real audit-event type in the catalog. If not yet, this scenario surfaces the gap — file action item from rehearsal.
- Pre-brief HR Representative on the scenario tone — this is a process rehearsal, not a real allegation against any real employee.
- Confirm Legal Liaison understands distinction between (a) good-faith investigation of an anomaly, (b) accusatory statement requiring HR + Legal evidentiary discipline.

### Participants briefed (T-24h Slack post template — sensitive tone)
> Tabletop **TT-04** runs **Wed 2026-12-02 14:00 UTC** on Zoom `<link>`. Scenario: insider-threat tabletop simulating a privileged employee triggering an anomalous PAT-issuance pattern. **This is a process rehearsal — no real employee under suspicion.** Roles: IC=`<name>`, Tech Lead=`<name>`, Security Lead=`<name>`, Legal Liaison=`<name>`, HR Rep=`<name>`, Comms Lead=`<name>`, Privacy Officer=`<name>`, Scribe=`<name>`. **Tabletop only — no live actions, no real revocations.** Sensitive: do not casually discuss the fictional employee name outside the session. Read RB-FM-258, RB-BREACH-NOTIF before the call.

## Injects (timeline)

### Inject 1 — T+00:00 (Detection)
> *"It is 15:42 UTC, Wednesday. Daily PAT-issuance audit report just landed. Anomaly: employee `sre_lead_004` issued 47 PATs in the prior 12 hours, all scoped `tenant.read.all + tenant.audit.read.all`, across 47 different customer tenants. 30-day median for this employee is 2 PATs/week. There is no concurrent customer-escalation ticket pattern that explains the spike. The employee is currently active in Slack and the support-tool console. PagerDuty paged you SEV-2 on the anomaly alert. Go."*

**Expected:** IC declares SEV1 immediately (insider-threat-possible); within 10 min consults `RB-FM-258-insider-exfil`; pages Security Lead + Legal Liaison + HR Rep silently (DM, not channel — avoid tipping the suspect if they're in a shared channel).

### Inject 2 — T+12:00 (Detection, escalating signal)
> *"PostHog product-analytics confirms: same employee triggered `support_tool.export_audit_bundle` 47 times in the last 6 hours, each export bundling 30 days of audit events for a different tenant. The audit-event records show `consent_token_id: null` on every export — there is no customer ticket / consent token attached. Per CTRL-PRIV-016, support-tool reads without consent_token are policy violations regardless of intent. Security Lead, your call: revoke now or observe?"*

**Decision point D-1:** Revoke employee access immediately (Clerk session kill + PAT auto-rotation + support-tool ACL removal — irreversible in the sense that the employee will instantly know we suspect them) vs. observe (silent forensics buildup, risk of further exfiltration). This is THE central tension of the scenario.

### Inject 3 — T+28:00 (HR + Legal complication)
> *"HR Rep just looped in: the employee has a pending performance review next week. There has been no PIP, no flag from manager, no whistleblower tip. The employee's badge access log shows they're currently in the office (one of the few employees still on-site post-hybrid). Legal Liaison, what is your read on (a) the legal exposure of immediate revocation without HR-led conversation, (b) chain-of-custody requirements for evidence preservation if this proceeds to investigation, (c) whether to immediately preserve a forensic image of their corporate laptop?"*

**Decision points D-2 / D-3:**
- D-2 (Legal Liaison + HR): revocation timing posture — immediate (containment-first) vs HR-led conversation first (process-first). Reference applicable employment law jurisdiction (Brazilian CLT for BR-based employees, at-will for US, etc.).
- D-3 (Security Lead + Legal): forensic preservation — laptop image, chat exports, badge log preservation, audit-event archival.

### Inject 4 — T+50:00 (Customer-impact assessment)
> *"Tech Lead: review of the 47 exports — they are spread across 47 different tenants, no concentration on any specific industry or customer. 12 of the 47 tenants are lighthouse / enterprise customers under MSA — those have explicit breach-notification SLAs (4h written notice for any unauthorized access). The other 35 are mid-market on the standard ToS (no MSA notification SLA, but LGPD/GDPR notification clocks would still apply if personal data was accessed). Privacy Officer, what does this mean for notification posture?"*

**Decision point D-4:** Notification scope — proactively notify 47 customers? Only the 12 lighthouse? Only after forensic confirmation of exfiltration? LGPD Art. 33 + GDPR Art. 33 clocks: do we have "reasonable certainty" of breach affecting personal data at T+50?

### Inject 5 — T+72:00 (Investigation pivot)
> *"Update at T+72: HR-led conversation occurred (assuming the team chose this path; if not, facilitator narrates an equivalent investigative-action result). The employee's explanation: 'I was preparing a quarterly internal audit-coverage report; my manager (who is on PTO this week) asked me last Friday to pull these bundles for analysis. I forgot to attach consent tokens because I was operating in internal-only mode.' Manager (reached via emergency contact): 'I asked for 5 sample bundles, not 47. We never discussed the broader scope.' Now: is this insider-malicious (lying), insider-negligent (genuine misunderstanding), or process-broken (legitimately ambiguous policy)? Each path has different action items, customer-comms posture, and HR consequence."*

**Decision points D-5 / D-6:**
- D-5 (IC + HR + Legal): characterise the incident — malicious / negligent / process-gap. Each carries different downstream actions.
- D-6 (Comms Lead + Customer Comms Lead + Privacy Officer): final notification posture based on characterisation. **Reminder**: even if benign, the data WAS accessed without consent tokens; CTRL-PRIV-016 was violated; some notification is likely warranted regardless of intent.

### Inject 6 — T+95:00 (Post-Incident process hardening)
> *"Suppose the team converged on 'insider-negligent + process-gap' (most realistic). What changes structurally? Action items must include: (a) bulk-PAT-issuance dual-approval requirement (currently single-approval per CTRL-AUTH-010 §3.1); (b) consent-token enforcement (block support-tool reads without explicit consent token); (c) PostHog anomaly alert wired to real-time PagerDuty rather than daily report; (d) HR + Manager attestation flow for bulk-export legitimate-use cases. Rank these by impact + effort."*

## Decision points summary

| # | Decision | Owner | NIST phase | Reversibility |
|---|---|---|---|---|
| D-1 | Revoke access now vs observe | IC + Security Lead | Containment | **partially-irreversible** (tip-off) |
| D-2 | Revocation timing — immediate vs HR-led | IC + Legal Liaison + HR | Containment | reversible (HR-led can pivot) |
| D-3 | Forensic preservation (laptop image, exports) | Security Lead + Legal | Detection / Analysis | reversible (preserves optionality) |
| D-4 | Customer notification scope (all 47 / 12 lighthouse / wait) | IC + Privacy Officer + Customer Comms | Recovery | (notify) **irreversible** |
| D-5 | Incident characterisation (malicious / negligent / process-gap) | IC + HR + Legal | Post-Incident | reversible (can be re-characterised with new evidence) |
| D-6 | Final notification posture per characterisation | Comms Lead + Privacy Officer + Customer Comms | Recovery | (notify) **irreversible** |

## Comms templates

### Internal Slack post (T+5, PRIVATE channel — not main eng)
```
[PRIVATE — DM Group, not #incidents]
@<IC> @<Tech Lead> @<Security Lead> @<Legal Liaison> @<HR Rep> @<Privacy Officer> @<Comms Lead> @<Scribe>

We have a potential insider-threat signal: anomalous PAT-issuance + audit-bundle export pattern by sre_lead_004 in the last 12h. Per RB-FM-258 §Comunicação, we are NOT opening a public incident channel until investigation confirms. All work happens in this DM group + a private Notion page.

IC: <name>
Sensitive — do not discuss outside this group.
```

### Customer email (T+90, lighthouse-tenant version)
```
Subject: [PRIVILEGED] CoreLink security event affecting your account
Body:
Dear <CISO name>,

We are writing to inform you of a security event that occurred between 03:42 and 15:42 UTC on 2026-12-02. During an internal audit of administrative tooling, we detected that an audit-data export for your tenant was performed by a CoreLink support engineer without the required customer-consent token, in violation of our internal access policy (CTRL-PRIV-016).

What happened:
- An internal employee performed an audit-bundle export of your tenant's data spanning 30 days.
- The export was performed via a sanctioned internal tool, but without the customer-consent attestation our policy requires.
- Subsequent investigation has determined the export was <malicious / negligent / process-gap>; we are completing our internal review.

What was accessed:
- Audit metadata: <event types, but not raw PII payloads — to be confirmed>
- Time window: 30 days preceding 2026-12-02.

What we are doing:
- The employee's access has been <suspended / under review> pending investigation.
- We have instituted a dual-approval requirement for any bulk-export operation effective immediately.
- We are providing a written postmortem within 7 days.
- We are filing notification with the ANPD (LGPD Art. 33) and the relevant GDPR DPA within the 72h window.

What we recommend:
- No customer-side action is required for data integrity (no data was modified).
- We will provide a full forensic report and stand by for a debrief at your CISO's convenience.

— CoreLink Privacy Officer (<name>)
```

### Regulator notification (LGPD ANPD draft, T+90)
```
Notificação de incidente de segurança — Art. 33 LGPD
Controlador: CoreLink (Humangr Labs)
Tipo de incidente: acesso não autorizado a dados pessoais por colaborador interno (escopo: metadados de auditoria)
Data do incidente: 2026-12-02
Hora da detecção: 15:42 UTC
Caracterização preliminar: <malicioso / negligente / falha de processo>
Dados afetados: metadados de eventos de auditoria de 47 tenants (~<N> titulares estimados)
Medidas adotadas: revogação de acesso do colaborador; revisão de processo; implementação de aprovação dupla para exportações em massa; coordenação com encarregados dos controladores afetados
Próximos passos: laudo forense completo em 7 dias; revisão de CTRL-PRIV-016 + CTRL-AUTH-010; comunicação aos titulares se confirmada exposição de dados pessoais sensíveis
Contato: privacy@<corelink-domain>
```

### HR-facing internal write-up (T+95, drafted by HR Rep + Legal Liaison)
```
Subject: TT-04 internal-process write-up — sre_lead_004 PAT-issuance anomaly (PRIVILEGED)

Sequence of events: <objective timeline>
Investigation conducted: <interviews, evidence preserved, scope reviewed>
Characterisation: <malicious / negligent / process-gap, with rationale>
HR action: <separation / written warning / training / no action — per characterisation + employment-jurisdiction law>
Process-gap remediation: <list, with owners>
Legal: this document is privileged under attorney-client / work-product doctrine where applicable; do not redistribute.
```

## Post-incident artefacts list

- [ ] PostHog dashboard export (47 audit-export events with timestamps + tenant IDs)
- [ ] Audit-event chain export covering the 12h window
- [ ] Forensic laptop-image preservation order (Legal-signed, if executed)
- [ ] HR-led conversation summary (privileged)
- [ ] Manager attestation interview summary
- [ ] LGPD ANPD notification draft (sealed)
- [ ] GDPR DPA notification drafts (if applicable; sealed)
- [ ] Customer email drafts (12 lighthouse + 35 mid-market; sealed)
- [ ] Action items into Linear epic `IR-TT-2026-Q4-02`
- [ ] CTRL-AUTH-010 amendment proposal (bulk-PAT dual-approval)
- [ ] CTRL-PRIV-016 enforcement amendment proposal (consent-token block at API level)
- [ ] Updates to `RB-FM-258-insider-exfil.md`

## Success criteria

1. **SEV1 declared with private-channel discipline** (not public #incidents).
2. **HR + Legal + Privacy seats all engaged** within first 30 min.
3. **Revocation vs observation decision** made with explicit reversibility note.
4. **Forensic preservation** discussed and ordered (or rationalised against) by Security Lead + Legal.
5. **Notification posture** decided across (regulator / lighthouse customers / mid-market customers) with LGPD/GDPR clock explicitly named.
6. **Incident characterisation** (malicious / negligent / process-gap) chosen with rationale.
7. **At least 5 action items** captured (insider-threat tabletops typically surface the most action items of any scenario).

## Failure modes during exercise

- Team opens public #incidents channel and tips off the suspected insider → action item: harden private-DM protocol in `RB-FM-258-insider-exfil.md`.
- Team makes a revocation call without Legal Liaison + HR present → action item: hard-gate revocation on (Legal + HR ack).
- Team conflates "negligent" with "no incident" and skips notifications → Privacy Officer must push back; if they don't, facilitator flags as gap.
- Team forgets chain-of-custody for forensic preservation → action item: 1-page chain-of-custody quick-ref.
- Team treats this exercise lightly given fictional employee → facilitator re-grounds: "the discipline matters more than the conclusion".

## Cross-references

- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`.
- `specs/05_quality/runbooks/RB-FM-258-insider-exfil.md` (canonical, annual tabletop cadence).
- `specs/05_quality/runbooks/RB-BREACH-NOTIF.md`.
- `specs/05_quality/runbooks/RB-FM-205-admin-mistake.md`.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` §2 SEV0 procedure note.
- `specs/03_architecture/failure_modes.md` FM-258, FM-205, FM-302.

## Controls evidenced

CC7.3 · CC7.4 · CC7.5 · plus CTRL-AUTH-010 (dual-approval), CTRL-PRIV-014, CTRL-PRIV-016 (consent-token enforcement), CTRL-AUDIT-003 (immutable audit), CTRL-ANTI-FRAUD-001 (PostHog anomaly signals).

External frameworks: LGPD Art. 33, GDPR Art. 33/34, ISO/IEC 27035-1 §6–7, applicable employment-law jurisdictions.
