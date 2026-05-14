---
id: "ADR-S11-006"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s11", "consent", "purpose-limitation", "enum-discipline"]
---

# ADR-S11-006 — 12-Arm Closed `ConsentPurpose` Enum Discipline

## Status

Accepted — 2026-05-13.

## Context

GDPR Art. 5(1)(b) + LGPD Art. 6 II: purpose limitation. Each consent
must be tied to a specific, explicit, legitimate purpose. The
implementation question: **how do we enforce specificity at the type
level?**

Naive: `ConsentPurpose(String)` allows arbitrary purposes; vulnerable
to scope creep ("marketing" → "marketing + analytics + retargeting +
...").

Open enum (`#[non_exhaustive]`): allows future additions but doesn't
enforce that ALL purposes used in code are pre-registered.

## Decision

**12 canonical purposes, closed enum** (privacy_model.md §5.6.1):

```rust
pub enum ConsentPurpose {
    // Core service (4)
    ServiceDelivery,
    SecurityIncidentResponse,
    BillingFiscalCompliance,
    LegalObligation,

    // Product analytics (3)
    UsageAnalytics,
    PerformanceTelemetry,
    ProductImprovementResearch,

    // Optional (3)
    MarketingEmail,
    PersonalizedRecommendations,
    ThirdPartyIntegrations,

    // Anomaly / abuse (2)
    AbuseDetectionML,
    AccountRecoveryAssistance,
}
```

NO `#[non_exhaustive]`. Adding a purpose REQUIRES a spec change +
this ADR update + DPIA refresh.

`LegalBasis` mapping per purpose is **fixed at compile time** (no
runtime swap). E.g., `ServiceDelivery → Contract`, `MarketingEmail →
Consent`. The `legal_basis_for(purpose)` function is `const`.

## Rationale

- **Closed enum** = pattern-match exhaustiveness at the compiler level;
  adding a purpose is a deliberate cross-functional decision (engineer
  + Privacy Officer + Legal sign-off).
- **Fixed legal_basis mapping** = forbids the fail-open swap pattern
  where `MarketingEmail` could silently become `LegitimateInterest`
  when consent is revoked (GPT P0-1 round-1 finding).
- **12 is a cap, not a target** = privacy_model.md §5.6.1 documents
  why each purpose is necessary and distinct. Adding a 13th forces a
  PRR.
- **Aligned with WP29 Op. 03/2013** ("granularity of consent"):
  fine-grained but not infinite.

## Consequences

**Positive**: regulator-defensible; compile-time guarantee that no code
path uses an unregistered purpose; DPIA per purpose is tractable (12
sections, not unbounded).

**Negative**: requesting a new purpose has CR overhead. Mitigated by
treating purpose additions as quarterly batch reviews.

**Forbidden**: `#[non_exhaustive]` on `ConsentPurpose`. Forbidden:
runtime swap of `LegalBasis` for a purpose. Forbidden: purpose strings
in DB / API.

## References

- WI-S11-003 §6 ConsentPurpose enum
- privacy_model.md §5.6.1 purpose taxonomy
- GDPR Art. 5(1)(b), LGPD Art. 6 II
- WP29 Op. 03/2013 (Purpose Limitation)
- WP29 Op. 06/2014 (Legitimate Interest)
- ADR-S11-005 (symmetric schema; this ADR's sibling)
