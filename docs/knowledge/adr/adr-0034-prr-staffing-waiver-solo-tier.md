---
type: "ADR"
title: "ADR-0034 — PRR staffing waiver path for solo-tier sprints"
description: "Why solo-tier HIGH_RISK sprints can SEAL via three named PRR sign-off waiver paths with an audit trail, while the Crypto SME slot stays non-waivable."
source_files:
  - "specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "prr", "governance", "staffing", "waiver", "solo-tier"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0034 — PRR staffing waiver path for solo-tier sprints

CoreLink's HIGH_RISK sprints require 11 PRR sign-offs, but the program runs solo-tier (one engineer + Architect + contracted Crypto SME), so a strict 11/11 requirement blocks SEAL whenever a reviewer is unavailable and leaves `_TBD_` slots accumulating across sprints. This ADR records the named, audited waiver paths that close that structural defect without silently skipping reviews.

# Context

HIGH_RISK sprints require 11 PRR sign-offs per framework §33.5.4.3, but solo-tier reality means strict 11/11 blocks sprint SEAL when reviewers are unavailable, producing a persistent carry-forward defect of TBD slots (ADR-0034:25-27).

# Decision

Three waiver paths are adopted with explicit boundaries and audit trail: Option A (Architect compensation, capped at 3 per sprint to prevent rubber-stamping, applicable to SRE/Compliance/Privacy/QA/Engineer), Option B (Owner + one named external peer in lieu of two distinct peer reviewers, engineer roles only), and Option C (sprint extension via a new ADR); the Crypto SME slot is NON-WAIVABLE for crypto-load-bearing WIs — if unavailable the sprint extends rather than ships, and AppSec is mandatory-emphatic per the decision matrix (ADR-0034:29-63).

# Consequences

Sprint SEAL becomes achievable in solo-tier reality and the persistent TBD defect closes structurally with every waiver captured explicitly, traded against the risk of Architect compensation becoming a rubber stamp (mitigated by the 3-per-sprint cap) and the Crypto SME non-waivability creating hard sprint-extension risk; a `validate_signoff_calendar.py` CI gate enforces documented waiver paths near the PRR date (ADR-0034:65-81).

# Citations

1. `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md:25-27` — 11 sign-offs block solo-tier SEAL, producing accumulating TBD slots (Context).
2. `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md:29-63` — the three waiver paths and the non-waivable Crypto SME / mandatory AppSec matrix (Decision).
3. `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md:65-81` — structural defect closed vs rubber-stamp/extension risks and the CI gate (Consequences).
