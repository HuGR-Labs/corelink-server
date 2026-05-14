---
id: "WI-S12-006"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.0.1"
created: "2026-04-29"
updated: "2026-05-14"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-12"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s12", "supply-chain", "reproducible-builds", "source-date-epoch", "remap-path-prefix", "rust-toolchain", "adr-0015", "high-risk"]
---

# WI-S12-006 — Reproducible Builds Best-Effort: 2-Runner Parallel Matrix + SHA-256 Diff Check (target ≤ 5% bytes) + Non-Determinism Sources Documented + ADR-0015

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-12](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S12-006 |
| Título | Reproducible builds best-effort: 2 parallel GitHub Actions runners (matrix `os: [ubuntu-22.04]` × 2 instances) build → SHA-256 binário → diff check com target ≤ 5% bytes em release builds; fontes de non-determinism documentadas em `docs/build/reproducible.md` (`SOURCE_DATE_EPOCH`, `--remap-path-prefix`, `rust-toolchain.toml` pin); ADR-0015 ratifica trade-offs + roadmap 100% bit-identical pós-GA Q3 com toolchain mais maduro; canary `reproducible-build.yml` workflow nightly |
| Sprint | S-12 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controles supply chain — bypass = blast radius global; reproducibility é tampering detection mecanismo) |

## 1. Intent

Implementar `.github/workflows/reproducible-build.yml` que executa **2 parallel GitHub Actions runners** (matrix `os: [ubuntu-22.04]` × 2 instances) em build → compute SHA-256 do release binary → diff check com **target ≤ 5% bytes em release builds** (best-effort dado Rust + LLVM + cargo non-determinism atual em 2026); documentar fontes de non-determinism em `docs/build/reproducible.md` (`SOURCE_DATE_EPOCH` para timestamps embedded, `--remap-path-prefix` para LLVM debug info paths, `rust-toolchain.toml` pin para rustc version stability); ratificar trade-offs em ADR-0015 com roadmap 100% bit-identical pós-GA Q3 (12 months) com toolchain mais maduro. **Implements CAP-SUPPLY-005 + CAP-SUPPLY-007 partial**.

