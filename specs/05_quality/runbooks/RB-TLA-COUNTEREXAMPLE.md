---
id: "RB-TLA-COUNTEREXAMPLE"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-28"
updated: "2026-04-28"
owner: "Architect"
final_approver: "Architect"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "rb", "tla", "counterexample", "wi-s11-008", "s11", "stub", "planned"]
---

# RB-TLA-COUNTEREXAMPLE — TLC Counterexample Found (Invariant Violation Detected)

> **Status: PLANNED stub (Lote 10.11.0-bis-prime cycle 14)** — full SOP a ser drafted pré-PRR por Architect + Crypto SME.
> **WI:** WI-S11-008 (TLA+ dsr_erasure_atomicity) + WI-S06-006 (TLA+ CI gate) | **CTRL:** CTRL-FORMAL-001 | **PAT:** PAT-FORMAL-VERIFICATION-001 (resilience_patterns.md §3.6) | **SLA:** triage ≤ 4h, mitigation plan ≤ 24h, fix or waiver ≤ 7d

## Pré-condições

- TLA+ CI gate active (`.github/workflows/tla_check.yml`).
- TLC v1.8.0 SHA-256 pinned (ADR-0042 §A1).
- Invariant + property suites declared in `specs/tla/*.cfg`.

## Detecção

### Sinais primários

- TLC run em CI emite counterexample (state trace) com invariant ou property violation.
- CI workflow `tla-check` job fails with non-zero exit.
- PR blocked merge per branch protection.

## Step 1: Triage (≤ 4h)

1. **Reproduce locally**: `bash scripts/run_tlc_corelink.sh <spec>` com same cfg.
2. **Inspect counterexample**: TLC outputs full state trace; identify the action sequence that violates the invariant.
3. **Classify root cause**: 
   - Spec bug (invariant too strong / model incorrect) — Architect responsibility.
   - Implementation bug (real semantic flaw) — Implementation owner responsibility.
   - State space explosion / TLC config gap — increase bounds + re-run.

## Step 2: Mitigation plan (≤ 24h)

- **Spec bug**: weaken or correct invariant; require Crypto SME + Architect dual review (HIGH_RISK lane).
- **Implementation bug**: file P0 ticket; pause merge of feature branch until fixed.
- **Bounds gap**: increase MaxConcurrentErasures / MaxAuditChainLen / MaxAttempts; re-run TLC.
- **No fix possible**: temporary waiver via ADR-0014 waiver workflow (90d max; auto-expire forces re-evaluation).

## Step 3: Fix or waiver (≤ 7d)

- Submit corrective PR with TLC GREEN run as evidence.
- Update invariant_registry.md status if waiver granted.
- Document counterexample minimization steps in commit message.

## Post-incident

- Post-mortem: why was the counterexample missed during spec design?
- Update spec docstring + cfg comments.
- Cross-reference if counterexample reveals real production race condition.

## References

- `WI-S11-008` TLA+ dsr_erasure_atomicity formal spec.
- `WI-S06-006` TLA+ CI gate + property test 100k race.
- `resilience_patterns.md` §3.6 PAT-FORMAL-VERIFICATION-001.
- `ADR-0042` TLC v1.8.0 SHA-256 pinned bootstrap.
- `invariant_registry.md` §4 obligation matrix.
