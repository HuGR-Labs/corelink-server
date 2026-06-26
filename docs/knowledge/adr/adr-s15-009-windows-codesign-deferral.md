---
type: "ADR"
title: "ADR-S15-009 — Windows Authenticode: no unsigned fallback, deferral path only"
description: "Forbids shipping an unsigned Windows release; lets macOS + Linux ship at the sprint SEAL while Windows ships later but always signed, preserving the honest '3 OSes signed' claim."
source_files:
  - "specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "ops", "release", "windows", "authenticode", "codesign", "deferral", "s15"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S15-009 — Windows Authenticode: no unsigned fallback, deferral path only

The S-15 ship gate requires release binaries signed on three OSes, but the Windows Authenticode EV certificate has a 1-2 week vendor lead that can slip past the sprint window. Rather than admit an "unsigned-with-warning" Windows fallback — which would dilute the trust claim and trigger SmartScreen warnings — this ADR makes the only Windows shipment path a signed one, decoupling the sprint SEAL from cert-procurement variance. It exists to keep "3 OSes signed" an honest claim: when the third OS ships, it ships signed.

# Context

A prior draft admitted an unsigned-with-warning Windows fallback if EV cert acquisition slipped; the Lote 10.15 codex P1 review rejected it on two grounds — it weakens the hard CAP-CLI-002 "signed on 3 OSes" DoD line and it inflicts SmartScreen first-launch warnings on Windows users — so a decision was needed that preserves the invariant without blocking the whole sprint on cert procurement, as framed at `specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md:36-57`.

# Decision

The decision (D1-D4) is: never ship an unsigned Windows release — the signing workflow is gated on cert presence and silently skips publishing an unsigned asset; macOS + Linux ship at the D+15 SEAL independent of Windows; Windows ships in a subsequent run via `workflow_dispatch` against the released tag once the cert is ready (so end users get a delayed-but-signed binary); and the sprint promise is updated to "macOS + Linux at D+15 SEAL; Windows signed within +1 sprint contingent on cert", recorded at `specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md:62-96`.

# Consequences

"3 OSes signed" stays an honest claim with no SmartScreen exposure and the timeline is decoupled from vendor SLA, at the cost that Windows users wait 1-N weeks for their first binary and the SEAL evidence pack must mark Windows "deferred" rather than "complete" — and the deferral is bounded, not indefinite (a post-mortem opens if still unsigned at +1 sprint), per `specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md:99-128`.

# Citations

1. `specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md:36-57` — the EV-cert lead-time problem and the codex P1 rejection of the unsigned fallback (Context).
2. `specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md:62-96` — D1-D4: no unsigned ship, two-OS SEAL, deferred signed Windows, updated sprint promise.
3. `specs/03_architecture/adrs/ADR-S15-009-windows-codesign-deferral.md:99-128` — positive/negative consequences and the bounded (post-mortem) deferral.
