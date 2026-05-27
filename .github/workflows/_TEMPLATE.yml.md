# `.github/workflows/_TEMPLATE.yml` — Best-practice workflow template

This document is the canonical reference for authoring new GitHub Actions workflows in this repository. It is intentionally a `.md` (not a runnable `.yml`) so the YAML body is non-loadable; copy the body below into a new `.github/workflows/<name>.yml` file when starting a workflow.

Conformance checklist for every new workflow:

- [ ] SHA-pinned third-party `uses:` (no `@v4`, `@stable`, `@main`)
- [ ] Top-level `permissions: { contents: read }`; job-level upgrades only where needed
- [ ] `paths:` filter on `push` and `pull_request` triggers
- [ ] `concurrency:` block with `cancel-in-progress` PR-only
- [ ] `timeout-minutes:` on every job (never rely on the 360-min default)
- [ ] Rust workflows: `Swatinem/rust-cache` with `shared-key` aligned to workspace policy
- [ ] Node workflows: `cache: pnpm` + `cache-dependency-path: pnpm-lock.yaml`
- [ ] No duplicate workspace-wide gates (defer to `cas_foundation.yml`)
- [ ] No duplicate `validate_specs.py` (defer to `spec_validation.yml`)
- [ ] Secrets scoped to jobs that need them (declared in `env:` at job level, not workflow level, unless cross-job)
- [ ] Job-level `if:` guards for release-only steps (e.g., cosign sign on push-to-main only)
- [ ] Fan-out to matrix jobs where independence allows (TLC specs, OS × language, etc.)
- [ ] **actionlint clean** — `actionlint -no-color .github/workflows/<name>.yml` exits 0 locally; CI gate `actionlint.yml` will re-verify on PR. Catches: invalid syntax, unknown runner labels (e.g. retired `macos-13`), undefined expression contexts, shellcheck violations in `run:` blocks, deprecated `set-env`/`add-path`. Disables for canonical false-positives (GHA `${{ }}` inside single-quoted echos, intentional word-splitting) MUST carry a `# shellcheck disable=SCxxxx` comment with a one-line rationale.

Reference SHA pins (current as of 2026-05-15):

```
actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683        # v4.2.2
actions/setup-python@0b93645e9fea7318ecaed2b359559ac225c90a2b    # v5.3.0
actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020      # v4.4.0
actions/setup-java@3a4f6e1af504cf6a31855fa899c6aa5355ba6c12      # v4.7.0
actions/upload-artifact@6f51ac03b9356f520e9adb1b1b7802705f340c2b # v4.5.0
actions/download-artifact@fa0a91b85d4f404e444e00e005971372dc801d16 # v4.1.8
arduino/setup-protoc@f4d5893b897028ff5739576ea0409746887fa536    # v3.0.0
dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8  # stable as of 2026-04-29
Swatinem/rust-cache@400e7407cfd7a091e5fbb6afec01ec146c432b7c     # v2.7.7
EmbarkStudios/cargo-deny-action@df3b2489a2ea6e663bae7ac3784afc3231452051 # v2.0.4
sigstore/cosign-installer@1aa8e0f2454b781fbf0fbf306a4c9533a0c57409 # v3.7.0
pnpm/action-setup@a3252b78c470c02df07e9d59298aecedc3ccdd6d       # v4.1.0
```

When a new action is needed, look up the SHA at the desired release on GitHub (`/releases/tag/vX.Y.Z` → "commit" → 40-char SHA) and pin with a trailing comment.

---

## Canonical template (copy into `.github/workflows/<name>.yml`)

