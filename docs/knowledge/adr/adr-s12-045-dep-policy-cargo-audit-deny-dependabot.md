---
type: "ADR"
title: "ADR-S12-045 — Dep Policy: cargo-audit + cargo-deny + Dependabot Canonical Config"
description: "The three composited supply-chain controls — cargo-audit for RUSTSEC advisories, cargo-deny for policy enforcement, and staggered Dependabot for automated updates — with pinned versions and prebuilt installs."
source_files:
  - "specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s12", "supply-chain", "cargo-audit", "cargo-deny", "dependabot"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S12-045 — Dep Policy: cargo-audit + cargo-deny + Dependabot Canonical Config

CoreLink's HIGH_RISK supply-chain lane needs three things no single tool delivers: RUSTSEC advisory detection within 24h, policy enforcement (license/yanked/sources/advisories) at every merge, and bounded automated dep updates. This ADR composites cargo-audit + cargo-deny + Dependabot into the canonical configuration, with pinned tool versions, SHA-pinned actions, and prebuilt-binary installs that removed the os-error-2 from-source compile race on the contended self-hosted Mac.

# Context

The supply-chain lane (FF-HR-005) requires three composited controls — RUSTSEC CVE detection, policy enforcement at every PR, and automated weekly dep updates — and no single tool covers all three axes. The Rust ecosystem supplies cargo-audit (advisory scanner), cargo-deny (policy enforcement), and Dependabot (update PRs) (`specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:23-37`).

# Decision

**cargo-audit**: `--deny warnings` PR gate + a daily 06:00 UTC cron classifying HIGH/CRITICAL to SEV-2/3, pinned at `0.22.x`, installed via the SHA-pinned `taiki-e/install-action` prebuilt binary. **cargo-deny**: PR gate + offset daily cron, policy from a root `deny.toml`, pinned at `0.19.x`, native install (the Linux-only EmbarkStudios Docker action was retired for the macOS fleet). **Dependabot**: weekly-per-ecosystem but staggered Mon–Thu (the original single-Monday batch near-OOM'd the runner fleet), groups for security/minor-patch/major, auto-merge only patch + minor non-security on green CI, never major or security, 10-PR cap. A lockfile-diff PR comment is mandatory on every `Cargo.lock` change (`specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:39-82`). Rationale covers why two tools not one, why SHA-pin all actions (mutable-ref supply-chain attack), why weekly not daily (reviewer fatigue), why never auto-merge major/security (semver-break risk), and why version-pin the tools (reproducible CI) (`specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:84-113`).

# Consequences

INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST are enforced at the CI gate with no manual audit; RUSTSEC detection latency is ≤24h; reviewer fatigue is minimised by weekly batching + low-risk auto-merge; and typosquat risk drops via crates.io-only sources + the mandatory lockfile-diff comment. Trade-offs: cargo-deny `multiple-versions = "deny"` needs an explicit `skip` list for known transitive dupes, and version pinning adds maintenance overhead mitigated by patch/minor auto-merge (`specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:127-138`). Two amendments record the prebuilt-binary install swap (os-error-2 fix, no version change) and the cargo-deny `0.16.4 → 0.19.8` bump to parse CVSS 4.0 advisory vectors.

# Citations

1. `specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:23-37` — Context: the 3 composited controls, no single tool covers all.
2. `specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:39-82` — Decision: cargo-audit, cargo-deny, staggered Dependabot, lockfile diff.
3. `specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:84-113` — Rationale: two-tools, SHA-pin, weekly, no-auto-merge, version-pin.
4. `specs/03_architecture/adrs/ADR-S12-045-dep-policy-cargo-audit-deny-dependabot.md:127-138` — Consequences and mitigated trade-offs.
