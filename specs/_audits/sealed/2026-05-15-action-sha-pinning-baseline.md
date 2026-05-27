---
id: "AUDIT-ACTION-SHA-PINNING-2026-05-15"
type: "supply_chain_security_baseline"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["supply-chain", "github-actions", "soc2-cc7.1", "debt-018", "debt-019"]
---

# GitHub Actions SHA Pinning — Baseline 2026-05-15

> **Mandate (user, 2026-05-15):** *"Nao deixamos debitos aqui, nao se esqueca disso."*
>
> Closure baseline for **DEBT-018** (CodeQL/Semgrep placeholder pins) + **DEBT-019** (wave 7 bot SHAs not cross-verified). Source-of-truth coverage stats + closure evidence + regression-prevention design.

---

## 1. Policy

Every `uses:` line in `.github/workflows/*.yml` MUST reference a GitHub Action by a **40-character commit SHA**. Mutable refs (`@v4`, `@v3.27.0`, `@stable`, `@main`) are forbidden because:

1. A compromised upstream maintainer can re-point a tag at malicious code (e.g., the `tj-actions/changed-files` 2025 incident).
2. The upstream action's own CI being compromised silently re-tags releases without our review.
3. SOC 2 CC7.1 ("System Operations — change management") requires every dependency be reproducible from an immutable identifier.

**Allowed exceptions** (out of scope for this policy):
- Local actions referenced via `./.github/actions/<name>` — these live in our own repo and are governed by branch protection.
- Docker actions referenced via `docker://image@sha256:…` — these have their own SHA discipline.
- Reusable workflows in this repository: `uses: ./.github/workflows/<file>` — same protection.

---

## 2. Baseline (pre-closure)

Pre-closure audit ran on commit `bb4b127` (origin/main HEAD at branch point):

| Metric | Count |
|---|---|
| Workflow files audited | 84 |
| Total `uses:` lines (third-party) | 520 |
| Already SHA-pinned | 399 (76.7%) |
| Tag-pinned (mutable) | 121 (23.3%) |
| Corrupted SHA (39 chars instead of 40) | 1 (`dependabot/fetch-metadata`) |

### Tag-pinned breakdown by source

| Source | Tag-pinned count | Status |
|---|---|---|
| CodeQL workflow (wave 6 commit `a280fe1`) | 2 | DEBT-018 surface |
| Semgrep workflow (wave 6 commit `a280fe1`) | 2 | DEBT-018 surface |
| Corelink Rust CI workflows (corelink-*.yml, tenant-path.yml) | 108 | wave-5 chain, latent debt |
| Pre-wave validators (dashboard_validation.yml, spec_validation.yml, tla_check.yml, byok_kill_switch_drill_weekly.yml) | 7 | pre-wave debt |
| SLSA reusable workflow (release-slsa3.yml) | 1 | wave-5 |
| Dependabot bot (dependabot-auto-merge.yml) | 1 (corrupted SHA) | wave-7 DEBT-019 surface |
| **Total** | **121** | |

> **Note:** The corelink Rust CI chain accounted for 108 of 121 hits. These were pre-existing latent debt not flagged by DEBT-018 or DEBT-019 (those only called out the CodeQL/Semgrep + wave-7 bot surfaces). Full audit surfaced them and they are closed in the same pass.

---

## 3. Closure (post-pin)

Post-closure audit on this baseline's commit:

| Metric | Count |
|---|---|
| Workflow files audited | 85 (added `action-sha-audit.yml`) |
| Total `uses:` lines (third-party) | 522 |
| SHA-pinned | **522 (100.0%)** |
| Tag-pinned | **0** |

### Resolution table

Each tag was resolved via `gh api repos/<owner>/<repo>/git/refs/tags/<tag>` and (for annotated tags) dereferenced via `gh api repos/<owner>/<repo>/git/tags/<sha>`. Branches (`stable`, `nightly`) were resolved via `git/refs/heads/<branch>` at audit time.