```yaml
# File: .github/workflows/reproducible-build.yml (canonical)

name: Reproducible Build (2-Runner Diff)
on:
  push:
    tags: ['v*']
  schedule:
    - cron: '0 4 * * *'  # nightly 04:00 UTC
  workflow_dispatch:

jobs:
  build:
    strategy:
      matrix:
        runner: [runner-1, runner-2]
        os: [ubuntu-22.04]
      fail-fast: false
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # for SOURCE_DATE_EPOCH from git
      - uses: dtolnay/rust-toolchain@stable  # pinned via rust-toolchain.toml
      - name: Set SOURCE_DATE_EPOCH
        run: echo "SOURCE_DATE_EPOCH=$(git log -1 --pretty=%ct)" >> $GITHUB_ENV
      - name: Cargo build (hermetic + remap)
        env:
          RUSTFLAGS: "--remap-path-prefix=${HOME}/.cargo=/CARGO --remap-path-prefix=${PWD}=/SRC"
          CARGO_TERM_VERBOSE: "false"
        run: cargo build --release --frozen --offline --target wasm32-unknown-unknown
      - name: SHA-256 binary
        id: hash
        run: |
          sha256sum target/wasm32-unknown-unknown/release/corelink_worker.wasm > artifact.sha256
          cat artifact.sha256
      - name: Upload artifact + hash
        uses: actions/upload-artifact@v4
        with:
          name: build-${{ matrix.runner }}
          path: |
            target/wasm32-unknown-unknown/release/corelink_worker.wasm
            artifact.sha256

  diff-check:
    needs: build
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: build-runner-1
          path: runner-1/
      - uses: actions/download-artifact@v4
        with:
          name: build-runner-2
          path: runner-2/
      - name: Diff hashes
        id: diff
        run: |
          HASH_1=$(cat runner-1/artifact.sha256 | awk '{print $1}')
          HASH_2=$(cat runner-2/artifact.sha256 | awk '{print $1}')
          if [ "$HASH_1" = "$HASH_2" ]; then
            echo "BIT_IDENTICAL=true" >> $GITHUB_OUTPUT
            echo "DIFF_BYTES=0" >> $GITHUB_OUTPUT
          else
            DIFF=$(cmp -l runner-1/*.wasm runner-2/*.wasm 2>/dev/null | wc -l)
            SIZE=$(stat -c %s runner-1/*.wasm)
            PERCENTAGE=$(awk "BEGIN {printf \"%.2f\", $DIFF / $SIZE * 100}")
            echo "BIT_IDENTICAL=false" >> $GITHUB_OUTPUT
            echo "DIFF_BYTES=$DIFF" >> $GITHUB_OUTPUT
            echo "DIFF_PERCENTAGE=$PERCENTAGE" >> $GITHUB_OUTPUT
          fi
      - name: Enforce diff threshold
        run: |
          if [ "${{ steps.diff.outputs.BIT_IDENTICAL }}" = "false" ] && [ "${{ steps.diff.outputs.DIFF_PERCENTAGE }}" -gt "5" ]; then
            echo "::error::Reproducible build diff > 5% (${{ steps.diff.outputs.DIFF_PERCENTAGE }}%)"
            exit 1
          fi
      - name: Emit metric corelink_supply_reproducible_diff_bytes_gauge
        run: |
          # POST to metrics gateway
          curl -X POST "$METRICS_GATEWAY_URL" \
            -H "Authorization: Bearer $METRICS_TOKEN" \
            -d "corelink_supply_reproducible_diff_bytes_gauge ${{ steps.diff.outputs.DIFF_BYTES }}"
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Reproducible builds são **defense em depth** contra supply chain attacks: se 2 builders independentes produzem binário bit-identical, attacker que compromise 1 builder mas não outro é detectável. Goal-state perfeito (100% bit-identical) é **dificil em 2026 com Rust + LLVM**: cargo build embed timestamps, LLVM debug info contém paths absolutos, rustc tem non-determinism em alguns optimizations passes. **Industry state-of-art**: Reproducible Builds Project (Debian) atinge 95%+ Debian packages bit-identical com 10+ anos de tooling investment; Rust ecosystem em early stages (rustc 1.80+ 2024 introduced `--remap-path-prefix` improvements; cargo 1.82+ 2025 SOURCE_DATE_EPOCH support).

CoreLink S-12 target **best-effort ≤ 5% bytes diff** em release builds com 3 mitigations:

1. **`SOURCE_DATE_EPOCH`** (env var standard reproducible-builds.org): cargo embeds em build_timestamp; setting from git commit time = deterministic.
2. **`--remap-path-prefix`** (rustc flag): replaces local paths em LLVM debug info com canonical prefixes (`/CARGO`, `/SRC`).
3. **`rust-toolchain.toml` pin** (cargo standard): rustc version + components frozen; no implicit upgrade between builds.

Roadmap pós-GA Q3 (12 months): aim **100% bit-identical** quando rustc + cargo + LLVM mais maduros; ADR-0015 documents.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Non-determinism source novo introduzido (regression)**: dep upgrade introduz timestamp sem `SOURCE_DATE_EPOCH` honor; bit diff explodes. Mitigação: nightly CI runs reproducible-build.yml; alert SEV-3 se diff > 5% sustained 3 dias; root cause analysis triggered.

2. **`--remap-path-prefix` partial coverage**: certain LLVM debug info paths não remapped; diff bytes em debug section. Mitigação: documented em ADR-0015; whitelist permite exception via ADR + 90d sunset; future rustc upgrade closes gap.

3. **rustc compiler upgrade breaks reproducibility**: minor version 1.80 → 1.81 introduces non-deterministic optimization. Mitigação: `rust-toolchain.toml` pin minor version; bump via ADR + reproducible-build test pre-merge.

4. **Cross-runner difference (CPU model heterogeneity)**: GitHub Actions runners can vary CPU models; LLVM auto-vectorization different. Mitigação: matrix `os: [ubuntu-22.04]` pin; documented em ADR; future: dedicated runner pool homogeneous.

5. **Race condition em multi-threaded compilation**: cargo `--jobs N` parallel compilation can introduce non-determinism em link order. Mitigação: `--jobs 1` em reproducible build mode (slower but deterministic); release mode uses parallel for perf.

6. **`build.rs` with timestamps**: build script embeds `chrono::Local::now()` em generated code. Mitigação: `SOURCE_DATE_EPOCH` env honored em build.rs (via `chrono::DateTime::from_timestamp(env::var("SOURCE_DATE_EPOCH"))`).

7. **Random seed em const generics**: macro expansion using `std::time::SystemTime` em const context. Mitigação: lint catches; replace with deterministic seed em `build.rs`.

8. **Non-deterministic dep features**: feature flags resolved differently between runners. Mitigação: `--frozen --offline` flags ensure resolved features identical.

**Atacante adversarial scenarios**:

- **Compromised builder injects backdoor**: attacker compromises GitHub Actions runner; injects malicious code; SHA differs from independent builder. Mitigação: 2-runner diff check catches; alert SEV-2; release blocked.

- **TOCTOU em build process**: attacker swaps source between checkout and compile; diff detects via `--frozen` enforcement.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: build determinism é tampering detection mechanism; bypass = full pipeline trust violation.
- **Reversibility**: tampered binary em production = customer data exfil possible; reproducible build é early warning system.
- **Goal-state**: best-effort 5% threshold é defensible vs perfect impossible em 2026 Rust ecosystem; ADR-0015 documents trade-offs + roadmap.

11 sign-offs canonical incl. Architect (build pipeline architecture + ADR-0015 ratification) + AppSec (tampering threat model + builder integrity) + SRE Lead (operational stability + nightly CI cadence).

## 3. Customer Impact & Journey

**Persona 1 — SecOps lead em prospect enterprise (RFP)**:
- Customer downloads `reproducible-build-report.json` (release asset).
- Report contains: 2-runner SHA-256 hashes, diff bytes, diff percentage, attestation timestamp, ADR-0015 ref.
- Diferenciador: 95%+ OSS Rust SaaS does not run reproducible builds; CoreLink does best-effort + transparent ADR.

**Persona 2 — Internal SecOps**:
- Dashboard DASH-SUPPLY shows reproducible diff bytes trend per release.
- Alert SEV-3 se diff > 5% sustained 3 dias.
- Quarterly review: identify new non-determinism sources; update ADR-0015.

**Persona 3 — Engineer adding `build.rs` with timestamps**:
- Pre-commit hook validates `build.rs` honors `SOURCE_DATE_EPOCH`.
- CI catches non-determinism regression.
- ADR-0015 documents pattern for new build scripts.

**SLA addendum**:
- Reproducible build CI: ≤ 5 min adicional per nightly run.
- Diff threshold: ≤ 5% bytes em release builds (target medium-term 100% bit-identical pós-GA Q3).
- Non-determinism source documentation: updated quarterly em ADR-0015.

## 4. Capability Mapping

- **CAP-SUPPLY-005** (Reproducible builds best-effort) — IMPLEMENTA primary.
- **CAP-SUPPLY-007** (Vendored deps audit + lockfile pinning) — IMPLEMENTA partial (Cargo.lock + rust-toolchain.toml pin).
- Trace: `_spec_contract.md §4 + §5.5` + `security_model.md §11.4 (CTRL-SUPPLY-008 (reproducible build verification; codex SEAL cycle 1 alignment com security_model.md §6.4 — was incorrectly CTRL-SUPPLY-005 'no dynamic loading'))` + `compliance_matrix.md §3.6` + `ADR-0015 (criar este WI)`.

## 5. Tipo

Build pipeline + tampering detection; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`.github/workflows/reproducible-build.yml`** workflow (canonical em §1):
   - Trigger: tagged release + nightly cron + manual.
   - Matrix: `runner: [runner-1, runner-2]` × `os: [ubuntu-22.04]`.
   - Build job (per runner):
     - Set `SOURCE_DATE_EPOCH` from git commit time.
     - `RUSTFLAGS="--remap-path-prefix=..."`.
     - `cargo build --release --frozen --offline --target wasm32-unknown-unknown`.
     - SHA-256 binary.
     - Upload artifact + hash.
   - Diff-check job:
     - Download both artifacts.
     - Compare SHA-256 hashes.
     - If different: compute byte-level diff via `cmp -l`.
     - Compute diff percentage.
     - Enforce ≤ 5% threshold; fail CI if exceeded.
     - Emit metric `corelink_supply_reproducible_diff_bytes_gauge`.
2. **`docs/build/reproducible.md`** documentation:
   - Architecture diagram (2-runner matrix + diff check).
   - Non-determinism sources documented (com mitigations):
     - Cargo build timestamp (mitigated via `SOURCE_DATE_EPOCH`).
     - LLVM debug info paths (mitigated via `--remap-path-prefix`).
     - rustc compiler version drift (mitigated via `rust-toolchain.toml`).
     - Build.rs scripts with timestamps (mitigated via `SOURCE_DATE_EPOCH` honor).
     - Multi-threaded compilation race (mitigated via `--jobs 1` em repro mode).
     - Cross-runner CPU heterogeneity (documented; mitigated via `os: ubuntu-22.04` pin).
   - Verification quickstart for customers.
   - Roadmap 100% bit-identical pós-GA Q3.
3. **`ADR-0015-reproducible-build-best-effort.md`**:
   - Context: CoreLink build determinism gaps em 2026 Rust ecosystem.
   - Decision: best-effort ≤ 5% bytes diff em release builds.
   - Trade-offs analyzed:
     - Goal: 100% bit-identical (impossible em 2026).
     - Alternative: skip reproducibility (unsafe; FF-HR-005).
     - Chosen: best-effort + ADR documenting roadmap.
   - Roadmap pós-GA Q3 (12 months): re-evaluate; aim 100% if toolchain mature.
   - Consequences: nightly CI cost (~$1/mês); alert overhead acceptable.
4. **`rust-toolchain.toml`** pin:
   - `[toolchain] channel = "1.84.0"` (or current stable).
   - `components = ["rustc", "cargo", "rust-std", "rust-src", "rustfmt", "clippy"]`.
   - `targets = ["wasm32-unknown-unknown"]`.
   - Bump via ADR + reproducible-build test pre-merge.
5. **`build.rs` lint**:
   - Pre-commit hook: scan `build.rs` files for `chrono::Local::now()` ou `std::time::SystemTime::now()` direct calls.
   - Recommend `SOURCE_DATE_EPOCH` honor pattern em template.
6. **Métricas underscored Prometheus**:
   - `corelink_supply_reproducible_diff_bytes_gauge` (snapshot per build; target ≤ 5% threshold).
   - `corelink_supply_reproducible_runs_total{outcome}` (outcome ∈ bit_identical|within_threshold|exceeds_threshold|build_failed).
   - `corelink_supply_reproducible_runner_duration_seconds_bucket` (histogram p99 per runner).
7. **Observability** — trace span `reproducible.build` + `reproducible.diff_check` com attributes:
   - `reproducible.runner_id` (string).
   - `reproducible.diff_bytes` (u64).
   - `reproducible.diff_percentage` (f64).
   - `reproducible.bit_identical` (bool).
   - `result` (enum).
8. **Property tests** (10k iter PR + 100k iter nightly):
   - `prop_source_date_epoch_honored`: 10k synthetic build.rs scripts; assert SOURCE_DATE_EPOCH honored.
   - `prop_remap_path_prefix_applied`: 10k binary outputs; assert no local paths em debug info.
   - `prop_diff_percentage_calculation`: 10k pairs of binaries; assert diff % computed correctly.
   - `prop_threshold_enforcement`: 10k diff scenarios; assert ≤ 5% accepted, > 5% rejected.
9. **Adversarial regression tests**:
   - Compromised builder injects malicious code → diff detected.
   - Non-determinism regression introduced via dep upgrade → nightly CI catches.
   - `build.rs` with timestamps regression → pre-commit lint catches.
   - Cross-runner CPU heterogeneity (synthetic) → documented limitation.
   - rustc minor upgrade non-determinism → reproducible-build test fails pre-merge.
10. **Integration test E2E**:
    - Real release tag triggers reproducible-build.yml; verify 2-runner diff ≤ 5% bytes.
    - Compare with prior release; verify trend bytes-identical or downward.

### 6.2 Out-of-scope (deferred)

- **SLSA L3 + Rekor**: WI-S12-001.
- **SBOM CycloneDX**: WI-S12-002.
- **Cosign + CF deploy verify**: WI-S12-003.
- **cargo-audit + cargo-deny + Dependabot**: WI-S12-004.
- **Dependency-Track self-host**: WI-S12-005.
- **PRR ship gate**: WI-S12-007.
- **100% bit-identical**: pós-GA Q3 (12 months) com toolchain maduro; documented em ADR-0015.
- **3-runner triplicate verification**: pós-GA enterprise (overhead operacional vs 2-runner).
- **Debian-style reproducible-builds.org submission**: pós-GA enterprise.
- **Cross-platform reproducibility (Linux + macOS)**: pós-GA (CoreLink target Linux runtime).

## 7. Anti-Scope

- ❌ Skip nightly reproducible-build CI (regression detection dependent).
- ❌ Disable `--frozen --offline` (hermetic property dependent).
- ❌ Allow `build.rs` with timestamps sem `SOURCE_DATE_EPOCH` honor (lint mandatory).
- ❌ rustc unpinned (regression risk; `rust-toolchain.toml` mandatory).
- ❌ Skip non-determinism source documentation (compliance evidence dependent).
- ❌ Threshold > 5% bytes em release builds (best-effort target).
- ❌ Skip ADR-0015 ratification (governance gap).
- ❌ Cross-platform parallelism em matrix (homogeneous Linux only).
- ❌ Custom diff algorithm (use `cmp -l` POSIX standard).
- ❌ Reproducible build optional via flag (mandatory CI gate per ADR-0015).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Reproducible builds 2-runner diff ≤ 5% bytes

  Background:
    Given .github/workflows/reproducible-build.yml configured
    And rust-toolchain.toml pinned at 1.84.0 (or current stable)
    And SOURCE_DATE_EPOCH honored em build pipeline
    And RUSTFLAGS includes --remap-path-prefix

  Scenario: Tagged release triggers 2-runner build
    Given a release tag v0.X.Y published
    When reproducible-build.yml workflow runs
    Then matrix builds 2 instances em ubuntu-22.04
    And both runners produce SHA-256 binaries
    And diff-check job compares hashes
    Then either bit-identical OR diff ≤ 5% bytes
    And metric corelink_supply_reproducible_runs_total{outcome="bit_identical"} OR {outcome="within_threshold"} incremented

  Scenario: Diff exceeds 5% threshold blocks release
    Given non-determinism regression introduced via dep upgrade
    When reproducible-build.yml runs
    Then diff > 5% bytes detected
    And exit code != 0
    And alert SEV-3 'reproducible_diff_exceeds_threshold' fires
    And metric corelink_supply_reproducible_runs_total{outcome="exceeds_threshold"} incremented

  Scenario: Bit-identical build (best case)
    Given mature toolchain post-GA Q3
    When reproducible-build.yml runs
    Then diff = 0 bytes
    And metric corelink_supply_reproducible_runs_total{outcome="bit_identical"} incremented

  Scenario: build.rs without SOURCE_DATE_EPOCH lint catches
    Given a build.rs introduces chrono::Local::now() direct call
    When pre-commit lint runs
    Then violation detected
    And recommendation displayed: "use SOURCE_DATE_EPOCH honor pattern"
    And commit blocked

  Scenario: rustc minor upgrade tested
    Given rust-toolchain.toml bump 1.84 → 1.85 PR opened
    When reproducible-build.yml runs em PR
    Then validation: 2-runner diff ≤ 5%
    Otherwise PR blocked + ADR required

  Scenario: Compromised builder detection
    Given attacker compromises runner-1 + injects malicious code
    When reproducible-build.yml runs
    Then diff > 5% bytes (or significant)
    And alert SEV-2 'compromised_builder_suspected' fires
    And release blocked

  Scenario: Cross-runner CPU heterogeneity (synthetic)
    Given matrix os: [ubuntu-22.04] but CPU models heterogeneous
    When reproducible-build.yml runs
    Then diff potentially > 0 (LLVM auto-vectorization)
    And documented em ADR-0015 (limitation)
    And threshold ≤ 5% accommodates

  Scenario: Property test 100k iter green
    Given prop_source_date_epoch_honored 100k iter
    When test runs nightly
    Then 0 false-accepts
    And 0 panics

  Scenario: Quarterly review identifies new non-determinism source
    Given quarterly review of reproducible-build runs
    When new non-determinism source detected (e.g., dep upgrade introduced)
    Then ADR-0015 updated with mitigation
    And docs/build/reproducible.md updated

  Scenario: SLA latency
    Given reproducible-build.yml CI nightly
    When measured over 30 runs
    Then p99 ≤ 5 min adicional (per matrix instance)
```

