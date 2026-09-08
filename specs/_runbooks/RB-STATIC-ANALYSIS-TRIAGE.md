---
id: "RB-STATIC-ANALYSIS-TRIAGE"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "AUDIT-2026-05-15-STATIC-ANALYSIS-BASELINE"
tags: ["runbook", "static-analysis", "codeql", "semgrep", "triage", "r-prep", "pre-pentest", "soc2-cc8-1"]
---

# RB-STATIC-ANALYSIS-TRIAGE — Triage playbook for CodeQL + Semgrep findings

> **Owned by:** R-prep wave. **Triggered by:** any failed run of
> `codeql.yml` or `semgrep.yml`, OR any new entry on the GitHub
> Security tab > Code scanning alerts page.
>
> **SLA summary:** CRITICAL ≤ 24h triage + fix or waiver; HIGH ≤ 7d
> fix or waiver; ERROR (Semgrep) ≤ 7d; WARNING/NOTE deferrable but
> tracked weekly per `AUDIT-2026-05-15-STATIC-ANALYSIS-BASELINE.md` §5
> drift ledger.

---

## 1. Trigger detection

A triage cycle is initiated by any of:

1. A PR check fails on `CodeQL ${language}` or `Semgrep scan` job
   (severity gate hit HIGH+CRITICAL on CodeQL OR ERROR on Semgrep).
2. The weekly cron run posts a non-zero ΔHIGH or ΔERROR vs
   `AUDIT-2026-05-15-STATIC-ANALYSIS-BASELINE.md` §3 baseline.
3. A new GitHub Security tab > Code scanning alert appears on `main`
   with severity HIGH or CRITICAL.
4. Out-of-band Slack alert in `#corelink-security` from the on-call
   Security lead.

If ANY of (1..4) occurs, this runbook applies.

---

## 2. Roles

| Role | Responsibilities |
|---|---|
| On-call security lead | Primary first-responder. Owns triage decision + SLA clock start. |
| AppSec advisor | Co-owner; validates severity, reviews remediation design. |
| Engineering lead (per affected component) | Writes the patch + regression test. |
| Owner (Gustavo Schneiter) | Approves emergency remediation. Signs off on waiver/ADR for MEDIUM wontfix. |

---

## 3. Triage matrix

| Tool/Severity | Verdict path | SLA (clock starts at alert) |
|---|---|---|
| CodeQL CRITICAL (`security-severity ≥ 9.0`) | Fix or revert offending PR. PR cannot merge. | ≤ 24h |
| CodeQL HIGH (`security-severity ≥ 7.0`) | Fix or file ADR-waiver. PR cannot merge until resolved. | ≤ 7d |
| CodeQL MEDIUM | Fix in current sprint OR ADR with expires_at ≤ GA + 90d. | ≤ 30d |
| CodeQL LOW/NOTE | Track in next-sprint backlog. | ≤ next sprint+1 |
| Semgrep ERROR (bundled rulepack) | Fix or suppress with `# nosemgrep` + reason + ADR ref. | ≤ 7d |
| Semgrep ERROR (CoreLink custom rule) | Fix; suppression only with Owner approval (the rule encodes a charter rule). | ≤ 7d |
| Semgrep WARNING | Track; auto-bundle into the weekly drift ledger entry. | ≤ 30d |
| Semgrep NOTE | No action unless aggregate count regresses. | best-effort |

---

## 4. Triage steps (per-alert)

1. **Reproduce locally.**
   - CodeQL: `gh code-scanning alert view <id> --json` to inspect the
     SARIF detail; pull the PR locally and rerun the relevant
     `cargo build` / `pnpm test` to confirm scope.
   - Semgrep: `semgrep --config ./semgrep.yml --config p/security-audit
     <path>` to reproduce inline.

2. **Classify (Real / False-positive / Acceptable).**
   - **Real** → fix branch + regression test; PR refs this runbook in
     the description.
   - **False-positive** → see §5 suppression policy below.
   - **Acceptable (waived)** → file an ADR with `expires_at` and link
     it in the suppression comment.

3. **Update the drift ledger** (`AUDIT-2026-05-15-STATIC-ANALYSIS-BASELINE.md`
   §5) with the date, tool, ruleset, delta, triage issue link, and
   final status.

