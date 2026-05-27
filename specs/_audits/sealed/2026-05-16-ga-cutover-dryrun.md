# RB-GA-CUTOVER Dry-Run Audit — 2026-05-16

> **Doc kind:** dry-run execution audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Wave / Stream:** Wave-24 R-PREP / `wt/r-prep-ga-cutover-dryrun` (agent: Claude Opus 4.7 background worker).
> **Base:** `main` @ `33138b5` (wave-23 SEAL tip — "merge wt/r-prep-debt-008-mutation-wave23 into main (wave-23)").
> **Scope:** End-to-end simulated execution of `specs/_runbooks/RB-GA-CUTOVER.md` §3 (11-step T-0h cutover sequence) against in-process fakes, with §4 greenlight evidence capture (G1..G6 + composite) and §5 rollback-trigger evaluation. Backstop for the §9 production dress-rehearsal mandate.
> **Cross-ref:** `specs/_runbooks/RB-GA-CUTOVER.md` v1.0.0 (wave-19 `dc39000`), `dashboards/alerts/dash-ga-greenlight.yml` (wave-19), `specs/_audits/sealed/2026-05-16-wave23-closure.md` §"GA cutover dry-run readiness assessment".

---

## 1. Pre-cutover state — T-72h checklist (simulated GREEN)

The full `RB-GA-CUTOVER.md` §0 checklist (T-7d items) and §1 (T-72h items) are asserted GREEN as the entry precondition for the dry-run. The dry-run runner (`scripts/ga-cutover-dryrun.sh`) emits the per-check verdict into the evidence JSON under `pre_cutover_checklist[]`. The full enumerated set (10 sentinel checks) is:

| # | Check | Maps to RB-GA-CUTOVER section | Status (dry-run) |
|---|---|---|---|
| 1 | `wave18_seal_landed` | §0.1.1 (wave-18 SEAL merged on `main`) | GREEN |
| 2 | `validate_specs_green` | §0.1.2 | GREEN (real run: exit 0) |
| 3 | `validate_references_green` | §0.1.2 | GREEN (real run: exit 0) |
| 4 | `all_21_prrs_approved` | §0.1.3 | GREEN |
| 5 | `codex_reviews_ge_8` | §0.1.4 | GREEN |
| 6 | `rb_ga_launch_rollback_drilled_within_7d` | §0.2.1 | GREEN |
| 7 | `schema_freeze_active_t_minus_72h` | §1.1 | GREEN |
| 8 | `secrets_matrix_clean` | §1.3 | GREEN |
| 9 | `regulatory_no_breach_attestation_signed` | §0.7 | GREEN |
| 10 | `rb_dress_rehearsal_completed_at_t_minus_14d` | §9 | GREEN (dress-rehearsal mandate is the parent stream for this dry-run; this audit is the *backstop*, not the substitute) |

Items 1, 4, 5, 6, 7, 8, 9, 10 are simulated GREEN (the in-process dry-run cannot itself verify they are true at production T-72h; those checks remain owned by the named SREs in the runbook). Items 2 and 3 are independently verified live by the harness against the present worktree.

`validate_specs.py` exit code: `0` (453 schema-validated specs).
`validate_references.py` exit code: `0` (0 dangling references).

## 2. Step-by-step execution log

Each step from RB-GA-CUTOVER §3.1..§3.11 was implemented as a `step_N_body` bash function in `scripts/ga-cutover-dryrun.sh`. Each function (a) updates the in-process metric registry (an associative array fake of the Prometheus rules in `dash-ga-greenlight.yml`), (b) calls `audit_emit` to append a synthetic JSONL line to the dry-run audit bag, and (c) returns 0/1 to feed into the per-step PASS/FAIL outcome. Per-step wall-clock duration is captured by the `run_step` wrapper.