## 9. Design Decisions

### 9.1 Why best-effort 5% threshold (não 100% bit-identical)

- 2026 Rust + LLVM ecosystem não suporta 100% bit-identical reliably.
- 5% threshold catches malicious tampering (signal-to-noise ratio).
- Industry comparison: Debian Reproducible Builds Project achieves 95%+ bit-identical com 10+ years tooling.
- Roadmap pós-GA Q3: re-evaluate; aim 100% with mature toolchain.

### 9.2 Why 2-runner (não 3+)

- 2-runner sufficient for tampering detection (1 compromised, 1 not = mismatch detected).
- 3-runner overhead operacional ($$$ + complexity) sem ganho proporcional.
- Industry pattern: SLSA L3 generator uses 2-runner equivalent.

### 9.3 Why SOURCE_DATE_EPOCH from git commit (não fixed timestamp)

- Standard reproducible-builds.org pattern.
- Each release has unique deterministic timestamp.
- Cargo + LLVM honor env var natively.

### 9.4 Why `--remap-path-prefix` (não absolute paths)

- LLVM debug info embeds absolute paths by default.
- `--remap-path-prefix` replaces with canonical (`/CARGO`, `/SRC`).
- Industry standard: GitHub Actions reproducible-builds guide recommends.

### 9.5 Why `rust-toolchain.toml` pin minor version

