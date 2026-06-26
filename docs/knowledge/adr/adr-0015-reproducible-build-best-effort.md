---
type: "ADR"
title: "ADR-0015 — Reproducible builds best-effort (≤5% byte-diff), 100% post-GA"
description: "Adopts a best-effort reproducible-build posture with a 2-runner diff matrix, three hermetic flags, and a 5% byte-diff CI gate, with a roadmap to 100% bit-identical post-GA."
source_files:
  - "specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "supply-chain", "reproducible-builds", "tamper-detection", "s12"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0015 — Reproducible builds best-effort (≤5% byte-diff), 100% post-GA

Reproducible builds are CoreLink's defense-in-depth against a compromised builder: if two independent runners produce a near-identical binary, an attacker who tampers with one but not the other is detectable. Because the 2026 Rust + LLVM toolchain cannot reliably hit 0% bit-identical for the WASM artifact, this ADR commits to a best-effort posture — a ≤5% byte-diff gate today, with a roadmap to 100% post-GA. It matters as the governance decision that makes tamper-detection a CI gate without overclaiming a reproducibility CoreLink cannot yet deliver.

# Context

The goal state is 100% bit-identical output, but Rust reproducibility is immature: `--remap-path-prefix` still leaks some DWARF paths, `SOURCE_DATE_EPOCH` requires `build.rs` cooperation, and LLVM parallel codegen can reorder symbols (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:49-57`). CoreLink's primary artifact is a `wasm32-unknown-unknown` binary built on heterogeneous GitHub Actions runners under a ~$5/month CI budget (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:60-65`).

# Decision

Adopt best-effort reproducibility: a 2-runner parallel matrix, three hermetic flags (`SOURCE_DATE_EPOCH`, `--remap-path-prefix` + `codegen-units=1`, a pinned `rust-toolchain.toml`), a ≤5% byte-diff CI gate, nightly + tag-triggered runs, quarterly review, and a roadmap to 100% post-GA Q3 (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:71-85`). The 5% threshold was chosen as the industry-minimum defensible value — empirically the flags reduce diff to <2%, and a real backdoor typically changes >10% of bytes; a 10% threshold was rejected as masking regressions (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:118-128`).

# Consequences

- Two builders at ≤5% diff give concrete tamper-detection against compromised-runner attacks, and the hermetic flags cut binary diff from a typical 20–40% to <5% (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:168-174`).
- Residuals are accepted within the 5% budget: unremapped LLVM DWARF paths, runner CPU heterogeneity, and the `--jobs 1` slowdown (tolerable because the workflow is nightly/tag-triggered, not on the PR hot path) (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:181-188`).
- The path to 100% is a quarterly-reviewed roadmap gated on rustc/LLVM maturation (`specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:206-214`).
- Related supply-chain decision: [ADR-0014](/adr/adr-0014-sbom-format-cyclonedx.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:49-57` — Rust reproducibility immaturity (path leaks, codegen non-determinism).
2. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:60-65` — CoreLink WASM artifact + runner + budget constraints.
3. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:71-85` — the decision: hermetic flags + 5% gate + roadmap.
4. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:118-128` — why 5% (rejecting a 10% threshold).
5. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:168-174` — tamper-detection + diff reduction.
6. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:181-188` — accepted residuals.
7. `specs/03_architecture/adrs/ADR-0015-reproducible-build-best-effort.md:206-214` — the 100% post-GA roadmap.
