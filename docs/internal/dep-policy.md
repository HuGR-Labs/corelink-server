---
doc_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-13"
updated: "2026-05-13"
owner: "Gustavo Schneiter"
tags: ["dep-policy", "supply-chain", "license", "cargo-audit", "cargo-deny", "dependabot", "wi-s12-004"]
---

# CoreLink Dependency Policy

> **Implements**: WI-S12-004 §6.1.7 (ST-011) · **Enforces**: INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST
> **ADR refs**: ADR-S12-045 (dep policy), ADR-S12-046 (license allowlist), ADR-S12-047 (license review quarterly)

---

## 1. Overview

CoreLink operates under a HIGH_RISK supply-chain lane (FF-HR-005).  Dependency hygiene is a
continuous control surface: the Rust ecosystem had 600+ RUSTSEC advisories in 2024 (tripled
vs. 2022); event-stream (2018), ua-parser-js (2021), and XZ Utils (2024) demonstrate that
automated detection + license enforcement + bounded auto-merge is the minimum defensible posture.

Three composited controls implement the policy:

| Control | Tool | Cadence |
|---|---|---|
| RUSTSEC advisory detection | `cargo-audit` | PR gate + daily cron |
| Policy enforcement (license/yanked/sources/bans) | `cargo-deny` + `deny.toml` | PR gate + daily cron |
| Automated dep updates | Dependabot | Weekly (Monday 08:00 BRT) |

---

## 2. License Allowlist

### 2.1 Allowed licenses (INV-SUPPLY-LICENSE-ALLOWLIST)

The following SPDX expressions are permitted in transitive dependencies.  All others are denied
by default (`[licenses].default = "deny"` in `deny.toml`).

| SPDX Expression | Category | Rationale |
|---|---|---|
| `MIT` | Permissive | Widely compatible; no copyleft; OSI-approved |
| `Apache-2.0` | Permissive | Patent grant clause; OSI-approved |
| `Apache-2.0 WITH LLVM-exception` | Permissive | LLVM/Rust compiler ecosystem standard |
| `BSD-2-Clause` | Permissive | Attribution-only; OSI-approved |
| `BSD-3-Clause` | Permissive | Attribution + non-endorsement; OSI-approved |
| `ISC` | Permissive | Functionally equivalent to MIT; OSI-approved |
| `MPL-2.0` | Weak copyleft (file-level) | File-level only; SaaS-compatible; OSI-approved |
| `Unicode-DFS-2016` | Data/font | ICU data files; Unicode Consortium standard |
| `Unicode-3.0` | Data/font | Newer Unicode license; forwarded allowance |
| `Zlib` | Permissive | Short permissive; OSI-approved |
| `CC0-1.0` | Public domain | OSI-approved public domain dedication |
| `0BSD` | Permissive | Zero-clause BSD; OSI-approved |

**To add a new license**: open an ADR referencing this document, obtain Compliance Officer
sign-off, bump `deny.toml` allow list, and update this table.

### 2.2 Banned licenses

| SPDX Expression | Category | Reason |
|---|---|---|
| `GPL-1.0`, `GPL-2.0`, `GPL-2.0+`, `GPL-2.0-only`, `GPL-2.0-or-later` | Strong copyleft | Viral; distribution trigger; legal risk |
| `GPL-3.0`, `GPL-3.0+`, `GPL-3.0-only`, `GPL-3.0-or-later` | Strong copyleft | Viral; SaaS distribution risk |
| `AGPL-1.0`, `AGPL-3.0`, `AGPL-3.0-only`, `AGPL-3.0-or-later` | Network copyleft | Triggers at network use; highest SaaS risk |
| `SSPL-1.0` | Service copyleft | MongoDB-style anti-cloud provision |
| `Commons-Clause` | Commercial restriction | Not OSI-approved; restricts commercial use |
| `BUSL-1.1` | Business source | Not OSI-approved; time-limited; commercial restriction |

Additionally, `[licenses].copyleft = "deny"` catches any copyleft license not in the explicit
deny list (e.g. LGPL, EUPL, CDDL, EPL).