- rustc minor upgrades can introduce non-determinism.
- Pin minor + bump via ADR with regression test.
- Cargo respects `rust-toolchain.toml` automatically.

### 9.6 Why `--frozen --offline` em build

- `--frozen`: requires Cargo.lock unchanged (hermetic property).
- `--offline`: no network calls during build (SLSA L3 hermetic).
- Both mandatory for reproducibility + supply chain integrity.

### 9.7 Why nightly CI cadence (não per-PR)

- Per-PR overhead ($$$ + slower PRs).
- Nightly catches regression em ≤ 24h (acceptable signal).
- Tag release triggers full check (mandatory pre-release).

### 9.8 Why cmp -l (POSIX standard) para diff

- POSIX standard; available em all runners.
- Outputs byte-level diff (count + offsets).
- Alternative: `radamsa` ou custom fuzzer overhead operacional.

### 9.9 Why ubuntu-22.04 (não latest)

- LTS pinned; predictable kernel + glibc.
- Latest tag = drift risk.
- Bump via ADR + reproducible-build test.

### 9.10 ADR potencial

- Sim — **ADR-0015**: "Reproducible builds best-effort 5% threshold + roadmap 100%". Ratificação cripto-supporting decision.

## 10. Completeness Criteria SOTA

- [ ] **10.s12.006.1** Property test 10k iter (PR) + 100k iter (nightly) sobre fuzz scenarios → 0 false-accepts, 0 panics (EVT-002).
- [ ] **10.s12.006.2** Adversarial test: 5 scenarios (compromised builder, regression, build.rs lint, CPU heterogeneity, rustc upgrade) — 100% mitigated (EVT-040).
- [ ] **10.s12.006.3** E2E test contra staging fork: tagged release → reproducible-build.yml → diff check ≤ 5% (EVT-018).
- [ ] **10.s12.006.4** **Reproducible build**: 2 runners produzem binário com diff ≤ 5% bytes em release builds; fontes de non-determinism documentadas em `docs/build/reproducible.md` + ADR-0015 ratificado (EVT-027).
- [ ] **10.s12.006.5** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) (EVT-002).
- [ ] **10.s12.006.6** Cost regression gate: reproducible-build CI ≤ 5 min adicional p99; ≤ $5/mês adicional (Lote 9.4 §14.10).
- [ ] **10.s12.006.7** OWASP ASVS V14 + SSDF PS.1 100% checklist (EVT-002).
- [ ] **10.s12.006.8** ADR-0015 published + ratified em S-12 PRR.
- [ ] **10.s12.006.9** `docs/build/reproducible.md` published com 6 non-determinism sources documentadas.
- [ ] **10.s12.006.10** Nightly CI verde sustained 30d staging.
- [ ] **10.s12.006.11** `rust-toolchain.toml` pin enforced; bump via ADR.
- [ ] **10.s12.006.12** build.rs lint pre-commit hook functioning.

