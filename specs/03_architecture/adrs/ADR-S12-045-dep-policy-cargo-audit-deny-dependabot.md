---
id: "ADR-S12-045"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-05-13"
updated: "2026-06-10"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
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
- **Version**: pinned to `0.22.x` (was `0.21.x` in prose / `0.22.1` in CI until
  v1.3.0 reconciled them); bump via ADR + Security review.
- **Install**: SHA-pinned `taiki-e/install-action` **prebuilt binary** (since v1.3.0;
  was `cargo install` from source) — removes the os-error-2 from-source compile race
  on the contended self-hosted Mac. Mirrors the cargo-deny install method.

### cargo-deny

- **Trigger**: PR gate + daily cron (30 min offset from cargo-audit to avoid DB contention).
- **Policy canonical source**: `deny.toml` at workspace root.
- **Install**: cargo-deny CLI installed natively via taiki-e/install-action (SHA-pinned).
  The EmbarkStudios/cargo-deny-action is a Linux-only Docker container action and
  hard-fails on the macOS self-hosted fleet, so it was retired in favour of the
  native install (2026-06-02).
- **Version**: `0.19.x` (was `0.16.x` until v1.2.0); bump via ADR + Security review.

### Dependabot

- **Schedule**: weekly per ecosystem, **staggered Mon–Thu 08:00 BRT** (v1.1.0,
  2026-06-02): cargo + github-actions Mon, npm/admin-ui Tue, npm/docs + npm/wasm
  Wed, npm-root + pip Thu. Rationale: the original single-Monday batch overwhelmed
  the self-hosted runner fleet (5 macOS runners = one Mac, post CI-migration) when
  ~9 grouped dep PRs triggered CI simultaneously (near-OOM 2026-06-02). Each
  ecosystem stays **weekly**, so CAP-SUPPLY-004 cadence is intact; security-updates
  remain immediate (not bound to the staggered day). Canonical config:
  `.github/dependabot.yml` header.
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
| cargo-audit | 0.22.1 | installed via taiki-e/install-action@fd2f5e3d... (v2.81.9) prebuilt | Quarterly + ADR |
| cargo-mutants | 25.0.1 (PR/nightly gate) · 27.0.0 (workspace nightly) · latest (per-crate) | installed via taiki-e/install-action@fd2f5e3d... prebuilt | test-tooling (not security-gated) |
| cargo-deny | 0.19.8 | installed via taiki-e/install-action@fd2f5e3d... (v2.81.9) | Quarterly + ADR |
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
| 1.1.0 | 2026-06-02 | Gustavo | Retired the Linux-only EmbarkStudios/cargo-deny-action container in favour of the native taiki-e/install-action install on the macOS self-hosted fleet. |
| 1.2.0 | 2026-06-10 | Gustavo (via Claude Opus 4.8) | **cargo-deny `0.16.4` → `0.19.8`** (+ taiki-e/install-action `v2.49.49`/`v2.49.45` → `v2.81.9`, SHA `fd2f5e3d…`). See Amendment 1.2.0 below. |
| 1.3.0 | 2026-06-10 | Gustavo (via Claude Opus 4.8) | **cargo-audit + cargo-mutants + cargo-nextest install: `cargo install` (from source) → SHA-pinned taiki-e prebuilt binary** across all 13 call sites (versions unchanged). Reconciled cargo-audit pin prose `0.21.x` → `0.22.1` (CI reality). See Amendment 1.3.0 below. |

## Amendment 1.3.0 — prebuilt tool installs (os-error-2 compile-race fix)

**Trigger:** `cargo install cargo-audit` / `cargo install cargo-mutants` /
`cargo install cargo-nextest` compile the tools **from source**, pulling the heavy
`aws-lc-sys` C/asm build. On the contended self-hosted Mac this repeatedly hits a
filesystem race (`No such file or directory (os error 2)` / `clang: no input files`),
**cancelling/failing the cargo-audit and cargo-mutants gates** intermittently and
forcing documented-flake admin merges.

**Change:** all 13 from-source install sites switched to the **SHA-pinned
`taiki-e/install-action@fd2f5e3d…` (v2.81.9) prebuilt binary** — `cargo-audit` (2),
`cargo-mutants` (9: PR gate, workspace nightly, 6 per-crate nightly, mutation-nightly),
`cargo-nextest` (1, in the PR mutation gate). **Versions are unchanged** (cargo-audit
`0.22.1`, cargo-mutants `25.0.1`/`27.0.0`/latest-per-crate, cargo-nextest latest) — a
pure install-mechanism swap, so kill-rate baselines do not shift.

**Security review (§14.s12.004.1 — cargo-audit is high-risk-lane tooling):**
- **No version change** → no change to which advisories cargo-audit detects or which
  mutants cargo-mutants generates. Pure delivery-mechanism change.
- **Supply chain:** the prebuilt binaries are the upstream projects' own signed GitHub
  release artifacts, fetched by the already-trusted, SHA-pinned `taiki-e/install-action`
  (same action+SHA already used for cargo-deny). Trust surface is **narrowed**, not
  widened: a pinned prebuilt artifact vs. compiling arbitrary transitive crates.io
  source (incl. `aws-lc-sys` build scripts) at install time.
- **Reliability is itself a security property:** a gate that flakes-red gets bypassed;
  a reliable gate gets enforced.
- **Approved-by:** Gustavo Schneiter (owner / final_approver), 2026-06-10, via the
  Security-review gate for this change.

## Amendment 1.2.0 — cargo-deny version bump (CVSS 4.0 advisory parse)

**Trigger (critical fix available — out-of-cadence bump per the bump policy):**
cargo-deny `0.16.4` bundles a `rustsec`/`cvss` version that cannot parse **CVSS 4.0**
advisory vectors. A new advisory (`RUSTSEC-2026-0073`, libcrux-poly1305) uses the
`CVSS:4.0/…` format, so `0.16.4` hard-fails at advisory-database load
(`failed to load advisory database: parse error … TOML parse error`) on **every** PR
touching `crates/*/src/**` — the advisories gate was red repo-wide.

**Change:** bump cargo-deny `0.16.4` → `0.19.8` (manifest latest), installed via the
SHA-pinned `taiki-e/install-action@fd2f5e3d…` (v2.81.9) prebuilt binary in all four
call sites (`cargo-deny.yml`, `dependabot-policy.yml`, `cas_foundation.yml`,
`lockfile-diff.yml`; the last switched from `cargo install` to the prebuilt binary to
also remove the from-source compile race on the contended runner).

**Security review (§14.s12.004.1 — high-risk-lane tooling, SOC2 CC8.1 change-mgmt):**
- **Posture: strictly positive.** `0.19.8` parses newer advisories (incl. CVSS 4.0) that
  `0.16.4` silently fails to load — it catches *more* vulnerabilities, not fewer. No
  detection regression.
- **No policy change.** Verified locally against the unchanged `deny.toml`:
  `cargo-deny check` → `advisories ok, bans ok, licenses ok, sources ok` (only two
  pre-existing `unmatched-skip` warnings; no new denials or allowances).
- **Supply chain:** prebuilt binary via SHA-pinned action; reproducible; no new network
  trust beyond the already-trusted taiki-e/install-action.
- **Approved-by:** Gustavo Schneiter (owner / final_approver), 2026-06-10, via the
  Security-review gate for this change.
