# Security-CI local bundle — B-128 / B-139 / B-142

Date: 2026-09-08 (America/Sao_Paulo)

This is a bounded local execution record for the security-CI lanes. GitHub
Actions startup is unavailable in this environment because the organization
rejects hosted jobs before runner allocation; no Actions run is represented as
green evidence here. Secret values and finding payloads are intentionally not
included.

## Source and tools

| item | value |
|---|---|
| source checkout before this change | `56207153f556b4c8c48680f29ce7c25564791840` |
| Semgrep | `1.164.0` (isolated Python 3.12 venv) |
| gitleaks | `8.30.1` |
| secrets-matrix interpreter | Python `3.14.5` |
| CodeQL CLI | unavailable locally (not installed; no Java runtime) |

## Bundle commands

* `bash tests/test_classify_runner_failure.sh` — B-128 wrapper contract,
  including storage/linker classification, mixed-failure polarity, signal
  status preservation, and bounded timeout.
* Semgrep 1.164.0 with the six bundled packs plus `./semgrep.yml`, SARIF
  output, and metrics disabled. At the pre-policy baseline (`--error`) it
  scanned 3,687 tracked files with 474 rules and reported 3,915 findings
  (exit 1); 3,711 were `corelink.*` findings. No finding text is copied here.
  The source policy change removes `--error` pending rule-by-rule triage, so a
  later dispatch can report this baseline without converting it into a false
  red.
* `gitleaks detect --source . --config .gitleaks.toml --redact --no-banner
  --exit-code 1` (full-history local equivalent of the scheduled lane): 6,795
  commits / ~296 MB scanned, 26 redacted baseline findings (exit 1). These
  are pre-existing history findings, not values introduced by this change.
* `python3 scripts/validate_secrets_matrix.py --json-out <redacted temp path>`
  (exit 0), `python3 scripts/check-env-contract.py` (exit 0), and
  `bash scripts/secrets-checklist-verify.sh` (exit 0; stale matrix rows remain
  soft warnings) (the secrets-drift lane's three validators). The bash
  verifier now applies the same exact-path exclusion as the Python validator
  for the three synthetic raw-curl fixture variables.
* Workflow-structure inspection confirms CodeQL and secrets-drift scheduled
  jobs select `corelink`; this is the local B-142 equivalent. It does not claim
  that GHAS-backed CodeQL analysis ran while GHAS is disabled.

The final commit SHA and command exit/count summary are appended by the agent
that integrates this isolated worktree.

## Review repair (local, no live-closure claim)

The review repair is based on `9258fe48a97f29105fe104b603956bf3ee49177c`
(the source bundle commit) and is intentionally still open for runtime
evidence:

* B-142 remains `open`: CodeQL and secrets-drift select `corelink`; CodeQL
  accepts manual dispatch only from `refs/heads/main`; permissions are limited
  to `contents: read` and `security-events: write`; checkout credentials are
  not persisted. The GHAS preflight recognizes only an explicit
  Advanced-Security/code-scanning-disabled response as disabled. Auth,
  network, malformed, and other API failures fail closed with redacted error
  context. An enabled run with no `codeql-results/**/*.sarif` fails closed.
  This is source evidence, not a live CodeQL run.
* The secrets-drift lane uses `pull_request_target`, checks out the trusted
  base at `.trusted`, checks out the PR head at `.candidate` as data, and runs
  only absolute trusted-base validator paths. Both checkouts use
  `persist-credentials: false`; manual dispatch is main-only. No candidate
  script is executed. The adversarial test replaces the candidate validator
  with a marker-writing process and proves the marker is untouched.
* The synthetic `CORELINK_HTTP_*` exclusions are tested as exact-path
  exclusions: the raw-curl fixture passes, while the same three names added to
  a production path fail the trusted bash verifier.
* B-139 remains `open/manual`: Semgrep is report-only pending rule-by-rule
  promotion policy and a real dispatch. B-128 remains `open`: the wrapper
  contract is locally tested, but induced runner ENOSPC/linker evidence is
  necessarily runtime evidence.

### Repair validation

| command | result |
|---|---|
| `bash tests/test_classify_runner_failure.sh` | PASS |
| `python3 -m pytest -q tests/test_secrets_drift_security.py tests/test_validate_secrets_matrix.py` | 7 passed |
| `python3 scripts/check-env-contract.py` | PASS; 53/53 vars forwarded |
| `bash scripts/secrets-checklist-verify.sh --self-test` | PASS |
| `actionlint 1.7.12 .github/workflows/{codeql,secrets-drift,semgrep}.yml` | PASS |
| `git diff --check` | PASS |
| structural B-142 scan | 13 `ubuntu-*` jobs; 0 naked scheduled jobs; remains open pending runtime |

Action references are SHA-pinned in source: checkout
`9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0`, CodeQL action
`7188fc363630916deb702c7fdcf4e481b751f97a`, and upload-artifact
`043fb46d1a93c77aae656e7c1c64a875d1fc6a0a`. Local tool versions are Python
3.14.5, actionlint 1.7.12, Semgrep 1.164.0, and gitleaks 8.30.1. No secret
values, API responses, CodeQL findings, or SARIF payloads are recorded here.
