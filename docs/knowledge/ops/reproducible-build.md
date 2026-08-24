---
type: "Runbook"
title: "Reproducible-build process"
description: "Best-effort reproducible builds: two legs on one self-hosted host diffing the SHIPPED corelink-cli binary, the hermetic flag set (SOURCE_DATE_EPOCH + remap-path-prefix + codegen-units=1 + jobs 1 + locked/offline), the ≤5% byte-diff gate ratified by ADR-0015 — and the 2026-08-24 repair of a lane that hashed a wasm artifact the build cannot produce, which is why it had never once measured anything."
source_files:
  - "docs/build/reproducible.md"
  - "specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md"
  - "rust-toolchain.toml"
source_blobs:
  - "docs/build/reproducible.md@735e3ec802a1eee1048cd4731d19251592ed2856"
checkpoint_sha: "c0fd1177ba2a9fe0c88a87596863f8dba953b76f"
provenance: "AUTHORED"
tags: ["ops", "reproducible-build", "supply-chain", "tamper-detection", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Reproducible-build process

Reproducible builds are CoreLink's supply-chain tamper-detection defense-in-depth: if the same source
commit compiles twice to within a tiny byte-diff, a builder that has been tampered with between the two
is mechanically detectable. The gate is **best-effort** — ≤5% byte diff, ratified by ADR-0015 — on the
theory that Rust 1.91 + LLVM 21 cannot reliably reach 0%.

**Two things about that were wrong until 2026-08-24 (B-016).** First, the lane hashed
`target/wasm32-unknown-unknown/release/corelink_worker.wasm`, which **cannot exist**: the crate declares
no `[lib] crate-type = ["cdylib"]`, and the deployed Worker is TypeScript anyway. Every run failed after a
successful compile, so this control measured nothing at all for a year while appearing to be a control.
It now builds the artifact that actually ships — the `corelink-cli` release binary — with `release-cli.yml`'s
own command. Second, the "cannot reliably hit 0%" premise is not what the first real measurement found:
run `32726344224` came back **bit-identical** (`58cc00a6…`, 4 691 516 bytes, 0 differing bytes). The 5%
tolerance stays as the gate, but on this artifact the observed diff is zero. This is the build-integrity counterpart of the
GA tag's signed freeze in the [release process](/ops/release-process.md); the decision rationale lives in
[ADR-0015](/adr/adr-0015-reproducible-build-best-effort.md).

# Role
- The tamper-detection control: a 2-runner mismatch surfaces a compromised builder.
- The determinism harness: a fixed hermetic flag set that strips the known non-determinism sources.
- The customer-trust surface: any SecOps lead can rebuild the published CLI binary and compare hashes.

# How it works
1. One self-hosted job builds `corelink-cli` twice into separate target directories — separate so the
   second leg is a real recompile and not a cache hit re-reading the first leg's output — SHA-256-hashes
   both, and gates the byte difference at ≤5% (`docs/build/reproducible.md:12-54`). Both legs share a
   host, so the result is build determinism, NOT cross-environment reproducibility; the earlier design
   claimed the stronger property with two hosted runners and never executed a single step.
2. Every build applies the hermetic flag set: `SOURCE_DATE_EPOCH` from `git log -1 --pretty=%ct`,
   `--remap-path-prefix`, `-C codegen-units=1`, `--jobs 1`, `--frozen --offline`, `CARGO_INCREMENTAL=0`
   (`docs/build/reproducible.md:56-70`).
3. All six non-determinism sources are documented with mitigation + residual risk (cargo timestamp, LLVM
   DWARF paths, rustc version drift, build.rs timestamps, link-order race, CPU heterogeneity)
   (`docs/build/reproducible.md:71-192`).
4. The workflow is `workflow_dispatch`-only: two full release builds of a 207-dependency crate do not
   belong on a pull request, and the self-hosted fleet is the owner's own machine
   (`docs/build/reproducible.md:14-21`).
5. ADR-0015 ratifies the decision: 2-runner matrix + three hermetic flags + ≤5% threshold + nightly/tag CI
   + quarterly review + the post-GA-Q3 roadmap (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:69-86`).
6. The 5% value was chosen because the hermetic flags reduce diff to <2% in practice while a real backdoor
   typically changes >10% of bytes — 5% catches it with margin
   (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:118-128`).
7. A customer re-verifies by downloading the released binary and rebuilding it from the same commit with
   the same pinned toolchain and remap set, then comparing hashes (`docs/build/reproducible.md:249-288`).
   SLSA provenance for the same binaries comes from `release-slsa3.yml`, whose subjects are the published
   release assets themselves.

# Invariants
- The build is hermetic: `--frozen --offline` means no network during compile and `Cargo.lock` cannot
  drift — an SLSA-hermetic property and a Cargo-MITM mitigation
  (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:222-227`).
- Both legs use `--jobs 1` to eliminate link-order non-determinism, even though release CI uses parallel
  jobs for throughput (`docs/build/reproducible.md:159-173`).
- The rustc toolchain is pinned to exact minor `1.91.1` via `rust-toolchain.toml`; a bump follows the §3.3
  upgrade procedure — because the workflow has **no `pull_request` trigger** (moved off per-PR 2026-06-02)
  it does not gate the PR automatically, so the bumper must trigger a `workflow_dispatch` green run and
  land an ADR-0015 amendment (`docs/build/reproducible.md:110-133`, `rust-toolchain.toml:13`).
- A `build.rs` that bypasses `SOURCE_DATE_EPOCH` (e.g. `SystemTime::now()`) is blocked by the pre-commit
  lint `scripts/build_rs_lint.sh` (`docs/build/reproducible.md:220-248`).
- Threshold change requires Security Lead + Architect sign-off; a toolchain bump requires the workflow
  green + Architect sign-off (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:242-247`).

# Gotchas
- ≤5% is acceptance, not the goal: 0 bytes is `bit_identical` (the post-GA-Q3 target), 0<diff≤5% is
  `within_threshold` (current best-effort), >5% FAILS the gate. The first measured run was
  `bit_identical` (`docs/build/reproducible.md:36-40`).
- A >50% diff escalates to SEV-2 (compromised-builder suspicion), distinct from the SEV-3 regression alarm
  (`docs/build/reproducible.md:205-207`).
- Reproducibility is NOT skippable: it was explicitly rejected as an alternative because skipping it means
  a compromised builder can inject backdoors with no automated detection path
  (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:104-109`).

# Citations
1. `docs/build/reproducible.md:12-54` — the two-leg architecture, the 2026-08-24 correction, and what the
   result does and does not prove.
2. `docs/build/reproducible.md:14-22` — why the lane was repaired: the wasm artifact could not exist.
3. `docs/build/reproducible.md:36-40` — outcome bands (bit_identical / within_threshold / exceeds).
4. `docs/build/reproducible.md:56-70` — the hermetic build flag set, matched to `release-cli.yml`.
5. `docs/build/reproducible.md:71-192` — the six documented non-determinism sources.
6. `docs/build/reproducible.md:110-133` — rustc exact-minor pin + upgrade procedure.
7. `docs/build/reproducible.md:159-173` — `--jobs 1` for deterministic link order.
8. `docs/build/reproducible.md:205-207` — SEV-3 regression vs SEV-2 compromised-builder alert.
9. `docs/build/reproducible.md:220-248` — the build.rs timestamp pre-commit lint.
10. `docs/build/reproducible.md:249-288` — customer verification quickstart (rebuild the CLI binary).
11. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:69-86` — the ratified decision controls.
12. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:104-109` — skip-reproducibility rejected.
13. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:118-128` — why the 5% threshold.
14. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:222-227` — hermetic `--frozen --offline` STRIDE.
15. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:242-247` — amendment sign-off requirements.
