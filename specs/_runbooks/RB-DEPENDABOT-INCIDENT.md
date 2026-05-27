---
id: "RB-DEPENDABOT-INCIDENT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Security Lead"
final_approver: "Security Lead"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S12-004"
tags: ["runbook", "r-prep", "dependabot", "supply-chain", "cve", "incident", "auto-merge"]
inherits_from: ["VDP-001", "RB-SECURITY-VULNERABILITY-INTAKE"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §5.4, §7, §10.

# RB-DEPENDABOT-INCIDENT — Dependabot surge / CVE-burst triage

> **Owned by:** Security Lead. **Triggered by:**
> - Dependabot opens >5 PRs in <1h (typical CVE-cascade signature),
> - `dependabot-policy` workflow fails on any open PR,
> - `dependabot-auto-merge` workflow refuses to enable auto-merge on a PR
>   labeled `security`,
> - any P0 RUSTSEC / GHSA advisory targeting a transitive crate we ship.
>
> **Companion docs:**
> - Auto-merge policy: `.github/workflows/dependabot-auto-merge.yml`
> - Policy gate:       `.github/workflows/dependabot-policy.yml`
> - Dependabot config: `.github/dependabot.yml`
> - Dep policy:        `docs/internal/dep-policy.md`
> - License source:    `deny.toml [licenses]`
> - Incoming-vuln intake: `RB-SECURITY-VULNERABILITY-INTAKE`
> - Audit baseline:    `specs/_audits/sealed/2026-05-15-dependabot-policy.md`

---

## 1. SLA

| Stage | Target |
|---|---|
| Acknowledge a CVE-burst surge | **15 min** (Security on-call) |
| Triage every open dependabot PR | **2 h** from first PR |
| Force-merge or close decision per PR | **24 h** for HIGH+CRITICAL CVEs, **7 d** for MEDIUM, **30 d** for LOW |
| Post-incident audit entry filed | **48 h** after surge closes |

A "surge" = 5+ Dependabot PRs opened against `main` in any 60-minute window.

---

## 2. Roles

| Role | Responsibility |
|---|---|
| **Security Lead** | Owns triage queue. Calls force-merge. Files incident audit. |
| **SRE on-call** | Confirms CI capacity (auto-merge can consume runner minutes). Pauses if needed. |
| **AppSec advisor** | Validates each CVE's exploitability in CoreLink context. |
| **Engineering lead (per area)** | Reviews PRs that touch their crate. Writes regression test if needed. |
| **Compliance** | Logs evidence into audit-chain (CC7.4 vendor-management evidence). |

Escalation path: Security Lead → SRE on-call → Owner (Gustavo Schneiter).
Pager: PagerDuty service `corelink-supply-chain`.

---

## 3. Decision tree — per dependabot PR

```
For each open dependabot PR (triaged oldest-first):

  ┌─ Is `dependabot-policy` gate RED?
  │    YES → go to §4 (policy-gate failure paths)
  │    NO  ↓
  ├─ Is the PR labeled `security` AND update-type = semver-patch?
  │    YES → auto-merge should have fired. Verify with `gh pr view`.
  │           If auto-merge is NOT queued, run §5 (auto-merge debug).
  │           If queued and CI green → wait for merge; record metric.
  │    NO  ↓
  ├─ Is update-type semver-major?
  │    YES → manual review only. Assign engineering lead via CODEOWNERS.
  │           Run §6 (major-update triage).
  │    NO  ↓
  ├─ Is update-type semver-minor (non-security)?
  │    YES → batched weekly review. Add `triage:weekly` label and stop.
  │    NO  ↓
  └─ Is update-type semver-patch (non-security)?
       YES → batched weekly review with the minors. `triage:weekly`.
```

The auto-merge workflow (`dependabot-auto-merge.yml`) enforces this tree
in CI. Manual triage is the **escape valve** when CI signals contradict
the policy or when a CVE-burst overruns the weekly cadence.

---

## 4. Policy-gate failure paths

`dependabot-policy.yml` will fail one of these steps. Each has a fixed playbook.

### 4.1 cargo-deny licenses (banned license detected)

1. Inspect the PR's `Cargo.lock` diff:
   ```bash
   gh pr diff <PR#> -- Cargo.lock | grep -B2 -E '(AGPL|GPL-[23]|SSPL|Commons-Clause|BUSL|CDLA-Sharing)'
   ```
2. If a transitive dep pulled in a banned license, the upstream crate
   upgrade is the cause. **DO NOT add the license to the allowlist** —
   instead:
   - Pin the offending transitive dep to the prior version via
     `[patch.crates-io]` or workspace dep override.
   - Open a ticket against the immediate-upstream crate.
   - Close the dependabot PR with comment "blocked: transitive license
     drift; pin landed in <PR#>".
3. If the license is legitimately needed (rare — should never happen for
   a banned license), open an ADR per `docs/internal/dep-policy.md §2.3`.
   ADR + Compliance Officer + Security Lead sign-off required.

### 4.2 Cargo.lock banned-license signature scan

Same as §4.1 — second-line check; same remediation.

### 4.3 Forbidden governance-file modification

If a Dependabot PR touches `.github/CODEOWNERS`,
`.github/workflows/*.yml`, `SECURITY.md`, `deny.toml`, `cosign.pub`,
`.well-known/*`, or `specs/_security/*`:

1. **Treat as suspicious.** This is the signature of a compromised
   upstream package that ships a malicious `postinstall` hook editing
   repo files in CI, OR a Dependabot bug.
2. Page the Security Lead immediately.
3. Close the PR (do NOT merge).
4. Run `git log --all --grep="<dependency-name>" -p` to check for prior
   contamination on `main`.
5. Open an incident in PagerDuty. Run §7 forensic protocol.

### 4.4 Missing required status check

Branch-protection requires every check listed in `dependabot-policy.yml`
§7. If one is missing on a Dependabot PR head SHA:

1. Check workflow runs: `gh run list --branch <branch>`.
2. If a workflow was skipped by `paths:` filter, that's expected — but
   the `dependabot-policy` gate is conservative and will block. Either:
   - Run the missing workflow manually: `gh workflow run <name> --ref <branch>`.
   - OR update the required-check list in `dependabot-policy.yml` Step 7
     if the path-skip is legitimate (requires ADR + Security review).

### 4.5 Skip-hook commit message

`--no-verify` / `--no-gpg-sign` / `[skip ci]` in a Dependabot PR commit
indicates upstream packaging tampering or a maintainer bypass.

1. Close the PR.
2. Page Security Lead.
3. Forensic protocol (§7).

---

## 5. Auto-merge debug — security-patch PR not auto-merging

When you expect auto-merge to fire but it didn't:

```bash
# 1. Verify PR labels and title.
gh pr view <PR#> --json labels,title,author

# 2. Verify Dependabot metadata classification.
gh run list --workflow=dependabot-auto-merge.yml --branch <branch> --limit 1
gh run view <RUN#> --log | grep -E '(update-type|is_patch|is_security|eligible)'

# 3. Verify branch protection thinks all required checks are green.
gh pr checks <PR#>

# 4. Verify auto-merge is actually queued.
gh pr view <PR#> --json autoMergeRequest
```

Common root causes:
- The PR title lacks `[security]` prefix AND no `security` label — verify
  upstream advisory metadata (some `RUSTSEC-` advisories don't get labels
  auto-applied; manually add `security` label to fix).
- `update-type` is `semver-minor` because the upstream fix bumped the
  minor version (common for `tokio`, `serde`). Auto-merge intentionally
  excludes these — manual review.

---

## 6. Major-update triage

semver-major Dependabot PRs are **always manual**. Per-area engineering
lead reviews. Steps:

1. Read upstream CHANGELOG + MIGRATION guide.
2. Run `cargo doc --no-deps` and check for ABI-breaking changes in our
   public surface.
3. Run full test matrix locally: `cargo test --all-features --workspace`.
4. If the major is purely internal (no crate API change): merge directly.
5. If the major lands new API or removes deprecated calls: split into
   - one PR pinning the prior version,
   - one PR applying our migration,
   - one PR landing the major upgrade.

---

## 7. CVE-burst surge protocol

When 5+ Dependabot PRs open within 60 min (typical when an advisory drops
against `openssl`, `tokio`, `serde`, `actions/checkout`, etc.):

### 7.1 Immediate (T+0..15min)

1. Confirm the surge: `gh pr list --author "app/dependabot" --state open`.
2. Identify the common CVE — usually the PR titles share the advisory ID.
3. Verify exploitability per `RB-SECURITY-VULNERABILITY-INTAKE §2`. If
   exploitable in CoreLink hosted context → CRITICAL.

### 7.2 Triage (T+15min..2h)

For each PR (parallel, by priority):

| Priority | Filter |
|---|---|
| P0 | label `security` + update-type semver-patch + CRITICAL CVE |
| P1 | label `security` + update-type semver-patch + HIGH CVE |
| P2 | label `security` + update-type semver-minor |
| P3 | label `security` + update-type semver-major |
| P4 | non-security |

P0 + P1: should be auto-merging. Verify and unblock per §5.
P2 + P3: assign engineering lead per CODEOWNERS. Manual merge after review.
P4: defer to weekly cadence.

### 7.3 Force-merge protocol

If a P0 PR is blocked by `dependabot-policy` for a **non-license** reason
(e.g., a required check is flaking) and the CVE window cannot wait:

1. Security Lead + one of {SRE on-call, Engineering lead} must
   co-approve in writing (Slack `#sec-incidents` thread or PR review).
2. Re-run the failing check via `gh run rerun <run-id>`.
3. If the check is genuinely unfixable in the window, **only** then:
   - The Security Lead temporarily flips the required check to
     `non-required` via repo settings (NOT via API — manual UI action
     leaves an audit trail).
   - Merge the PR with squash.
   - Restore the required check immediately.
   - File §8 incident audit within 48h citing the bypass.

**NEVER** use `--no-verify`, `--admin`, or `gh api` to bypass branch
protection. The bypass must be a configuration change, not a flag.

### 7.4 Runner-minute throttle

If the surge consumes >50% of monthly GitHub Actions runner minutes:

1. SRE on-call pauses non-essential workflows (canonical-consistency,
   docs-lychee, mutation-expansion) via workflow_dispatch disable.
2. Resume after surge closes.

---

## 8. Post-incident audit entry

Within 48h of surge close, the Security Lead files an audit entry in
`specs/_audits/YYYY-MM-DD-dependabot-surge-<ID>.md` capturing:

- Trigger CVE / advisory IDs.
- Number of PRs opened / auto-merged / manual / closed.
- Force-merge events (with co-approver names + bypass justification).
- Mean time to merge per priority.
- Any policy-gate failures + remediations.
- Lessons learned + dependabot.yml / dep-policy.md updates required.

This entry becomes evidence for SOC 2 CC7.4 (vendor management) and
CC6.6 (vulnerability remediation).

---

## 9. Templates

### 9.1 PR close comment — banned license

```
Closing per RB-DEPENDABOT-INCIDENT §4.1.

This PR introduces a banned license (`<SPDX>`) via transitive dep
`<crate>`. Per `docs/internal/dep-policy.md §2.2`, this license is not
on the CoreLink allowlist. No exception will be granted.

Mitigation: pin `<upstream-crate>` to `<prior-version>` until the
transitive dep is replaced. Tracking: <ticket>.
```

### 9.2 PR force-merge comment

```
Force-merging per RB-DEPENDABOT-INCIDENT §7.3.

Co-approvers: @<security-lead>, @<sre-oncall>.
CVE: <advisory-id>, severity <CRITICAL|HIGH>.
Bypass reason: <required-check-name> is flaking; merge window expires
<UTC-timestamp>. Audit entry: specs/_audits/2026-MM-DD-dependabot-surge-<ID>.md.
```

### 9.3 Surge-acknowledged page reply

```
Acknowledged Dependabot surge — <N> PRs opened in last 60 min targeting
advisory <ID>. Triage in progress. ETA for P0 closure: <UTC-timestamp>.
Incident channel: #sec-incidents-<ID>.
```

---

## 10. Charter compliance

- **No `--no-verify`** — enforced by `dependabot-policy.yml` Step 5.
- **License allowlist enforced before merge** — enforced by Step 2 + 3.
- **Auto-merge ONLY for semver-patch security** — enforced by
  `dependabot-auto-merge.yml` Step 3 (`is_patch && is_security`).
- **All required status checks must pass** — enforced by Step 7 + branch
  protection.
- **Audit emit** — `corelink.security.dependency_auto_merged` event
  emitted as structured log; deferred to audit-chain ingestion (tracked
  in audit baseline §6).

End of runbook.
