---
id: "RB-FM-157"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "supply-chain", "typosquat", "stub"]
---

# RB-FM-157 — Typosquatting Dependency

> **FM:** FM-157 (S=4, O=2, D=4, RPN=32, P1) | **CTRL:** CTRL-SUPPLY-001 + cargo-deny | **SLA:** detect ≤ 24h, remediate ≤ 7d

## Detecção

- cargo-deny scan flag dep com nome similar a popular crate (e.g., `serde` vs `serdee`).
- Manual lockfile review identifies suspicious crate.
- RustSec advisory for typosquatted crate.
- Telemetry: dep network calls inesperadas.

## Comunicação

- **SEV-2.** Page Security lead + Engineer.
- Internal channel.

## Mitigação imediata

1. Quarantine dep: rollback PR que introduziu typosquat.
2. cargo-deny tighten policy se applicable.
3. Audit: o que typosquat dep fez no nosso build? CI logs review.
4. SBOM update.

## Resolução

- Hot fix: rollback dep; replace com legítimo.
- Cold fix:
  - cargo-deny sources allowlist tightened.
  - Dependency review process strengthen (mandatory PR review + lockfile diff explicitly checked).
  - Supply chain review quarterly.

## References

- `failure_modes.md` FM-157.
- `specs/04_sprints/S12/_spec_contract.md`.
- RustSec database <https://rustsec.org/>.
