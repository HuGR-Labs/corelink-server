---
type: "Runbook"
title: "Engineering onboarding & the buddy protocol"
description: "The post-GA new-engineer ramp — offer to first-PR in ≤5 days, independent in ≤30 — and the senior 'buddy' role that compresses the jargon gap."
source_files:
  - "docs/internal/ENGINEERING-ONBOARDING.md"
  - "docs/internal/onboarding/BUDDY-PROTOCOL.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "onboarding", "buddy", "process", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Engineering onboarding & the buddy protocol

A 262-doc spec corpus and ~96 crates make CoreLink intimidating on Day 1, so onboarding is an explicit
runbook: a day-by-day path from signed offer to *first PR merged in ≤ 5 working days* and *independent
productivity in ≤ 30 days*, paired with an assigned senior "buddy" whose whole job is to compress the
"who do I ask about X" gap. This concept captures both halves — the new-hire timeline (access, first
build, architecture deep-dive, request trace, first PR) and the buddy's cadence, translation duties,
and the boundaries of what a buddy is NOT. Related: [the cross-team tech-lead handoff runbook](/ops/cross-tl-handoff.md).

# Role

It is the people-process runbook for ramping a new backend/platform/SRE/security engineer onto
CoreLink with a defined timeline and a defined support role, so neither the ramp nor the mentorship is
ad hoc.

# How it works

- The goal is explicit: first PR merged ≤ 5 days, independent productivity ≤ 30 days `docs/internal/ENGINEERING-ONBOARDING.md:6-6`.
- Day 0 (manager-owned) provisions laptop, access requests, and ≤ 3 pre-reads `docs/internal/ENGINEERING-ONBOARDING.md:51-98`.
- Day 1 runs the first workspace build + spec-validator smoke `docs/internal/ENGINEERING-ONBOARDING.md:114-141`.
- Day 2 walks the four canonical architecture docs in a fixed order `docs/internal/ENGINEERING-ONBOARDING.md:145-185`.
- Day 3 traces one request end-to-end crate-by-crate (signup → tier-select → Stripe → audit) `docs/internal/ENGINEERING-ONBOARDING.md:219-237`.
- Days 4–5 ship one curated first-PR item paired with a senior `docs/internal/ENGINEERING-ONBOARDING.md:239-266`.
- The buddy cadence tapers daily → weekly over Weeks 1–4 (~10–12h total) `docs/internal/onboarding/BUDDY-PROTOCOL.md:22-31`.
- The buddy's always-on duty is translating internal shorthand (R-charter, SEAL, InMemoryFake, P0/P1) `docs/internal/onboarding/BUDDY-PROTOCOL.md:72-89`.

# Invariants

- Every PR (including the first) must pass build, test, clippy `-D warnings`, `cargo deny`, `validate_specs.py`, and pre-commit `docs/internal/ENGINEERING-ONBOARDING.md:253-258`.
- New-hire branches use `<handle>/<short-desc>`; the `wt/` prefix is reserved for orchestrator worktrees `docs/internal/ENGINEERING-ONBOARDING.md:250-252`.
- The buddy is explicitly NOT the reviewer-of-record, manager, domain mentor, or on-call backup `docs/internal/onboarding/BUDDY-PROTOCOL.md:92-99`.
- The Day-30 buddy→manager note is manager-prep only; the new hire never sees it directly `docs/internal/onboarding/BUDDY-PROTOCOL.md:122-125`.

# Gotchas

- New hires are explicitly told NOT to over-prepare: anything beyond the 3 pre-reads before Day 1 is over-reading `docs/internal/ENGINEERING-ONBOARDING.md:90-98`.
- Most crates are still `InMemoryFake` by charter — read the trait, not the placeholder body; this is intentional, not a bug `docs/internal/ENGINEERING-ONBOARDING.md:357-360`.

# Citations

1. `docs/internal/ENGINEERING-ONBOARDING.md:6-6` — the ≤5d / ≤30d ramp goal.
2. `docs/internal/ENGINEERING-ONBOARDING.md:51-98` — Day 0 access + pre-reads.
3. `docs/internal/ENGINEERING-ONBOARDING.md:90-98` — the don't-over-prepare rule.
4. `docs/internal/ENGINEERING-ONBOARDING.md:114-141` — Day 1 first build + validator smoke.
5. `docs/internal/ENGINEERING-ONBOARDING.md:145-185` — Day 2 four-doc architecture deep-dive.
6. `docs/internal/ENGINEERING-ONBOARDING.md:219-237` — Day 3 end-to-end request trace.
7. `docs/internal/ENGINEERING-ONBOARDING.md:239-266` — Days 4–5 first-PR flow.
8. `docs/internal/ENGINEERING-ONBOARDING.md:250-252` — branch-naming convention.
9. `docs/internal/ENGINEERING-ONBOARDING.md:253-258` — the mandatory per-PR gate set.
10. `docs/internal/ENGINEERING-ONBOARDING.md:357-360` — the InMemoryFake charter convention.
11. `docs/internal/onboarding/BUDDY-PROTOCOL.md:22-31` — buddy cadence + time commitment.
12. `docs/internal/onboarding/BUDDY-PROTOCOL.md:72-89` — buddy translation duties.
13. `docs/internal/onboarding/BUDDY-PROTOCOL.md:92-99` — what the buddy is NOT.
14. `docs/internal/onboarding/BUDDY-PROTOCOL.md:122-125` — Day-30 note is manager-prep only.
