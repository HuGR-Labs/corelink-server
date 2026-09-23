# TechLead wave plan — bounded Luna fixes

Date: 2026-09-22  
Baseline: `92156bf0e`  
Mode: four disjoint documentation repairs; no shared registry/index/ledger writes.

## Acceptance items

| ID | Item | Baseline |
|---|---|---|
| FX-SERVER | server pilot source/evidence pins and DSR mapping match `origin/main@140e16e` | RED |
| FX-BILLING | billing has a skill and no overstated post-update atomicity or execution evidence | RED |
| FX-CF | cf relation anchors/dependency inventory/procedure states are explicit and atomic | RED |
| FX-E2E | e2e has a skill, accurate tamper coverage, and actionable local recovery | RED |

## Conflict map

All four WPs are `CONFLICT_FREE`: each may write only its package's skill/docs and a uniquely named
evidence file. No agent may modify `STANDARD.md`, `registry.json`, `index.md`, `PUBLICATION-PREFLIGHT`,
the hash ledger, or another package.

## Return shape

`COMMIT <sha>; VERDICT <PASS|FIX_FIRST|BLOCKED>; FILES <paths>; GATE <command/output>; BLOCKERS <max 3>; NEXT <one sentence>`

## Lead gate

The lead will cold-check parent/baseline, diff scope, document checker, exact changed bytes, and stale
claims before accepting any commit. A documentation repair may explicitly preserve `UNKNOWN`,
`REVIEWED_NOT_EXECUTED`, or `BLOCKED`; it may not convert those states into approval.
