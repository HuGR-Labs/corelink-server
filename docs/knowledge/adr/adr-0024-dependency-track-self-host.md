---
type: "ADR"
title: "ADR-0024 — Dependency-Track self-host + CVE alerting"
description: "Why continuous CVE matching against shipped SBOMs runs on a self-hosted OWASP Dependency-Track instance with a severity-routed webhook rather than a SaaS scanner."
source_files:
  - "specs/03_architecture/adrs/ADR-0024-dependency-track-self-host.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "supply-chain", "sbom", "cve", "dependency-track", "s12"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0024 — Dependency-Track self-host + CVE alerting

CoreLink needs continuous CVE matching against every shipped SBOM dependency to satisfy SOC 2 CC7.1 and the S-12 supply-chain mandate — not just build-time advisory checks. This ADR records why that capability is met by a self-hosted OWASP Dependency-Track instance feeding a Cloudflare-Worker alert webhook, rather than a per-developer SaaS scanner, and what the operational cost of that choice is.

# Context

`cargo-audit` covers RUSTSEC advisories at build time but misses NVD CVEs published before the RUSTSEC mirror syncs, non-Rust transitive deps in vendored patches, and retroactive matching against historical SBOMs of already-deployed releases (ADR-0024:36-39). Three options were weighed — GitHub Advanced Security, Snyk, and Dependency-Track — on cost, vendor lock-in, self-hostability, and CycloneDX-native support (ADR-0024:30-34).

# Decision

Deploy Dependency-Track v4.11 (digest-pinned; bumped only via a new ADR) on a Docker-Compose VPS backed by Neon Postgres, fronted by Caddy, with a `corelink-dt-webhook` Worker that consumes `NEW_VULNERABILITY` events and routes by CVSS severity (Critical → Slack+Email+PagerDuty SEV-2, down to Low → log-only), HMAC-authenticated via `X-Hub-Signature-256`, with a capped KV dead-letter queue, nightly mock-CVE injection, and an `SLO-SUPPLY-CVE-DETECTION` of p99 ≤ 15 min from detection to alert (ADR-0024:43-62).

# Consequences

The result is continuous CVE matching over historical SBOMs from redundant NVD/OSV/GHSA sources with automated SOC 2 CC7.1 evidence, traded against the operational overhead of running a VPS + Postgres (the Java Quarkus runtime is not CF-Worker-native), mitigated by a DR runbook, Neon PITR, auto-TLS, and the DLQ+reconciliation path for silent-failure detection (ADR-0024:71-78).

# Citations

1. `specs/03_architecture/adrs/ADR-0024-dependency-track-self-host.md:30-39` — option comparison table and the cargo-audit coverage gaps motivating DT (Context).
2. `specs/03_architecture/adrs/ADR-0024-dependency-track-self-host.md:43-62` — DT v4.11 on VPS+Neon, severity-routed HMAC webhook, DLQ, nightly mock-inject, 15-min SLO (Decision).
3. `specs/03_architecture/adrs/ADR-0024-dependency-track-self-host.md:71-78` — positive/negative consequences and mitigations (Consequences).
