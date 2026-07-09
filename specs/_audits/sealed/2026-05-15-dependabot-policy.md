---
id: AUDIT-DEPENDABOT-POLICY-2026-05-15
type: audit
doc_status: ACTIVE
audit_status: ACTIVE
version: 1.0.0
created: 2026-05-15
reviewers: [Claude Opus 4.7 — r-prep dependabot agent]
scope: [dependabot, supply-chain, license, auto-merge, soc2-cc7.4, soc2-cc6.6]
branch: wt/r-prep-dependabot-auto
head: pending
parent: WI-S12-004
tags: [audit, dependabot, supply-chain, r-prep, soc2, cc7.4, cc6.6, auto-merge, policy]
---

# Dependabot Policy Baseline — 2026-05-15

> Audit of the Dependabot configuration + auto-merge policy + license
> enforcement gate at the start of R-PREP. Records the policy decisions
> that scope auto-merge to **security-patch only**, expands the ecosystem
> coverage from 2 (cargo + actions) to 4 (cargo + actions + npm + pip),
> and introduces the `dependabot-policy.yml` gate.
>
> SOC 2 mapping: **CC6.6** (vulnerability remediation), **CC7.4** (vendor
> & supply-chain management). This document is the auditor-facing
> evidence for the policy design.

---

## 1. Current state (pre-change snapshot)

