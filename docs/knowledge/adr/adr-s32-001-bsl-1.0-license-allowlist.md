---
type: "ADR"
title: "ADR-S32-001 — Permit BSL-1.0 in the license allowlist"
description: "Formally ratifies the Boost Software License 1.0 as a permitted license so dual-licensed transitive deps (ryu/ryu-js) clear the SBOM audit unambiguously rather than as CONDITIONAL."
source_files:
  - "specs/03_architecture/adrs/ADR-S32-001-bsl-1.0-license-allowlist.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "compliance", "license", "supply-chain", "bsl-1.0", "cargo-deny", "s32"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S32-001 — Permit BSL-1.0 in the license allowlist

The wave-32 SBOM/license audit surfaced two transitive deps (`ryu`, `ryu-js`) dual-licensed `Apache-2.0 OR BSL-1.0`; the Apache-2.0 leg already satisfies the OR, so the auditor flagged them CONDITIONAL and recommended a short ADR to make the result unambiguous. This ADR adds BSL-1.0 to the permitted-license allowlist at both artifact sources, removing per-report manual "which leg" triage and pre-clearing future BSL-1.0 deps. It exists because BSL-1.0 is among the most permissive OSI-approved licenses — strictly less restrictive than already-allowlisted Apache-2.0 in compiled form.

# Context

The audit found `ryu` and `ryu-js` declared `Apache-2.0 OR BSL-1.0`; since Apache-2.0 is already allowlisted the OR is satisfied (flagged CONDITIONAL not fail), but the auditor recommended formally ratifying BSL-1.0 for three reasons — defensive completeness (an `A OR B` dep forces per-report reasoning about which leg if only A is allowed), pre-clearing future dual-licensed algorithmic libraries, and BSL-1.0 being maximally permissive — as recorded at `specs/03_architecture/adrs/ADR-S32-001-bsl-1.0-license-allowlist.md:28-57`.

# Decision

The decision adds `BSL-1.0` to the permitted-license allowlist at both the `cargo-deny.toml` allow block and the canonical compliance allowlist doc, permitting it for direct, transitive (any depth), and build/dev dependencies — justified because BSL-1.0 is OSI-approved, functionally MIT-plus-machine-readable-notice-exemption, and strictly less restrictive than Apache-2.0 in compiled form (no NOTICE file, no patent-retaliation clause) — recorded at `specs/03_architecture/adrs/ADR-S32-001-bsl-1.0-license-allowlist.md:58-112`.

# Consequences

License-audit reports for `ryu`/`ryu-js` become clean with no manual Apache-2.0-leg reasoning, future BSL-1.0 deps onboard without re-deciding, and there is no customer-facing notice change (compiled binaries already exempt); the only follow-up is a separate commit updating the two allowlist artifacts plus the SBOM generator, per `specs/03_architecture/adrs/ADR-S32-001-bsl-1.0-license-allowlist.md:114-148`.

# Citations

1. `specs/03_architecture/adrs/ADR-S32-001-bsl-1.0-license-allowlist.md:28-57` — the ryu/ryu-js CONDITIONAL finding and the three reasons to ratify BSL-1.0 (Context).
2. `specs/03_architecture/adrs/ADR-S32-001-bsl-1.0-license-allowlist.md:58-112` — the allowlist decision and why BSL-1.0 is safe (permissions, obligations, comparison table).
3. `specs/03_architecture/adrs/ADR-S32-001-bsl-1.0-license-allowlist.md:114-148` — positive consequences and the follow-up implementation commit scope.