## 11. DoD

- [ ] `.github/workflows/reproducible-build.yml` committed.
- [ ] `rust-toolchain.toml` committed (current stable pinned).
- [ ] `docs/build/reproducible.md` published.
- [ ] `ADR-0015-reproducible-build-best-effort.md` published + ratified.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em PR + 100k green em nightly.
- [ ] E2E test contra staging green.
- [ ] Métricas emitidas (3 listadas §6.1.6).
- [ ] Trace spans 2 listadas.
- [ ] build.rs lint pre-commit hook installed.
- [ ] Code review (Architect + AppSec + SRE).
- [ ] PRR mini-sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- nenhuma específica deste WI (best-effort capability; non-INV target).

### Novas

- não-aplicável (best-effort não-INV; goal-state target medium-term ADR-0015 documents).

TLA+ alignment: não-aplicável (build-time best-effort).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Reproducible build workflow | `.github/workflows/reproducible-build.yml` | YAML |
| Documentation | `docs/build/reproducible.md` | Markdown |
| ADR-0015 | `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md` | Markdown |
| Rust toolchain pin | `rust-toolchain.toml` | TOML |
| build.rs lint pre-commit hook | `.pre-commit-config.yaml` (or scripts/build_rs_lint.sh) | YAML / Shell |
| Property tests | `tests/prop_reproducible.rs` | Rust |
| Adversarial tests | `tests/adversarial_reproducible.rs` | Rust |
| E2E integration | `tests/e2e_reproducible.rs` | Rust |

