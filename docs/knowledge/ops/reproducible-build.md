---
type: "Runbook"
title: "Reproducible-build process"
description: "Best-effort reproducible builds: the 2-runner SHA-256 diff matrix, the hermetic flag set (SOURCE_DATE_EPOCH + remap-path-prefix + codegen-units=1 + frozen/offline), the ≤5% byte-diff tamper-detection gate ratified by ADR-0015, and customer self-verification."
source_files:
  - "docs/build/reproducible.md"
  - "specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md"
  - "rust-toolchain.toml"
checkpoint_sha: "b5ce2bff384a09047f027082dcf4355136822242"
provenance: "AUTHORED"
tags: ["ops", "reproducible-build", "supply-chain", "tamper-detection", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Reproducible-build process

Reproducible builds are CoreLink's supply-chain tamper-detection defense-in-depth: if two independent
runners compile the same source commit to within a tiny byte-diff, an attacker who compromises one builder
but not the other is mechanically detectable. CoreLink runs a **best-effort** version of this — a 2-runner
SHA-256 diff with a **≤5% byte-diff gate** rather than 100% bit-identical, because Rust 1.91 + LLVM 21 in
2026 cannot reliably hit 0% (residual DWARF path leaks, runner CPU heterogeneity). The 5% threshold and
the roadmap to 100% post-GA Q3 are ratified in ADR-0015. This is the build-integrity counterpart of the
GA tag's signed freeze in the [release process](/ops/release-process.md); the decision rationale lives in
[ADR-0015](/adr/adr-0015-reproducible-build-best-effort.md).

# Role
- The tamper-detection control: a 2-runner mismatch surfaces a compromised builder.
- The determinism harness: a fixed hermetic flag set that strips the known non-determinism sources.
- The customer-trust surface: any SecOps lead can re-verify a release's hash from the published report.

# How it works
1. A 2-runner `ubuntu-22.04` matrix builds the artifact, each runner SHA-256-hashes its output, and a
   diff-check job gates the byte difference at ≤5% (`docs/build/reproducible.md:12-59`).
2. Every build applies the hermetic flag set: `SOURCE_DATE_EPOCH` from `git log -1 --pretty=%ct`,
   `--remap-path-prefix`, `-C codegen-units=1`, `--jobs 1`, `--frozen --offline`, `CARGO_INCREMENTAL=0`
   (`docs/build/reproducible.md:63-75`).
3. All six non-determinism sources are documented with mitigation + residual risk (cargo timestamp, LLVM
   DWARF paths, rustc version drift, build.rs timestamps, link-order race, CPU heterogeneity)
   (`docs/build/reproducible.md:78-191`).
4. The workflow triggers on `v*` tag push, nightly 04:00 UTC, and manual dispatch — not per-PR
   (`docs/build/reproducible.md:14-18`).
5. ADR-0015 ratifies the decision: 2-runner matrix + three hermetic flags + ≤5% threshold + nightly/tag CI
   + quarterly review + the post-GA-Q3 roadmap (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:69-86`).
6. The 5% value was chosen because the hermetic flags reduce diff to <2% in practice while a real backdoor
   typically changes >10% of bytes — 5% catches it with margin
   (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:118-128`).
7. A customer re-verifies by downloading the `reproducible-build-report` artifact
   (`gh run download --repo HumanGuardrail/corelink-server`) and re-running the same
   toolchain pin + flags locally to compare the hash (`docs/build/reproducible.md:251-273`).

# Invariants
- The build is hermetic: `--frozen --offline` means no network during compile and `Cargo.lock` cannot
  drift — an SLSA-hermetic property and a Cargo-MITM mitigation
  (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:222-227`).
- The reproducible matrix uses `--jobs 1` to eliminate link-order non-determinism, even though release CI
  uses parallel jobs for throughput (`docs/build/reproducible.md:161-172`).
- The rustc toolchain is pinned to exact minor `1.91.1` via `rust-toolchain.toml`; a bump requires the
  workflow green on the PR + an ADR-0015 amendment (`rust-toolchain.toml:13`).
- A `build.rs` that bypasses `SOURCE_DATE_EPOCH` (e.g. `SystemTime::now()`) is blocked by the pre-commit
  lint `scripts/build_rs_lint.sh` (`docs/build/reproducible.md:222-248`).
- Threshold change requires Security Lead + Architect sign-off; a toolchain bump requires the workflow
  green + Architect sign-off (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:242-247`).

# Gotchas
- ≤5% is acceptance, not the goal: 0 bytes is `bit_identical` (the post-GA-Q3 target), 0<diff≤5% is
  `within_threshold` (current best-effort), >5% FAILS CI and pages SEV-3
  (`docs/build/reproducible.md:55-59`).
- A >50% diff escalates to SEV-2 (compromised-builder suspicion), distinct from the SEV-3 regression alarm
  (`docs/build/reproducible.md:207-209`).
- Reproducibility is NOT skippable: it was explicitly rejected as an alternative because skipping it means
  a compromised builder can inject backdoors with no automated detection path
  (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:104-109`).

# Citations
1. `docs/build/reproducible.md:12-59` — the 2-runner matrix + diff-check architecture + outcomes.
2. `docs/build/reproducible.md:14-18` — workflow triggers (tag / nightly / manual, not per-PR).
3. `docs/build/reproducible.md:55-59` — outcome bands (bit_identical / within_threshold / exceeds).
4. `docs/build/reproducible.md:63-75` — the hermetic build flag set.
5. `docs/build/reproducible.md:78-191` — the six documented non-determinism sources.
6. `docs/build/reproducible.md:117-133` — rustc exact-minor pin + upgrade procedure.
7. `docs/build/reproducible.md:161-172` — `--jobs 1` for deterministic link order.
8. `docs/build/reproducible.md:207-209` — SEV-3 regression vs SEV-2 compromised-builder alert.
9. `docs/build/reproducible.md:222-248` — the build.rs timestamp pre-commit lint.
10. `docs/build/reproducible.md:251-273` — customer verification quickstart.
11. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:69-86` — the ratified decision controls.
12. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:104-109` — skip-reproducibility rejected.
13. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:118-128` — why the 5% threshold.
14. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:222-227` — hermetic `--frozen --offline` STRIDE.
15. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:242-247` — amendment sign-off requirements.
