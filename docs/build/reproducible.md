# Reproducible Builds — CoreLink Best-Effort Guide

> **WI-S12-006** | CAP-SUPPLY-005 | Version 1.0.0 | 2026-05-14
>
> Implements best-effort reproducible builds with a 2-runner SHA-256 diff
> check (target ≤ 5 % bytes in release builds) and documents all known
> non-determinism sources.  ADR-0015 ratifies the 5 % tolerance and the
> roadmap to 100 % bit-identical post-GA Q3.

---

## 1. Architecture — 2-Runner Matrix + Diff Check

```
┌─────────────────────────────────────────────────────────────┐
│  .github/workflows/reproducible-build.yml                   │
│                                                             │
│  Trigger: v* tag push | nightly 04:00 UTC | manual          │
│                                                             │
│  ┌────────────────────────┐  ┌────────────────────────┐    │
│  │  Build job (runner-1)  │  │  Build job (runner-2)  │    │
│  │  ubuntu-22.04 LTS      │  │  ubuntu-22.04 LTS      │    │
│  │                        │  │                        │    │
│  │  1. checkout (full)    │  │  1. checkout (full)    │    │
│  │  2. rust-toolchain     │  │  2. rust-toolchain     │    │
│  │     .toml pin 1.84.0   │  │     .toml pin 1.84.0   │    │
│  │  3. SOURCE_DATE_EPOCH  │  │  3. SOURCE_DATE_EPOCH  │    │
│  │     = git log -1 --ct  │  │     = git log -1 --ct  │    │
│  │  4. cargo build        │  │  4. cargo build        │    │
│  │     --release          │  │     --release          │    │
│  │     --frozen           │  │     --frozen           │    │
│  │     --offline          │  │     --offline          │    │
│  │     --jobs 1           │  │     --jobs 1           │    │
│  │     RUSTFLAGS remap    │  │     RUSTFLAGS remap    │    │
│  │  5. SHA-256 hash       │  │  5. SHA-256 hash       │    │
│  │  6. upload artifact    │  │  6. upload artifact    │    │
│  └──────────┬─────────────┘  └────────────┬───────────┘    │
│             │                              │                 │
│             └──────────┬───────────────────┘                 │
│                        ▼                                    │
│             ┌──────────────────────┐                        │
│             │  diff-check job      │                        │
│             │  ubuntu-22.04        │                        │
│             │                      │                        │
│             │  1. download both    │                        │
│             │  2. compare SHA-256  │                        │
│             │  3. if different:    │                        │
│             │     cmp -l (POSIX)   │                        │
│             │  4. compute % diff   │                        │
│             │  5. gate: ≤ 5 %      │                        │
│             │  6. emit metrics     │                        │
│             └──────────────────────┘                        │
└─────────────────────────────────────────────────────────────┘

Outcomes:
  bit_identical      diff = 0 bytes    → ideal (post-GA Q3 target)
  within_threshold   0 < diff ≤ 5 %   → acceptable (current best-effort)
  exceeds_threshold  diff > 5 %        → CI FAILS; alert SEV-3
```

---

## 2. Hermetic Build Flags

| Flag | Purpose |
|------|---------|
| `SOURCE_DATE_EPOCH` | Deterministic timestamp from `git log -1 --pretty=%ct`; Cargo + LLVM embed this instead of wall-clock time |
| `--remap-path-prefix=$PWD=/SRC` | Strips workspace-local absolute paths from LLVM debug info |
| `--remap-path-prefix=$HOME/.cargo=/CARGO` | Strips Cargo registry cache paths from LLVM debug info |
| `-C codegen-units=1` | Single codegen unit eliminates LLVM parallel codegen non-determinism |
| `--jobs 1` | Sequential compilation eliminates link-order non-determinism in the final `.wasm` |
| `--frozen` | Requires `Cargo.lock` unchanged; prevents silent dependency resolution drift |
| `--offline` | No network calls during compile (SLSA-hermetic property) |
| `CARGO_INCREMENTAL=0` | Disables incremental compilation state that could bleed across builds |

---

## 3. Non-Determinism Sources (All Six Documented)

### 3.1 Cargo Build Timestamp

**Source**: `cargo` embeds a `build_timestamp` string in the binary.

**Mitigation**: Set `SOURCE_DATE_EPOCH` to `git log -1 --pretty=%ct` before
`cargo build`.  Cargo and LLVM honour this environment variable and use it
instead of the current wall-clock time.

