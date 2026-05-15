---
id: "TT-01-DATA-BREACH"
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
tags: ["compliance", "ir", "tabletop", "scenario", "sev0", "breach", "byok", "lgpd-art-33", "gdpr-art-33", "gap-03", "wt-gap-03"]
---

# TT-01 — SEV0 Data Breach (Suspicious Access Patterns / Possible BYOK Envelope Leak)

> **Severity:** SEV0 · **Duration:** 120 min · **Quorum:** IC + Scribe + Comms Lead + Tech Lead + Legal Liaison + Privacy Officer + Security Lead + Customer Comms Lead (8 roles). Below 7 of 8 → reschedule.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` · **Evidence form:** `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md` · **Schedule:** Q3-2026 (target 2026-07-22).
>
> **FM linkage:** **FM-258** (insider data exfil) + **FM-253** (cross-tenant read) + **FM-156** (supply-chain → key compromise vector) + **FM-450** (DSR erasure incomplete; downstream effect).
> **CTRL coverage:** CTRL-PRIV-014/016/032 + CTRL-AUTH-010 + CTRL-AUDIT-001/003 + CTRL-BYOK-* + CTRL-ISO-001..005.
> **RB-* invoked:** `RB-BREACH-NOTIF` (canonical) · `RB-FM-258-insider-exfil` · `RB-FM-253-cross-tenant-read` · `RB-BYOK-REVOKE`.

## Scenario summary

It is **02:14 UTC on a Tuesday**. PagerDuty pages SEV-2 to L1 on-call: a Grafana alert (`anomaly_byok_envelope_fetch_rate`) fires on the `byok-adapter` worker — three customer tenants (`tnt_lighthouse_acme`, `tnt_lighthouse_globex`, `tnt_lighthouse_initech`) are observing envelope-fetch rates 18× their 30-day baseline within a 7-minute window. The fetches originate from a **single PAT** (`pat_2025...` redacted) tied to an internal support-engineer identity (`sre_user_237`).

There has been **no scheduled maintenance**. The support engineer in question is logged as PTO this week per HR system. The PAT was issued via the support-tool console 11 days ago for a normal CS escalation that closed 6 days ago — the PAT should have auto-expired, but a recent `pat-issuance` audit row shows it was renewed 4 hours ago through a path that does **not** appear in the standard CS console UI.

Within 8 minutes of the first alert, an additional signal fires: `byok_envelope_decrypt_failures_total` ticks 0 (all fetches succeeded), and an R2 access log shows 1,420 envelope objects downloaded from the BYOK key prefix into an external egress (Cloudflare egress destination ASN looks legitimate at first read — a major IaaS provider).

The team must decide:
1. Is this a real breach (insider) or a false positive (legitimate emergency access we don't see in HR yet)?
2. If real: containment, BYOK CMK posture, customer notification, LGPD/GDPR/CCPA notification, status-page strategy, ToS-invocation (suspend the affected internal account).
3. If false positive: how to confirm safely without tipping off a potential attacker.

## Pre-tabletop preparation

### Facilitator-localised anchors (T-1 week)
- Use staging tenant IDs from `staging-us-east-1` cluster.
- Pull a real `pat-issuance` audit-event sample from the dev catalog (`crates/corelink-audit/fixtures/pat_issuance_sample.json`) so the inject reads like a real log line.
- Confirm `RB-BREACH-NOTIF` v1.0.0 hashes match `legal/breach-notification/rb-breach-notif-decision-tree.yaml` (per `RB-FM-202-runbook-stale.md` discipline).

### Participants briefed (T-24h Slack post template)
> Tabletop **TT-01** runs **Tue 2026-07-22 14:00 UTC** on Zoom `<link>`. Scenario: suspicious BYOK access pattern, possible insider. Roles: IC=`<name>`, Scribe=`<name>`, Tech Lead=`<name>`, Comms Lead=`<name>`, Legal=`<name>`, Privacy=`<name>`, Security=`<name>`, Customer Comms=`<name>`. **Tabletop only — no live actions.** Bring laptop with FM catalog, ONCALL matrix, RB-BREACH-NOTIF, and the SOC 2 evidence rollup open. Read `specs/_compliance/ir-scenarios/TT-01-data-breach.md` §"Scenario summary" before the call.

## Injects (timeline)

> Facilitator reads each inject aloud at the listed clock time. **Do not pre-share injects 2–5 with the team.** Inject 1 is the pre-read scenario summary; the rest land cold.

### Inject 1 — T+00:00 (Detection, primary signal)
> *"It is 02:14 UTC. PagerDuty paged you SEV-2. Grafana alert `anomaly_byok_envelope_fetch_rate` fires across **three lighthouse tenants** simultaneously. Fetch rate is 18× the 30-day baseline in a 7-minute window. The origin is a single PAT tied to a support engineer (sre_user_237) currently on PTO per HR. There is no incident channel open yet. Go."*

**Expected team behaviour:** IC declares severity within 5 min; opens `#inc-2026-07-22-byok` (simulated); pages L2 EM-on-call (auto-300s per `ONCALL-ESCALATION-MATRIX.md` §3 SEV1 row — but team should up-rate this to SEV0 within first decision).

