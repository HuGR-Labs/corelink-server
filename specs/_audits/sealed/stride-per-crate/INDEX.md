# Per-crate STRIDE deep dives — INDEX

**Purpose:** depth-2 STRIDE analysis for the 12 most security-critical crates / surfaces, prepared for the external pentest engagement scheduled **2026-06-15** (D+0 baseline `specs/_audits/sealed/pentest/PENTEST-EVIDENCE-PACKAGE.md` v2.0.0).

**Method:** the coarse, asset-level STRIDE matrix (`specs/_audits/matrix-stride-ctrl.csv`) and the domain-level STRIDE tables in `PENTEST-EVIDENCE-PACKAGE §3` cover platform-wide threats. These per-crate deep dives go one level deeper: each crate is decomposed into its trust boundaries (TB-*-N) and every STRIDE category is exercised against each boundary with an attacker scenario, the control/invariant that blocks it, and a pointer to the adversarial / property / TLA+ test that kills it.

**Conventions:**
- Each file is ~3-5 pages, self-contained, cross-linked to `security_model.md §5` (STRIDE matrix), `invariant_registry.md`, `failure_modes.md`, and `PENTEST-EVIDENCE-PACKAGE §3`.
- STRIDE-row counts per boundary: 6 (S, T, R, I, D, E). Total STRIDE rows per file ≥ 24 (≥ 4 boundaries × 6 categories).
- Residual risks captured explicitly with severity and mitigation status.

## Files

| # | Surface | File | Pentest domain (PENTEST-EVIDENCE-PACKAGE §3) | Pentest priority |
|---|---|---|---|---|
| 1 | BYOK envelope ops | [`STRIDE-corelink-byok.md`](STRIDE-corelink-byok.md) | §3.3 BYOK envelope encryption | P0 |
| 2 | Merkle audit chain | [`STRIDE-corelink-audit-chain.md`](STRIDE-corelink-audit-chain.md) | §3.4 Audit chain | P0 |
| 3 | PAT issuance / verify | [`STRIDE-corelink-pat.md`](STRIDE-corelink-pat.md) | §3.1 Identity & Auth | P0 |
| 4 | Clerk JWT session | [`STRIDE-corelink-clerk.md`](STRIDE-corelink-clerk.md) | §3.1 Identity & Auth | P0 |
| 5 | DSR handling | [`STRIDE-corelink-dsr.md`](STRIDE-corelink-dsr.md) | Annex N privacy | P1 |
| 6 | Stripe webhook + reconcile | [`STRIDE-corelink-stripe-real.md`](STRIDE-corelink-stripe-real.md) | §3.2 Billing | P0 |
| 7 | Rate-limit + abuse | [`STRIDE-corelink-rate-limit.md`](STRIDE-corelink-rate-limit.md) | §3.5 D row (noisy-neighbor) | P1 |
| 8 | Admin dual-approval | [`STRIDE-corelink-dual-approval.md`](STRIDE-corelink-dual-approval.md) | §3.3 E row (kill-switch bypass); §3.1 E row | P0 |
| 9 | Tenant prefix derivation (integration surface) | [`STRIDE-corelink-tenant-path.md`](STRIDE-corelink-tenant-path.md) | §3.5 Multi-tenant isolation (integration) | P0 |
| 10 | Region failover routing | [`STRIDE-corelink-failover-router.md`](STRIDE-corelink-failover-router.md) | §3.5 D row + Annex N residency | P1 |
| 11 | Tenant prefix HMAC primitive | [`STRIDE-tenant-path.md`](STRIDE-tenant-path.md) | §3.5 T row (HMAC tenant prefix) | P0 |
| 12 | Privacy consent ledger | [`STRIDE-corelink-privacy-consent-ledger.md`](STRIDE-corelink-privacy-consent-ledger.md) | Annex N privacy | P1 |

## Coverage summary

- **12 crates × 6 STRIDE categories** per boundary × ≥ 4 boundaries per file → ≥ 288 distinct STRIDE rows.
- **Residual risks documented:** 36 (3 per file, averaging — see individual files for exact count).
- **Test pointers:** every STRIDE row links to a concrete `crates/<name>/tests/*.rs`, `crates/<name>/fuzz/`, `specs/tla/*.tla`, or runbook dry-run file.

## How this fits the pentest evidence package

- **Vendor scope (D+0):** vendor uses `PENTEST-EVIDENCE-PACKAGE §3` as platform-level STRIDE baseline plus these 12 per-crate files for component-level test planning.
- **Findings retest (D+30):** any P0/P1 finding referencing a crate covered here MUST cite the corresponding STRIDE file row that should have prevented it.
- **SOC 2 CC3.2:** evidence of risk-identification rigor — STRIDE applied at platform (matrix-stride-ctrl.csv) AND component (this folder) granularity.

## Maintenance

- Update cadence: per sprint that adds a new boundary in any listed crate, plus quarterly review.
- Owner: Security Lead.
- Validator: `python3 scripts/validate_specs.py` (cross-ref integrity).
