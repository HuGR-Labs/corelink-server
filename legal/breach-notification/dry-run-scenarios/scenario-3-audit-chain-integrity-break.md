---
id: "DRY-RUN-SCENARIO-003"
type: "dry_run_scenario"
version: "1.0.0"
created: "2026-05-13"
owner: "Privacy Officer (Gustavo Schneiter interim)"
scenario_name: "Audit Chain Integrity Break (Near-Miss)"
expected_severity: "SEV-3"
expected_jurisdictions: []
time_to_decision_target_hours: 4
---

# Dry-Run Scenario 3: Audit Chain Integrity Break (Near-Miss)

> **Cadence**: Optional / ad-hoc; useful for onboarding new Security Lead or Compliance hire
> **Participants**: Privacy Officer + Security Lead + Compliance

## 1. Scenario Description

The daily audit chain verifier job (INV-OBS-AUDIT-CHAIN-INTEGRITY + INV-AUDIT-APPEND-ONLY)
fires a SEV-1 alert: it detects a gap in the audit event hash chain. Chain links
`seq_1234` through `seq_1237` have a valid `prev_hash` linkage broken at `seq_1235`.

Investigation reveals: this was caused by a failed R2 Object Lock PutObject during
a brief R2 outage. The events were queued in-memory and re-emitted post-recovery,
but the sequence numbers were re-assigned non-atomically, causing a hash chain gap.

**No PII was exfiltrated**. The data involved is exclusively internal audit metadata
(event types, timestamps, tenant IDs in hashed form). The root cause is an infrastructure
failure, not a security attack.

This is a **near-miss** for INV-AUDIT-APPEND-ONLY (CRITICAL invariant) but does NOT
constitute a personal data breach under GDPR Art. 4(12) because:
1. No personal data was disclosed or accessed.
2. No data was lost — the audit events were all eventually written.
3. The gap was in hash chain ordering, not in data availability.

## 2. Simulation Timeline

| T | Event | Participant |
|---|---|---|
| T+0 | Daily audit chain verifier alert fires | Monitoring |
| T+15m | SRE on-call investigates | SRE |
| T+1h | Root cause confirmed: R2 outage + non-atomic sequence re-assignment | Security Lead |
| T+1h30m | Privacy Officer consulted: is this a personal data breach? | Privacy Officer |
| T+2h | Legal consulted (brief): confirm no regulatory notification required | Legal |
| T+3h | Decision tree consultation (participants exercise RB-BREACH-NOTIF) | All |
| T+4h | Target: decision finalized + post-mortem initiated | Privacy Officer |

## 3. Decision Tree Exercise

**Step 1 — Is this a personal data breach?** (GDPR Art. 4(12) + LGPD Art. 46 + CCPA §1798.82(h)):
- Was personal data accessed by unauthorized parties? NO.
- Was personal data lost or destroyed? NO (all events eventually written).
- Was personal data's availability compromised > 24h? NO (gap was transient during outage).
- Was personal data's integrity compromised? The *hash chain* integrity was compromised,
  but the underlying data content is intact. This is an infrastructure/evidence-trail issue,
  not a personal data breach.
- → **Expected result: NOT a personal data breach under GDPR/LGPD/CCPA**.
  → **Expected severity: SEV-3** (near-miss; regulatory notification NOT required per decision tree;
  internal post-mortem only).

**Step 2 — Notifications required**:
- Regulatory: NONE (per decision tree: `notify: []` for SEV-3 + Privacy Officer judgment).
- Customer: NONE (no PII exposure).
- Internal: Post-mortem required (< 14d window; Privacy Officer + Security Lead lead).

**Step 3 — Remediation steps** (exercise for participants):
- Fix: atomic sequence number assignment in R2 audit emit retry path.
- Fix: R2 Object Lock PutObject failure must fail-CLOSED (no silent re-sequence).
- Fix: Add integration test for R2 outage → chain gap scenario.
- Document: ADR or post-mortem for hash chain gap handling.

**Step 4 — Cross-reference**:
- Participants should note: FM-061 (audit Object Lock vs DSR — legal hold path).
- Reference RB-FM-* for infrastructure-level remediation.
- Note that `breach.notification_dispatched.v1` is NOT emitted (no dispatch occurred).

## 4. Key Learning Objectives

This scenario exercises:
1. **Not every INV violation = regulatory notification** — participants must distinguish
   infrastructure failures from personal data breaches.
2. **Audit fail ordering matters** — the root cause here is a failure in INV-AUDIT-APPEND-ONLY
   enforcement that would be caught BEFORE state mutation in the correct implementation.
3. **SEV-3 Privacy Officer judgment** — the decision tree has `notify: []` for SEV-3,
   but participants should understand this is a judgment call (Privacy Officer can escalate).
4. **Post-mortem vs notification** — SEV-3 triggers post-mortem without regulatory notification.

## 5. Pass Criteria

| Criterion | Target | Actual (fill) |
|---|---|---|
| Correctly classified as NOT a personal data breach | YES | *(fill)* |
| Correctly classified as SEV-3 (near-miss) | YES | *(fill)* |
| Correctly determined: no regulatory notification | YES | *(fill)* |
| Post-mortem initiated within 14d | YES | *(fill)* |
| INV-AUDIT-APPEND-ONLY remediation action items filed | YES | *(fill)* |
| EVT-017 evidence archived (optional for SEV-3) | Optional | *(fill)* |

## 6. Known Complexities (for facilitator)

- Participants may confuse "audit chain integrity break" with "breach" — this is
  intentional to test judgment.
- The GDPR definition of breach (Art. 4(12)) includes "loss of availability" — participants
  should be able to argue that transient availability loss of the audit chain does NOT
  meet this threshold (the personal data itself was not unavailable; only its audit trail).
- Post-mortem vs incident response: participants should understand the 14d post-mortem
  window and the difference between a post-mortem (internal) and a breach notification (regulatory).