**Residual risk**: A `build.rs` script that calls `chrono::Local::now()` or
`std::time::SystemTime::now()` directly (bypassing `SOURCE_DATE_EPOCH`) will
still embed a non-deterministic timestamp.  The pre-commit lint in
`scripts/build_rs_lint.sh` detects this pattern (see §6).

---

### 3.2 LLVM Debug Info Paths

**Source**: LLVM embeds absolute filesystem paths (workspace root, Cargo
registry cache) into the DWARF debug-info sections of the binary.  Two
runners on different GitHub Actions VMs will have different `$HOME` and
`$GITHUB_WORKSPACE` paths.

**Mitigation**: RUSTFLAGS includes:
```
--remap-path-prefix=${{ github.workspace }}=/SRC
--remap-path-prefix=$HOME/.cargo=/CARGO
```
This replaces runner-specific prefixes with canonical labels `/SRC` and
`/CARGO` before LLVM writes the DWARF data.

**Residual risk**: A subset of LLVM DWARF paths (e.g. system libc paths,
`core` stdlib paths) may not be fully remapped by current rustc 1.84.x.
These contribute to the ≤ 5 % tolerance budget.  Tracked in ADR-0015 §5
(Consequences) for resolution in the post-GA Q3 toolchain upgrade.

---

### 3.3 Rust Compiler (rustc) Version Drift

**Source**: A rustc minor-version upgrade (e.g. 1.84 → 1.85) can change
LLVM IR optimisation passes, introduction of new MIR rewrites, or linker
plugin behaviour — all of which can produce different byte sequences for
identical source.

**Mitigation**: `rust-toolchain.toml` pins `channel = "1.84.0"` (exact
minor).  Cargo automatically reads this file and installs the correct
toolchain on every runner.

**Upgrade procedure**:
1. Open PR bumping `rust-toolchain.toml`.
2. Reproducible-build workflow MUST pass on the PR (2-runner diff ≤ 5 %).
3. ADR-0015 amendment required documenting the new baseline diff and any
   new non-determinism sources introduced.

---

### 3.4 `build.rs` Scripts with Non-Deterministic Timestamps

**Source**: Any `build.rs` that calls `chrono::Local::now()`,
`std::time::SystemTime::now()`, or reads `/proc/uptime` directly generates
non-deterministic output embedded in the binary.

**Mitigation**:
1. Pre-commit lint (§6) blocks commits that introduce the banned patterns.
2. Compliant `build.rs` pattern:
   ```rust
   fn embed_build_timestamp() {
       let epoch = std::env::var("SOURCE_DATE_EPOCH")
           .ok()
           .and_then(|v| v.parse::<i64>().ok())
           .unwrap_or_else(|| {
               // Fallback only for local dev without SOURCE_DATE_EPOCH set.
               // CI always sets this env var.
               0
           });
       println!("cargo:rustc-env=BUILD_TIMESTAMP={epoch}");
   }
   ```

---

### 3.5 Multi-Threaded Compilation Link-Order Race

**Source**: When `cargo build --jobs N` (N > 1) compiles multiple crates
in parallel, the order in which object files are linked into the final
binary can vary between runs on the same machine (scheduler non-determinism)
or across machines.

**Mitigation**: The reproducible-build workflow uses `--jobs 1`.  This
slows the build but produces deterministic link order.

**Note**: Release CI (`cas_foundation.yml`) still uses parallel jobs for
throughput; only the reproducible-build matrix uses `--jobs 1`.

---

### 3.6 Cross-Runner CPU Heterogeneity

**Source**: GitHub Actions `ubuntu-22.04` runners share the same OS image
but may run on different CPU models (Intel Skylake vs. Cascade Lake vs.
AMD EPYC).  LLVM auto-vectorisation can produce different SIMD instruction
sequences for the same source.

**Mitigation**: The `os: [ubuntu-22.04]` matrix pin ensures identical kernel
+ glibc.  The `-C codegen-units=1` flag reduces LLVM parallelism-induced
variance.  `target-cpu=generic` (LLVM default) avoids host-native SIMD.

**Residual risk**: LLVM may still emit different instruction sequences for
micro-architecture variants within the same "generic" target.  This is a
documented limitation (see ADR-0015 §3 — Alternatives Rejected).  Tracked
for resolution via a dedicated runner pool (homogeneous hardware) post-GA
enterprise.

---

## 4. Metrics

Three Prometheus metrics are emitted per run:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `corelink_supply_reproducible_diff_bytes_gauge` | Gauge | `outcome` | Byte-level diff count between runner-1 and runner-2 |
| `corelink_supply_reproducible_runs_total` | Counter | `outcome` | Incremented each workflow run; `outcome ∈ {bit_identical, within_threshold, exceeds_threshold, build_failed}` |
| `corelink_supply_reproducible_runner_duration_seconds_bucket` | Histogram | — | Build duration p99 per runner |