### 2.3 License exceptions

Per-crate exceptions require:
1. ADR documenting the business justification.
2. Compliance Officer sign-off.
3. ADR ID referenced in `deny.toml [licenses].exceptions`.
4. 90-day sunset clock (re-evaluate at expiry).

---

## 3. Yanked Deps Policy (INV-SUPPLY-NO-YANKED)

Zero yanked deps are permitted in `Cargo.lock` on the main branch.

**Why**: a yanked crate means the author has retired the version (due to security issue or
bug).  Using a yanked version is a declared risk acceptance that the ecosystem has flagged.

**Enforcement**: `cargo-deny [advisories] yanked = "deny"` — CI gate fails on PR if any
transitive dep is yanked.

**Waiver**: allowed only with an explicit ADR + Security Lead sign-off + ≤ 7-day remediation
plan (replacement or patch).  Must be listed in `deny.toml [advisories].ignore` with the ADR ID.

---

## 4. Source Restrictions

### 4.1 Only crates.io

All external dependencies must come from the official crates.io registry.

**Enforcement**: `deny.toml [sources] unknown-registry = "deny"`.

**Why**: crates.io is the only officially audited registry.  Alternative registries lack the
same security posture; typosquat risk is materially higher.

### 4.2 Git dependencies

Git dependencies are blocked by default (`unknown-git = "deny"`).

To allowlist a git dep:
1. ADR documenting the business need (e.g., unmerged upstream fix).
2. Security review sign-off.
3. URL added to `deny.toml [sources].allow-git`.
4. Cargo.toml dep pinned to an explicit SHA commit hash (NOT a tag or branch).
5. 90-day sunset clock (upstream merge or replacement).

**Example** (Cargo.toml):
```toml
[dependencies]
# ADR-S12-XXX: pending upstream merge of fix for issue #1234.
some-crate = { git = "https://github.com/org/some-crate", rev = "abc123def456..." }
```

### 4.3 `[patch.crates-io]` vendor patches

Any `[patch.crates-io]` entry in workspace `Cargo.toml` requires:
1. Mandatory ADR + Security Lead review (pre-merge check).
2. Git dep pinned to explicit SHA (same as §4.2).
3. 90-day sunset clock.

Failure to comply: pre-merge CI check fails; cargo-deny blocks the git source.

---

## 5. RUSTSEC Triage SLA

| Severity | Triage SLA | Fix SLA | Alert |
|---|---|---|---|
| CRITICAL (CVSS ≥ 9.0) | ≤ 24h post-RUSTSEC publish | ≤ 7 days | SEV-2: Slack urgent + PagerDuty page (≤ 30s) |
| HIGH (CVSS ≥ 7.0) | ≤ 24h | ≤ 7 days | SEV-3: Slack notification (≤ 5 min) |
| MEDIUM (CVSS ≥ 4.0) | ≤ 72h | ≤ 30 days | No on-call alert; tracked in backlog |
| LOW | ≤ 7 days | ≤ 90 days | No alert; quarterly review |

**Unmaintained deps** (`unmaintained = "warn"` in deny.toml): flagged in CI output;
quarterly review required.  Replacement plan must be documented in an ADR.

**RUSTSEC waiver**: allowed only with ADR + Security Lead sign-off + 90-day sunset clock.
Waiver is listed in `deny.toml [advisories].ignore` with the ADR ID.

---

## 6. Dependabot Policy

### 6.1 Schedule

Weekly Monday 08:00 BRT (America/Sao_Paulo), batched to reduce reviewer fatigue.

### 6.2 Grouped PRs

| Group | Applies to | Update types |
|---|---|---|
| `security-updates` | security advisories | patch / minor / major |
| `non-security-minor-patch` | version updates | patch + minor |
| `non-security-major` | version updates | major only |

### 6.3 Auto-merge policy

| Update type | CI status | Decision |
|---|---|---|
| patch / minor (non-security) | all checks green | Auto-merge via squash |
| patch / minor (non-security) | any check fails | Manual review |
| major | any | Manual review (NEVER auto-merge) |
| security (any type) | any | Manual review (§9.8: fix may break API) |

