---
id: "AUDIT-2026-05-15-STATIC-ANALYSIS-BASELINE"
type: "audit_report"
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
tags: ["audit", "static-analysis", "codeql", "semgrep", "r-prep", "soc2-cc8-1", "pre-pentest", "wave-r-prep"]
---

# Static-analysis baseline — CodeQL + Semgrep landing (R-prep wave)

> **Wave:** R-prep (pre-pentest 2026-06-15) · **Date:** 2026-05-15
> **Workflows added:** `.github/workflows/codeql.yml`, `.github/workflows/semgrep.yml`
> **Custom rule pack:** `semgrep.yml` (repo root) — 4 CoreLink-specific rules.
> **Cross-refs:** SOC2 rollup CC8.1; ROADMAP-TO-GA.md §7 Wave R-7 (Evidence Gate);
> runbook `specs/_runbooks/RB-STATIC-ANALYSIS-TRIAGE.md`.

## 1. Purpose

This audit document is the **baseline ledger** for the static-analysis
gate that landed on `main` via PR `wt/r-prep-codeql-semgrep`. It pins
the initial finding counts (per language, per ruleset, per severity) so
that drift on subsequent runs is detectable and triageable against a
known-good starting point.

The data is meant to be re-collected (and committed as a delta) by the
weekly cron jobs (`codeql.yml` Mon 02:00 UTC + `semgrep.yml` Mon 03:00
UTC) — the runbook references this file as the authoritative comparison
target.

## 2. Scope

| Tool | Languages | Trigger | SARIF dest |
|---|---|---|---|
| CodeQL | rust, javascript-typescript, python | PR (paths-filtered) + push main + weekly Mon 02:00 UTC + workflow_dispatch | GHA Security tab, category `/language:*` |
| Semgrep | rust, ts/js, python, yaml, dockerfile (bundled engines) | PR + push main + weekly Mon 03:00 UTC + workflow_dispatch | GHA Security tab, category `semgrep` |

Bundled Semgrep rulesets in use (6):

1. `p/security-audit` — generic OSS security audit.
2. `p/rust` — Rust-specific.
3. `p/typescript` — TS/JS-specific.
4. `p/python` — Python-specific.
5. `p/owasp-top-ten` — OWASP Top-10 patterns.
6. `p/cwe-top-25` — MITRE CWE Top-25 patterns.

Custom CoreLink rules in `semgrep.yml` (4):

1. `corelink.rust.no-unwrap-in-src` — backstop for clippy `unwrap_used` slip-through.
2. `corelink.rust.no-tokio-in-lib-crates` — charter no-tokio rule (lib crates runtime-agnostic).
3. `corelink.rust.prop-assert-matches-struct-variant` — S-08 P1-1 anti-pattern.
4. `corelink.rust.no-expect-in-byok-src` — BYOK extra-strict (no `.expect()` ever).

## 3. Baseline expectations (initial run on `main`)

The first green run is expected to be **clean (zero HIGH/CRITICAL) on
PR mode**. Lower-severity findings are surfaced but do not block. The
table below is a placeholder ledger to be filled in by the first cron
run's step-summary output; the runbook §4 instructs the on-call to
edit this table and PR the result back.

### 3.1 CodeQL baseline

| Language | HIGH+CRITICAL | MEDIUM | LOW/Note | Notes |
|---|---|---|---|---|
| rust | TBD (post first run) | TBD | TBD | Experimental Rust pack — expect noise on `unsafe` blocks (FFI matrix). |
| javascript-typescript | TBD | TBD | TBD | Admin-UI + docs site scope. |
| python | TBD | TBD | TBD | Tooling (`scripts/`, conformance harness). |

### 3.2 Semgrep baseline — bundled rulepacks

| Ruleset | ERROR | WARNING | NOTE | Notes |
|---|---|---|---|---|
| p/security-audit | TBD | TBD | TBD | |
| p/rust | TBD | TBD | TBD | |
| p/typescript | TBD | TBD | TBD | |
| p/python | TBD | TBD | TBD | |
| p/owasp-top-ten | TBD | TBD | TBD | |
| p/cwe-top-25 | TBD | TBD | TBD | |

### 3.3 Semgrep baseline — CoreLink custom rules

Pre-merge sanity run is expected to be CLEAN. Any non-zero count on a
custom rule fails the PR (severity=ERROR by design).

| Rule | Initial count | Notes |
|---|---|---|
| corelink.rust.no-unwrap-in-src | TBD (target=0) | Cross-check against `cargo clippy -- -D clippy::unwrap_used` parity. |
| corelink.rust.no-tokio-in-lib-crates | TBD (target=0) | Existing crates already comply per audit AUDIT-2026-04-29 §charter. |
| corelink.rust.prop-assert-matches-struct-variant | TBD (target=0) | S-08 P1-1 sweep already cleared. |
| corelink.rust.no-expect-in-byok-src | TBD (target=0) | BYOK lane audit AUDIT-2026-04-30 §BYOK already cleared. |

## 4. Trend tracking

The weekly cron jobs emit a step summary with the per-ruleset counts.
The on-call (per `RB-STATIC-ANALYSIS-TRIAGE.md`) is expected to:

1. Open this file weekly.
2. Append a row to §5 (Drift ledger) with the current counts vs the
   baseline (§3) **iff** the delta on HIGH+CRITICAL ≥ 1 or any custom
   CoreLink rule fires.
3. If the delta is positive (regression), open a triage issue per the
   runbook §3 matrix.

Any drift entry that ages > 30 days without resolution blocks PRR-S20-GA
sign-off (ROADMAP §7 R-7 evidence gate hard requirement).

## 5. Drift ledger

| Date | Tool | Ruleset | ΔHIGH | ΔERROR | Triage issue | Status |
|---|---|---|---|---|---|---|
| 2026-05-15 | — | (baseline landed; first run pending) | — | — | — | OPEN |

## 6. Cross-references

- SOC2 evidence rollup: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` row **CC8.1**
  (change-management — static-analysis gate is one of three evidence
  artefacts that anchor PAT-DUAL-APPROVAL-001 + CTRL-SUPPLY-001 for the
  GA period).
- Roadmap gate: `ROADMAP-TO-GA.md` §7 Wave R-7 Evidence Gate — this
  baseline + the runbook are part of the PRR-S20-GA evidence package.
- Pre-pentest scope: `specs/_pentest/scope.md` (cutoff 2026-06-15).
- Triage runbook: `specs/_runbooks/RB-STATIC-ANALYSIS-TRIAGE.md`.
- Related supply-chain gates (parallel evidence under CC8.1):
  - `specs/_audits/cargo-audit-baseline-*.md` (RUSTSEC daily cron).
  - `cargo-deny` daily cron (`.github/workflows/cargo-deny.yml`).
  - `license-policy` enforcement (`.github/workflows/license-policy.yml`).
  - Fuzz nightly (`.github/workflows/fuzz-nightly.yml`).
  - Mutation nightly (`.github/workflows/mutation-nightly.yml`).

## 7. Reviewer notes (frozen on SEAL)

- Workflows use TBD-SHA placeholders for the CodeQL + Semgrep upstream
  actions; the runbook §5 documents the post-baseline SHA bump (replace
  TBD comment with the verified SHA after the first green run on main).
  ADR required for any subsequent version bump per §14.s12.004.1.
- Severity gate threshold (CodeQL): `security-severity >= 7.0` (CVSS
  HIGH+CRITICAL band). Threshold change requires ADR.
- Custom rules are intentionally narrow (path-scoped) to keep
  false-positive rate low for the pre-pentest landing.
