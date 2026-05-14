---
id: "ADR-S11-007"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s11", "privacy-notice", "semver", "material-change", "minor-change", "ctrl-priv-consent-005"]
---

# ADR-S11-007: Privacy Notice Material vs Minor Change Criteria

## Status

ACTIVE

## Context

GDPR EDPB Guidelines (WP29 WP260rev01 endorsed by EDPB) + LGPD ANPD guidance + CCPA CPPA regulations require that **material changes** to a privacy notice trigger force re-consent from affected data subjects. "Material change" is a legal judgment call. To enforce consistently (CTRL-PRIV-CONSENT-005), CoreLink codifies the criteria as a semver convention.

**Problem:** ad-hoc Privacy Officer judgment leads to inconsistency — a change classified as "minor" today might be classified as "major" tomorrow, causing audit defensibility gaps.

**Solution:** typed criteria table (this ADR). CI hook enforces semver bump is valid. Privacy Officer applies this table as written.

## Decision

### Major Bump (= Material Change → Force Re-consent)

A change MUST be classified as **major** (and MUST trigger force re-consent via WI-S11-003 `stale_consent_check`) when any of the following criteria are met:

| # | Criteria | Example |
|---|---|---|
| M-1 | New data category collected | "We now collect geolocation IP via consent" |
| M-2 | New sub-processor added | "We added Sentry for error tracking" |
| M-3 | New purpose for collecting/processing | "We will use analytics data for AI training" |
| M-4 | Retention period extended | "Audit log retention 7y → 10y" |
| M-5 | DSR SLA extended | "Erasure SLA 30d → 45d" |
| M-6 | Cross-border transfer to new region | "Added enam region; data may transfer EU→US under SCC" |
| M-7 | Legal basis changed for existing purpose | "Switched analytics from consent → legitimate interest" |

### Minor Bump (= Clarification → Silent, No Re-consent)

A change MAY be classified as **minor** (silent; no re-consent) when ALL of the following are true:
- No new data category, sub-processor, purpose, retention period, or SLA.
- No new cross-border transfer region.
- No change to legal basis.

| # | Criteria | Example |
|---|---|---|
| m-1 | Clarification of existing wording | "Clarified that 'analytics' includes error tracking" |
| m-2 | Typo / grammar fix | "Fixed 'colectamos' → 'coletamos' in pt-BR" |
| m-3 | DPO contact update | "DPO email dpo@hugr.dev → privacy@hugr.dev" |
| m-4 | Additional non-primary locale added | "Added pt-PT (does not substitute pt-BR primary)" |
| m-5 | Formatting/structure change (no content change) | "Reorganized sections for readability" |

### Borderline Cases

When a change does not clearly fit major or minor:
1. Privacy Officer reviews against this table.
2. Default: **minor** (defensible posture; classifying ambiguous changes as minor means worst case = a regulatory finding that a "material" change was treated as minor; mitigated by Privacy Officer dual-approval + quarterly audit).
3. If Privacy Officer cannot determine: escalate to Legal counsel for written opinion (EVT-044 annotation).
4. Document the borderline decision in PR description with reference to ADR-S11-007 + Privacy Officer sign-off.

## Rationale

- **Consistency:** typed criteria reduce judgment variance across 3 locales + multiple reviewers.
- **Defensibility:** regulator can review ADR-S11-007 + audit trail to verify classification.
- **CTRL-PRIV-CONSENT-005 alignment:** this ADR is the normative reference for the semver convention used in CI hook `validate_privacy_notice.py`.

## Alternatives Considered

- **Ad-hoc Privacy Officer judgment (rejected):** inconsistency risk; no audit trail of classification rationale.
- **Always major (rejected):** excessive re-consent friction; damages UX + re-consent completion rate metric.
- **External legal firm per-change (rejected):** 14+ day delay per change; incompatible with CI/CD cadence.

## Consequences

- CI hook `validate_privacy_notice.py` references this ADR.
- PR description MUST reference ADR-S11-007 section (M-1..M-7 or m-1..m-5 or borderline) for every notice change.
- Privacy Officer dual-approval REQUIRED for all major bumps.
- Quarterly audit review of all minor bump classifications (catch misclassifications).
