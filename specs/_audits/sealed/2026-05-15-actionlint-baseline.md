---
id: "AUDIT-ACTIONLINT-BASELINE-2026-05-15"
type: "ci_static_analysis_baseline"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["ci", "github-actions", "actionlint", "supply-chain", "debt-020", "static-analysis"]
---

# GitHub Actions actionlint — Baseline 2026-05-15

> **Mandate (user, 2026-05-15):** *"Nao deixamos debitos aqui, nao se esqueca disso."* + SOTA standard + maximum rigor.
>
> Closure baseline for **DEBT-020** (actionlint CI gate). Companion to `2026-05-15-action-sha-pinning-baseline.md` — SHA pinning protects against supply-chain drift; actionlint protects against syntactic / semantic workflow drift.

---

## 1. Policy

Every workflow under `.github/workflows/*.yml` MUST pass `actionlint` static analysis. The CI gate (`actionlint.yml`) runs on every PR touching `.github/workflows/**` or `.actionlint.yaml` and on every push to `main`. Failures block merge.

`actionlint` complements `scripts/verify-action-sha-pinning.py` (DEBT-018/019). The two gates are orthogonal:

| Gate | What it catches | Lives in |
|---|---|---|
| `action-sha-audit.yml` | Mutable refs (`@v4`, `@main`, `@stable`, 39-char corrupted SHAs) | `scripts/verify-action-sha-pinning.py` |
| `actionlint.yml` | YAML syntax errors, unknown runner labels, undefined contexts, shellcheck violations in `run:` blocks, deprecated `::set-env` / `::add-path` workflow commands, wrong `needs:` graph references | `rhysd/actionlint:1.7.12` |

---

## 2. Baseline (pre-fix)

Audit ran on branch-base commit `44cdf15` (origin/main HEAD at branch point), local actionlint `1.7.12` built from source.

**Pre-fix: 35 errors across 22 workflow files.**

### Breakdown by category

| Category | Count | Files | Severity |
|---|---|---|---|
| Unresolved git merge-conflict markers (YAML unparseable) | 10 | `corelink-hash.yml`, `corelink-meta.yml`, `corelink-worker.yml`, `corelink-client-verify.yml`, `corelink-reapi.yml`, `tenant-path.yml`, `byok_kill_switch_drill_weekly.yml`, `spec_validation.yml`, `tla_check.yml`, `dashboard_validation.yml` | **CRITICAL** — workflows would never run |
| YAML parse failure (embedded Python heredoc indentation bug) | 1 | `ac-bucket-acl-cron.yml` | CRITICAL — flagged as out-of-scope in DEBT-018 baseline §5; now fixed here |
| Undefined expression variable | 1 | `lockfile-diff.yml:166` (`${{repo}}` typo, should be Python `{repo}`) | HIGH — PR-comment URL would render broken |
| Wrong `needs:` graph reference | 1 | `sbom.yml:218` (`needs.sbom-generate.outputs.version` from a job not in its `needs:` list) | HIGH — silent empty-string at runtime |
| Retired runner label | 1 | `release-cli.yml:45` (`macos-13` — removed by GitHub) | HIGH — release CI would fail to provision runner |
| Shellcheck violations in `run:` blocks | 21 | various | INFO / WARNING |

### Pre-fix actionlint output (verbatim, abbreviated)

```
.github/workflows/ac-bucket-acl-cron.yml:77:0: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/byok_kill_switch_drill_weekly.yml:25:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/corelink-client-verify.yml:44:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/corelink-hash.yml:41:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/corelink-meta.yml:45:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/corelink-reapi.yml:54:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/corelink-worker.yml:45:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/dashboard_validation.yml:32:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/spec_validation.yml:31:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/tenant-path.yml:40:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/tla_check.yml:54:12: could not parse as YAML: could not find expected ':' [syntax-check]
.github/workflows/lockfile-diff.yml:166:696: undefined variable "repo" [expression]
.github/workflows/sbom.yml:218:27: property "sbom-generate" is not defined in object type {sbom-ntia-validate: ...} [expression]
.github/workflows/release-cli.yml:45:17: label "macos-13" is unknown [runner-label]
.github/workflows/bazel-starter-ci.yml:167:9: shellcheck SC2129 [shellcheck]
.github/workflows/buck2-starter-ci.yml:230:9: shellcheck SC2155 [shellcheck]
.github/workflows/codeql.yml:174:9: shellcheck SC2086 [shellcheck]
.github/workflows/compliance-weekly.yml:85:9: shellcheck SC2086 (×2), SC2129 [shellcheck]
.github/workflows/cosign-sign.yml:143:9: shellcheck SC2034 (WORKFLOW_REF unused) [shellcheck]
.github/workflows/coverage.yml:65:9: shellcheck SC2016 (×2) [shellcheck]
.github/workflows/docs-ci.yml:152/213/371:9: shellcheck SC2034 (i unused) [shellcheck]
.github/workflows/license-policy.yml:91:9: shellcheck SC2016 [shellcheck]
.github/workflows/notarize-macos.yml:119:9: shellcheck SC2046 [shellcheck]
.github/workflows/pentest-findings-sync.yml:58:9: shellcheck SC2086 [shellcheck]
.github/workflows/release-slsa3.yml:96:9: shellcheck SC2129 [shellcheck]
.github/workflows/reproducible-build.yml:134:9: shellcheck SC2129 (×2) [shellcheck]
.github/workflows/sbom.yml:171:9: shellcheck SC2015 (A && B || C is not if-then-else) [shellcheck]
.github/workflows/subprocessors-sync.yml:116:9: shellcheck SC2034 (rc unused) [shellcheck]
.github/workflows/terraform-drift.yml:141:9: shellcheck SC2129 [shellcheck]
```