### Inject 2 — T+12:00 (Detection, secondary signal raises severity)
> *"R2 access log review shows **1,420 envelope objects** downloaded from the BYOK key prefix in the last 8 minutes, all into the same egress destination (a major IaaS provider ASN). Tech Lead, what's your read on whether this is exfiltration in progress?"*

**Decision point D-1:** SEV up-rate from SEV-2 → SEV0? Trigger SOC 2 SEV0 procedure (`ONCALL-ESCALATION-MATRIX.md` §2 — "page **all** of L1+L2+L3+board chair simultaneously").

### Inject 3 — T+22:00 (Containment, decision pressure)
> *"You now have to decide: do you (a) revoke the suspicious PAT immediately, (b) leave it active to gather forensic data via increased audit verbosity, or (c) revoke + rotate the BYOK CMK for the three affected tenants per `RB-BYOK-REVOKE.md`? Note: CMK rotation is irreversible within the 24h pending-rotation window and would force re-encryption of all envelopes for those tenants — observable on customer dashboards."*

**Decision point D-2:** Containment action (a / b / c / hybrid). Each path has tradeoffs:
- **(a)** — fast containment, lose forensic trail.
- **(b)** — preserves forensic trail, blast radius keeps growing.
- **(c)** — strongest containment + customer-visible action (triggers status page yellow + customer comms within minutes).

### Inject 4 — T+45:00 (Eradication, complication)
> *"HR just messaged the channel: `sre_user_237`'s manager confirms the engineer is genuinely on PTO + flying internationally; HR cannot reach them. The PAT-issuance audit row from 4h ago was triggered through the **emergency-access workflow** (`/v1/pat/emergency-issue`) which requires dual-approval per CTRL-AUTH-010. The second approver field shows `sre_lead_002` — but `sre_lead_002` is also on PTO and their Clerk session shows last activity 3 days ago. Who actually approved?"*

**Decision point D-3:** Investigation pivot — is the dual-approval system itself compromised (insider with admin token) or is this audit evidence consistent with a stolen/replayed approver token? Tech Lead must surface CTRL-AUTH-010 + CTRL-AUDIT-003 enforcement details.

### Inject 5 — T+62:00 (Recovery + Legal pressure)
> *"It is now 03:16 UTC. Three lighthouse customers' security teams are paged by their own SIEM (we ship audit events to their SIEMs per the lighthouse contract). One CISO is already on a Slack-Connect channel asking `what is going on with our keys?`. Comms Lead, what does your status-page paragraph say at 03:20 UTC? Legal, the LGPD 72h clock — when does it start, and against which controller?"*

**Decision points D-4 / D-5:**
- D-4 (Comms): status-page paragraph + customer email body — drafted live.
- D-5 (Legal): LGPD Art. 33 §1º clock starts when the controller has reasonable certainty of breach affecting personal data; team must articulate whether this threshold is met at T+62 or requires deeper forensic confirmation. Decision-tree path per `RB-BREACH-NOTIF.md` §1.