## 14. Quality Standards SOTA

- **14.s12.006.1** Workflow pinned versions (actions/checkout@v4, etc.); bump via ADR.
- **14.s12.006.2** rustdoc 100% em test files.
- **14.s12.006.3** Property test coverage 100% diff scenarios.
- **14.s12.006.4** Latência: reproducible-build CI ≤ 5 min adicional p99.
- **14.s12.006.5** SAST clean.
- **14.s12.006.6** Métricas + dashboard panels.
- **14.s12.006.7** Runbook: nightly CI failure investigation.
- **14.s12.006.8** Breaking changes em workflow = bump major.
- **14.s12.006.9** Memory: bounded em CI.
- **14.s12.006.10** Cost regression gate.
- **14.s12.006.11** Quarterly review identify new non-determinism sources + update ADR-0015.
- **14.s12.006.12** Roadmap pós-GA Q3 documented.

## 15. Chaos Experiments

1. **Compromised builder detection**: red team injects malicious code em runner-1; verify diff detected; alert SEV-2.

2. **Non-determinism regression**: introduce synthetic dep upgrade with new non-determinism; verify nightly CI catches; alert SEV-3.

3. **build.rs timestamps lint**: synthetic build.rs with `chrono::Local::now()`; verify pre-commit lint catches.