---

## 3. Root-cause analysis

### 3.1 Unresolved merge-conflict markers (10 files) — most severe finding

Commit `cdf9458` ("merge wt/debt-010-ci-opt-p1 into main") landed with `<<<<<<< HEAD` / `=======` / `>>>>>>> wt/debt-010-ci-opt-p1` markers literally embedded in 10 workflow files. The commit message lists those files under `# Conflicts:`. The merge was committed without resolving them — those 10 workflows have been completely non-functional since `cdf9458` was pushed to main.

This is a process gap, not just a tooling gap:
- **Why CI didn't catch it:** the affected workflows trigger on `pull_request: paths:` filters for their own scoped paths (`crates/corelink-hash/**`, etc.). The merge commit itself didn't touch those crate paths, so the broken workflows never ran — invisible failure.
- **Why a human didn't catch it:** the post-merge `git status` was clean (markers are valid file content), and the affected workflows weren't part of the merge-commit's stated intent.
- **Why DEBT-018 SHA pinning didn't catch it:** the verifier `verify-action-sha-pinning.py` skips files where any `uses:` SHA appears valid; it doesn't parse YAML structurally. Markers happened to leave at least one valid `uses:` line per file.

This is exactly the class of defect actionlint catches at PR time (YAML must parse, period).

### 3.2 Resolution strategy

Two competing valid states existed for each file:
- **HEAD side** (post-DEBT-018): newer canonical SHA pins (`actions/checkout@v4.3.1`, `Swatinem/rust-cache@v2.9.1`, `dtolnay/rust-toolchain@stable@2026-05-15`) — these are the authoritative SHA values per the action-sha-pinning baseline.
- **Incoming side** (debt-010-ci-opt-p1): structural changes for CI-OPT-001/CI-OPT-004 (crate-scope, remove duplicate workspace gates, remove `audit:` job — cargo-audit moved to dedicated `cargo-audit.yml`).

**Correct merge** = topology from incoming + SHA values from HEAD. Implemented via:
1. Extract debt-010 version of each file via `git show 178cf78:.github/workflows/<file>`.
2. Run mechanical rewrite (`/tmp/rewrite_pins.py`) replacing each known v4.2.2 / v2.7.7 / etc. pin with its post-DEBT-018 canonical counterpart.
3. Collapse `dtolnay/rust-toolchain@<stable-sha>` + `with: toolchain: nightly` (debt-010 form) into `dtolnay/rust-toolchain@<nightly-sha>` (DEBT-018 canonical form, which uses the nightly-branch SHA directly).
4. Verify each result parses as YAML and passes `scripts/verify-action-sha-pinning.py`.

### 3.3 Other fixes — file-by-file

