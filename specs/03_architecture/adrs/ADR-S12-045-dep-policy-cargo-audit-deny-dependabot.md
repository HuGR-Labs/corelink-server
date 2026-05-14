---
id: "ADR-S12-045"
type: "adr"
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
tags: ["adr", "s12", "supply-chain", "cargo-audit", "cargo-deny", "dependabot", "dep-policy"]
---

# ADR-S12-045: Dep Policy — cargo-audit + cargo-deny + Dependabot Canonical Config

## Status

ACTIVE — WI-S12-004 SEALED.

## Context

CoreLink HIGH_RISK supply-chain lane (FF-HR-005) requires three composited controls for
dependency hygiene:

1. **RUSTSEC advisory detection** — detect CVEs in `Cargo.lock` within ≤ 24h of publication.
2. **Policy enforcement** — enforce license allowlist, yanked-dep blocking, source restrictions,
   and advisory denial at every PR merge point.
3. **Automated dep updates** — weekly batched Dependabot PRs with bounded auto-merge for
   minor/patch non-security updates.

No single tool covers all three axes.  The Rust ecosystem provides:
- `cargo-audit`: RUSTSEC advisory scanner (CVE-class focus).
- `cargo-deny`: policy enforcement (license + yanked + sources + bans + advisories).
- Dependabot: GitHub-native automated dep update PRs.

## Decision

### cargo-audit

- **PR gate**: `cargo audit --deny warnings` on every PR touching Rust code or workspace
  manifests.
- **Daily cron**: `0 6 * * *` UTC; parse JSON findings; classify HIGH/CRITICAL; emit SEV-2
  (CRITICAL) or SEV-3 (HIGH) alerts.
- **Version**: pinned to `0.21.x`; bump via ADR + Security review.

### cargo-deny

- **Trigger**: PR gate + daily cron (30 min offset from cargo-audit to avoid DB contention).
- **Policy canonical source**: `deny.toml` at workspace root.
- **Action**: EmbarkStudios/cargo-deny-action SHA-pinned.
- **Version**: `0.16.x`; bump via ADR + Security review.

### Dependabot

- **Schedule**: weekly Monday 08:00 BRT.
- **Groups**: security-updates / non-security-minor-patch / non-security-major.
- **Auto-merge policy**: patch + minor non-security + CI green → auto-merge squash.
  Major and security updates → manual review (NEVER auto-merged).
- **Rate limit**: 10 open PRs max.

### Lockfile diff

- Mandatory PR comment on every `Cargo.lock`-changing PR.
- Lists added/removed/upgraded deps + cargo-deny summary.
- Pair-review required for new dep additions.

## Rationale

### Why cargo-audit + cargo-deny (not single tool)?

- `cargo-audit`: tight RUSTSEC focus; fast; JSON output for classification pipeline.
- `cargo-deny`: broader policy surface (license allowlist, yanked, sources, bans).
- Industry standard: used in Mozilla Servo, Fuchsia, Bevy, Tauri.

### Why SHA-pin all GitHub Actions?

- HIGH_RISK lane FF-HR-005: supply chain attack via action tag mutable ref is documented
  (e.g., tj-actions/changed-files 2023 incident).
- Dependabot (github-actions ecosystem) maintains SHA pins automatically.

### Why weekly (not daily) Dependabot?

- Daily: 50+ PRs/week; reviewer fatigue degrades review quality.
- Weekly Monday: batch review fits sprint cadence; security-updates trigger immediately.

### Why NOT auto-merge major or security updates?

- Major: semver-breaking API change risk; requires manual compatibility validation.
- Security: security patch may contain semver-breaking change (§9.8 design decision);
  manual review ensures correctness before merge.

### Why version-pin cargo-audit + cargo-deny?

- Reproducible CI: same tooling version → same results across runs.
- Prevents silent policy change on tool bump.
- Bump cadence: quarterly review or when critical fix available; always via ADR.

## Tooling pin table

| Tool | Pinned version | Current SHA (action) | Bump policy |
|---|---|---|---|
| cargo-audit | 0.21.x | installed via cargo install | Quarterly + ADR |
| cargo-deny | 0.16.x | EmbarkStudios/cargo-deny-action@df3b2489... | Quarterly + ADR |
| actions/checkout | v4.2.2 | @11bd71901bbe5b1630ceea73d27597364c9af683 | Dependabot auto-merge |
| dtolnay/rust-toolchain | stable | @29eef336d9b2848a0b548edc03f92a220660cdb8 | ADR |
| Swatinem/rust-cache | v2.7.7 | @400e7407cfd7a091e5fbb6afec01ec146c432b7c | Dependabot auto-merge |
| dependabot/fetch-metadata | v2.3.0 | @d7267f607e4f2cf3e2f77fe2f5b19e0e31f2a8a | Dependabot auto-merge |

## Consequences

**Positive**:
- INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST enforced at CI gate — no manual audit needed.
- Detection latency ≤ 24h for RUSTSEC advisories.
- Reviewer fatigue minimised via weekly batching + auto-merge of low-risk updates.
- Typosquat risk reduced via crates.io-only source policy + lockfile diff mandatory comment.

**Negative / mitigated**:
- cargo-deny `multiple-versions = "deny"` requires explicit `skip` list for known transitive
  dupes (tonic 0.4/0.5 tower, RustCrypto digest migration).  Managed in deny.toml [bans].skip.
- Version pinning creates maintenance overhead; mitigated by Dependabot auto-merge for patch/minor.

## Related

- ADR-S12-046: License allowlist 7 OSI-approved + banned copyleft.
- ADR-S12-047: License review quarterly process.
- WI-S12-004: Implementation work item.
- `docs/internal/dep-policy.md`: operational runbook.

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-13 | Gustavo (via Claude Sonnet 4.6) | Initial creation — WI-S12-004 SEALED. |
