---
id: "TT-06-DDOS-ABUSE"
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
tags: ["compliance", "ir", "tabletop", "scenario", "sev1", "ddos", "abuse", "rate-limit", "cf-edge", "gap-03", "wt-gap-03"]
---

# TT-06 — DDoS / Abuse Storm (Legitimate-Looking Traffic from 10k IPs in 5 min)

> **Severity:** SEV1 · **Duration:** 90 min · **Quorum:** IC + Scribe + Comms Lead + Tech Lead + Security Lead (5 roles). Customer Comms Lead joins by T+20 if legitimate customer traffic is collaterally rate-limited.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` · **Evidence form:** `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md` · **Schedule:** Q1-2027 (target 2027-03-18).
>
> **FM linkage:** **FM-250** (DDoS volumetric no edge) + **FM-302** (billing leak, downstream if abusers gaming usage-billing) + **FM-400** (retry storm, secondary).
> **CTRL coverage:** CTRL-RATE-001 (rate-limit + back-pressure) + CTRL-NET-001/002/003 (CF WAF + Access + mTLS) + CTRL-ANTI-FRAUD-001 (abuse signals).
> **RB-* invoked:** `RB-FM-101-cf-edge-outage` (adjacent — edge surface) · `RB-FM-400-retry-storm` (related throttling discipline).

## Scenario summary

It is **13:08 UTC on a Friday**. Cloudflare WAF dashboard shows a request-rate spike on `/v1/cas/get` endpoints from baseline 800 RPS to **8,400 RPS** in a 5-minute window. The traffic comes from **~10,000 unique source IPs** across 47 ASNs, with realistic headers (User-Agent strings consistent with legitimate SDK clients, `Accept-Encoding: gzip,br`, plausible referer fields). Each individual IP is making 50–150 requests/min — below the per-IP rate-limit threshold of 200 RPM in `CTRL-RATE-001`.

The traffic is targeting **CAS-GET reads** on **public-readable content** (anonymous read tier, no auth required for `*.public.cas` namespace). Each request is for a different content-address hash, defeating CDN cache (CF cache miss → origin Worker hit → R2 read).

At T+8, two new signals fire:
1. **Origin R2 read-cost dashboard** climbs at 6× baseline — this is becoming an **economic attack**, not just availability.
2. Per-tenant cost-attribution shows the traffic is anonymous (no `X-Tenant-ID` header), so cost falls on CoreLink's bill, not a customer's.

The team must decide:
1. Is this DDoS (intent: deny service) or abuse (intent: free compute / cost attack)?
2. Containment: enable CF "Under Attack Mode" (forces JS challenge on all traffic — degrades legitimate customers); tighten rate limit per-IP (low-hanging); add IP-list / ASN-list block (slow due to scale); enable Bot Fight Mode (CF managed); selective per-endpoint rate limit on `/v1/cas/get`.
3. Communicate to customers (their traffic may be degraded).
4. Customer-billing reconciliation: ensure cost-attribution is correct (this incident must NOT show up on any customer's bill).

## Pre-tabletop preparation

### Facilitator-localised anchors (T-1 week)
- Confirm current CF rate-limit config (`200 RPM per IP per endpoint` baseline assumed; verify against actual).
- Pull baseline `/v1/cas/get` RPS for the prior 7 days (Grafana `cas-get-rps`).
- Confirm `*.public.cas` namespace logic — does the team understand the anonymous-read tier well enough to discuss it under pressure?

### Participants briefed (T-24h Slack post template)
> Tabletop **TT-06** runs **Wed 2027-03-18 14:00 UTC** on Zoom `<link>`. Scenario: DDoS / abuse storm on public-readable CAS-GET endpoints. Roles: IC=`<name>`, Tech Lead=`<name>`, Security Lead=`<name>`, Comms Lead=`<name>`, Scribe=`<name>`. Customer Comms Lead on standby. **Tabletop only.** Bring CF WAF playbook + rate-limit config + R2 cost dashboard.

## Injects (timeline)

### Inject 1 — T+00:00 (Detection)
> *"It is 13:08 UTC, Friday. CF WAF shows request-rate on /v1/cas/get spiked from 800 RPS baseline to 8,400 RPS in the last 5 minutes — 10× spike. Traffic is from ~10,000 unique IPs across 47 ASNs. Per-IP rate is 50–150 RPM, all BELOW the 200 RPM per-IP threshold in CTRL-RATE-001. Headers look legitimate. Each request is for a different content-address hash (defeats CDN cache). All traffic is anonymous (no auth required for *.public.cas namespace). PagerDuty paged SEV-2 on `cas-get-rps-anomaly`. Go."*

**Expected:** IC declares SEV1 (potential cost + availability impact); opens `#inc-2027-03-18-ddos`; pages Security Lead.