| File | Pre-fix issue | Fix applied | Functional change? |
|---|---|---|---|
| `ac-bucket-acl-cron.yml` | Multi-line `python3 -c "<heredoc>"` had the heredoc body at column 0, breaking YAML block-scalar indentation. | Collapsed the Python heredoc into a single-line `python3 -c '...'` form using `;` separators. | No (same Python logic). |
| `corelink-hash.yml`, `corelink-meta.yml`, `corelink-worker.yml`, `corelink-client-verify.yml`, `corelink-reapi.yml`, `tenant-path.yml`, `byok_kill_switch_drill_weekly.yml`, `spec_validation.yml`, `tla_check.yml`, `dashboard_validation.yml` | Unresolved merge conflict markers. | Resolved per §3.2 above (topology from `178cf78`, SHA pins refreshed to DEBT-018 canonical values). | **Intentional** — these were the structural changes the debt-010 merge was supposed to land. No new semantics introduced by this audit; we restored the intended post-merge state. |
| `lockfile-diff.yml:188` | `${{repo}}` inside a Python f-string was being interpreted by GHA as a context lookup (no such context). Intent was the Python local variable. | Changed to `{repo}` (Python f-string substitution). | Fixed — PR-comment URLs now render the correct repo path. |
| `sbom.yml:194` | `sbom-dt-ingest` job declared `needs: sbom-ntia-validate` but referenced `needs.sbom-generate.outputs.version`. | Expanded to `needs: [sbom-generate, sbom-ntia-validate]`. | Fixed — DT-ingest now actually receives the release version (previously empty string). |
| `release-cli.yml:45` | `macos-13` runner label retired by GitHub. | Switched to `macos-15-intel` for x86_64-apple-darwin builds. | Fixed — release CI would have failed to provision a runner; now uses the supported intel-darwin LTS image. |
| `bazel-starter-ci.yml:167` | SC2129 — 6 successive `>> "$GITHUB_STEP_SUMMARY"` redirects. | Wrapped in `{ ... } >> "$GITHUB_STEP_SUMMARY"`. | No (same output). |
| `buck2-starter-ci.yml:230` | SC2155 — `export VAR="$(cmd)"` masks `cmd`'s exit code. | Split into `VAR=$(cmd); export VAR`. | No (test injects an invalid PAT — exit code of `date +%s` cannot fail in practice; this is style hardening). |
| `codeql.yml:174` | SC2086 — unquoted `$SARIF_GLOB` for `ls`. | Rewrote with `shopt -s nullglob` + array: `sarif_files=(codeql-results/*.sarif)`. | No (same glob semantics; more robust). |
| `compliance-weekly.yml:85` | SC2086 — unquoted `${REF_ARG}` (intentional word-splitting). SC2129 — successive output redirects. | Replaced with a bash array `REF_ARGS=()` + `"${REF_ARGS[@]}"`. Grouped the three `reasons<<JSONEOF` redirects. | No. |
| `cosign-sign.yml:143` | SC2034 — `WORKFLOW_REF` declared but unused. | Removed the dead assignment. | No. |
| `coverage.yml:65` | SC2016 — false positive: GHA `${{ }}` expressions and markdown backticks inside single-quoted echo strings (evaluated before bash by GHA). | Added inline `# shellcheck disable=SC2016` with rationale. | No. |
| `docs-ci.yml:152/213/371` | SC2034 — loop counter `i` declared but unused (we only care about iteration count). | Renamed to `_` (POSIX convention for ignored vars). | No. |
| `license-policy.yml:91` | SC2016 — false positive: markdown backticks inside single-quoted echo. | Added inline disable. | No. |
| `notarize-macos.yml:119` | SC2046 — intentional word-splitting on `$(security list-keychains ...)` to pass each keychain path as a separate arg. | Added inline disable + rationale. | No. |
| `pentest-findings-sync.yml:58` | SC2086 — unquoted `$GITHUB_OUTPUT`. | Quoted. | No. |
| `release-slsa3.yml:96` | SC2129 — three successive `>> "$GITHUB_OUTPUT"`. | Grouped. | No. |
| `reproducible-build.yml:134` | SC2129 (×2) — multiple successive output redirects in if/else branches. | Grouped. | No. |
| `sbom.yml:171` | SC2015 — `A && B \| C` chain falsely reads as if-then-else (`C` will run if `B` fails, not just if `A` fails). | Converted to explicit `if … then … else … fi`. | **Subtle fix** — the old form could set `tsr_present=false` even when TSA succeeded but `echo "tsr_present=true" >> $GITHUB_OUTPUT` itself failed (extremely unlikely but the SC2015 logic gap is real). |
| `subprocessors-sync.yml:116` | SC2034 — `rc=$?` captured but never read. SC2129 — successive output redirects. | Removed dead assignment + grouped redirects. | No. |
| `terraform-drift.yml:141` | SC2129 (×3) — multiple successive output redirects per case branch. | Grouped each. | No. |

---

## 4. Closure (post-fix)

```
$ actionlint -no-color .github/workflows/*.yml
$ echo $?
0
```

**Post-fix: 0 errors across 82 workflow files.**

The verifier `scripts/verify-action-sha-pinning.py` re-confirms 100% SHA-pin coverage:

```
OK: 499 `uses:` line(s) across 82 workflow file(s) are SHA-pinned.
```

