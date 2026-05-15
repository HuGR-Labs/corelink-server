---
id: "TT-02-CASCADING-FAILURE"
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
tags: ["compliance", "ir", "tabletop", "scenario", "sev1", "cascading", "d1", "replica-lag", "audit-chain", "gap-03", "wt-gap-03"]
---

# TT-02 — SEV1 Cascading Failure (D1 Region-A Read-Only, Replica Lag, Stale Audit Heads)

> **Severity:** SEV1 · **Duration:** 90 min · **Quorum:** IC + Scribe + Comms Lead + Tech Lead (4 roles minimum). Customer Comms Lead joins by T+30 once customer-visible impact confirmed.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` · **Evidence form:** `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md` · **Schedule:** Q3-2026 (target 2026-09-09).
>
> **FM linkage:** **FM-152** (Neon outage / D1 equivalent) + **FM-105** (region replication divergence) + **FM-052** (R2 eventual consistency) + **FM-054** (KV stale > 60s) + **FM-202** (runbook stale, surfaced mid-incident).
> **CTRL coverage:** CTRL-FORMAL-001/002 (reconcile + Merkle proofs) + CTRL-AUDIT-001..003 + CTRL-RATE-001 (back-pressure).
> **RB-* invoked:** `RB-FM-101-cf-edge-outage` (partial — D1, not edge) · `RB-REGION-FAILOVER` · `RB-FM-105-region-replication-diverge` · `RB-SLO-CORRECT-VIOLATION` (audit-chain head divergence triggers).

## Scenario summary

It is **18:42 UTC on a Thursday**. The D1 cluster in **`production-region-a`** transitions to read-only mode. The replication lag dashboard (`d1_replica_lag_seconds`) climbs from a 30-day baseline of <2s to a sustained 45s. Within 6 minutes, two downstream signals fire:

1. `SLO-CORRECT-CAS` burn-rate alert (14× threshold) — the audit-chain reconciler (S-13) is seeing **head divergence**: region-A's claimed audit head lags region-B's by 12 events.
2. Customer-facing API: `audit/head/get` returns **stale data** for ~22% of requests routed to region-A. Three lighthouse customers' dashboards begin showing `last_audit_event_at` clocks that are 30–60s behind real time.

Initial hypothesis from Tech Lead glance: a Cloudflare D1 control-plane operation (per a status page post 4 minutes ago) is causing region-A to drop into a degraded state. **However**, the real root cause (revealed via injects) is more complex: a recently-deployed migration applied 2h earlier is causing replica-side write amplification on a specific table partition; under sustained load the replicas can't keep up and D1 auto-fences region-A into read-only to preserve consistency.

The team must:
1. Stabilise customer experience (route reads to region-B; degrade region-A gracefully).
2. Decide whether to roll back the migration (irreversible if any region-A writes have replicated).
3. Restore audit-chain head consistency (CTRL-FORMAL-001 reconciler must converge before SEV-1 can stand down).
4. Communicate to lighthouse customers whose audit-head dashboards show stale data — these dashboards are part of their compliance evidence.

## Pre-tabletop preparation

### Facilitator-localised anchors (T-1 week)
- Pull the latest D1 migration that was deployed in staging in the prior week as the "recent migration" reference (e.g., `migrations/0045_*.sql` if recently merged) — makes the inject feel real.
- Open Grafana `d1-replica-lag` + `audit-chain-head-divergence` dashboards in tabs.
- Refresh memory on `SLO-CORRECT-CAS` thresholds in `specs/05_quality/runbooks/RB-SLO-CORRECT-VIOLATION.md`.

### Participants briefed (T-24h Slack post template)
> Tabletop **TT-02** runs **Wed 2026-09-09 14:00 UTC** on Zoom `<link>`. Scenario: D1 region-A goes read-only with replica lag spike + customer-facing audit-chain head divergence. Roles: IC=`<name>`, Scribe=`<name>`, Tech Lead=`<name>`, Comms Lead=`<name>`. Customer Comms Lead =`<name>` on standby (paged T+30 if customer impact confirmed). **Tabletop only.** Bring SLO runbooks + FM catalog + region-failover RB.

## Injects (timeline)

### Inject 1 — T+00:00 (Detection)
> *"It is 18:42 UTC. D1 cluster in production-region-a transitioned to read-only 47 seconds ago. Replica lag is 45s and climbing. SLO-CORRECT-CAS burn-rate hit 14× two minutes ago. Customer dashboards (three lighthouse tenants) are showing `last_audit_event_at` 30–60s behind real time. PagerDuty paged you SEV-2 on the SLO burn-rate alert. Go."*

**Expected:** IC declares SEV1 (correctness + customer-visible impact); opens `#inc-2026-09-09-d1-region-a`; pages L2 EM-on-call.