### Inject 2 — T+8:00 (Analysis, motive question)
> *"Two new signals: (1) R2 read-cost dashboard is at 6× baseline ($N/min projected over the next hour) — this is now an **economic attack** as much as availability; (2) cost-attribution: all traffic anonymous, so cost falls on CoreLink, not on any customer. Security Lead: is this DDoS (intent: knock us down) or abuse (intent: scrape free compute / racks up our bill)? What signal would distinguish? Tech Lead: what is the per-hour cost trajectory if we do nothing?"*

**Decision point D-1:** Characterise — DDoS vs abuse. Determines containment posture:
- DDoS → "Under Attack Mode" + aggressive blocks acceptable.
- Abuse → narrower per-endpoint rate-limit; preserve legitimate customer experience.

Distinguishing signals:
- Different-hash-per-request pattern → consistent with **abuse** (scraping content), inconsistent with classic DDoS (which would hammer same endpoint with same payload).
- 47 ASNs spread + plausible headers → could be either (botnet for DDoS; or distributed scraping using residential proxies).
- Cost trajectory: ~$2.4k/hour at 8,400 RPS R2 reads → $58k/day if sustained. Hypothesis-shifting signal.

### Inject 3 — T+18:00 (Containment, decision pressure)
> *"You've characterised this as abuse (cost-attack via content scraping). Containment options: (a) CF Under Attack Mode — JS challenge on ALL /v1/cas/get traffic; degrades legitimate customer SDK clients (SDK clients can't solve JS); (b) Tighten per-IP rate limit on /v1/cas/get from 200→50 RPM (lower bar; will catch many attacker IPs but also many legitimate aggressive consumers); (c) Require authentication on /v1/cas/get even for public namespace (breaks the anonymous-read tier — this is a CONTRACT-level change with customers); (d) Per-endpoint CF Bot Fight Mode (CF managed); (e) Cost-cap: set a hard R2 read budget for the public-anonymous tier and serve 429 once exceeded (degraded service, but bounded cost). Choose."*

**Decision point D-2:** Containment choice. Each tradeoff:
- (a) **strongest containment**, breaks SDK clients → customer-visible negative impact.
- (b) **moderate containment**, hits attacker + some legitimate aggressive consumers.
- (c) **strongest long-term** but a **product/contract change** — IC cannot make unilaterally.
- (d) **CF managed**, low team effort, low explainability.
- (e) **bounded cost**, degraded service, but preserves legitimate UX until budget hit.

### Inject 4 — T+38:00 (Customer-impact + comms pressure)
> *"Containment chose (assume the team chose (b) + (d) — moderate path). Result: attacker traffic dropped from 8,400 to 1,200 RPS. Legitimate traffic also dropped from baseline 800 to 620 RPS — about 180 legitimate clients got 429'd in the last 5 min. Two customer Slack pings: 'why are we getting rate limited on public reads?' Comms Lead, draft a status-page note + a short customer DM template."*

