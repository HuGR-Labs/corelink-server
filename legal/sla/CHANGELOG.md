# CoreLink SLA — Changelog

All material changes to the CoreLink Service Level Agreement are tracked here.
Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versioning is SemVer.

Canonical reference: `specs/03_architecture/compliance_matrix.md`.

---

## [1.0.0] — 2026-05-14

### Added

- Initial contractual SLA v1.0.0 published under WI-S20-005.
- Per-tier uptime SLO (Free 99.0% / Starter 99.5% / Pro 99.9% / Enterprise 99.95%).
- Latency target (p99 GET) per tier.
- Freshness target for DSR erasure and billing reconciliation drift.
- Enterprise-only `SLO-INCIDENT-RESPONSE-SYNTHETIC-PAGE` (p99 < 5 min) and
  `SLO-BYOK-KILL-SWITCH` (p99 ≤ 5 min).
- Service-credit table (5% / 10% / 25% / 50% / 100% + termination) with
  per-SLO breakdown across availability, latency, and freshness.
- Force-majeure carve-outs aligned with `PAT-REGION-FAILOVER-001`.
- Scheduled maintenance window (Sun 06:00–08:00 UTC, ≤ 4 h/quarter).
- Claim procedure (30-day window, 15 business day determination SLA).
- Three-consecutive-month termination right and catastrophic-breach
  termination right.
- Governing law (Delaware default) with LGPD / GDPR / Schrems II carve-outs.
- Multi-jurisdictional Legal Counsel sign-off (US + EU + Brazil; tracked in
  `specs/_legal/lighthouse-legal-review-tracker.md`).
- Compliance mapping to SOC 2, ISO 27001, LGPD, GDPR, CCPA, EDPB SCCs.

### Notes

- Aligns with cumulative SLO ratification from S-13..S-19.
- Co-released with DPA v1.0.0 (3 locales) and EU SCC Module 2 reference.