(The line count rose from 498 pre-baseline to 499 because `actionlint.yml` itself adds one `actions/checkout` use; the Docker `uses:` line is `docker://rhysd/actionlint@sha256:…` which is excluded from the verifier's third-party action count per the policy in §1 of the SHA-pin baseline.)

---

## 5. Regression prevention

### 5.1 CI gate: `.github/workflows/actionlint.yml`

Triggers on every PR touching `.github/workflows/**`, on changes to `.actionlint.yaml`, and on every push to `main`. Concurrency-cancels superseded runs.

Permissions: `contents: read` (least-privilege).

The action itself is the official `rhysd/actionlint` Docker image, pinned to digest `sha256:b1934ee5f1c509618f2508e6eb47ee0d3520686341fec936f3b79331f9315667` (resolved from Docker Hub `rhysd/actionlint:1.7.12` on 2026-05-15). The friendly tag is in a trailing comment for human readability. When bumping:

1. Pull the new tag's manifest digest from Docker Hub: `curl -sS "https://hub.docker.com/v2/repositories/rhysd/actionlint/tags/?page_size=10" | python3 -c "import json,sys; [print(r['name'], r['digest']) for r in json.load(sys.stdin)['results']]"`
2. Update both the `@sha256:…` digest and the trailing `# rhysd/actionlint:<vX.Y.Z>` comment in `.github/workflows/actionlint.yml`.

### 5.2 Config: `.actionlint.yaml`

Lives at repo root. Today's content:

- `self-hosted-runner.labels: []` — we have no self-hosted runners. An accidental reference to one (or to a retired hosted label like `macos-13`) will fail the lint.
- `config-variables: [DT_ENDPOINT]` — the only `${{ vars.X }}` reference in the repo today (`sbom.yml` Dependency-Track endpoint).

### 5.3 Template checklist

`_TEMPLATE.yml.md` adds an "actionlint clean" item to the conformance checklist (item 13). Authors of new workflows are expected to run `actionlint -no-color .github/workflows/<name>.yml` locally before opening a PR; the CI gate is the backstop.

### 5.4 Allowed shellcheck disables

Three workflow files carry inline `# shellcheck disable=SCxxxx` comments for canonical false-positives. The disable comment MUST be immediately above the offending line and MUST include a one-line rationale. As of this baseline:

| File | Code | Rationale |
|---|---|---|
| `coverage.yml:67` | SC2016 | GHA `${{ }}` expressions + markdown backticks inside single-quoted echo strings — evaluated by GHA (not bash) before the script runs. |
| `license-policy.yml:93` | SC2016 | Markdown backticks inside single-quoted echo strings (literal, not command substitution). |
| `notarize-macos.yml:133` | SC2046 | Intentional word-splitting on `$(security list-keychains ...)` so each keychain path becomes a separate arg to `security list-keychains -s`. |

Any future disable that doesn't carry a rationale is grounds for a `/techlead` flag.

---

## 6. Out-of-scope artifacts surfaced (not blocking)

- The pre-existing `ac-bucket-acl-cron.yml` YAML parse failure that DEBT-018 baseline §5 had flagged "tracked separately — not in scope for SHA pinning closure" is closed here.
- `actionlint` does NOT lint `_TEMPLATE.yml.md` (deliberately a `.md` file so its YAML body is non-loadable). The template body is the reference for human authors; new workflows copy the body and rename to `.yml`, at which point actionlint kicks in.

---

## 7. Cross-references

- `specs/_audits/sealed/2026-05-15-action-sha-pinning-baseline.md` — companion supply-chain baseline (DEBT-018 + DEBT-019 closure).
- `specs/_audits/sealed/2026-05-15-debt-register.md` — DEBT-020 row added + marked CLOSED.
- `specs/_audits/sealed/2026-05-15-ci-workflow-optimization.md` — CI optimization audit; this work is the static-analysis half of that hardening.
- `.github/workflows/actionlint.yml` — the CI gate.
- `.actionlint.yaml` — the config.
- `.github/workflows/_TEMPLATE.yml.md` — author checklist now requires actionlint clean.
- SOC 2 CC7.1 rollup (`specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`) — change-management control evidence (the merge-conflict-marker incident is a process-failure data point for CC7.1 retrospectives).

---

## 8. Change log

| Date | Change | Author |
|---|---|---|
| 2026-05-15 | v1.0.0 — Baseline + closure of DEBT-020. Pre-fix 35 errors / 22 files → post-fix 0 errors. 10 unresolved merge-conflict-marker files resolved (post-DEBT-010-merge incident). | Gustavo (via Claude Opus 4.7 orchestrator, branch `wt/r-prep-actionlint`) |