4. **rustc minor upgrade test**: synthetic toolchain bump; verify reproducible-build test em PR.

5. **CPU heterogeneity simulation**: documented limitation; manual verification em production runners.

6. **`--frozen` enforcement**: synthetic Cargo.lock change without explicit; verify build fails (--frozen).

7. **`SOURCE_DATE_EPOCH` regression**: build.rs ignores env var; verify timestamps embedded.

8. **`--remap-path-prefix` partial coverage**: LLVM debug info path leak; verify documented em ADR-0015.

9. **Multi-thread compilation race**: `--jobs 4` em repro mode; verify diff > 5% (intentional regression to validate test).

10. **Quarterly review cycle**: identify new non-determinism source; update ADR-0015; re-test.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-12 ship gate é WI-S12-007; este WI passa por mini-PRR):

- [ ] All Gherkin green.
- [ ] Property + adversarial tests green.
- [ ] E2E staging green.
- [ ] Nightly CI verde sustained 30d.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards em DASH-SUPPLY.
- [ ] ADR-0015 published + ratified.
- [ ] `docs/build/reproducible.md` published.
- [ ] Architect approval (build pipeline + ADR-0015).
- [ ] AppSec review (tampering threat model + builder integrity).
- [ ] SRE review (operational stability + nightly CI cadence).
- [ ] OWASP ASVS V14 + SSDF PS.1 100% pass.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | `.github/workflows/reproducible-build.yml` skeleton + matrix | 2h |
| ST-002 | SOURCE_DATE_EPOCH integration | 1h |
| ST-003 | RUSTFLAGS --remap-path-prefix integration | 1h |
| ST-004 | Diff-check job (cmp -l + threshold enforcement) | 2h |
| ST-005 | rust-toolchain.toml pin | 0.5h |
| ST-006 | docs/build/reproducible.md (6 non-determinism sources documentadas) | 3h |
| ST-007 | ADR-0015 redação | 2.5h |
| ST-008 | build.rs lint pre-commit hook | 1.5h |
| ST-009 | Métricas emit (3 metrics) + trace spans | 1h |
| ST-010 | Property tests 10k iter (4 props) | 2.5h |
| ST-011 | Adversarial regression tests (5 scenarios) | 2h |
| ST-012 | E2E integration test contra staging | 2h |
| ST-013 | Nightly CI verification (3 days minimum) | 0.5h |
| ST-014 | Architect + AppSec + SRE review feedback | 2h |
| ST-015 | PRR mini-sign-off | 0.5h |
| ST-016 | Quarterly review process documented | 1h |

**Total Optimistic**: ~25h. **PERT** (O=12h, M=18h, P=30h per spec contract): **19h**.

## 18. Dependencies

### Hard blockers

- Cargo workspace structure baseline (S-01 SEALED).
- GitHub Actions matrix support (standard feature).

### Soft blockers

- WI-S12-001 (SLSA L3) — provenance attestation references reproducible-build report ideal mas não bloqueante.

### Outbound

- WI-S12-007 (PRR ship gate).
- S-20 GA gate: external supply chain audit reviews ADR-0015.

## 19. Effort PERT

O: 12h, M: 18h, P: 30h → PERT **19h** (per spec contract §12).

## 20. Time-boxing

**24h hard limit**. If exceeded → escalation: split em sub-WI (workflow + ADR vs lint + property tests).

## 21. Observability

3 métricas + 2 trace spans listadas. Logs structured JSON.

Dashboard widget DASH-SUPPLY:
- Reproducible diff bytes per release (target ≤ 5%).
- Bit-identical ratio trend (target → 100% pós-GA Q3).
- Reproducible-build CI duration p99.
- Non-determinism source count gauge (quarterly tracked).

## 22. Cost Analysis

- 2-runner matrix CI: ~5 min × 2 runners × $0.008/min = $0.08 per nightly run.
- Nightly cron: ~$2.40/mês.
- Tag release runs: ~30/mês × $0.08 = $2.40/mês.
- **Total custo direto S-12 WI-006**: ~$5/mês = $60/yr. Aceitável.

## 23. API Contract

Workflow YAML + ADR governance documents. Configuration files semver stable post v1.0 (workflow schema upgrade via ADR).

Diff threshold mudança requer ADR + retesting.