| Surface | Before | After |
|---|---|---|
| Ecosystems covered | cargo + github-actions | cargo + github-actions + npm (×4 dirs) + pip (×2 dirs) |
| Auto-merge scope | semver-minor + semver-patch (non-security) | **semver-patch SECURITY-only** |
| Policy gate | none | `dependabot-policy.yml` (license + check-presence + governance-file + hygiene) |
| Incident runbook | none | `RB-DEPENDABOT-INCIDENT` |
| Audit baseline | none | this doc |
| Open Dependabot PRs at audit time | 1 (#6, `non-security-major` group, 17 updates) | unchanged — PR will be re-evaluated under new policy on next push |

Source of truth before this audit: `WI-S12-004 §6.1.4 / §6.1.5`,
`docs/internal/dep-policy.md`, `deny.toml`.

---

## 2. Decisions and rationale

### 2.1 Scope auto-merge to security-patch only (DECISION-DPD-2026-05-15-01)

**Decision:** Auto-merge is permitted **only** when ALL hold:
- Dependabot metadata `update-type == version-update:semver-patch`
- PR is flagged as security (label `security` OR title prefix
  `[security]` OR update-type `version-update:security*`)
- All required status checks present + green
- Policy gate (`dependabot-policy.yml`) passes

**Reject:** auto-merge for non-security patch/minor updates.

**Rationale:**
- HIGH_RISK supply-chain lane (FF-HR-005) — the cheapest review-labor
  savings are CVE response speed, not routine maintenance. Routine
  upgrades flow through weekly grouped batches under human review.
- Non-security minor versions ship behavioral changes that pass CI but
  break downstream consumers; CI is not a substitute for human judgement
  on API surface.
- Security-patch updates have the strongest upstream "no API change"
  guarantee. They are the highest signal-to-noise candidates for
  automation.
- CVE-burst response is the one workload where humans-in-the-loop are
  too slow (CVE → exploit window can be <24h post-disclosure).

**Trade-off accepted:** routine patch/minor upgrades take longer to
land. Mitigated by Monday weekly cadence + `HumanGuardrail/security`
reviewers + grouped PRs (one PR per ecosystem per week).

### 2.2 Per-ecosystem coverage (DECISION-DPD-2026-05-15-02)

**Decision:** enable Dependabot for 4 package ecosystems across
6 directories:

| Ecosystem | Directories | Open-PR cap |
|---|---|---|
| cargo | `/` | 10 |
| github-actions | `/` | 5 |
| npm | `/`, `/apps/admin-ui`, `/apps/docs`, `/crates/corelink-wasm` | 3-5 each |
| pip | `/`, `/crates/corelink-py` | 3 each |

**Rationale:** matches every lockfile we ship to customers or operate in
production CI. Inventory was confirmed via `find -name package.json`,
`find -name requirements*.txt`, `find -name pyproject.toml`.

### 2.3 Security updates never grouped (DECISION-DPD-2026-05-15-03)

**Decision:** Dependabot groups in `dependabot.yml` apply ONLY to
`version-updates`. Security advisories open one PR per CVE.

**Rationale:**
- Grouping advisories means the auto-merge gate must succeed/fail on a
  bundle. One unsafe CVE in the bundle blocks safe fixes from landing.
- Per-advisory PRs let the gate enable auto-merge on the patch ones and
  defer the minor/major ones for review.
- Aligned with `RB-DEPENDABOT-INCIDENT §7.2` priority matrix.

### 2.4 License gate is fail-closed (DECISION-DPD-2026-05-15-04)

**Decision:** `dependabot-policy.yml` Step 2-4 fail closed on:
- Any cargo-deny `licenses` or `bans` finding,
- Any AGPL/GPL/SSPL/Commons-Clause/BUSL/CDLA-Sharing signature in
  Cargo.lock or `package-lock.json`,
- Unparseable SPDX expression (treated as banned).

**Rationale:** mirrors `deny.toml [licenses]` + `docs/internal/dep-policy.md §2`.
A transitive dep upgrade that drags in a banned license is a real
supply-chain attack vector (e.g., upstream relicenses to AGPL to force
disclosure of dependents). Belt-and-braces with `cargo-deny` + regex
sweep catches both classification errors.

### 2.5 Governance-file modifications block (DECISION-DPD-2026-05-15-05)

**Decision:** Dependabot PRs touching `.github/CODEOWNERS`,
`.github/workflows/*.yml`, `.github/dependabot.yml`, `SECURITY.md`,
`deny.toml`, `cosign.pub`, `.well-known/*`, or `specs/_security/*` FAIL
the policy gate.

**Rationale:** legitimate Dependabot PRs touch lockfiles + `package.json`
versions only. A PR that edits CI configs or governance files is the
fingerprint of a compromised upstream package (postinstall hook, build
script) or a Dependabot bug. Fail closed; escalate via
`RB-DEPENDABOT-INCIDENT §4.3`.

### 2.6 Required-check presence verified (DECISION-DPD-2026-05-15-06)

**Decision:** the policy gate verifies that each canonical required
check actually ran on the PR head SHA — not just that it didn't fail.

**Rationale:** branch protection considers a `skipped` check as non-
failing. A workflow with a `paths:` filter that excludes lockfile
changes would silently bypass a "required" gate. We close this hole by
listing the canonical required checks and failing the policy gate if
any are missing on the PR's head.

### 2.7 Audit event emission (DECISION-DPD-2026-05-15-07)

**Decision:** every auto-merge event emits a structured log line:
```
{
  "event": "corelink.security.dependency_auto_merged",
  "schema": "corelink.audit.v1",
  ...
}
```

**Rationale:** SOC 2 CC7.4 + CC6.6 require evidence of vendor/CVE
remediation actions. Until the audit-chain ingestion path is wired
(deferred — see §6), structured logs in the GitHub Actions log stream
serve as the evidence surface (queryable via `gh run view --log` +
S3 archival).

### 2.8 SHA-pinning enforced for all third-party actions (DECISION-DPD-2026-05-15-08)

**Decision:** every action used in `dependabot-auto-merge.yml` and
`dependabot-policy.yml` is SHA-pinned per HIGH_RISK lane FF-HR-005.

| Action | SHA | Version |
|---|---|---|
| `actions/checkout` | `11bd71901bbe5b1630ceea73d27597364c9af683` | v4.2.2 |
| `dependabot/fetch-metadata` | `d7267f607e9d3fb96fc2fbe83e0af444713e90b7` | v2.3.0 |
| `dtolnay/rust-toolchain` | `b3b07ba8b418998c39fb20f53e8b695cdcc8de1b` | stable |
| `taiki-e/install-action` | `9ba3ac3fd006a70c6e186a683577abc1ccf0ff3a` | v2.49.45 |

Bumping any SHA requires ADR + Security review per `§14.s12.004.1`.

---

## 3. Risk register

### 3.1 Auto-merge fires on a security PR that breaks main

**Likelihood:** LOW. **Impact:** HIGH (rollback required).

Mitigations:
- Required checks include `cargo-test` + `clippy -D warnings` —
  compilation + unit coverage required green.
- Squash-merge keeps revert atomic (`git revert <merge-sha>`).
- Post-merge monitoring: cas_foundation nightly + endurance-2h-nightly
  catch regressions within 12h.

Residual risk: a flaky test on the head SHA passes by chance; we
re-merge a regression that intermittent CI missed. Acceptance: tracked
under quarterly post-mortem review.

### 3.2 Dependabot mis-classifies a minor as patch (semver-patch false positive)

**Likelihood:** MEDIUM (upstream crates occasionally violate semver).
**Impact:** MEDIUM.

Mitigations:
- `cargo-deny` advisories block any new RUSTSEC findings.
- Auto-merge applies ONLY when the PR is also flagged `security` —
  reduces blast radius from "every patch" to "security patches only".
- Manual sample audit: Security Lead reviews 1 in N (target N=10)
  auto-merged PRs per quarter as part of SOC 2 walkthrough evidence.

### 3.3 CVE-burst surge exhausts CI runner minutes

**Likelihood:** MEDIUM during major CVE drops (e.g., XZ-Utils 2024).
**Impact:** MEDIUM.

Mitigations:
- `RB-DEPENDABOT-INCIDENT §7.4` defines the throttle protocol.
- `open-pull-requests-limit` per ecosystem caps the surge at 10+5+5+5+3+3+3+3 = 37 PRs maximum simultaneously.
- Concurrency groups in `dependabot-auto-merge.yml` cancel stale runs.

### 3.4 Malicious upstream package edits governance files

**Likelihood:** LOW (requires upstream compromise). **Impact:** CRITICAL.

Mitigations:
- `dependabot-policy.yml` Step 6 fails closed on any modification to
  governance paths.
- Dependabot PRs use a separate token scope; cannot push to `main`
  without auto-merge consent.
- SHA-pinning of every action narrows the supply-chain surface to
  pinned upstreams + audit on bump.

### 3.5 Branch-protection bypass via "non-required check" skip

**Likelihood:** LOW. **Impact:** HIGH (silent policy bypass).

Mitigations:
- `dependabot-policy.yml` Step 7 verifies that each required check is
  present on the PR head SHA (not just non-failing).
- Quarterly branch-protection review (tracked in
  `specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md`).

---

## 4. Required CI checks (snapshot)

The canonical required-check list enforced by `dependabot-policy.yml §7`:

- `cargo-deny / cargo-deny`
- `cargo-audit / cargo-audit-pr`
- `cas_foundation / cargo-test`
- `cas_foundation / clippy`
- `action-sha-audit / sha-audit`
- `dco-check / DCO`

This list MUST stay in sync with the branch-protection ruleset on
`main`. Drift between the two is detected by the weekly compliance
review.

---

## 5. Current dependabot PR state (audit-time)

`gh pr list --author "app/dependabot" --state open --limit 30`:

| # | Title | Ecosystem | Update type | Labels | Disposition |
|---|---|---|---|---|---|
| 6 | `deps(deps): bump the non-security-major group across 1 directory with 17 updates` | cargo | semver-major (group) | none | Manual review per §6 RB-DEPENDABOT-INCIDENT. Will be split per-crate after the new dependabot.yml takes effect on next weekly run. |

No open security PRs. No backlog. Clean state at audit time.

---

## 6. Deferred work

### 6.1 Audit-chain ingestion of auto-merge events

`corelink.security.dependency_auto_merged` is emitted as a structured
log line today; a downstream sidecar must tail GitHub workflow logs and
forward to the audit chain. **Tracked as:** open follow-up — file under
`specs/_audits/dependabot-followup-tickets.md` (to create alongside
`replication-followup-tickets.md` style).

Until then, auditor evidence is:
1. `gh run view <run-id> --log` for the workflow run.
2. S3 archival of workflow logs (already configured per
   `cf-deploy-prod.yml` log-retention block).

### 6.2 Prometheus metric ingestion

`corelink_supply_dependabot_prs_total` is emitted as a log line; an
external job (Grafana Agent on the CI bastion) must ingest. Wiring is
the SRE on-call's responsibility — tracked under
`specs/_audits/sealed/perf-optimization-followup-tickets.md`.

### 6.3 Quarterly auto-merge sample audit

Security Lead must review a sample of N=10 auto-merged PRs per quarter
as SOC 2 walkthrough evidence. First sample audit due 2026-08-15.

---

## 7. Validation

Pre-merge checks performed by this agent:

```
python3 -c "import yaml; yaml.safe_load(open('.github/dependabot.yml'))"            # OK
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/dependabot-auto-merge.yml'))"  # OK
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/dependabot-policy.yml'))"      # OK
python3 scripts/validate_specs.py | tail -2                                        # OK
```

---

## 8. Sign-off

| Reviewer | Role | Date |
|---|---|---|
| Claude Opus 4.7 (r-prep agent) | Author | 2026-05-15 |
| Security Lead | Approver | _pending_ |
| Compliance Officer | SOC 2 evidence approver | _pending_ |

End of audit.