**Decision point D-3:** Comms posture — public status page (transparent about cost-attack defense) or quiet remediation (avoid signaling attacker that we're tightening)? Customer Comms Lead must decide for the 2 already-pinging customers.

### Inject 5 — T+58:00 (Eradication + billing reconciliation)
> *"Containment holding at T+58. Attacker traffic continues to probe but is now mostly 429'd at the edge. Eradication: (i) confirm the cost of the incident — $N billed to CoreLink during the window before containment; (ii) ensure no customer was charged for the attacker traffic (the *.public.cas namespace is anonymous → must verify cost-attribution code path); (iii) longer-term: tighten the anonymous-read tier's defaults (per-endpoint rate-limit; per-content-hash cooldown; CDN-cache-bypass detection). Tech Lead: walk us through the cost-attribution verification."*

**Decision point D-4:** Cost-attribution verification — Tech Lead names exact audit-event types (`cas.read.anonymous`, `billing.attribution.tenant_id_null`) + queries to run. Goal: prove via audit chain that no customer-id was attributed.

### Inject 6 — T+78:00 (Lessons + structural change)
> *"Action items to consider: (i) anonymous-read tier needs a hard cost cap by default (option (e) above) — propose making this a product change; (ii) per-content-hash cooldown (5min per hash per IP) — defeats the scraping pattern; (iii) signed-URL gate for *.public.cas — moves to a 'public-but-rate-limited' model; (iv) abuse-signal feedback to PostHog (build a real-time abuse dashboard); (v) CF Workers Bot Management upgrade (paid tier with ML classifier). Rank by impact + effort."*

## Decision points summary

| # | Decision | Owner | NIST phase | Reversibility |
|---|---|---|---|---|
| D-1 | Characterise — DDoS vs abuse | Security Lead + Tech Lead | Detection / Analysis | reversible |
| D-2 | Containment posture (a/b/c/d/e or combo) | IC + Tech Lead + Security Lead | Containment | (c) **product-contract level, requires CTO approval** |
| D-3 | Comms posture (public status page vs quiet) | Comms Lead + IC | Recovery | reversible (text) |
| D-4 | Cost-attribution verification | Tech Lead | Eradication | reversible |

## Comms templates

### Internal Slack post (T+5)
```
@channel — SEV1 incident
Channel: #inc-2027-03-18-ddos
Symptom: /v1/cas/get RPS spike 800→8,400 from 10k IPs / 47 ASNs. Below per-IP threshold. Per-request unique content-hash. Cost trajectory: ~$2.4k/hour.
IC: <name> / Tech Lead: <name> / Security Lead: <name> / Comms Lead: <name> / Scribe: <name>
Hypothesis: distributed content scraping / cost-attack (TBD).
Customer-visible: TBD (containment may collateral-damage legit traffic).
Status page: holding pending containment + collateral-damage assessment.
Update cadence: every 15 min.
```

### Status-page paragraph (T+30, yellow — only if customers complaining)
```
[INVESTIGATING] We are mitigating an unusual traffic pattern on our public content-read endpoints. Some customers may experience rate-limit responses (HTTP 429) on aggressive request rates during the mitigation window. Authenticated customer reads and CAS-PUT writes are unaffected. We will update every 30 minutes.
```

### Customer DM template (T+40, for the 2 already-pinging customers)
```
Hi <name>,

We are actively mitigating a distributed abusive-traffic event on our public CAS endpoints. Out of caution we tightened per-IP rate limits, which briefly caught some legitimate aggressive request patterns. If you are seeing 429s on your SDK clients, please reduce concurrency below 50 requests/min/IP for the next ~30 minutes; we will restore normal limits once the abuse subsides.

Your billing is not affected — the abusive traffic is anonymous and bills to CoreLink, not to any customer account.

— CoreLink SRE Lead (<name>)
```

### Postmortem doc seed (T+85, draft)
```
Title: 2027-03-18 abuse-storm on /v1/cas/get
Severity: SEV1 (cost + availability)
Duration: 13:08 UTC → <resolution>
Root cause: distributed content scraping leveraging per-IP rate-limit bypass via massive IP-set; defeats CDN cache via per-request unique content-hash.
Impact: ~$N in R2 read-cost during the mitigation window; ~180 legitimate clients briefly 429'd during containment.
Containment: tightened per-IP rate limit on /v1/cas/get; enabled CF Bot Fight Mode for the endpoint.
Eradication: per-content-hash cooldown deployed; cost cap proposed for anonymous-read tier.
Customer billing: verified clean (no tenant_id attribution on the attacker traffic; cost fell on CoreLink).
Action items: <listed>
```

## Post-incident artefacts list

- [ ] CF WAF dashboard export (RPS + ASN distribution)
- [ ] R2 read-cost timeline + total bill impact ($)
- [ ] Cost-attribution query output (proving zero tenant_id on attacker traffic)
- [ ] Per-IP rate-limit config diff (before/after)
- [ ] CF Bot Fight Mode enablement audit
- [ ] Customer DMs sent (count + content)
- [ ] Status-page text (if published)
- [ ] Postmortem doc (sealed within 7d)
- [ ] Action items into Linear epic `IR-TT-2027-Q1-02`
- [ ] Product proposal: cost-cap on anonymous-read tier (if (e) became a follow-up)

## Success criteria

1. **SEV1 declared** within 5 min.
2. **DDoS vs abuse characterisation** completed by T+15 with named distinguishing signals.
3. **Containment chosen** with explicit collateral-damage acknowledgement.
4. **Cost-attribution verified** clean (zero tenant_id attribution on attacker traffic).
5. **Customer-comms posture** decided (public vs quiet) with rationale.
6. **At least 3 action items** captured, one of which must address per-content-hash cooldown OR cost-cap on anonymous tier.

## Failure modes during exercise

- Team chooses (a) Under Attack Mode without recognising SDK-client breakage → action item: SDK-client-aware CF rule.
- Team chooses (c) require auth on public namespace without IC checking with CTO / Product → action item: hard-gate product-contract changes on CTO ack in IC playbook.
- Team forgets cost-attribution verification → action item: "cost-attribution audit must be part of any anonymous-tier incident's runbook".
- Team conflates volumetric DDoS with abuse (different containment regimes) → training gap action item.

## Cross-references

- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`.
- `specs/05_quality/runbooks/RB-FM-101-cf-edge-outage.md` (edge surface adjacent).
- `specs/05_quality/runbooks/RB-FM-400-retry-storm.md`.
- `specs/03_architecture/failure_modes.md` FM-250, FM-302, FM-400.

## Controls evidenced

CC7.3 · CC7.4 · CC7.5 · CC6.6 (network controls) · plus CTRL-RATE-001 (rate-limit + back-pressure), CTRL-NET-001 (CF WAF), CTRL-NET-002 (CF Access), CTRL-ANTI-FRAUD-001 (abuse signals).

External frameworks: ISO/IEC 27035-1 §6–7, OWASP top-10 (rate-limit class), CF DDoS managed defenses.
