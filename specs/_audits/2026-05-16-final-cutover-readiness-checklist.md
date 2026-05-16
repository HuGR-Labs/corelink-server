# Final Cutover Readiness — 1-Page Boolean Checklist — 2026-05-16

> **Companion to:** `specs/_audits/2026-05-16-final-cutover-readiness.md` (full D-day morning consolidation).
> **Read on:** cutover D-day morning, ≤ 5 minutes. Tick each box; if ANY row is `[ ]` (NO) at signature time, decision is **CONDITIONAL GO** or **NO-GO** per §10.1 of the companion doc.
> **Printable:** intended for paper printout; one column of checkboxes; each row self-contained.

---

## A. Sprint impls + production wiring (must be 100%)

- [ ] All 21 spec sprints S-00 → S-20 SEALED (`grep -r work_status specs/04_sprints | grep SEALED | wc -l` ≥ 279).
- [ ] All 9 R-prep waves SEALED (wave-18 → wave-26).
- [ ] `validate_specs.py` exit 0 at cutover-day HEAD.
- [ ] `validate_references.py` exit 0 at cutover-day HEAD.
- [ ] `validate_canonical_consistency.py` exit 0 at cutover-day HEAD.
- [ ] `validate_inv_promotion.py` exit 0 at cutover-day HEAD.

## B. INV-CRITICAL TLA+ coverage (must be 100%)

- [ ] 61 / 61 CRITICAL invariants have TLA+ proof OR §4.3 documented exemption.
- [ ] All 11 INV-CRITICAL subsystems TLA-VERIFIED (BYOK, audit-chain, DSR, tenant isolation, CAS+dedup, multipart, AC, Auth incl. PAT-revoke, replication+shadow, ratelimit+WallClock, Stripe MatClock).
- [ ] 0 orphan INV references in code.
- [ ] 143 / 143 WI-declared INV coverage.

## C. Adversarial review trend (must be PASS)

- [ ] Wave-18 PASS (9.5/10, 0 P0/P1 outstanding).
- [ ] Wave-19 PASS (8.86/10 post-closure).
- [ ] Wave-20 PASS (9.40/10).
- [ ] Wave-21 PASS (9.55/10).
- [ ] Wave-22 PASS (9.45/10).
- [ ] Wave-23 PASS (9.20/10).
- [ ] Wave-24 CLOSED (6.95/10 CONDITIONAL → closed wave-25).
- [ ] Wave-25 adversarial review SEALED with verdict ≥ PASS (no outstanding P0/P1).
- [ ] Wave-26 adversarial review SEALED with verdict ≥ PASS (no outstanding P0/P1).
- [ ] 0 P0 outstanding at cutover-day HEAD.
- [ ] 0 P1 outstanding at cutover-day HEAD.

## D. Mutation kill-rate (DEBT-008)

- [ ] 8 crates empirically CLOSED (audit-chain, hash, dedup, tenant-path, billing-stripe-mat, chunker, multipart-schema, clerk).
- [ ] 5 crates on CI-nightly lane with last 7-day streak ≥ 75% floor (pat, dual-approval, ratelimit, r2-multipart, quota-cas).
- [ ] CI-nightly cron `mutation-nightly.yml` last 7 runs all GREEN.

## E. Chaos coverage

- [ ] 8 isolated fail-CLOSED scenarios green (`cargo test --workspace --features chaos`).
- [ ] 3 combined-failure scenarios green (Partition+BYOK, D1+Stripe, Shadow+Audit-export).
- [ ] 16 `#[test]` cases total in chaos corpus passing.
- [ ] 6 SEV-1/SEV-2 fail-CLOSED alerts wired and exercised in last drill.
- [ ] 0 fail-OPEN regressions since harness SEAL.

## F. Endurance / perf

- [ ] 24h endurance harness SEALED (wave-22, 9.4/10).
- [ ] 10-min compressed dress-run SEALED with 0 SLO + 0 INV violations (wave-25).
- [ ] 7-day soak streak ≥ 168h observation captured pre-cutover (wave-27 evidence).
- [ ] Perf regression CI 5%/15% tolerances active and last 7-day green.

## G. GA-1 feature freeze