### Inject 6 (optional) — T+85:00 (Post-Incident)
> *"Forensics confirms at T+85: the attacker was the PAT itself, replayed from a credential-leak GitHub repo (one of the lighthouse customers accidentally committed the PAT to a public repo 9 days ago, exposed for 6h before being rotated by GitHub secret-scanning — but the attacker already had it). No internal insider. What changes about your communications, your customer obligations, and your action items?"*

**Decision point D-6:** Updated comms (no insider, but a customer accidentally leaked their support PAT) + contractual implications under the lighthouse MSA.

## Decision points summary (canonical checklist)

| # | Decision | Owner | NIST phase | Reversibility |
|---|---|---|---|---|
| D-1 | SEV up-rate SEV-2 → SEV0 | IC | Detection | reversible |
| D-2 | Containment choice (revoke / observe / rotate CMK) | IC + Tech Lead | Containment | (a) reversible, (b) reversible, (c) **irreversible** |
| D-3 | Investigate dual-approval integrity | IC + Security Lead | Detection / Analysis | reversible |
| D-4 | Status-page text + customer email draft | Comms Lead + Customer Comms Lead | Recovery | reversible (text revision) |
| D-5 | LGPD/GDPR/CCPA notification clock — start now or after forensic confirmation | Legal Liaison + Privacy Officer | Recovery | **irreversible** (notify cannot be unsaid) |
| D-6 | Post-discovery comms revision + customer MSA invocation | IC + Legal + Customer Comms | Post-Incident | reversible |

## Comms templates (pre-drafted skeletons the team localises in §6 of evidence form)

### Internal Slack post (T+5, IC opens incident channel)
```
@channel — SEV0 incident
Channel: #inc-2026-07-22-byok
Scenario: anomalous BYOK envelope-fetch pattern across 3 lighthouse tenants; possible insider OR external compromise of a support PAT.
IC: <name>
Tech Lead: <name>
Scribe: <name>
Comms Lead: <name>
Legal Liaison paged: yes
Privacy Officer paged: yes
Customer Comms Lead paged: yes
Status page: drafting yellow update
Customer comms: drafting per RB-BREACH-NOTIF decision tree
Update cadence: every 15 min until containment confirmed
```

### Status-page paragraph (T+20, initial)
```
[INVESTIGATING] At <UTC>, we detected anomalous access patterns affecting a subset of customer key envelopes. Out of an abundance of caution, we are investigating and have temporarily restricted the credential involved. Customer data remains encrypted with customer-controlled keys; no integrity loss observed. We will update this page every 30 minutes.
```

### Customer-direct email (T+45, sent only to the 3 affected tenants)
```
Subject: [ACTION RECOMMENDED] CoreLink security event affecting your tenant
Body:
Dear <CISO name>,

At <UTC>, our anomaly-detection systems flagged an unusual pattern of envelope fetches affecting your tenant. The credential involved (PAT `pat_2025...redacted`) has been revoked. We are conducting forensic analysis to determine root cause and full blast radius.

What we know so far:
- Affected tenant: <tnt_id>
- Number of envelope objects accessed: <N>
- Time window: <UTC start> – <UTC end>
- Customer data confidentiality: preserved (your CMK still controls decryption)

What you should do:
- Rotate any shared secrets accessible by your CoreLink integration.
- Review your CoreLink audit log via /v1/audit/export for the time window above.
- Treat the PAT as compromised; reissue if needed via the support-tool console.

We will provide a written postmortem within 7 days and stand by for a debrief call at your convenience.

— CoreLink Security Lead (<name>)
```

### LGPD ANPD notification draft (T+72:00, if clock started at T+45)
```
Notificação de incidente de segurança — Art. 33 LGPD
Controlador: CoreLink (Humangr Labs)
Data do incidente: 2026-07-22
Hora da detecção: 02:14 UTC
Dados afetados: chaves criptográficas (envelopes); titulares = <N> usuários de 3 tenants enterprise
Riscos: confidencialidade dos envelopes (potencial); integridade preservada; disponibilidade preservada
Medidas adotadas: revogação imediata da credencial; rotação de CMK em curso; notificação aos controladores afetados
Próximos passos: laudo forense em 7 dias; coordenação com encarregados dos controladores afetados
Contato: privacy@<corelink-domain>
```