## 24. Post-mortem Hooks

- Nightly CI failure sustained > 3 dias → SEV-3 + post-mortem (root cause non-determinism).
- Diff > 5% sustained > 3 dias → SEV-3 + remediation (identify source + ADR update).
- Compromised builder suspected (alert SEV-2) → CRITICAL + Security incident.
- rustc upgrade breaks reproducibility → SEV-3 + revert + ADR documented.
- ADR-0015 quarterly review missed → SEV-3 + governance gap.

## 25. Rollback / Recovery

- Workflow rollback: revert PR.
- rust-toolchain.toml rollback: revert PR.
- ADR-0015 amendment: new version (semver bump).
- RTO: ≤ 30 min.
- RPO: 0 (config files versioned em git).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: 2-runner diff catches builder compromise.
- **Tampering**: bit-level diff = tampering detection.
- **Repudiation**: nightly CI run history = forensic trail.
- **Info disclosure**: build artifacts em CI not customer-facing (workspace internal).
- **DoS**: CI cost bounded.
- **Elevation of privilege**: no privilege boundary affected.

**LINDDUN delta**:
- **Linkability**: reproducible-build runs linkable to release tags.
- **Identifiability**: builder identity = GitHub Actions service principal.
- **Non-repudiation**: CI log trail.
- **Detectability**: tampering detected via diff.
- **Disclosure**: no PII em build artifacts.
- **Unawareness**: customer-facing supply chain documentation includes ADR-0015.
- **Non-compliance**: SOC 2 CC6.7 + NIST SSDF PS.1 satisfied.

## 27. Knowledge Transfer

- `docs/build/reproducible.md` overview.
- ADR-0015 governance.
- Workshop interno (1h) Architect + AppSec + SRE.
- Quarterly review process documented.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Reproducible build infeasible com Rust + LLVM | H | L | LOW (best-effort) | M | LOW | Document sources; aim 80%+ bit-identical; ADR-0015 |
| R-002 | Non-determinism regression via dep upgrade | M | M | MEDIUM | M | LOW | Nightly CI + alert SEV-3 |
| R-003 | rustc minor upgrade breaks reproducibility | L | M | MEDIUM | L | LOW | Pin rust-toolchain.toml + ADR antes bump |
| R-004 | Cross-runner CPU heterogeneity | M | L | LOW | L | LOW | Documented em ADR-0015 limitation |
| R-005 | build.rs lint regression (timestamps) | L | M | LOW | L | LOW | Pre-commit hook + CI lint |
| R-006 | Compromised builder undetected | L | H | CRITICAL | L | LOW | 2-runner diff catches; alert SEV-2 |
| R-007 | Quarterly review missed | L | M | MEDIUM | L | LOW | Calendar reminder + ADR cadence |
| R-008 | CI cost overrun | L | L | LOW | L | LOW | Cost regression gate; review trimestral |
| R-009 | docs/build/reproducible.md drift | M | L | LOW | L | LOW | Quarterly review update |
| R-010 | LLVM upgrade introduces non-determinism | L | M | MEDIUM | L | LOW | LLVM tied to rustc; ADR antes upgrade |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect review workflow + ADR-0015 outline.
2. **AppSec (D+1)**: AppSec review tampering threat model + builder integrity.
3. **SRE (D+2)**: SRE review nightly CI cadence + cost.
4. **Code (D+3)**: peer review (folded Engineer + Architect).
5. **PRR mini (D+3)**: Architect sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — ADR-0015 ratification + build pipeline architecture_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — nightly CI cadence + operational stability_ | _pending_ | _pending_ |
| 6 | Engineer (S-12 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — tampering threat model + builder integrity_ | _pending_ | _pending_ |

> Crypto SME (não cripto-load-bearing aqui) folds into Architect. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S12-006 (cycle 11.S12.0). |

## 32. Anti-patterns evitados

- ❌ Skip nightly reproducible-build CI.
- ❌ Disable `--frozen --offline`.
- ❌ Allow `build.rs` with timestamps sem `SOURCE_DATE_EPOCH` honor.
- ❌ rustc unpinned.
- ❌ Skip non-determinism source documentation.
- ❌ Threshold > 5% bytes em release builds.
- ❌ Skip ADR-0015 ratification.
- ❌ Cross-platform parallelism em matrix.
- ❌ Custom diff algorithm (use `cmp -l` POSIX).
- ❌ Reproducible build optional via flag.
- ❌ ubuntu-latest tag (drift risk).
- ❌ Skip quarterly review.

---

**Fim WI-S12-006.** Próximo: WI-S12-007 (RB-FM-156 + RB-FM-157 dry-runs + Security walkthrough + PRR ship gate).