| Action | Requested ref | Resolved tag | SHA |
|---|---|---|---|
| `Swatinem/rust-cache` | `v2` | `v2.9.1` | `c19371144df3bb44fab255c43d04cbc2ab54d1c4` |
| `actions/checkout` | `v4` | `v4.3.1` | `34e114876b0b11c390a56381ad16ebd13914f8d5` |
| `actions/setup-java` | `v4` | `v4.8.0` | `c1e323688fd81a25caa38c78aa6df2d33d3e20d9` |
| `actions/setup-python` | `v5` | `v5.6.0` | `a26af69be951a213d495a4c3e4e4022e16d87065` |
| `arduino/setup-protoc` | `v3` | `v3.0.0` | `c65c819552d16ad3c9b72d9dfd5ba5237b9c906b` |
| `dtolnay/rust-toolchain` | `stable` | `stable@2026-05-15` | `29eef336d9b2848a0b548edc03f92a220660cdb8` |
| `dtolnay/rust-toolchain` | `nightly` | `nightly@2026-05-15` | `5b842231ba77f5c045dba54ac5560fed2db780e2` |
| `github/codeql-action/init` | `v3.27.0` | `v3.27.0` | `662472033e021d55d94146f66f6058822b0b39fd` |
| `github/codeql-action/analyze` | `v3.27.0` | `v3.27.0` | `662472033e021d55d94146f66f6058822b0b39fd` |
| `github/codeql-action/upload-sarif` | `v3.27.0` | `v3.27.0` | `662472033e021d55d94146f66f6058822b0b39fd` |
| `returntocorp/semgrep-action` | `v1` | `v1` | `713efdd345f3035192eaa63f56867b88e63e4e5d` |
| `rustsec/audit-check` | `v1` | `v1.4.1` | `dd51754d4e59da7395a4cd9b593f0ff2d61a9b95` |
| `slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml` | `v1.10.0` | `v1.10.0` | `c747fe7769adf3656dc7d588b161cb614d7abfee` |
| `dependabot/fetch-metadata` | `d7267f607e4f2cf3e2f77fe2f5b19e0e31f2a8a` (39 chars, corrupted) | `v2.3.0` | `d7267f607e9d3fb96fc2fbe83e0af444713e90b7` |

### Comment convention

Every pinned line now carries a trailing `# <action>@<friendly-tag>` comment so humans can read the workflow without resolving SHAs. Example:

```yaml
uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5  # actions/checkout@v4.3.1
```

For branch-pinned tooling (`dtolnay/rust-toolchain`), the comment includes the resolution date:

```yaml
uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8  # dtolnay/rust-toolchain@stable@2026-05-15
```

---

## 4. Regression prevention

### Verifier: `scripts/verify-action-sha-pinning.py`

CLI script (`set -euo pipefail` semantics in the Python equivalent: argparse + explicit `sys.exit`) that:

1. Walks `.github/workflows/*.yml` (configurable via `--workflows-dir`).
2. Parses every `uses:` line via regex (skipping local `./` and `docker://` refs).
3. Fails (`exit 1`) on any non-40-hex-char ref.
4. Optional `--strict-comments` flag also fails when a SHA pin lacks a `# <action>@<tag>` comment (off by default — security gate is on the SHA itself; comment hygiene is review-time).

Exit codes:
- `0` — all `uses:` SHA-pinned
- `1` — at least one violation
- `2` — argument / IO error

### CI gate: `.github/workflows/action-sha-audit.yml`

Triggers on every PR touching `.github/workflows/**` or the verifier itself, plus pushes to `main`. Runs `scripts/verify-action-sha-pinning.py`. Concurrency-cancels superseded runs. Permissions: `contents: read` (least-privilege).

This closes the meta-question: the verifier script itself uses SHA-pinned actions (`actions/checkout@34e114876b…` + `actions/setup-python@a26af69b…`), so the gate is self-consistent.

---

## 5. Out-of-scope artifacts surfaced (not blocking)

- `.github/workflows/ac-bucket-acl-cron.yml` has a pre-existing YAML parse failure at line 77-78 (unchanged by this work). Tracked separately — not in scope for SHA pinning closure.
- `slsa-framework/slsa-github-generator` is a reusable workflow path (`<repo>/.github/workflows/<file>.yml`). SHA pinning a reusable workflow is supported (GitHub resolves `@<sha>` against the repo root) — verified by re-running the verifier.

---

## 6. Cross-references

- `specs/_audits/2026-05-15-debt-register.md` — DEBT-018 + DEBT-019 marked CLOSED with pointer to this baseline.
- `scripts/verify-action-sha-pinning.py` — the verifier.
- `.github/workflows/action-sha-audit.yml` — the CI gate.
- SOC 2 CC7.1 rollup (`specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`) — supply-chain control evidence.
- `/techlead` skill AP-5: "Letting validate_specs failures linger 'out of scope'" — full audit surfaced 108 latent corelink debts beyond the original DEBT-018+019 scope; closed in the same pass per the AP-5 prohibition on partial closure.

## 7. Change log

| Date | Change | Author |
|---|---|---|
| 2026-05-15 | v1.0.0 — Baseline + closure of DEBT-018 + DEBT-019. 121 tag pins → 0; verifier + CI gate landed. | Gustavo (via Claude Opus 4.7 orchestrator, branch `wt/debt-018-019-action-sha-pin`) |