### GDPR Art. 33 DPA notification (parallel; 72h clock)
```
Subject: GDPR Art. 33 notification — CoreLink security event 2026-07-22
Body: <equivalent structured content, mapping to DPA's preferred template>
```

## Post-incident artefacts list

The scribe ensures the following land in the sealed evidence form §5–§7:

- [ ] Audit-event-type list (every `pat.*`, `byok.*`, `tenant.*` event-type referenced in decisions)
- [ ] PD timeline export placeholder (real incident would attach; tabletop notes "would attach actual export")
- [ ] Status-page text drafts (initial + resolution)
- [ ] Customer email body
- [ ] LGPD + GDPR notification drafts
- [ ] PAT-revocation audit-event signature (CTRL-AUDIT-001)
- [ ] BYOK CMK posture summary (if rotated: pre/post key fingerprints; if not: rationale)
- [ ] Action items into Linear epic `IR-TT-2026-Q3-01`
- [ ] Updates required for `RB-BREACH-NOTIF` decision tree if gap surfaced
- [ ] Updates required for `RB-FM-258-insider-exfil` if gap surfaced

## Success criteria

A successful execution of TT-01 demonstrates:

1. **Severity declared correctly** within 5 min of Inject 1 (SEV-2 acceptable initial, escalated to SEV0 within 15 min when Inject 2 lands).
2. **Containment decision made** with explicit reversibility tag within 30 min.
3. **Customer comms drafted** within 60 min and reviewed by Customer Comms Lead.
4. **Legal notification clock** explicitly named (start time + jurisdiction).
5. **Dual-approval audit chain** traced via real CTRL IDs (CTRL-AUTH-010 + CTRL-AUDIT-003).
6. **Three (3) action items minimum** captured into the Linear epic for runbook / detection / process gaps.

## Failure modes during exercise

If the team:
- Fails to escalate to SEV0 by T+15 → facilitator notes in §7.2 as a major gap → action item to harden severity-declaration training.
- Argues for >30 min on containment choice → facilitator forces a decision at T+30 ("IC, make the call") to preserve clock budget.
- Cannot articulate the LGPD Art. 33 clock-start → action item: Legal Liaison drives a 1-page LGPD quick-reference (link from `RB-BREACH-NOTIF`).
- Skips Customer Comms Lead's seat (e.g., role left empty because no real customer is affected) → facilitator inserts Inject 5 (CISO Slack-Connect) earlier; cannot complete TT-01 without Customer Comms participation.

## Cross-references

- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` §1 (NIST mapping), §5 (execution rules).
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` §2 SEV0 procedure note + §3 SEV1 matrix.
- `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` + `legal/breach-notification/rb-breach-notif-decision-tree.yaml`.
- `specs/05_quality/runbooks/RB-FM-258-insider-exfil.md`.
- `specs/05_quality/runbooks/RB-FM-253-cross-tenant-read.md`.
- `specs/05_quality/runbooks/RB-BYOK-REVOKE.md` (semestral dry-run cadence).
- `specs/03_architecture/failure_modes.md` FM-258 + FM-253 + FM-156.
- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` GAP-03 row.

## Controls evidenced

CC7.3 · CC7.4 · CC7.5 · plus CTRL-PRIV-014, CTRL-PRIV-016, CTRL-PRIV-032 (breach notification), CTRL-AUTH-010 (dual-approval), CTRL-AUDIT-001 + CTRL-AUDIT-003, CTRL-BYOK-* (CMK rotation discipline), CTRL-ISO-001..005 (tenant isolation).

External frameworks: LGPD Art. 33 + GDPR Art. 33/34 + CCPA §1798.82 + ISO/IEC 27035-1 §6–7.
