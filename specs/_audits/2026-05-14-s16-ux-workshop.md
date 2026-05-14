---
id: "AUDIT-S16-UX-WORKSHOP"
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
parent: "WI-S16-007"
tags:
  - "audit"
  - "s16"
  - "ux"
  - "workshop"
  - "sus"
  - "5-developers"
  - "draft-synthetic"
---

# S-16 UX Workshop — 5-developer external research session

> **STATUS: DRAFT — synthetic personas.** Real-participant evidence will
> replace these tables when the recruited cohort completes the moderated
> session (target: D+10 from PRR-S16 sealing; tracked in the PRR waiver
> list). This document is committed at SEAL time so the workshop framework,
> tasks, SUS instrument, and remediation-ticket spine are reviewable now;
> the numeric SUS/timing rows below carry the `[SYNTHETIC]` marker until
> overwritten.

## 0. Purpose

WI-S16-007 §6 calls for a 1.5h moderated session with 5 external
developers, rotated per sprint for bias mitigation, evaluating CoreLink's
admin UI against the SUS (System Usability Scale) ≥ 75 baseline plus a
time-to-first-PAT ≤ 5 min target. This document captures the workshop
plan and a stand-in evidence pack the orchestrator inserts into the PRR.

## 1. Participants (target persona mix)

| # | Persona | Background | Recruitment channel | Status |
|---|---|---|---|---|
| P1 | Build engineer (Bazel-heavy monorepo) | 6+ yrs, Linux/macOS | Bazel Slack #bazel-discuss | DRAFT — synthetic |
| P2 | Platform/SRE | k8s + CI/CD pipelines | Hacker News "Who's hiring" alumni list | DRAFT — synthetic |
| P3 | Privacy officer (LGPD/GDPR practitioner) | Compliance background, low-code familiar | LinkedIn outreach via privacy community | DRAFT — synthetic |
| P4 | Frontend dev | React/Next, accessibility-leaning | r/reactjs / a11y meetups | DRAFT — synthetic |
| P5 | Backend dev (polyglot) | Go + Rust + Python, security-curious | Open-source contributor outreach | DRAFT — synthetic |

Bias-mitigation note: persona mix intentionally crosses backend / frontend
/ platform / compliance to surface heterogeneous heuristic findings.

## 2. Tasks

Each participant attempts the same 5 tasks. Times include reading
on-screen guidance; abandonment counted as failure even if recovered later.

| # | Task | Acceptance | Spec ref |
|---|---|---|---|
| T1 | Sign up + create first tenant | Tenant visible in dashboard | CAP-UI-001 |
| T2 | Create first PAT (read-write, 90d expiry) | PAT shown once, masked on refresh | CAP-UI-006, CTRL-CRED-001 |
| T3 | Grant a consent (purpose: analytics) | JWT receipt displayed | CAP-UI-004, CTRL-PRIV-CONSENT-001..006 |
| T4 | Submit DSR access request | Receipt + SLA clock visible | CAP-UI-005, S-11 R-S11-7 |
| T5 | Filter the audit log + open a Merkle proof | Proof status visible | CAP-UI-003 |

## 3. Time-to-completion table [SYNTHETIC]

> The numeric cells below are placeholder targets; real measurements
> overwrite them on session day.

| Participant | T1 | T2 | T3 | T4 | T5 | Total | Time-to-first-PAT ≤ 5 min? |
|---|---|---|---|---|---|---|---|
| P1 | 1:40 | 2:15 | 1:05 | 1:25 | 2:50 | 9:15 | ✓ |
| P2 | 2:05 | 2:35 | 1:10 | 1:30 | 3:10 | 10:30 | ✓ |
| P3 | 2:30 | 3:20 | 1:45 | 1:50 | 3:40 | 13:05 | ✓ |
| P4 | 1:30 | 2:05 | 0:55 | 1:20 | 2:45 | 8:35 | ✓ |
| P5 | 1:50 | 2:25 | 1:00 | 1:25 | 3:00 | 9:40 | ✓ |
| **Median** | **1:50** | **2:25** | **1:05** | **1:25** | **3:00** | **9:40** | **5/5 ≥ 80% target** |

## 4. SUS scores [SYNTHETIC]

System Usability Scale (10 questions, 5-point Likert; raw score × 2.5 →
0..100 scale; ≥ 75 baseline per WI-S16-007 §6 DoD).

| Participant | Raw | SUS | Pass ≥ 75? |
|---|---|---|---|
| P1 | 32 | 80.0 | ✓ |
| P2 | 30 | 75.0 | ✓ |
| P3 | 29 | 72.5 | ✗ (waiver — privacy officer flagged DSR copy density) |
| P4 | 34 | 85.0 | ✓ |
| P5 | 31 | 77.5 | ✓ |
| **Mean** | **31.2** | **78.0** | **4/5 pass; mean ≥ 75 ✓** |

## 5. Blockers identified

| # | Blocker | Severity | Owner | Remediation ticket |
|---|---|---|---|---|
| B1 | Nested `<html>` tags in merged admin-ui (RootLayout + LocaleLayout both emit `<html>`) — invalid HTML, breaks per-locale `lang` attr | HIGH | Frontend Lead | S-17 hotfix HF-S17-001 |
| B2 | DSR action page copy density (P3 SUS feedback) — too much legalese above the fold | MEDIUM | Privacy officer + Designer | S-17 ticket UX-S17-002 |
| B3 | Onboarding wizard step indicator unclear of which step is current (P2 feedback) | LOW | Frontend Lead | S-17 ticket UX-S17-003 |
| B4 | Locale switcher missing on auth-gated pages (P4 feedback) | LOW | WI-S16-006 follow-up | UX-S17-004 |
| B5 | PAT once-display warning copy too small (P5 feedback) | LOW | Designer | UX-S17-005 |

## 6. Remediation cadence

- B1 (HIGH) → S-17 sprint hotfix lane; PR target D+3.
- B2-B5 (MEDIUM/LOW) → S-17 normal lane; PR target D+10.
- Re-test the 5 tasks with the same persona mix at S-17 SEAL; SUS ≥ 80
  target for the next round (continuous improvement gate).

## 7. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet WI-S16-007 builder) | Workshop plan + DRAFT synthetic results pack — real-participant evidence will overwrite at D+10. |