### Inject 2 — T+10:00 (Analysis, hypothesis pivot)
> *"Cloudflare status page just posted: 'D1 control plane experiencing elevated latency in EU-WEST region'. Tech Lead, are we facing an upstream issue or our own bug? What do you check first?"*

**Decision point D-1:** Hypothesis selection — upstream (CF) vs internal (our migration / our load). Tech Lead must articulate the diagnostic: check D1 query-plan stats, recent migration log, write-amplification metrics. The CF status is a **red herring** — the actual issue is internal.

### Inject 3 — T+22:00 (Containment + Recovery decision)
> *"You confirmed: a migration deployed 2h ago (`migrations/0045_audit_chain_partition_index.sql`) adds an index that's causing 8× write amplification on the `audit_chain_events` partition for region-A under current load. Replicas can't keep up. Options: (a) failover reads to region-B, keep region-A in read-only and let it catch up; (b) rollback the migration (requires D1 control-plane write — but region-A is read-only — so we'd rollback on region-B and rely on replication; risk: if region-A has any unreplicated writes, they're lost); (c) reduce write load via CTRL-RATE-001 emergency throttle + wait for replicas to catch up. Choose."*

**Decision point D-2:** Containment / Recovery choice. Each has tradeoffs:
- (a) **lowest risk**, slowest recovery, full customer impact during wait.
- (b) **fastest recovery** if no unreplicated writes — but irreversible data-loss potential.
- (c) **medium risk + medium speed**, but customer-facing latency increases.

### Inject 4 — T+40:00 (Audit-chain consistency complication)
> *"While you were deciding, the audit-chain reconciler (CTRL-FORMAL-001) flagged a Merkle proof mismatch between region-A and region-B audit heads at events index 8,243,117. Region-A claims hash X; region-B claims hash Y. This is a **correctness signal**, not just an availability signal. Tech Lead, what's your move? Comms Lead, does this change customer comms?"*

**Decision point D-3:** Correctness violation handling — this is a `SLO-CORRECT-CAS` zero-budget SEV-1. Tech Lead must reference `RB-SLO-CORRECT-VIOLATION.md` decision tree. Comms Lead must decide whether to upgrade status-page from yellow → orange, and whether to issue a proactive customer notification (lighthouse contracts have a 4h customer-comms SLA on audit-chain anomalies).

### Inject 5 — T+62:00 (Recovery + customer-comms pressure)
> *"It is now 19:44 UTC. Replicas are caught up. Region-A is back to read-write. The Merkle mismatch turned out to be a transient artefact of the reconciler comparing pre-replication state — region-B was 'ahead' because region-A's last 12 events hadn't replicated yet; after replication caught up, the heads converged. Customer dashboards are now consistent. **But**: lighthouse customer 'Acme' just emailed: 'we saw a 90-second gap in our audit dashboard between 18:42 and 18:44 UTC. Per our MSA §7.2 we need a written explanation within 4h.' Recovery acceptance criteria — do you stand SEV-1 down? Customer Comms Lead, what do you send to Acme?"*

**Decision points D-4 / D-5:**
- D-4 (Tech Lead + IC): SEV-1 stand-down criteria — must include (i) replica lag <2s sustained 10 min, (ii) zero burn-rate budget regeneration, (iii) audit-head convergence verified by reconciler, (iv) post-mortem assigned.
- D-5 (Customer Comms Lead): drafts response to Acme's MSA §7.2 request — including the explicit statement that no audit events were lost, only briefly delayed in cross-region replication.

## Decision points summary

| # | Decision | Owner | NIST phase | Reversibility |
|---|---|---|---|---|
| D-1 | Hypothesis (upstream CF vs internal migration) | Tech Lead + IC | Analysis | reversible |
| D-2 | Containment / Recovery choice (failover / rollback / throttle) | IC + Tech Lead | Containment / Recovery | (b) potentially **irreversible** |
| D-3 | Audit-chain Merkle mismatch escalation (correctness path) | Tech Lead + IC | Detection / Eradication | reversible |
| D-4 | SEV-1 stand-down | IC | Recovery / Post-Incident | reversible |
| D-5 | Customer MSA §7.2 written explanation to Acme | Customer Comms Lead + IC | Recovery | reversible |

## Comms templates

### Internal Slack post (T+5)
```
@channel — SEV1 incident
Channel: #inc-2026-09-09-d1-region-a
Symptom: D1 region-a read-only, replica lag 45s, SLO-CORRECT-CAS burn 14×, 3 lighthouse customers see stale audit dashboards.
IC: <name> / Tech Lead: <name> / Scribe: <name> / Comms Lead: <name>
Hypothesis: upstream CF event vs internal migration (under investigation).
Customer-visible: yes (audit dashboard lag); status page: drafting yellow.
Update cadence: every 15 min.
```

