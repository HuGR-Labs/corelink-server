---
id: "ADR-0037"
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
tags: ["adr", "s12", "supply-chain", "dependency-track", "cve-alerts", "self-host", "neon-postgres", "webhook", "high-risk"]
---

# ADR-0037 — Dependency-Track Self-Host Architecture + CVE Alerting

## Status

ACTIVE

## Context

CoreLink requires **continuous CVE matching** against all shipped SBOM dependencies
to satisfy SOC 2 CC7.1 and the sprint S-12 supply chain hardening mandate.

Three options were evaluated:

| Option | Cost | Vendor lock-in | Self-hostable | CycloneDX native |
|--------|------|----------------|---------------|-----------------|
| GitHub Advanced Security | $~19/dev/mo | High (GitHub) | No | Partial |
| Snyk | $~25/dev/mo | High | No | Yes |
| **Dependency-Track (OWASP)** | **~$15/mo infra** | **None (OSS)** | **Yes** | **Yes** |

`cargo-audit` (WI-S12-004) covers RUSTSEC advisories at build time but misses:
1. CVEs published in NVD before RUSTSEC mirror sync.
2. Non-Rust transitive deps in vendored patches.
3. Retroactive CVE matching against historical SBOMs of deployed releases.

## Decision

Deploy **Dependency-Track v4.11** (pinned; bump via ADR) on:

- **Backend runtime**: Docker Compose on a Linode/Hetzner VPS (~$10/mo),
  connected to Neon Postgres small tier (~$5/mo, PITR enabled, HA replica).
- **Frontend SPA**: served via Caddy reverse proxy (Let's Encrypt TLS + HSTS).
- **CVE alert webhook**: `corelink-dt-webhook` Cloudflare Worker consumes
  DT `NEW_VULNERABILITY` events; routes by CVSS severity:
  - Critical (≥ 9.0): Slack + Email + PagerDuty SEV-2.
  - High (≥ 7.0): Slack + Email.
  - Medium (≥ 4.0): Slack only.
  - Low/Info: log only.
- **HMAC authentication**: `X-Hub-Signature-256` header required on every
  webhook event; shared secret rotated quarterly (CTRL-AUTH-014).
- **Dead-letter queue**: CF KV `dt:webhook:dlq:<event_id>`; cap 1 000 events;
  SEV-2 alert on overflow; daily reconciliation via `corelink-dt-reconcile`.
- **Mock CVE injection nightly**: `corelink-dt-cli inject-mock` validates the
  E2E alert path daily; SEV-3 alert if nightly test fails.
- **SLO-SUPPLY-CVE-DETECTION**: p99 ≤ 15 min from CVE detection to alert delivery,
  sustained 30d staging.

## Rationale

- **Self-hostability**: data residency + cost control; no vendor lock-in.
- **CycloneDX native**: aligned with WI-S12-002 SBOM generation.
- **Maturity**: 5+ years production at Goldman Sachs, Toyota, and others.
- **Cost**: ~$15/mo vs $25+/dev/mo for SaaS alternatives.
- **Redundant CVE matching**: NVD + OSV + GHSA sources + OSS Index manual fallback.

## Consequences

- **Positive**: continuous CVE matching covering historical SBOMs + NVD/OSV/GHSA;
  SOC 2 CC7.1 evidence automated; DT dashboard for SecOps team.
- **Negative**: operational overhead (VPS + Postgres management; quarterly DR test);
  Java Quarkus runtime not CF Worker-native (VPS required).
- **Mitigations**: DR runbook `docs/internal/dt-dr-runbook.md`; Neon PITR;
  Caddy auto-TLS; DLQ + reconciliation for silent failure detection.

## CVE Alert Thresholds (documented per hard constraint)

| Severity | CVSS range | Channels | SLA |
|----------|------------|----------|-----|
| Critical | 9.0–10.0 | Slack + Email + PagerDuty SEV-2 | ≤ 15 min p99 |
| High | 7.0–8.9 | Slack + Email | ≤ 15 min p99 |
| Medium | 4.0–6.9 | Slack only | Best-effort |
| Low | 0.1–3.9 | Log only | N/A |
| Info | 0.0 | Log only | N/A |

## DT Version Pin Policy

- DT version `4.11.x` pinned via Docker image digest in `infra/dependency-track/docker-compose.yml`.
- Bump to `4.12+` requires: (1) new ADR ratification, (2) staging upgrade test, (3) SRE sign-off.
- Image digests refreshed quarterly or on DT security advisory.

## Related

- WI-S12-005: `specs/04_sprints/S12/work_items/WI-S12-005-dependency-track-self-host-cve-alerts.md`
- DR runbook: `docs/internal/dt-dr-runbook.md`
- SLO: `SLO-SUPPLY-CVE-DETECTION` (corelink_supply_dt_alert_delivery_duration_seconds p99 ≤ 900s)

## Change Log

| Version | Date | Author | Change |
|---------|------|--------|--------|
| 1.0.0 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | Initial ADR (WI-S12-005 SEALED). |
