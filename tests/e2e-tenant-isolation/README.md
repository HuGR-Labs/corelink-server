# `e2e-tenant-isolation` — adversarial harness pinning `INV-TENANT-ISOLATION`

End-to-end adversarial test harness exercising the canonical
cross-tenant isolation invariant across the six-layer auth model
(`auth_model.md §8.1`).

## Scenario inventory (25 total)

| # | Scenario | Layer(s) | INV / FM | STRIDE row |
|---|---|---|---|---|
| 01 | CAS read other-tenant denied + audited | 2, 5, 6 | INV-TENANT-ISOLATION | tenant-path TB-tp-2 I |
| 02 | CAS write to other-tenant prefix denied | 2, 5 | INV-TENANT-ISOLATION | tenant-path TB-tp-2 E |
| 03 | LIST enumeration does not leak other tenant | 5 | INV-TENANT-ISOLATION | tenant-path TB-tp-2 I |
| 04 | BYOK DEK wrap with wrong AAD rejected | 4 | INV-BYOK-AAD-BIND | byok §2 |
| 05 | BYOK envelope tamper rejected with audit | 4, 6 | INV-BYOK-AAD-BIND | byok §2 |
| 06 | Audit cross-tenant query requires dual-approval | 2, 6 | INV-DUAL-APPROVAL-CROSS-TENANT | audit-chain query |
| 07 | D1 row spoofing rejected (JWT wins) | 1 | INV-JWT-BODY-CONFLICT | tenant-path TB-tp-3 S |
| 08 | Idempotency key collision independent per tenant | 1, 2 | INV-IDEMPOTENCY-TENANT-SCOPED | tenant-path TB-tp-3 I |
| 09 | Quota crosstalk isolated | 3 | INV-QUOTA-TENANT-SCOPED | rate-limit |
| 10 | Rate-limit crosstalk isolated | 3 | INV-RATELIMIT-TENANT-SCOPED | rate-limit |
| 11 | Stripe webhook replay cross-tenant rejected | 1, 6 | INV-STRIPE-WEBHOOK-IDEMPOTENT-CROSS-TENANT | billing |
| 12 | PAT cross-tenant use rejected | 1, 2 | INV-PAT-TENANT-SCOPED | pat |
| **13** | **Timing oracle: constant-time auth probe** | 1 | INV-AUTH-TIMING-PARITY / THR-I-002 | tenant-path TB-tp-1 I |
| **14** | **CAS cache-poisoning cross-tenant isolated** | 5 | INV-CAS-PREFIX-SCOPED / FM-CAS-002 | cas cache row |
| **15** | **CMK rotation race: no half-state envelope** | 4 | INV-BYOK-CMK-ROTATION-ATOMIC / FM-BYOK-005 | byok rotation |
| **16** | **PAT revoke ToCToU — no window** | 1 | INV-PAT-REVOKE-TOCTOU-SAFE / FM-PAT-003 | pat revoke |
| **17** | **Idempotency mixed-case cross-tenant** | 1, 2 | INV-IDEMPOTENCY-BYTE-EXACT | tenant-path TB-tp-3 I |
| **18** | **Audit chain leaf forge rejected** | 6 | INV-AUDIT-CHAIN-NON-FORGEABLE | audit-chain |
| **19** | **Cross-region replay: residency enforced** | 1, 2 | INV-RESIDENCY-REGION-PINNED / FM-RESIDENCY-001 | residency |
| **20** | **Cross-tenant DSR submission rejected** | 1, 2 | INV-DSR-TENANT-CONTEXT-MATCH | dsr §2.1 |
| **21** | **Quota inheritance: siblings isolated** | 3 | INV-QUOTA-SIBLING-NON-INHERITED | quota hierarchy |
| **22** | **R2 multipart upload forge rejected** | 5 | INV-MULTIPART-UPLOAD-TENANT-BOUND / FM-CAS-007 | cas multipart |
| **23** | **Stripe webhook cross-account spoof rejected** | 1, 6 | INV-STRIPE-WEBHOOK-IDEMPOTENT-CROSS-TENANT | billing webhook |
| **24** | **KV partition: PAT revoke fail-CLOSED** | 1 | INV-KV-REPLICATION-FAIL-CLOSED / FM-AUTH-013 | kv-replication |
| **25** | **Audit query injection rejected** | 6 | INV-AUDIT-QUERY-TENANT-SCOPED | audit-chain query |

Bold rows (13..25) are the pentest-readiness expansion landed for the
external pentest engagement on 2026-06-15. Baseline wave-5 scenarios
(01..12) are unchanged.

## Charter compliance

- `#![forbid(unsafe_code)]` crate-wide.
- No `unwrap` / `expect` / `panic` / indexing in `src/` (lib lints
  `deny`); test module locally `allow`s by design.
- Every scenario asserts the audit event was emitted **before** the
  rejection return path (fail-CLOSED ordering, `INV-TENANT-ISOLATION`
  layer 6).
- All scenarios are deterministic — fixed UUIDv7 constants for
  `TENANT_A` / `TENANT_B`; logical clocks for time-bound scenarios
  (16, 24).

## Pentest evidence cross-link

This harness is the canonical artefact for the multi-tenant isolation
control surface in:

- `specs/_pentest/PENTEST-EVIDENCE-PACKAGE.md` (P0 surface: §3.5
  Multi-tenant isolation)
- `specs/_audits/stride-per-crate/STRIDE-corelink-tenant-path.md`
  (test coverage column for TB-tp-1 / TB-tp-2 / TB-tp-3 / TB-tp-4)
- `specs/_audits/stride-per-crate/STRIDE-corelink-byok.md`
  (rotation + AAD binding rows)
- `specs/_audits/stride-per-crate/STRIDE-corelink-audit-chain.md`
  (chain non-forgeability + query injection rows)
- `specs/_audits/stride-per-crate/STRIDE-corelink-dsr.md`
  (cross-tenant DSR rejection)
- `specs/04_sprints/S20/pentest_gate.md` (gate evidence for the
  2026-06-15 external engagement)

Scenario count moved 12 → 25 in 2026-05-15 as part of the
pre-pentest hardening pass.

## Running

```bash
cargo build -p e2e-tenant-isolation
cargo clippy -p e2e-tenant-isolation --tests -- -D warnings
cargo test  -p e2e-tenant-isolation
```