| # | RB-GA-CUTOVER step | Simulated fake | Outcome | Duration (ms) | Audit emit? |
|---|---|---|---|---|---|
| S1 | §3.1 — Provision / verify R2 buckets (5 regions × 5 purposes = 25) | `fake_r2_provision_all` (InMemoryR2: bucket inventory list + synthetic CORS/lifecycle attestation) | PASS | 126 | yes |
| S2 | §3.2 — Apply Neon shadow migrations (idempotent) | `fake_neon_apply_additive` (two-pass: first pass 17 stmts; second pass 0 ERROR/FATAL) | PASS | 131 | yes |
| S3 | §3.3 — Deploy CF Worker gradual rollout 1→10→50→100 | `fake_worker_rollout` per stage (p99 violation rate, dedup drift, SEV-0/1 count, hold pass) | PASS | 142 | yes |
| S4 | §3.4 — Enable BYOK orchestrator | `fake_byok_smoke` (singleton-fake→real provider per active tenant; 0 `BYOKProviderUnavailable`) | PASS | 120 | yes |
| S5 | §3.5 — Enable Stripe live mode + DLQ consumers | `fake_stripe_webhook_smoke` (test event signed-verified, 0 DLQ landings) | PASS | 110 | yes |
| S6 | §3.6 — Enable Clerk JWT + WebAuthn admin | `fake_clerk_webauthn_smoke` (JWKS 3 keys, WebAuthn enroll OK) | PASS | 112 | yes |
| S7 | §3.7 — Enable audit-chain Logpush + Neon shadow sync | `fake_audit_logpush_neon_sync` (head SHA in R2 + Neon; integrity violation == 0) | PASS | 120 | yes |
| S8 | §3.8 — Enable DSR Statuspage cron | `fake_dsr_cron_first_run` (24 runs / 24h, 0 failures) | PASS | 122 | yes |
| S9 | §3.9 — DNS cutover apply (TTL 60s, 4 CNAMEs) | `fake_dns_cutover_apply` (≥ 3 resolvers per CNAME) | PASS | 124 | yes |
| S10 | §3.10 — Enable customer-facing rate limits | `fake_rate_limit_flip_ga` (posture preview→ga; synthetic load trips canonical) | PASS | 115 | yes |
| S11 | §3.11 — Statuspage transition + `public_status=GA` | `fake_statuspage_transition` (components OPERATIONAL; signup_open flag) | PASS | 114 | yes |

Total wall-clock for the §3 sequence: ~1.4 s (in dry-run; the production target is ≤ 4 h with 15-min holds between gradual rollout stages per §3 preamble). All 11 audit emits are captured under `audit_emits[]` in the JSON evidence.

## 3. Greenlight verdict per G1..G6

Each criterion is evaluated against the same recording-rule thresholds declared in `dashboards/alerts/dash-ga-greenlight.yml`. The metric registry is read at the end of §3 sequence (S11 complete). All 6 are GREEN.

| Gate | Recording rule | Threshold | Metric value (dry-run) | Verdict |
|---|---|---|---|---|
| G1 | `slo:greenlight:p99_latency_regions_ok` | violation rate < 0.01 across all 5 regions, sustained 30 min | 0.003 max across stages 1/10/50/100 | GREEN |
| G2 | `slo:greenlight:audit_chain_integrity` | `corelink_audit_chain_integrity_violation_total == 0`, sustained 24h | 0 | GREEN |
| G3 | `slo:greenlight:sev01_zero_72h_ok` | 0 SEV-0/1 incidents in 72h prior + during cutover | 0 SEV-0/1 across all 4 gradual stages | GREEN |
| G4 | `slo:greenlight:pilot_attestations_ok` | `corelink_ga_pilot_attestation_signed_count >= 5` at T-24h ± 6h | 5 | GREEN |
| G5 | `slo:greenlight:neon_shadow_lag_ok` | `neon_shadow_replication_lag_seconds_p99 <= 300`, sustained 30 min | 42 | GREEN |
| G6 | `slo:greenlight:dsr_cron_24h_success` | runs > 0 AND failed == 0, sustained 24h prior | 24 runs / 0 failed | GREEN |
| Composite | `slo:greenlight:composite_ok` | logical AND of G1..G6 | 1 | GREEN |

`scripts/ga-cutover-dryrun.sh` exit code: `0`.
`scripts/analyze-ga-cutover-dryrun.py reports/ga-cutover-dryrun-2026-05-16.json` exit code: `0`.

## 4. Rollback trigger evaluation (RB-T1..RB-T6)

Each of the 6 §5.1 trigger conditions is evaluated against the same in-process metric registry. None fired.

| # | Trigger | Source signal | Fired? |
|---|---|---|---|
| RB-T1 | Any SEV-0 incident during cutover window or 4h post-§3.11 | PagerDuty severity = SEV-0 | NO |
| RB-T2 | ≥ 2 SEV-1 incidents within 30 min during cutover window | PagerDuty incident burst | NO |
| RB-T3 | P99 latency SLO breach > 15 min at any §3.3 stage or post-§3.11 | `slo:cas_get_latency_violation_rate:1h` ≥ 14.4× target × 15 min | NO |
| RB-T4 | Audit-chain integrity break detection | `corelink_audit_chain_integrity_violation_total > 0` | NO |
| RB-T5 | Any G1..G6 flips RED > 5 min during cutover window | `slo:greenlight:composite_ok == 0` for 5 min | NO (composite = 1) |
| RB-T6 | Lighthouse customer signals withdrawal | `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` §3 state transition | NO |

