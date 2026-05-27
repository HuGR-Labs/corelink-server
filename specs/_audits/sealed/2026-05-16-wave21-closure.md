# Wave-21 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-readiness rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-21 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave21-sweep`.
> **Base:** `main` @ `30e5f66` (wave-20 SEAL tip: "merge wt/r-prep-wave18-codex-p2-p3-closure into main (wave-20)").
> **Scope:** INV registry hygiene + DEBT register survey + wave-21 stream catalogue + GA-readiness snapshot post wave-20 streams cataloguing + wave-22 candidate streams.
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-wave20-closure.md` (predecessor), `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.0, `specs/03_architecture/invariant_registry.md`.

---

## 1. Wave-21 scope — 10 streams catalogued

Wave-21 is the next post-wave-20 R-PREP hardening wave, dispatched on `main` @ `30e5f66`. Ten parallel streams catalogued (this stream is #10):

| # | Stream | Branch | Disposition |
|---|---|---|---|
| 1 | Wave-18 streamB B-P1-01 — Neon shadow RLS `WITH CHECK` + emit-discipline cleanup | `wt/r-prep-neon-shadow-trait-cleanup` | **IN FLIGHT** (wave-21) |
| 2 | DEBT-008 mutation full sweep — `corelink-pat` + `corelink-clerk` empirical baseline | `wt/r-prep-debt-008-mutation-sweep` | **IN FLIGHT** (wave-21) |
| 3 | DEBT-014 FT-6 / FT-7 / FT-8 / FT-9 — remaining TLA+ followups (FT-7 CRITICAL-upgrade prioritised) | `wt/r-prep-debt-014-tla-specs` | **IN FLIGHT** (wave-21) |
| 4 | DEBT-015 apps/docs Node 22 ESM blockers (a)+(b) — draft-page stubs + MDX extensionless normalisation | `wt/r-prep-debt-015-node22-esm` | **IN FLIGHT** (wave-21) |
| 5 | Secrets matrix validator false-positive (single-letter `X` env-var pattern) | `wt/r-prep-secrets-x-false-positive` | **IN FLIGHT** (wave-21) |
| 6 | Tenant config region-resolver wiring across boot path | `wt/r-prep-tenant-config-region-resolver` | **IN FLIGHT** (wave-21) |
| 7 | Wave-19 streamA A-P1-01 follow-on doc-fix (audit-export memory profile spec note) | `wt/r-prep-w19-p1-01-doc-fix` | **IN FLIGHT** (wave-21) |
| 8 | Wallclock unification across route boundaries (`Clock` trait expansion) | `wt/r-prep-wallclock-cross-route` | **IN FLIGHT** (wave-21) |
| 9 | Wave-20 adversarial review — codex pass over the 9 wave-20 closure streams | `wt/r-prep-wave20-adversarial-review` | **IN FLIGHT** (wave-21) |
| 10 | **Wave-21 INV registry sweep + DEBT register survey + closure audit** (this stream) | `wt/r-prep-inv-registry-wave21-sweep` | **CLOSED via this commit** |

Streams #1–#9 are dispatched in parallel by the orchestrator; this stream (#10) performs the hygiene + cataloguing pass against the same `30e5f66` base. Per the user mandate (charter §"DO NOT close these in this stream since they're still in flight. Just survey + report current state."), streams #1–#9 are surveyed below but **not** closed by this audit. Their SEAL commits land separately and the next wave-22 sweep reconciles.

---

## 2. Wave-20 caveat closures — verification (cross-ref wave-21 streams #1, #7, #8)

The wave-20 closure audit (`2026-05-16-wave20-closure.md §4.2`) flagged 7 agent-closable wave-21 candidates. Verification of each against the wave-21 dispatch map:

| Wave-20 caveat | Wave-21 stream | Status |
|---|---|---|
| Wave-18 streamB B-P1-01 — RLS `WITH CHECK` on Neon shadow migrations | **Stream #1** `wt/r-prep-neon-shadow-trait-cleanup` | IN FLIGHT — branch dispatched; awaits SEAL commit. |
| Wave-18 streamA A-P1-02 — `audit_export.rs:423` cross-tenant arm Err discarded | (rolled into wave-20 commit `5a0addf` — audit-export-fail-closed-emit-discipline) | **CLOSED wave-20** — `let _ = sink.emit(...)` replaced with branch-on-Err returning 503; verified in `crates/corelink-worker/src/routes/audit_export.rs`. |
| Wave-18 streamA A-P1-03 — `audit_export.rs:799-808` mid-stream chain-break emit Err discarded | (rolled into wave-20 commit `5a0addf`) | **CLOSED wave-20** — same commit; mid-stream short-circuit lands trailer with `exit_status="emit_failed"`. |
| Wave-18 streamA A-P1-01 follow-on (memory profile doc note) | **Stream #7** `wt/r-prep-w19-p1-01-doc-fix` | IN FLIGHT — doc-only fix; awaits SEAL. |
| DEBT-014 FT-6/FT-7/FT-8/FT-9 | **Stream #3** `wt/r-prep-debt-014-tla-specs` | IN FLIGHT — FT-7 CRITICAL-upgrade prioritised per wave-20 §7. |
| DEBT-008 mutation full sweep `corelink-pat` + `corelink-clerk` | **Stream #2** `wt/r-prep-debt-008-mutation-sweep` | IN FLIGHT — empirical baseline run dispatched. |
| DEBT-010 CI optimisation P2/P3 (7 of 11 still OPEN) | (deferred post-GA per wave-20 §4.2) | Deferred — no wave-21 stream allocated; tracked in `ci-optimization-followup-tickets.md`. |
| DEBT-013 perf optimization OPT-03(b) / OPT-04 phase 2 / OPT-08 | (deferred post-GA per wave-20 §4.2) | Deferred — explicit deferrals per `perf-optimization-followup-tickets.md`. |
| DEBT-015 apps/docs Node 22 ESM blockers (a)+(b) | **Stream #4** `wt/r-prep-debt-015-node22-esm` | IN FLIGHT. |

**Wave-20 caveats verified closed by wave-20 commits:** 2 (audit-export emit-discipline pair A-P1-02 + A-P1-03 via `5a0addf`).
**Wave-20 caveats addressed by wave-21 in-flight streams:** 6 (streams #1, #2, #3, #4, #7 above).
**Wave-20 caveats explicitly deferred post-GA:** 2 (DEBT-010 P2/P3 + DEBT-013 OPT-03(b)/OPT-04ph2/OPT-08).

Additional wave-21 streams beyond the wave-20 caveat backlog: #5 (secrets X false-positive), #6 (tenant config region resolver), #8 (wallclock cross-route), #9 (wave-20 adversarial review). These are net-new wave-21 surfaces (not on the wave-20 §7 list).

---

## 3. DEBT register state post-wave-21 dispatch (pre-SEAL)

Per `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.0 (last reconciled in wave-20). No DEBT closures performed by this stream (survey-only, per charter). The state below reflects the post-wave-20 canonical OPEN rows; wave-21 in-flight streams have **not yet** flipped to CLOSED in the register.

### 3.1 Open count + per-priority breakdown (canonical rows; pre-wave-21-SEAL)

| Priority | Open IDs | Count | Wave-21 closure ETA |
|---|---|---|---|
| **P0** | DEBT-003 (AWS Artifact PDF — user-bound) | 1 | Pending human (no wave-21 stream) |
| **P1** | DEBT-008 (partial 1/3 — pat + clerk remain) | 1 partial | **Stream #2** ETA wave-21 SEAL |
| **P1** | DEBT-010 (partial 4/11 — 7 P2/P3 deferred post-GA) | 1 partial | Deferred post-GA |
| **P1** | DEBT-013 (partial 6/10 — 4 explicit deferrals) | 1 partial | Deferred post-GA |
| **P1** | DEBT-014 (partial 5/9 — FT-6/7/8/9 remain) | 1 partial | **Stream #3** ETA wave-21 SEAL |
| **P2** | DEBT-015 (Node 22 ESM blockers a+b) | 1 | **Stream #4** ETA wave-21 SEAL |
| **P2** | DEBT-016 (Statuspage go-live — user-bound) | 1 | Pending human (no wave-21 stream) |

**Total truly-OPEN canonical rows pre-wave-21-SEAL:** 7 (unchanged from wave-20 close).
**Post-wave-21-SEAL projection** (assuming streams #2, #3, #4 SEAL successfully): 4 OPEN (DEBT-003 + DEBT-010 deferred + DEBT-013 deferred + DEBT-016) — with DEBT-008 and DEBT-014 flipping to fully CLOSED, DEBT-015 flipping to CLOSED.

### 3.2 Closed count

Per wave-20 §6: 24 CLOSED (16 explicit closures + 8 duplicate-row reconciliations). No new closures this wave; closed count unchanged at **24**.

### 3.3 Closure ETA summary

| ETA bucket | Rows |
|---|---|
| Wave-21 SEAL (next 1–2 weeks) | DEBT-008, DEBT-014, DEBT-015 (via streams #2/#3/#4) |
| Pre-GA Gate (T+30d) | DEBT-003 (user-bound; AWS Artifact PDF download) |
| T-7d pre-launch | DEBT-016 (user-bound; Statuspage provisioning) |
| Post-GA (T+90d horizon) | DEBT-010 P2/P3 (7 tickets), DEBT-013 deferrals (4 tickets) |

---

## 4. INV registry state (post-wave-21 sweep)

Per `python3 scripts/validate_canonical_consistency.py` on this branch (post-sweep):

| Metric | Count |
|---|---|
| INVs declared (registry §3 rows) | **191** |
| └ CRITICAL | **60** |
| └ HIGH | **127** |
| └ MEDIUM | **4** |
| └ LOW / UNKNOWN | **0** |
| Aliases declared (registry §5) | 13 |
| TLA+ verified (declared INVs proved in `specs/tla/*.tla`) | 76 |
| Code-referenced (declared INVs cited in `crates/*/src/`) | 103 |
| Test-referenced (declared INVs cited in `crates/*/tests/`) | 89 |
| Orphan refs (in code, NOT in registry+aliases) | **0** |
| CRITICAL without TLA+ proof | **0** |
| Declared with NO code/test reference | 70 (forward-looking / spec-only) |
| Declared test-only (test ref but no src/) | 18 (assertions about absent-behaviour) |

Per `python3 scripts/validate_inv_promotion.py`: registry coverage **143/143** (all WI-declared INVs present in registry §3). Identical to wave-20 close — **no INV DRAFT entries with sufficient coverage surfaced for promotion** in this sweep window (wave-20 commits did not introduce new INV references in code/tests that required canonical promotion).

**INV-DRAFT → INV-PROMOTED count this wave:** **0** (registry stable; full coverage maintained).

### 4.1 Why no promotions this wave

Wave-20 closures (`wt/r-prep-tokio-postgres-binder`, `wt/r-prep-audit-export-fail-closed-emit-discipline`, `wt/r-prep-neon-shadow-rls-with-check-and-emit-discipline`, etc. — see wave-20 closure §1 streams 1–9) all landed **emit-discipline + binder wiring** changes that re-used **existing canonical INVs** (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER, INV-AUTH-SCHEMA-RLS-DEFAULT-ON, INV-AUDIT-CHAIN-EMIT-ORDERED). No net-new INV identifiers introduced in source/test code; therefore the registry coverage 143/143 is stable and no DRAFT-tier entries require promotion. The 70 declared-but-unreferenced entries are intentional forward-looking entries (per wave-20 §5) — their forward-looking status is preserved by design.

---

## 5. GA-readiness state — post wave-21 dispatch

What's left blocking GA after wave-21 streams complete (cross-ref wave-20 §4):

### 5.1 User-bound items (unchanged from wave-20 §4.1)

| Item | Status | Blocker |
|---|---|---|
| LFPDPPP MX attorney sign-off | Pending | Mexican attorney sign-off on residency + retention; no agent can execute. |
| FW-H-* role nominations | Pending | PRR dual-hat row decompositions across S-06 / S-09 / S-13 — Gustavo to onboard / nominate. |
| External pentest engagement kickoff | Scoped | Vendor + SOW pending; scope SEALED wave-19 (`2026-05-16-pre-ga-pentest-scope.md`). |
| Pilot signups (≥ 3 design-partners) | Pending | Onboarding flow ready (S-19 SEALED); pilot agreements + DPA signing pending external counterparty. |
| AWS Artifact PDF download (DEBT-003 closure) | Pending | Human downloads + `sha256sum` to fill `TBD-on-receipt` in `BYOK-FIPS-ATTESTATION-MATRIX.md`. |
| Statuspage `status.corelink.humangr.com` go-live (DEBT-016) | Pending | Operator follows `STATUSPAGE-INIT.md` T-7d pre-launch. |

### 5.2 Agent-closable, post-wave-21 SEAL — projected residual

Assuming all 9 wave-21 in-flight streams SEAL successfully:

| Residual item | Severity | Disposition |
|---|---|---|
| DEBT-010 CI optimisation P2/P3 (7 tickets) | P2/P3 | Explicitly deferred post-GA per wave-20 §4.2. |
| DEBT-013 perf optimisation deferrals (4 tickets: OPT-03b, OPT-04ph2, OPT-08, OPT-03a infeasible) | P2 | Explicitly deferred post-GA. |
| Wave-20 adversarial review findings (stream #9) | TBD | Findings surface during wave-21 SEAL; trigger wave-22 streams if P0/P1 surfaces. |
| Wave-21 codex Opus review (P0 streams: #1 RLS WITH CHECK + #3 TLA FT-7 CRITICAL) | TBD | Mandatory per wave-21 SEAL gate (charter §"all P1-classified streams must close before wave-22 unblocks P2"). |

### 5.3 GA gate posture

Per `RB-GA-CUTOVER.md` greenlight dashboard (wave-19 commit `dc39000`):

- **Spec corpus:** 191 INVs declared, 0 orphan, 0 CRITICAL without TLA+. ✅ GREEN.
- **Test/code coverage:** 103 src-referenced + 89 test-referenced; 70 forward-looking + 18 test-only by design. ✅ GREEN.
- **Mutation kill-rate:** corelink-audit-chain 84.24% (above 75% floor); pat + clerk pending wave-21 stream #2 empirical landing. 🟡 YELLOW (stream #2 will green).
- **Replication SLO observation streak:** SLO §4.27–§4.29 wiring landed wave-15 DEBT-011; observation streak accumulating. ✅ GREEN.
- **DEBT P0 OPEN:** 1 (DEBT-003 user-bound). 🟡 YELLOW pending human action.
- **External pentest:** scope SEALED; engagement pending. 🟡 YELLOW pending vendor + SOW.

Net GA-Limited gate readiness: **3 user-bound items + 2 wave-21 in-flight streams (#2 mutation + #3 TLA FT-7) gate the GREEN cutover**. No new structural blockers surfaced wave-21.

---

## 6. Wave-22 candidate streams + caveats from wave-21 findings

Recommended wave-22 dispatch list (ordered by GA-blocker severity, contingent on wave-21 SEAL):

| # | Stream | Rationale | Estimated cost |
|---|---|---|---|
| 1 | **Wave-21 adversarial review (codex Opus pass on wave-21 P1 streams)** | Mandatory per charter §"all P1-classified streams must close before wave-22 unblocks P2". Cross-review streams #1 (RLS WITH CHECK), #2 (mutation), #3 (TLA FT-7 CRITICAL), #4 (Node 22 ESM). | ~1 codex Opus pass + 1 audit doc per stream. |
| 2 | **Wave-21 stream-#9 adversarial findings absorption** | If stream #9 surfaces P0/P1 findings on wave-20 closure streams, dispatch fix agents. | Branch-per-finding; size depends on findings. |
| 3 | **DEBT-010 P2 CI optimisation batch** (concurrency cancel + shared rust-cache key + TLC matrix + paths-filter audit) | Post-GA polish; ~30 min/PR cumulative savings. | 1 Sonnet × 4 P2 tickets. |
| 4 | **DEBT-013 perf optimisation post-GA batch** (OPT-03b + OPT-04 phase 2 + OPT-08) | Post-GA polish; tail-latency p99 ~3–5% additional reduction. | 1 Sonnet × 3 deferrals. |
| 5 | **DEBT-014 FT-7 follow-on if stream #3 only ships partial** (e.g., FT-6 + FT-8 + FT-9 land but FT-7 byok_dek_race needs deeper TLA spec) | CRITICAL-upgrade prioritised; depends on wave-21 stream #3 outcome. | 1 Sonnet + 1 dedicated TLA spec branch. |
| 6 | **Pentest engagement kickoff support (RFP + vendor shortlist + SOW template)** | User-bound for final vendor selection; agent can draft RFP. | 1 Sonnet × draft RFP + shortlist + SOW template. |
| 7 | **Pilot onboarding tooling polish** (DPA signing automation, pilot success dashboard) | Post-design-partner-signup hygiene; depends on first pilot signups. | 1 Sonnet × ≤2 weeks. |

### 6.1 Caveats from wave-21 sweep findings

- **No INV registry drift detected this wave.** Registry stable at 191 declared / 143 WI-coverage; no DRAFT promotion warranted.
- **No DEBT register drift detected this wave.** 7 canonical OPEN rows pre-SEAL; wave-21 streams #2/#3/#4 in flight will reduce OPEN to 4 post-SEAL.
- **Wave-20 streamA caveat A-P2-01..A-P2-05 (5 low-severity audit-export findings)** — flagged wave-19 codex review, classified non-GA-blocker. No wave-21 stream allocated; tracked as background polish.
- **TokioPostgresExecutor binder (wave-20 commit `a7ddea3`)** — closure verified via `crates/corelink-worker/src/main.rs` boot path; no follow-on caveats surfaced wave-21.
- **NeonExecutor RLS WITH CHECK gap remains structurally open** until stream #1 SEALs. If stream #1 SEAL slips wave-21, escalate to wave-22 P0 priority (defense-in-depth gap vs `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` CRITICAL).
- **Codex Opus is the bound resource for adversarial review.** Wave-22 stream #1 (adversarial pass) requires 1 codex Opus invocation per wave-21 P1 stream — 4 minimum.

### 6.2 Wave-22 entry caveats

- **DCO + Co-Authored-By preserved** on every commit (same as wave-19/20/21).
- **No `--no-verify` hooks.** Pre-commit failures must surface root cause.
- **Synchronous Bash only.** No `run_in_background`. Same charter as wave-19/20.
- **40-min time budget per stream** (matches wave-20/21 cadence); 6–8 streams per wave realistic.
- **Wave-22 SEAL gate:** all wave-21 P1 streams (#1 RLS, #2 mutation, #3 TLA FT-7) verified closed before wave-22 unblocks any new P2/P3 streams.

---

## 7. Quality gates verified

Per the wave-21 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present. |
| Canonical consistency validator | `python3 scripts/validate_canonical_consistency.py` | exit 0 — 191 INVs declared, 0 orphan refs, 0 CRITICAL without TLA+. |
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — 440 with schema + 9 YAML-only (449 total). |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references. |

---

## 8. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave21-sweep`
- **Base commit:** `30e5f66` (wave-20 SEAL tip)
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-21 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 9. Cross-references

- `specs/_audits/sealed/2026-05-16-wave20-closure.md` (wave-20 closure; predecessor).
- `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.0 (DEBT register canonical state; no new changelog entry this wave — survey-only).
- `specs/03_architecture/invariant_registry.md` (unchanged this wave; full coverage maintained).
- `specs/_audits/sealed/2026-05-15-canonical-consistency-baseline.md` (CI ratchet floor; DEBT-004 closure log §3.1).
- Wave-19/20 audit docs (cross-referenced via wave-20 closure §10).
- `RB-GA-CUTOVER.md` (wave-19 cutover runbook; greenlight dashboard).
- `specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md` (wave-19 pentest scope; engagement checklist).