- [ ] GA-1 freeze ACTIVE since 2026-05-16 (`scripts/check-ga-freeze-allowed.py` exit 0 on T-7d → T-0h diff).
- [ ] 0 unauthorized §3.a or §3.b exception merges within T-72h.
- [ ] `reports/ga-freeze-monitor.json` ledger reviewed and matches `main` history.
- [ ] DEFER drift detector (wave-25 stream #4) shows counter stable at 8.

## H. Prod-deploy dress-run

- [ ] Wave-26 prod-deploy dress-run PROCEED (9.36/10).
- [ ] All 13 dress-run steps PASS (S0 prep-ring + 11x §3 + S12 cleanup).
- [ ] Greenlight composite `slo:greenlight:composite_ok == 1` in dress-run.
- [ ] 0 / 6 rollback triggers fired in dress-run.
- [ ] T-14d staffed staging-environment dress-rehearsal SEALED per `RB-GA-CUTOVER.md §9`.
- [ ] GA tag draft at `docs/release/v1.0.0-GA-tag-draft.txt` reviewed and signature slots populated.

## I. Compliance posture (7 of 8 + 1 attorney-bound)

- [ ] SOC 2 Type I attestation ready.
- [ ] ISO 27001 Stage-1 audit ready.
- [ ] GDPR rollup current (2026-05-15).
- [ ] LGPD rollup current (2026-05-15).
- [ ] PCI DSS SAQ-A current (2026-05-15).
- [ ] CCPA inherits from GDPR pipeline.
- [ ] FedRAMP Moderate documented NOT-IN-SCOPE.
- [ ] LFPDPPP MX attorney sign-off received (DEBT-025) — OR explicit waiver filed.

## J. Security posture (3 GREEN-blocked-on-external-DEFER + rest GREEN)

- [ ] BYOK 4-provider matrix complete (DEBT-003 AWS Artifact PDF received) — OR explicit waiver filed.
- [ ] RLS WITH CHECK enforced and last sweep GREEN.
- [ ] Audit fail-CLOSED enforced (INV-AUDIT-EMIT-ATOMIC family).
- [ ] WallClock cross-route SEALED (wave-21 9.55/10).
- [ ] Pentest scope SEALED (wave-19); SOW countersigned (D-28).
- [ ] Pentest retest letter received with **zero HIGH/CRITICAL outstanding** (DEBT-026 — earliest 2026-07-29).
- [ ] Pre-GA security attestation rollup signed by Owner + Security Lead.

## K. SLO greenlight (G1..G6 production-real)

- [ ] G1 — p99 latency ≤ SLO across 5 regions (30 min window).
- [ ] G2 — audit-chain integrity verifier (`corelink_audit_chain_integrity_violation_total == 0`) over 24h.
- [ ] G3 — zero SEV-0 / SEV-1 in 72h prior to T-0h.
- [ ] G4 — pilot signups ≥ 5 design-partner attestations.
- [ ] G5 — Neon shadow lag p99 ≤ 300s (30 min).
- [ ] G6 — DSR cron 24h success rate 100% (runs > 0 AND failed == 0).
- [ ] Composite `slo:greenlight:composite_ok == 1`.

## L. Operational readiness

- [ ] All §0.2 SEV-0 / SEV-1 runbooks reviewed within last 30d.
- [ ] Primary + secondary + tertiary SRE rota confirmed in PagerDuty for T-0h ± 24h.
- [ ] War-room availability confirmed (Owner + CTO + VPProduct + VPSec).
- [ ] Customer comms drafted + signed-off (pilot pre/during/post + public blog + investors).
- [ ] Statuspage banner template ready (DEBT-016 provisioning complete at T-7d).
- [ ] T-7d infrastructure baseline snapshot captured (D1 + R2 + Neon + audit-chain head SHA per region).
- [ ] DPO + Legal regulatory no-breach attestation signed (`§0.7`).

## M. External DEFER items (8 — all 8 lines below must be `[x]` or explicit waiver)

- [ ] DEBT-025 — LFPDPPP MX attorney sign-off received OR waiver filed.
- [ ] FW-H-3 Security Lead nominated OR ADR-0034b dual-hat fallback (on-call SRE Lead acts as Signature 2) applied.
- [ ] DEBT-026 — Pentest vendor SOW countersigned.
- [ ] DEBT-003 — AWS Artifact PDF downloaded + sha256 recorded.
- [ ] DEBT-016 — Statuspage `status.corelink.humangr.com` go-live complete.
- [ ] G4 — Pilot signups ≥ 5 design-partner attestations (sales-bound).
- [ ] DEBT-026 final — Pentest retest letter zero HIGH/CRITICAL (vendor-bound; earliest 2026-07-29).
- [ ] Owner sign-off (ADR-0034b 2-key) — populated in §10 of companion doc.

---

## N. Final 2-key signature

**Signature 1 — Owner:**

- [ ] Decision: ☐ GO  ☐ CONDITIONAL  ☐ NO-GO
- [ ] Name: Gustavo Schneiter
- [ ] Date: ________
- [ ] Signature: ________________________________________________________

**Signature 2 — Security Lead OR on-call SRE Lead (per ADR-0034b dual-hat fallback):**

- [ ] Decision: ☐ GO  ☐ CONDITIONAL  ☐ NO-GO
- [ ] Name: ________________________________________________________
- [ ] Date: ________
- [ ] Signature: ________________________________________________________

---

**Decision rule:** if every row in A..M is `[x]` (or carries an explicit Owner-approved waiver per `GA-GATE-GO-NOGO-TEMPLATE.md §3`), and both N signatures are GO, then **execute `RB-GA-CUTOVER.md §0..§3` immediately**. Otherwise the decision is CONDITIONAL or NO-GO per `2026-05-16-final-cutover-readiness.md §10.1` pre-condition gate.

Cross-ref: `specs/_audits/2026-05-16-final-cutover-readiness.md` (full evidence) + `specs/_runbooks/RB-GA-CUTOVER.md §0` (cutover entry point).

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
