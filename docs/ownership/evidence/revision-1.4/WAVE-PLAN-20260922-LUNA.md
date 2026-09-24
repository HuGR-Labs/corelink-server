# TechLead wave plan — Luna blocker closure

Date: 2026-09-22  
Campaign branch baseline: `6115a38fbfcad4480fe6b7a180a4af7d1761e91f`  
Observed `origin/main`: `140e16eab6315bfdec1a0e4a9892d8557a071781`

## Go / no-go

**GO: parallel, read-only first-pass reviews.** The work is disjoint because agents write no
campaign files, no issue bodies, no registry/index, and no shared ledger. They return compact verdict
cards only. The lead records accepted cards serially after cold-checking the evidence.

No contract freeze is needed: every WP inspects an existing immutable artifact set and has no
producer/consumer write dependency on another WP. The changing remote main is isolated to the
main-drift census WP and is not used as an excuse to rewrite pilot docs.

## Acceptance spine (currently RED)

| ID | Acceptance item | Baseline status |
|---|---|---|
| AC-STD | Current `STANDARD.md` has an independent cold verdict with G0–G3 closed or explicitly blocked | RED |
| AC-HASH | Current `corelink-hash` four-byte set has one fresh independent verdict per artifact and 42/42 relations reconciled | RED |
| AC-BILLING | Current `corelink-billing` four-byte set has independent verdicts and semantic H-profile evidence | RED |
| AC-CF | Current `corelink-cf-bindings` four-byte set has independent verdicts and atomic relation evidence | RED |
| AC-SERVER | Current `corelink-server` four-byte set is reconciled against current main and composition-root evidence | RED |
| AC-E2E | Current `e2e-billing-flow` four-byte set has independent verdicts for ordering/refund/recovery claims | RED |
| AC-POP | Current-main Cargo census and 105-package reconciliation has no unclassified tracked manifest | RED |
| AC-DEDUPE | Remaining package rows have explicit semantic alias/backlog decisions with evidence | RED |
| AC-AUTH | Owner/peer review and approval routes are evidenced without fabricated authority | RED |

The acceptance items are review-gated and cannot be auto-green from agent self-report. A card is
eligible for lead verification only when it contains exact artifact hashes, scope, verdict, blockers,
and evidence paths.

## Work-package table

| WP | Owner scope | Model | Writes | Depends on |
|---|---|---|---|---|
| LUNA-STD | `docs/ownership/STANDARD.md` + current standard review evidence | gpt-6-luna | none | — |
| LUNA-HASH | `docs/ownership/crates/corelink-hash/**` | gpt-6-luna | none | — |
| LUNA-BILLING | `docs/ownership/crates/corelink-billing/**` | gpt-6-luna | none | — |
| LUNA-CF | `docs/ownership/crates/corelink-cf-bindings/**` | gpt-6-luna | none | — |
| LUNA-SERVER | `docs/ownership/crates/corelink-server/**` | gpt-6-luna | none | — |
| LUNA-E2E | `docs/ownership/crates/e2e-billing-flow/**` | gpt-6-luna | none | — |
| LUNA-POP | tracked `Cargo.toml` census + current-main delta | gpt-6-luna | none | — |
| LUNA-DEDUPE | bounded remaining backlog rows, read-only | gpt-6-luna | none | — |
| LUNA-AUTH | CODEOWNERS/review routes + existing review evidence | gpt-6-luna | none | — |

Conflict map: all pairs `CONFLICT_FREE` (read-only scopes and compact return cards). The lead is the
only writer of the wave ledger, standard freeze record, registry, issue ledger, or package docs.

## Return firewall

Each agent must return exactly:

`VERDICT <APPROVE|FIX_FIRST|REJECT|BLOCKED>; SCOPE <paths>; SHA <hashes>; EVIDENCE <paths>; BLOCKERS <max 5>; NEXT <one sentence>`

No transcript dump, no issue publication, no production execution, no branch creation, and no edits to
shared campaign files.

## Lead done gate

1. Verify each reported SHA and path from the lead checkout.
2. Re-run the applicable documentary checker/census cold.
3. Reject any card that relies on a changed byte, stale baseline, inferred runtime reachability, or
   self-approval.
4. Record only verified cards in the next evidence ledger; unresolved cards remain BLOCKED.
5. Do not freeze or publish until AC-STD, AC-POP, AC-DEDUPE, and the four-pilot review matrix are
   independently satisfied.