### Status-page paragraph (T+18, initial, yellow)
```
[INVESTIGATING] At 18:42 UTC, a subset of API requests routed to our region-a cluster are experiencing elevated latency and may return audit timeline data that is up to 60 seconds behind real time. Customer audit data integrity is preserved; the events are being correctly committed and will appear in dashboards shortly. We are investigating root cause and will update every 30 minutes.
```

### Status-page paragraph (T+62, resolution)
```
[RESOLVED] The latency event affecting region-a is fully resolved as of 19:44 UTC. Replica lag has returned to baseline (<2s). All audit events committed during the incident window are present and consistent across regions. No data was lost. A written postmortem will be published within 7 days.
```

### Lighthouse customer email (T+80, drafted in response to Acme's MSA §7.2)
```
Subject: [POSTMORTEM-PRELIM] CoreLink 2026-09-09 region-a latency event
Body:
Hi Acme team,

This is the preliminary written explanation requested under our MSA §7.2, within the 4h SLA.

Incident window: 18:42–19:44 UTC, 2026-09-09 (62 minutes).
Customer-visible symptom: audit dashboard `last_audit_event_at` clock briefly behind real time (max gap observed: 90 seconds).
Audit data integrity: PRESERVED. No audit events were lost; all events were correctly committed to our append-only ledger and verified by our cross-region Merkle reconciler upon resolution.
Root cause: a database migration deployed 2h prior introduced write amplification on the audit_chain_events table partition for region-a; replicas could not keep up under sustained load, triggering an automatic read-only fence (this is a safety feature, not a fault).
Containment: read traffic was failed over to region-b; write load was reduced via emergency rate limiting; replicas caught up; region-a returned to read-write.
Action items: (1) tighten migration write-amplification pre-flight checks; (2) add migration-impact replica-lag canary; (3) cross-region audit-head reconciler frequency review.

Full postmortem within 7 days. Standing by for a debrief call.

— CoreLink SRE Lead (<name>)
```

## Post-incident artefacts list

- [ ] PD timeline export (placeholder; tabletop notes "would attach")
- [ ] Migration rollback decision rationale (whether (a)/(b)/(c) chosen + why)
- [ ] Audit-chain reconciler convergence log (Merkle hashes pre + post)
- [ ] Status-page text drafts
- [ ] Customer email to Acme (full text)
- [ ] Action items into Linear epic `IR-TT-2026-Q3-02`
- [ ] Updates to `RB-FM-105-region-replication-diverge.md` if gap surfaced
- [ ] Updates to `RB-D1-MIGRATION-APPLY.md` (migration pre-flight checks)
- [ ] Audit-event-type list referenced (`audit.event.append`, `reconciler.merkle.mismatch`, `d1.replica.fence`, etc.)

## Success criteria

1. **SEV1 declared** within 5 min of Inject 1.
2. **Hypothesis pivot** (CF vs internal) completed by T+20 with explicit named diagnostic.
3. **Containment choice** committed by T+30 with reversibility tag.
4. **Audit-chain Merkle mismatch** correctly identified as transient (or escalated to true correctness violation) — Tech Lead references `RB-SLO-CORRECT-VIOLATION.md`.
5. **Customer comms drafted** within 60 min; status-page yellow up by T+20.
6. **SEV-1 stand-down criteria** explicitly named before declaring resolved.
7. **At least 3 action items** captured into Linear.

## Failure modes during exercise

- Team chooses option (b) rollback without checking unreplicated-writes count → facilitator inserts complication: "you rolled back; 4 audit events are now missing from region-A; what next?" — turns into eradication branch with extended timeline.
- Team conflates availability (D1 read-only) with correctness (audit-head divergence) → facilitator pauses and re-reads `SLO-CORRECT-CAS` definition; this is a training gap to capture as action item.
- Comms Lead under-drafts status page (doesn't mention preservation of data integrity) → customer trust impact in real incident; capture as action item.

## Cross-references

- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` §3 SEV1.
- `specs/05_quality/runbooks/RB-SLO-CORRECT-VIOLATION.md`.
- `specs/05_quality/runbooks/RB-FM-105-region-replication-diverge.md`.
- `specs/05_quality/runbooks/RB-REGION-FAILOVER.md`.
- `specs/_runbooks/RB-D1-MIGRATION-APPLY.md`.
- `specs/03_architecture/failure_modes.md` FM-052, FM-105, FM-152, FM-202.

## Controls evidenced

CC7.3 · CC7.4 · CC7.5 · plus CTRL-FORMAL-001 (reconcile job), CTRL-FORMAL-002 (Merkle proofs), CTRL-AUDIT-001..003, CTRL-RATE-001 (back-pressure throttle).