4. **For HIGH/CRITICAL on `main`**: post a Slack notification in
   `#corelink-security` and tag `@security-lead` within the SLA.

---

## 5. False-positive / suppression convention

### 5.1 CodeQL suppression

Use a `// codeql[<rule-id>]` inline suppression on the offending line.
**Required adjacent comment** with the form:

```rust
// codeql[rs/example-rule] — false-positive: <one-line reason>; ADR-XXXX
```

ADR linkage is mandatory. Suppressions without an ADR ref are blocked
by code review.

### 5.2 Semgrep suppression

Use `# nosemgrep: <rule-id>  # reason: <one-line>; ADR-XXXX` on the
offending line. The rule pack (`semgrep.yml`) §header documents this
convention. CoreLink custom-rule suppressions additionally require
Owner approval on the PR (the rule encodes a charter invariant).

### 5.3 Waiver lifetime

All suppressions have a maximum lifetime tied to the referenced ADR's
`expires_at`. The weekly cron audits all `nosemgrep` / `codeql`
comments and posts a warning if any have aged past `expires_at`.

---

## 6. SHA-pin bump procedure

The current CodeQL workflow has three immutable refs, all verified against the
same upstream release:

- `github/codeql-action/init@7188fc363630916deb702c7fdcf4e481b751f97a` — `v4.37.1`.
- `github/codeql-action/analyze@7188fc363630916deb702c7fdcf4e481b751f97a` — `v4.37.1`.
- `github/codeql-action/upload-sarif@7188fc363630916deb702c7fdcf4e481b751f97a` — `v4.37.1`.

The friendly `v4.37.1` declaration, the workflow env value, and all three SHA
comments are one approval unit. A version/SHA mismatch is drift, not a cosmetic
comment issue, and requires correction before merge. Version bumps require an
ADR + Security review (§14.s12.004.1).
- `returntocorp/semgrep-action@v1` → resolve current v1.x tag SHA via
  `gh api repos/returntocorp/semgrep-action/git/ref/tags/v1`.
- `returntocorp/semgrep` container digest → resolve via
  `docker pull returntocorp/semgrep:1.x && docker inspect --format='{{index .RepoDigests 0}}' returntocorp/semgrep:1.x`.

Procedure:

1. Open a PR titled `chore(r-prep): bump CodeQL + Semgrep action pins`.
2. Resolve the new upstream release tag through the GitHub API, verify the
   release commit, and update all three CodeQL refs together. Update the env
   version and trailing friendly-tag comments in the same change; never change
   only the comment or only the SHA.
3. Reference this runbook §6 in the PR body and retain the release/SHA evidence.
4. Require Security review (§14.s12.004.1 — pinning is part of the high-risk
   lane tooling policy) before merging.

---

## 7. Cross-references

- Baseline: `specs/_audits/sealed/2026-05-15-static-analysis-baseline.md`.
- SOC2 evidence rollup: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
  row **CC8.1** (change-management — this runbook is the operational
  arm of the static-analysis gate evidence package).
- Roadmap: `ROADMAP-TO-GA.md` §7 Wave R-7 Evidence Gate.
- Related runbooks:
  - `RB-PENTEST-FINDING-RESPONSE.md` — escalation path for findings
    that overlap with external pentest scope (2026-06-15+).
  - `RB-SECURITY-VULNERABILITY-INTAKE.md` — public/CVE intake; this
    runbook covers internal CI-discovered findings.
  - `RB-SECRETS-DRIFT.md` — companion supply-chain gate runbook.
- Workflows:
  - `.github/workflows/codeql.yml` — CodeQL gate.
  - `.github/workflows/codeql-evidence-watchdog.yml` — independent nightly
    proof that all three CodeQL matrix jobs and their retained SARIF artifacts
    exist. A missing run, failed leg, stale artifact, or unexpected matrix
    population is an evidence gap, not a clean scan.
  - `.github/workflows/semgrep.yml` — Semgrep gate.
- Custom rule pack: `semgrep.yml` (repo root).
- Ignore list: `.semgrepignore` (repo root).
