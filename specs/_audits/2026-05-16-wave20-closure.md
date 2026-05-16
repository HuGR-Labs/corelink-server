# Wave-20 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-readiness rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-20 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave20-sweep`.
> **Base:** `main` @ `2eec064` (wave-19 SEAL tip: "merge wt/r-prep-audit-export-async-pages into main (wave-19)").
> **Scope:** INV registry hygiene + DEBT register reconciliation + wave-20 stream catalogue + GA-readiness snapshot post wave-20.
> **Cross-ref:** `specs/_audits/2026-05-15-debt-register.md` (changelog v1.2.0 entry for this sweep); `specs/03_architecture/invariant_registry.md`; wave-19 closure docs (`2026-05-16-stripe-wasm32-gate-lift.md`, `2026-05-16-neon-shadow-real-driver.md`, `2026-05-16-wave18-adversarial-review-streamA-audit-export.md`, `2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md`).

---

## 1. Wave-20 scope — 10 streams catalogued

Wave-20 is a post-wave-19 R-PREP hardening wave focused on hygiene + GA-gate prep rather than net-new feature surfaces. Ten parallel streams catalogued (this stream is #10):

| # | Stream | Branch | Disposition |
|---|---|---|---|
| 1 | wave-18 codex P1/P2/P3 closures (streamA audit-export) | `wt/r-prep-audit-export-streaming` (wave-18) + `wt/r-prep-audit-export-payload-column` + `wt/r-prep-audit-export-async-pages` (wave-19) | CLOSED via wave-19 commits `dc8a6bb` + `f6f7c12` |
| 2 | wave-18 codex P1/P2/P3 closures (streamB Neon shadow real driver) | `wt/r-prep-neon-shadow-real-driver` (wave-19) | CLOSED via wave-19 commit `ec6e83b` |
| 3 | Stripe `corelink-stripe-real` wasm32 crate-root cfg gate lift | `wt/r-prep-stripe-wasm32-gate-lift` (wave-19) | CLOSED via wave-19 commit `1991cca` |
| 4 | DSR `dsr_erasure_log.outcome_json` snapshot column + reader rehydration | `wt/r-prep-dsr-outcome-json-snapshot` (wave-19) | CLOSED via wave-19 commit `4b9f10d` |
| 5 | CLI `verify-ndjson` HTTP-aware variant | `wt/r-prep-cli-verify-ndjson-http` (wave-19) | CLOSED via wave-19 commit `7ec5435` |
| 6 | techlead skill v2.0.0 → v2.1.0 (AP-11 feature-flag matrix hardening) | `wt/r-prep-techlead-skill-v2-1` (wave-19) | CLOSED via wave-19 commit `ba3ef2d` |
| 7 | GA cutover runbook RB-GA-CUTOVER end-to-end | `wt/r-prep-ga-cutover-runbook` (wave-19) | CLOSED via wave-19 commit `dc39000` |
| 8 | Pre-GA external pentest scope doc + engagement checklist | `wt/r-prep-pentest-scope-doc` (wave-19) | CLOSED via wave-19 commit `5db72eb` |
| 9 | Adversarial review of wave-18 P0 streams (audit-export + Neon shadow) | `wt/r-prep-wave18-codex-review` (wave-19) | CLOSED via wave-19 commit `2a25a14` |
| 10 | **Wave-20 INV registry promotion + DEBT register sweep + closure audit** (this stream) | `wt/r-prep-inv-registry-wave20-sweep` (wave-20) | CLOSED via this commit |

Streams 1–9 are wave-19 closures retroactively rolled up under the wave-20 hygiene umbrella for tracking. Stream #10 is the present sweep.

---

## 2. Wave-18 codex P1/P2/P3 closure summary

Cross-ref streams #1, #2, #5 from §1.

### 2.1 Stream A — Audit-export streaming + mid-stream trailer (wave-19 codex review)

Per `2026-05-16-wave18-adversarial-review-streamA-audit-export.md` (wave-19 builder pass). The wave-18 SEAL caveat #4 surfaced 5 P1 + 5 P2 + 1 P3 findings on the audit-export route. Wave-19 closures:

- **A-P1-01 (server-side memory profile)** — addressed by `wt/r-prep-audit-export-async-pages` (commit `f6f7c12`): `R2ListPager` async generator at the route boundary; per-request row materialisation now demand-driven; proptest @ 10k iterations validates page-by-page emission.
- **A-P1-04 (`ExportAuditRow.exit_status` string-prefix protocol regression)** — addressed by `wt/r-prep-audit-export-payload-column` (commit `dc8a6bb`): additive `payload_json: Option<String>` field on `ExportAuditRow`; `exit_status="verify_failed"` kept clean; serde back-compat preserved; CHANGELOG entry pinning wire shape.
- **A-P1-02 / A-P1-03 (audit-emit Err discarded on cross-tenant arm + mid-stream chain-break arm)** — REMAINS OPEN (wave-21 candidate; see §7). The current code still uses `let _ = sink.emit(...)` on these two paths; recommendation is to branch on the Err and surface 503 (cross-tenant arm) or short-circuit the stream (chain-break arm).
- **A-P1-05 (no proptest on audit-emit-BEFORE-trailer ordering)** — addressed by `wt/r-prep-audit-export-async-pages` (commit `f6f7c12`): proptest block over (row_count, tamper_index) asserts ordering invariant.
- **A-P2-01..A-P2-05** — flagged as follow-on; not closed wave-19 (low-severity, no GA-blocker classification).

### 2.2 Stream B — Neon analytics shadow real driver (wave-19 codex review)

Per `2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md`. The wave-18 SEAL caveat #3 surfaced 5 P1 findings. Wave-19 closures:

- **B-P1-02..B-P1-05 (real driver: `RealNeonShadowSink`, `NeonProjectResolver`/`EnvVarResolver`, `NeonExecutor` trait + `tokio-postgres` adapter, daily reconciliation cron, PD alert rules, runbook `RB-NEON-SHADOW-LAG.md`)** — addressed by `wt/r-prep-neon-shadow-real-driver` (commit `ec6e83b`); per `2026-05-16-neon-shadow-real-driver.md` §2.
- **B-P1-01 (RLS `USING`-only — missing `WITH CHECK`)** — **REMAINS OPEN** (wave-21 candidate; see §7). Recommendation was to ship `wt/r-prep-neon-shadow-rls-with-check-and-emit-discipline` but no such branch landed. The current `migrations/neon/0001_audit_events_shadow.sql:83-88` still uses `USING (...)` only. Defense-in-depth gap vs `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` CRITICAL.

### 2.3 Stream #5 — CLI verify-ndjson HTTP-aware variant

`wt/r-prep-cli-verify-ndjson-http` (commit `7ec5435`) addresses the wave-18 customer-CLI path: the verify-ndjson tool now handles streamed HTTP responses (chunked transfer-encoding) and emits the same canonical exit codes as the file-input variant. Integration test added; SOTA bar pass.

---

## 3. Wave-19 caveat closures — pin-by-pin

Per §1 + the wave-19 commit log (`git log --oneline 2eec064 ~10..2eec064`):

| Caveat | Closure commit | Verified by |
|---|---|---|
| Stripe `corelink-stripe-real` wasm32 compile gate (wave-19) | `1991cca` ("wave-19: lift `corelink-stripe-real` crate-root wasm32 gate (R-PREP)") | `cargo check --target wasm32-unknown-unknown -p corelink-stripe-real` green; per-module gate on `client.rs` (depends on `reqwest::blocking`) preserved. Audit doc `2026-05-16-stripe-wasm32-gate-lift.md` SEALED. |
| TokioPostgresExecutor / RealNeonShadowSink driver (wave-18 caveat #3) | `ec6e83b` ("wave-19 R-prep — ship RealNeonShadowSink driver closing wave-18 caveat #3") | `crates/corelink-audit-chain/src/neon_shadow/real.rs` lands the production sink; `NeonExecutor` trait + `tokio-postgres` adapter binder at boot path; daily reconcile cron `.github/workflows/neon-shadow-reconcile-daily.yml`; runbook `RB-NEON-SHADOW-LAG.md`. Audit doc `2026-05-16-neon-shadow-real-driver.md` §2 enumerates deliverables. |
| Audit-export `ExportAuditRow.payload` column (wave-18 caveat #4) | `dc8a6bb` ("wave-19: lift `ExportAuditRow` payload column (closes wave-18 caveat #4)") | Additive `payload_json: Option<String>` field on `ExportAuditRow`; `exit_status="verify_failed"` kept clean; serde back-compat preserved. |
| Audit-export async page-by-page generator (post-wave-18 P0 close on A-P1-01) | `f6f7c12` ("wave-19(audit-export): async page-by-page generator via R2ListPager + proptest @ 10k iter") | `R2ListPager` async generator at route boundary; per-request row materialisation demand-driven; proptest @ 10k iter validates. |
| DSR `dsr_erasure_log.outcome_json` snapshot column + reader rehydration | `4b9f10d` ("wave-19: dsr_erasure_log.outcome_json snapshot column + reader rehydration") + renumber `b1d8d48` | Per-row outcome snapshot for re-derivation post-WI consumption; reader rehydrates from the snapshot column. |
| **RLS WITH CHECK (wave-18 streamB B-P1-01)** | **NOT CLOSED** | The recommended `wt/r-prep-neon-shadow-rls-with-check-and-emit-discipline` stream did not land in wave-19. `migrations/neon/0001_audit_events_shadow.sql:83-88` still uses `USING (...)` only. **Flagged as wave-21 candidate stream (see §7).** |

---

## 4. GA-readiness state post wave-20

What's left blocking GA after this wave (user-bound items dominate):

### 4.1 User-bound items (out of agent reach — must be triggered by Gustavo or external counterparties)

| Item | Status | Blocker |
|---|---|---|
| **LFPDPPP MX attorney sign-off** | Pending | DSR S-11 cross-border (Mexico LFPDPPP) requires a Mexican attorney to sign off on the residency + retention policy. No agent can execute. |
| **FW-H-* (Final-Approver / Final-Witness / Hold-Approver) role nominations** | Pending | PRR dual-hat row decompositions across S-06 / S-09 / S-13 require named operators per role. Gustavo to onboard / nominate. |
| **External pentest engagement** | Scoped | `2026-05-16-pre-ga-pentest-scope.md` SEALED (wave-19 commit `5db72eb`); engagement-checklist drafted; vendor + statement-of-work pending. |
| **Pilot signups (≥ 3 design-partners)** | Pending | Onboarding flow ready (S-19 SEALED); pilot agreements + DPA signing pending external counterparty action. |
| **AWS Artifact PDF download (DEBT-003 closure)** | Pending | Human downloads AWS Artifact SOC 2 + FIPS attestation PDF and runs `sha256sum` to fill the `TBD-on-receipt` row in `BYOK-FIPS-ATTESTATION-MATRIX.md`. |
| **Statuspage `status.corelink.humangr.com` go-live (DEBT-016)** | Pending | Operator follows `STATUSPAGE-INIT.md` provisioning playbook T-7d pre-launch. |

### 4.2 Agent-closable but still OPEN (wave-21 candidates)

| Item | Severity | Notes |
|---|---|---|
| Wave-18 streamB B-P1-01 — RLS `WITH CHECK` on Neon shadow migrations | P1 (defense-in-depth) | Recommended branch `wt/r-prep-neon-shadow-rls-with-check-and-emit-discipline`; estimated ~120 LOC + 1 additive migration + 2 proptest blocks. |
| Wave-18 streamA A-P1-02 — `audit_export.rs:423` cross-tenant arm Err discarded | P1 | Branch on emit result; surface 503 on Err (mirrors row-emit arm). |
| Wave-18 streamA A-P1-03 — `audit_export.rs:799-808` mid-stream chain-break emit Err discarded | P1 | Drop trailer frame on emit Err OR short-circuit stream with body-truncate. |
| DEBT-014 FT-6 / FT-7 / FT-8 / FT-9 (4 of 9 TLA+ followups still OPEN) | P2 (HIGH-severity refinements) | FT-7 (`byok_dek_race`) CRITICAL-upgrade prioritised; FT-6/FT-8/FT-9 HIGH-severity. |
| DEBT-008 mutation full sweep (2/3 remaining: `corelink-pat` + `corelink-clerk`) | P1 | CI nightly run will land empirical baselines; if either < 75 % floor, dispatch fix agent. |
| DEBT-010 CI optimization P2/P3 (7 of 11 still OPEN) | P2/P3 (post-GA) | Concurrency cancel, shared rust-cache key, TLC matrix, paths-filter audit (P2); top-level permissions, sparse-checkout, fuzz merge (P3). |
| DEBT-013 perf optimization OPT-03(b) / OPT-04 phase 2 / OPT-08 | P2 (post-GA) | Explicitly deferred per `perf-optimization-followup-tickets.md`; not GA blockers. |
| DEBT-015 apps/docs Node 22 ESM blockers (a)+(b) | P2 | Stub or remove `draft: true` from referenced security/residency pages; normalize MDX cross-links to extensionless form. |

---

## 5. INV registry state (post-sweep)

Per `python3 scripts/validate_canonical_consistency.py` against this branch (post-sweep; no changes to registry §3 in this wave — registry was already at 0 orphan post-DEBT-004 closure):

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

Per `python3 scripts/validate_inv_promotion.py`: registry coverage 143/143 (all WI-declared INVs present in registry §3).

**INV-DRAFT → INV-PROMOTED count this wave:** **0** (registry already at full coverage post-DEBT-004 closure on 2026-05-15; no orphan refs in code surfaced by wave-19 commits that needed promotion).

---

## 6. DEBT register state (post-sweep)

Per the reconciliation in `specs/_audits/2026-05-15-debt-register.md` v1.2.0 changelog entry (this wave):

| Class | Pre-sweep count | Post-sweep count | Delta |
|---|---|---|---|
| **OPEN** | 9 (with 8 duplicate-rows shadowed by closures + 1 legacy from §3) | **9 (canonical OPEN, no duplicate drift)** | -8 duplicates flipped to CLOSED |
| **CLOSED** | 16 (across §4.1 closures table + inline strikethrough rows) | **24** (= 16 + 8 duplicates reconciled) | +8 |
| **DEFERRED (explicit waiver-style)** | 0 in §5 Waivers table | 0 | 0 |

### 6.1 Per-priority breakdown (canonical OPEN rows post-sweep)

| Priority | Open IDs | Notes |
|---|---|---|
| **P0** | DEBT-003 | Single OPEN; AWS Artifact PDF download (user-bound). |
| **P1** | DEBT-008 (partial 1/3) · DEBT-010 (partial 4/11) · DEBT-013 (partial 6/10 with explicit deferrals) · DEBT-014 (partial 5/9 with FT-6/7/8/9 OPEN) | 4 partials; DEBT-008 remaining 2/3 awaiting CI nightly; DEBT-010 remaining 7/11 post-GA; DEBT-013 remaining 4 explicit deferrals; DEBT-014 remaining 4 of 9 TLA+ followups (FT-7 CRITICAL upgrade prioritised). |
| **P2** | DEBT-015 · DEBT-016 | DEBT-015 apps/docs Node 22 ESM (blockers a+b); DEBT-016 statuspage provisioning (user-bound). |

**Total truly-OPEN canonical rows post-sweep:** 7 (1 P0 + 4 P1 partials + 2 P2). No duplicate-row drift.

---

## 7. Wave-21 candidate streams + caveats

Recommended wave-21 dispatch list (ordered by GA-blocker severity):

| # | Stream | Rationale | Estimated cost |
|---|---|---|---|
| 1 | **Wave-18 streamB B-P1-01 — RLS `WITH CHECK` on Neon shadow migrations** | Defense-in-depth gap vs `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` CRITICAL; single wiring bug in `RealNeonShadowSink` would silently leak cross-tenant INSERTs. | ~120 LOC + 1 additive migration + 2 proptest blocks. |
| 2 | **Wave-18 streamA A-P1-02 + A-P1-03 — audit_export emit-Err discipline (cross-tenant + mid-stream chain-break arms)** | Surface 503 on cross-tenant Err (currently silent); drop trailer frame OR short-circuit stream on chain-break Err (currently violates `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`). | ~40 LOC + 2 integration test additions. |
| 3 | **DEBT-014 FT-7 (`byok_dek_race` TLA+ — CRITICAL severity upgrade)** | CRITICAL invariant currently without TLA+ proof; was a HIGH-severity followup but escalated by CTRL-CRYPTO review. | 1 new TLA+ spec + CI config + nightly cfg. |
| 4 | **DEBT-014 FT-6 / FT-8 / FT-9 (remaining TLA+ followups: multipart_finalize, runbook CI exposure, signup re-signup idempotency)** | HIGH-severity TLA+ coverage gaps; FT-8 has CI-surfacing component. | 3 new TLA+ specs (or 2 + 1 CI workflow). |
| 5 | **DEBT-008 mutation full sweep — `corelink-pat` + `corelink-clerk`** | First CI nightly will land empirical baselines; if either < 75 % kill-rate floor, dispatch fix agent. | Wait-and-watch for first nightly cron (`23 5 * * *`); fix-agent on regression. |
| 6 | **DEBT-015 apps/docs Node 22 ESM blockers (a)+(b)** | Stub/remove `draft: true` from referenced security/residency pages; normalize MDX cross-links to extensionless form; then re-run build on Node 22. | ~60 LOC across `apps/docs/` + dependency-graph hygiene. |
| 7 | **Pentest engagement kickoff (DEBT-pentest)** | Scope SEALED wave-19; vendor selection + statement-of-work + kickoff. User-bound, but agent can draft RFP + vendor checklist if dispatched. | RFP draft + vendor shortlist + SOW template. |

### 7.1 Wave-21 entry caveats

- **Codex Opus is the bound resource.** Synchronous Bash + bounded depth — same charter as wave-19/20. No worktree-spawning subagents.
- **DCO + Co-Authored-By preserved** on every commit.
- **No --no-verify hooks.** Pre-commit failures must surface root-cause.
- **40-min time budget per stream** (matches wave-20 cadence); 4-6 streams per wave realistic.
- **Wave-21 SEAL gate:** all P1-classified streams (#1, #2, #3) must close before wave-22 unblocks any P2-classified streams.

---

## 8. Quality gates verified

Per the wave-20 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present. |
| Canonical consistency validator | `python3 scripts/validate_canonical_consistency.py` | exit 0 — 191 INVs declared, 0 orphan refs, 0 CRITICAL without TLA+. |
| Spec corpus validator | `python3 scripts/validate_specs.py` | (rerun in CI; this audit doc lives in `_audits/` which is in `SKIP_ALL`). |
| Reference validator | `python3 scripts/validate_references.py` | (rerun in CI). |

---

## 9. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave20-sweep`
- **Base commit:** `2eec064`
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-20 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 10. Cross-references

- `specs/_audits/2026-05-15-debt-register.md` v1.2.0 (this sweep's changelog entry; duplicate-row reconciliation rationale).
- `specs/03_architecture/invariant_registry.md` (unchanged this wave; full coverage maintained).
- `specs/_audits/2026-05-15-canonical-consistency-baseline.md` (the CI ratchet floor; DEBT-004 closure log §3.1).
- Wave-19 audit docs: `2026-05-16-stripe-wasm32-gate-lift.md` (stream #3); `2026-05-16-neon-shadow-real-driver.md` (stream #2); `2026-05-16-wave18-adversarial-review-streamA-audit-export.md` (stream #1 source); `2026-05-16-wave18-adversarial-review-streamB-neon-shadow.md` (stream #2 source).
- `RB-GA-CUTOVER.md` (wave-19 cutover runbook; greenlight dashboard).
- `specs/_audits/2026-05-16-pre-ga-pentest-scope.md` (wave-19 pentest scope; engagement checklist).