```yaml
# yaml-language-server: $schema=https://json.schemastore.org/github-workflow.json
name: <WORKFLOW-NAME>

# Short description of what this gates.
# Sprint / WI link: WI-S__-___ §_
# Quality lane: STANDARD | HIGH_RISK
# Fail-closed: YES (per FF-HR-005)
#
# This workflow runs:
#   1. <step 1>
#   2. <step 2>
#   3. <step 3>

on:
  pull_request:
    paths:
      - '<scoped/path/**>'
      - '.github/workflows/<name>.yml'
  push:
    branches: [main]
    paths:
      - '<scoped/path/**>'
      - '.github/workflows/<name>.yml'
  # Add only if relevant:
  # schedule:
  #   - cron: '37 4 * * *'
  # workflow_dispatch:

# Workflow-level least-privilege. Jobs that need more (id-token: write for
# cosign, pull-requests: write for auto-merge) override at job level.
permissions:
  contents: read

# Cancel duplicate in-flight runs on PR force-push. Push-to-main runs are
# NOT cancelled (release safety): the ternary on `cancel-in-progress`
# restricts cancellation to pull_request events.
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.event_name == 'pull_request' }}

env:
  # Common env. Cargo workflows usually want these two:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  # -------------------------------------------------------------------
  # 1. Example Rust gate (workspace cargo lives in cas_foundation.yml;
  # per-crate workflows should run only crate-scoped commands).
  # -------------------------------------------------------------------
  build-test:
    name: build + test (<crate>)
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable as of 2026-04-29
        with:
          components: clippy, rustfmt

      - name: Cargo cache (shared workspace key)
        uses: Swatinem/rust-cache@400e7407cfd7a091e5fbb6afec01ec146c432b7c # v2.7.7
        with:
          workspaces: ". -> target"
          shared-key: workspace-r1   # share with cas_foundation.yml

      # If your crate emits proto stubs, install protoc here:
      # - name: Install protoc
      #   uses: arduino/setup-protoc@f4d5893b897028ff5739576ea0409746887fa536 # v3.0.0
      #   with:
      #     repo-token: ${{ secrets.GITHUB_TOKEN }}

      # Crate-scoped only — do NOT run --workspace here (cas_foundation does that).
      - name: clippy (<crate>, -D warnings)
        run: cargo clippy --package <crate> --all-targets -- -D warnings

      - name: tests (<crate>)
        run: cargo test --package <crate> --all-targets

  # -------------------------------------------------------------------
  # 2. Example Node / pnpm gate
  # -------------------------------------------------------------------
  docs-build:
    name: pnpm build (<package>)
    runs-on: ubuntu-latest
    timeout-minutes: 12
    defaults:
      run:
        working-directory: <apps/path>
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2

      - name: Set up pnpm
        uses: pnpm/action-setup@a3252b78c470c02df07e9d59298aecedc3ccdd6d # v4.1.0
        with:
          version: "10.32.1"

      - name: Set up Node.js
        uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4.4.0
        with:
          node-version: "20"
          cache: pnpm
          cache-dependency-path: pnpm-lock.yaml

      - name: Install
        run: pnpm install --frozen-lockfile

      - name: Build
        run: pnpm run build

  # -------------------------------------------------------------------
  # 3. Example matrix fan-out (TLC, OS × lang, etc.)
  # -------------------------------------------------------------------
  matrix-example:
    name: ${{ matrix.spec }}
    runs-on: ubuntu-latest
    timeout-minutes: 12
    strategy:
      fail-fast: false
      matrix:
        spec: [tenant_isolation, cas_integrity, gc_correctness, audit_immutability]
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - run: timeout 480s bash scripts/run_tlc_corelink.sh ${{ matrix.spec }}

  # -------------------------------------------------------------------
  # 4. Example release-only job with elevated permissions
  # (id-token: write only at job level; only runs on push to main)
  # -------------------------------------------------------------------
  release-sign:
    name: cosign keyless sign (release only)
    runs-on: ubuntu-latest
    timeout-minutes: 10
    needs: [build-test]
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    permissions:
      id-token: write
      contents: read
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: sigstore/cosign-installer@1aa8e0f2454b781fbf0fbf306a4c9533a0c57409 # v3.7.0
        with:
          cosign-release: 'v2.4.1'
      - run: cosign sign-blob --yes --bundle out.bundle out.bin
```

---

## Anti-patterns to avoid

1. **`uses: actions/checkout@v4`** — unpinned floating ref. Pin to SHA.
2. **Workflow-level `permissions: write-all`** — too broad. Default to `contents: read` and grant per job.
3. **No `timeout-minutes:`** — a runaway job consumes 6h before GitHub kills it.
4. **No `concurrency:`** — PR force-pushes spawn parallel duplicates, doubling billable minutes.
5. **Re-running workspace-wide cargo per crate** — `cargo clippy --workspace` belongs in ONE workflow only (`cas_foundation.yml`).
6. **Re-running `validate_specs.py` per ship-gate** — belongs in ONE workflow only (`spec_validation.yml`).
7. **Missing `paths:` on cargo workflows** — docs-only PRs should not trigger Rust gates.
8. **`shared-key: ` omitted on Swatinem/rust-cache** — cross-workflow cache hits become impossible.
9. **Inline `cargo install <tool>` without `--locked --version =X.Y.Z`** — non-reproducible toolchain pulls.
10. **Steps that read secrets without scoping** — declare `env: SECRET: ${{ secrets.X }}` at the step level, not workflow level.

---

## Cross-references

- Audit: `specs/_audits/sealed/2026-05-15-ci-workflow-optimization.md`
- Tickets: `specs/_audits/sealed/ci-optimization-followup-tickets.md`
- ADR-0044 — SHA-pin policy + cosign keyless signing
- WI-S01-007 — `cas_foundation.yml` convergence design
- ROADMAP-TO-GA.md §1 (Wave R-1 cleanup tail)
- Engineering onboarding (post R1-9): `docs/internal/ENGINEERING-ONBOARDING.md` will reference this template.
- `.github/CONTRIBUTING.md` does not exist today; when authored, link this template from its "CI / Workflows" section.

---

## Maintenance

When bumping SHA pins, update both this template AND the reference list in the audit. SHA bumps are mechanical: read GitHub Release notes for the action, find the commit SHA on the tag, replace.

Owner: orchestrator. Next refresh: at S-12 supply-chain hardening or whenever a major action releases (whichever first).
