---
id: "ADR-S12-047"
type: "adr"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["adr", "s12", "supply-chain", "license-review", "quarterly", "legal", "compliance"]
---

# ADR-S12-047: License Review Quarterly Process

## Status

ACTIVE — WI-S12-004 SEALED.

## Context

Automated license enforcement (cargo-deny + deny.toml) validates SPDX expressions declared in
`Cargo.toml` files.  However, it does not verify that the declared SPDX expression matches the
actual license of the code in the crate (license SPDX drift).  Known drift examples:
- A crate declares `MIT` in Cargo.toml but contains code copied from a GPL-licensed project.
- A crate changes its effective license in a release without updating the SPDX field.

Additionally, the Rust ecosystem occasionally introduces new crates with novel license
expressions that may be business-compatible but not yet in the CoreLink allowlist.

Quarterly review is the cost-effective balance between continuous (too expensive) and annual
(too infrequent) cycles.

## Decision

### Quarterly review cadence

Every 3 months (Q1/Q2/Q3/Q4 calendar), the Compliance Officer + Legal team conduct a
structured review of CoreLink's dependency license posture.

### Review process

1. **Sample 5% of deps** from current `Cargo.lock` (random sample; prioritise new additions
   since last review and deps with > 10k weekly downloads).
2. **Verify SPDX drift**: for each sampled dep, check that the declared SPDX expression in
   `Cargo.toml` matches the actual license file(s) in the crate source (available via
   `cargo metadata --format-version 1` + crates.io API).
3. **Evaluate new license expressions**: if any sampled dep uses an unlisted license that
   could be business-compatible, evaluate for addition to the allowlist (ADR process per
   ADR-S12-046 §"Addition process").
4. **Review unmaintained deps**: `cargo-deny [advisories] unmaintained = "warn"` output from
   the past quarter; create replacement plan for each unmaintained dep (ADR or backlog item).
5. **Document findings**: create `specs/03_architecture/adrs/ADR-S12-XXXX-license-review-YYYYQQ.md`
   with the outcome of the review, decisions made, and any new allowlist additions.
6. **Sign-off**: Compliance Officer + Legal sign-off required to close the review.

### Output artefacts

| Artefact | Description |
|---|---|
| `ADR-S12-XXXX-license-review-YYYYQQ.md` | Quarterly review outcome document |
| `deny.toml` update (if needed) | Allowlist addition with ADR ref in comment |
| `docs/internal/dep-policy.md §2` update | Updated allowlist table |
| Backlog items | Replacement plans for unmaintained deps |

### Calendar trigger

A recurring calendar event "CoreLink License Review Q{N} YYYY" is created in the org calendar
at the start of each year.  The Compliance Officer is the owner; Legal is a required attendee.

## Rationale

### Why quarterly (not continuous or annual)?

- **Continuous**: manual sample audit cost-prohibitive; automated tools cover SPDX expressions.
- **Annual**: too infrequent; a GPL drift could persist 12 months before detection.
- **Quarterly**: balanced cadence; fits SOC 2 CC7.1 vulnerability detection cycle; allows
  timely response to new license expression in ecosystem (e.g., BUSL crates emerging in 2023).

### Why 5% sample (not 100%)?

- 100% audit of 200+ transitive deps per quarter is not cost-effective.
- 5% sample (~10-15 deps) plus prioritisation of new additions provides statistically
  meaningful coverage.
- Automated cargo-deny gate provides continuous 100% SPDX expression enforcement; manual
  review targets the gap (SPDX drift) that automation cannot close.

### Why ADR per quarterly review?

- Provides audit trail for SOC 2 CC7.1 + OWASP ASVS V14.
- Documents decisions (new licenses approved or rejected, unmaintained dep replacements).
- Creates searchable history of CoreLink's license posture evolution.

## Consequences

**Positive**:
- Compliance Officer has a structured, repeatable process.
- SOC 2 CC7.1 evidence trail: quarterly review + cargo-deny CI gate.
- Unmaintained deps tracked and replaced on a regular cadence.

**Negative / mitigated**:
- Requires 4–8 hours/quarter from Compliance Officer + Legal.  Mitigated by: 5% sample
  scope; structured process reduces ramp-up per cycle.
- Manual drift detection is probabilistic (5% sample misses 95% of crates).  Mitigated by:
  automated SPDX enforcement at every PR; drift risk bounded to sampled crates.

## Related

- ADR-S12-045: Dep policy (cargo-audit + cargo-deny + Dependabot).
- ADR-S12-046: License allowlist 7 OSI-approved + banned copyleft.
- `deny.toml [advisories] unmaintained = "warn"` — quarterly review trigger source.
- `docs/internal/dep-policy.md §8` — operational process guide.
- SOC 2 CC7.1: monitoring of system components for vulnerabilities.
- OWASP ASVS V14: configuration and dependency verification.
- NIST SP 800-218 SSDF PW.4: analyse third-party software components.

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | Initial creation — WI-S12-004 SEALED. |