Dashboard: **DASH-SUPPLY** widget "Reproducible diff bytes per release".

Alert: SEV-3 if `outcome = exceeds_threshold` sustained 3 consecutive nightly
runs (regression detection).  SEV-2 if diff > 50 % (compromised-builder
suspicion).

---

## 5. Trace Spans

| Span | Attributes |
|------|-----------|
| `reproducible.build` | `runner_id` (string), `duration_ms` (u64), `result` (enum) |
| `reproducible.diff_check` | `diff_bytes` (u64), `diff_percentage` (f64), `bit_identical` (bool), `outcome` (enum) |

---

## 6. Build.rs Lint Pre-Commit Hook

`scripts/build_rs_lint.sh` scans every `build.rs` in the workspace for
banned timestamp patterns.  Install once:

```bash
# Add to .pre-commit-config.yaml (see file for current SHA-pins):
# - repo: local
#   hooks:
#     - id: build-rs-timestamp-lint
#       name: build.rs timestamp lint
#       language: script
#       entry: scripts/build_rs_lint.sh

# Or invoke directly:
bash scripts/build_rs_lint.sh
```

**Banned patterns** (exit 1 if found in any `build.rs`):
- `chrono::Local::now()`
- `chrono::Utc::now()`
- `SystemTime::now()`
- `UNIX_EPOCH`

**Correct pattern**: Read `SOURCE_DATE_EPOCH` from `std::env::var` and fall
back to `0` for local dev.

---

## 7. Customer Verification Quickstart

A SecOps lead or enterprise prospect can verify the 2-runner diff report
for any release:

```bash
# 1. Download the reproducible-build-report artifact from the GitHub Actions
#    run associated with the release tag.
gh run download --name reproducible-build-report --repo humangr-labs/corelink-server

# 2. Inspect the step summary (printed to GITHUB_STEP_SUMMARY in CI).
#    The report table includes: outcome, bit_identical, diff_bytes, diff_percentage.

# 3. Re-verify locally (requires the same rust-toolchain.toml pin):
rustup toolchain install 1.84.0
rustup target add wasm32-unknown-unknown
SOURCE_DATE_EPOCH=$(git log -1 --pretty=%ct) \
RUSTFLAGS="--remap-path-prefix=$(pwd)=/SRC --remap-path-prefix=$HOME/.cargo=/CARGO -C codegen-units=1" \
  cargo build --release --frozen --offline --jobs 1 --target wasm32-unknown-unknown -p corelink-worker

sha256sum target/wasm32-unknown-unknown/release/corelink_worker.wasm
# Compare the hash with the artifact.sha256 from the CI run.
```

---

## 8. Roadmap — 100 % Bit-Identical Post-GA Q3

| Milestone | Date | Target |
|-----------|------|--------|
| S-12 GA | 2026-Q2 | Best-effort ≤ 5 % diff; ADR-0015 v1.0.0 |
| Post-GA Q3 | 2026-Q4 | Re-evaluate: rustc 1.88+ LLVM path-remap improvements; aim 100 % bit-identical |
| Post-GA Q4+ | 2027+ | 100 % bit-identical if toolchain mature; extend to 3-runner triplicate; consider Reproducible-Builds.org submission |

Quarterly review process (per ADR-0015 §6):
1. Run `scripts/build_rs_lint.sh` over all `build.rs` files.
2. Review nightly CI diff trend (DASH-SUPPLY).
3. Identify new non-determinism sources (dep upgrades, rustc bumps).
4. Update ADR-0015 + this document.
5. Calendar trigger: first week of each quarter.

---

## 9. References

- [ADR-0015](../specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md) — trade-offs + roadmap.
- [Reproducible Builds Project](https://reproducible-builds.org/) — industry standard.
- [SOURCE_DATE_EPOCH specification](https://reproducible-builds.org/docs/source-date-epoch/) — standard env var.
- [rustc `--remap-path-prefix`](https://doc.rust-lang.org/rustc/command-line-arguments.html#--remap-path-prefix-remap-path-prefixes-in-all-output) — rustc docs.
- [rust-toolchain.toml reference](https://rust-lang.github.io/rustup/overrides.html#the-toolchain-file) — rustup docs.
- `WI-S12-006` spec: `specs/04_sprints/_sealed/S12/work_items/WI-S12-006-reproducible-builds-2-runner-diff-adr.md`.