Triggers fired: **0 / 6**. The §5.2 2-key auth decision-tree (SRE Lead + Product Lead) was not entered. No `RB-S1`..`RB-S5` rollback actions were executed.

## 5. GA-readiness score (calibrated)

Calibration formula (rubric anchored to wave-23 closure rubric §6 + this dry-run's quality):

| Dimension | Weight | Score (0..10) | Weighted |
|---|---|---|---|
| Greenlight criteria GREEN count (6/6) | 0.30 | 10.0 | 3.00 |
| §3 step pass rate (11/11 PASS) | 0.20 | 10.0 | 2.00 |
| Per-step audit emit completeness (11/11) | 0.10 | 10.0 | 1.00 |
| Rollback trigger evaluator non-fire (6/6 NO) | 0.10 | 10.0 | 1.00 |
| Dry-run vs production parity (in-process fakes only — *not* real Cloudflare staging) | 0.20 | 5.0 | 1.00 |
| Evidence reproducibility (script + analyzer exit-code gated; JSON schema deterministic) | 0.10 | 9.0 | 0.90 |
| **Total** | **1.00** | — | **8.90 / 10** |

**Score: 8.9 / 10.** The single quality discount is the dry-run/production parity dimension: this audit asserts the *runbook orchestration is internally consistent and the recording-rule thresholds evaluate cleanly when fed canonical greenlight values*, but does NOT substitute for the §9 mandated production dress-rehearsal against a real staging environment (Cloudflare DO + R2 + Neon + Clerk + Stripe sandbox). That dress-rehearsal remains a hard precondition for GA cutover authorization per §8 2-key sign-off.

## 6. Recommendation

**Recommendation: PROCEED to wave-25 pentest engagement.**

Rationale:

1. All 6 greenlight criteria evaluate GREEN against the canonical recording-rule thresholds (`dash-ga-greenlight.yml`).
2. All 11 §3 steps execute cleanly under the in-process fake harness with deterministic per-step audit emits.
3. No §5 rollback trigger fires; the 2-key auth decision tree is not engaged.
4. `validate_specs.py` + `validate_references.py` are GREEN at the dry-run base SHA (`33138b5`).
5. Wave-25 pentest engagement (the parent dispatch context) does NOT require the §9 production dress-rehearsal to start; it requires only that the GA-cutover orchestration logic is internally consistent and audit-emitting (which this dry-run attests).
6. The §9 production dress-rehearsal at T-14d remains independently mandated and is NOT satisfied by this audit; it must be executed by SRE Lead against a real staging environment when T-14d enters the window.

Block list:

- The §9 dress-rehearsal still needs a real staging environment slot (Cloudflare DO + R2 + Neon + Clerk + Stripe sandbox) before T-14d. Not blocking wave-25 pentest start; blocking T-0h cutover authorization.
- §0.4 customer comms templates (`marketing/launch/COMMS/*.md`) must be authored and 2-key signed-off per VPProduct + CEO. Not blocking wave-25 pentest; blocking T-7d entry.

No RETRY or BLOCK condition fires.

---

## Annex A — Evidence artifacts

- Dry-run script: `scripts/ga-cutover-dryrun.sh`
- Analyser script: `scripts/analyze-ga-cutover-dryrun.py`
- Evidence JSON: `reports/ga-cutover-dryrun-2026-05-16.json`
- Runbook source: `specs/_runbooks/RB-GA-CUTOVER.md` (§0.5 dry-run history subsection updated by this audit)
- Recording rules: `dashboards/alerts/dash-ga-greenlight.yml`

## Annex B — Reproducer

```bash
git worktree add .claude/worktrees/agent-ga-cutover-dryrun -b wt/r-prep-ga-cutover-dryrun 33138b5
cd .claude/worktrees/agent-ga-cutover-dryrun
bash scripts/ga-cutover-dryrun.sh --date 2026-05-16
python3 scripts/analyze-ga-cutover-dryrun.py reports/ga-cutover-dryrun-2026-05-16.json
# Both exit 0 if all 6 greenlights GREEN AND all 11 steps PASS.
```

## Annex C — Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Wave-24 dry-run agent (Claude Opus 4.7) | Initial dry-run audit — 11/11 §3 steps PASS, 6/6 greenlights GREEN, 0/6 rollback triggers fired, GA-readiness 8.9/10, recommendation PROCEED to wave-25 pentest engagement. |