**Required CI checks** (all must pass before auto-merge):
- `cargo-audit PR gate` (cargo-audit.yml)
- `cargo-deny` (cargo-deny.yml + cas_foundation.yml)
- `cargo-test` (cas_foundation.yml)
- `clippy -D warnings` (cas_foundation.yml)

### 6.4 Rate limit

Maximum 10 open Dependabot PRs at any time (DoS protection).

---

## 7. Lockfile Diff PR Review

Every PR that modifies `Cargo.lock` receives an automated PR comment (via `lockfile-diff.yml`)
containing:

- Added / removed / upgraded dependency count.
- cargo-deny policy check summary.
- Instructions for pair-review of new dep additions.

**Pair-review on new deps**: any PR adding a new crate (not present in the previous
`Cargo.lock`) must receive a second reviewer specifically examining the new dep's provenance,
license, and maintainer reputation.

---

## 8. Quarterly Legal Review

**Cadence**: every 3 months (calendar reminder + ADR-S12-047 documents the process).

**Scope**:
1. Sample 5% of deps; verify SPDX expression matches actual code (license drift detection).
2. Review `deny.toml` allowlist for new licenses to add or remove.
3. Document decisions in ADR-S12-XXXX-license-review-YYYYQQ.md.

**Responsible**: Compliance Officer + Legal (sign-off required).

---

## 9. Metrics

The following Prometheus metrics are emitted by the CI pipelines:

| Metric | Labels | Description |
|---|---|---|
| `corelink_supply_cargo_audit_findings_total` | `{severity}` | severity ∈ critical\|high\|medium\|low\|info |
| `corelink_supply_cargo_audit_runs_total` | `{outcome}` | outcome ∈ pass\|fail |
| `corelink_supply_cargo_deny_violations_total` | `{rule}` | rule ∈ license\|yanked\|sources\|advisory\|copyleft\|unknown_registry |
| `corelink_supply_dependabot_prs_total` | `{outcome,update_type}` | outcome ∈ auto_merged\|manual\|failed; update_type ∈ patch\|minor\|major\|security |
| `corelink_supply_dep_count_gauge` | — | Total deps in Cargo.lock |
| `corelink_supply_yanked_deps_count_gauge` | — | Yanked deps count (target = 0) |

Dashboard: DASH-SUPPLY (Grafana Cloud) — cargo-audit findings trend 30d + cargo-deny violations
trend 30d + Dependabot PR auto-merge ratio + yanked deps gauge.

---

## 10. Post-mortem Triggers

| Event | Severity | Action |
|---|---|---|
| CRITICAL CVE in production > 7 days | SEV-2 | Post-mortem: root cause RUSTSEC delay or triage gap |
| Yanked dep introduced via auto-merge | SEV-3 | Post-mortem + cargo-deny rule strengthen |
| GPL dep merged (license audit miss) | CRITICAL | Legal + post-mortem + remediation ≤ 30 days |
| Vendor patch merged without ADR | SEV-3 | Retroactive ADR + Security review |
| Auto-merge minor breaking change | SEV-3 | Revert ≤ 24h + post-mortem |

---

## 11. References

- `deny.toml` — canonical policy file at workspace root.
- `.github/workflows/cargo-audit.yml` — RUSTSEC detection workflow.
- `.github/workflows/cargo-deny.yml` — policy enforcement workflow.
- `.github/workflows/dependabot-auto-merge.yml` — auto-merge workflow.
- `.github/dependabot.yml` — Dependabot configuration.
- ADR-S12-045 — Dep policy: cargo-audit + cargo-deny + Dependabot canonical config.
- ADR-S12-046 — License allowlist: 7 OSI-approved + banned copyleft.
- ADR-S12-047 — License review quarterly process.
- [RustSec Advisory Database](https://rustsec.org/)
- [cargo-deny documentation](https://embarkstudios.github.io/cargo-deny/)
- NIST SP 800-218 SSDF PW.4 (third-party software components).
- OWASP ASVS V14 (configuration and dependency verification).
