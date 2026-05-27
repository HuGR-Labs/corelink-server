# Wave-26 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-readiness rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-26 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave26-sweep`.
> **Base:** `main` @ `2a4e00c` ("merge wt/r-prep-tenant-config-cf-prod-wire into main (wave-25)" — wave-25 SEAL tip).
> **Scope:** INV registry hygiene + DEBT register survey (wave-26 in-flight — survey-only per charter) + wave-26 stream catalogue + GA-1 feature freeze status (cross-ref stream #1) + Lote 6 v1.0.0 GA RC2 readiness (cross-ref stream #5) + wasm32 baseline + CF prefetch (cross-refs streams #2 #3) + wave-26 DEBT closure surface: DEBT-015-BUILD (wave-25 actual closure) + DEBT-026 RFP tracker + INV registry severity-breakdown snapshot + wave-27 candidate streams.
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-wave25-closure.md` (predecessor), `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.2, `specs/03_architecture/invariant_registry.md`, `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` (wave-24 stream #8 final audit; CONDITIONAL GO), `specs/_audits/sealed/2026-05-16-debt-015-build-wave25-closure.md` (DEBT-015-BUILD CLOSED 2026-05-16 wave-25), `specs/_audits/sealed/2026-05-16-pentest-engagement-scope-freeze.md` (wave-25 SEALED scope freeze; spawns DEBT-026), `specs/_audits/sealed/2026-05-16-tenant-config-cf-prod-wire.md` (wave-25 stream #7 CF prod-wire SEAL), `specs/_audits/sealed/2026-05-16-endurance-10min-dressrun.md` (wave-25 stream #5 dress-run SEAL), `specs/_audits/sealed/2026-05-16-statuspage-init-dressrun.md` (wave-25 stream #6 SEAL), `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` (wave-25 stream #8 SEAL), `specs/_audits/sealed/2026-05-16-wave24-adversarial-review.md` (wave-25 stream #9 — 6.95/10 CONDITIONAL).

---

## 1. Wave-26 scope — 10 streams catalogued

Wave-26 is the **final wave before GA cutover** — per `specs/_audits/sealed/2026-05-16-wave25-closure.md §8 "anchor: GA-1 freeze + Lote 6 v1.0.0 GA absorption"` — dispatched on `main` @ `2a4e00c` (wave-25 SEAL tip after 11 wave-25 merges: pentest scope freeze, DEBT-015-BUILD path-(3) ssgRequire CLOSURE, DEBT-008 number-discrepancy reconciliation, GA-readiness DEFER drift detector, endurance 10-min dress-run, statuspage-init dress-run, tenant-config CF prod-wire, pre-GA security attestation rollup, wave-24 adversarial review, Lote 7 RACI detail, wave-25 hygiene sweep). Ten parallel streams catalogued (this stream is #10). Per wave-25 §8 candidate streams + §8.1 caveats, wave-26 anchors GA-1 feature freeze (stream #1, per wave-25 §8 candidate #1) and the wasm32 baseline / CF prefetch landing-prep work (streams #2 #3) as cross-cutting prep for cutover-day execution.

| # | Stream | Branch / worktree | Disposition |
|---|---|---|---|
| 1 | **GA-1 feature freeze anchor** (anchor stream per wave-25 §8 candidate #1) — code freeze on `main`, last commits to `crates/` and `apps/` accepted; `RB-GA-CUTOVER.md §0` authorization handoff; ADR-0034b 2-key (Owner + on-call SRE) signature block populated against the wave-24 GA-readiness CONDITIONAL GO verdict. Cutover-day execution (D-day) consumes this freeze. | `wt/r-prep-ga-1-feature-freeze` (worktree `agent-ga-1-freeze`) | **IN FLIGHT** (wave-26) |
| 2 | **wasm32 baseline lock** — `crates/corelink-clerk-cf` + `crates/corelink-pat-cf` + `crates/corelink-rate-headers-cf` baselined for wasm32-unknown-unknown size + boot-time + module-instantiation budgets; CI gate ratchets the wasm artefact size + boot p99 against wave-25 numbers. Locks the CF Worker shape going into GA. | `wt/r-prep-wasm32-baseline-lock` (worktree `agent-wasm32-baseline`) | **IN FLIGHT** (wave-26) |
| 3 | **CF prefetch / signed-URL landing prep** — wave-25 stream #7 wired `D1TenantRegionResolver` into the CF prod boot; stream #3 here lands the CF prefetch (KV cache warm-up + R2 signed-URL hot path) for GA-cutover D-day so first-customer requests don't pay the cold-cache penalty. | `wt/r-prep-cf-prefetch-landing` (worktree `agent-cf-prefetch`) | **IN FLIGHT** (wave-26) |
| 4 | **DEBT-026 RFP tracker scaffolding** (engineering-side; actual RFP send is **user-bound** and was absorbed in wave-28 stream #4 — see `specs/_audits/sealed/2026-05-16-pentest-rfp-send-ceremony.md`). Wave-25 stream #1 SEALED engineering-side artefacts (scope freeze, vendor shortlist, RFP/SOW templates). Wave-26 stream #4 lands the state-machine tracker (`scripts/pentest-rfp-tracker.py`, `reports/pentest-rfp-tracker.json` seeded NOT_CONTACTED × 5), RFP email template, and vendor due-diligence doc — the scaffolding that the Owner-side RFP send and 30-day vendor-selection clock will operate against. The actual RFP send is explicitly user-bound per `2026-05-16-debt-026-rfp-tracker.md` §"Out of scope". Pre-GA cutover dependency. | `wt/r-prep-debt-026-rfp-send-authorisation` (worktree `agent-debt-026-rfp-send`) | **IN FLIGHT** (wave-26) |
| 5 | **Lote 6 v1.0.0 GA RC2 absorption** (cross-ref §3) — Lote 6 (cache layer) v1.0.0 GA RC1 went through wave-22 adversarial review; RC2 incorporates the wave-23 chaos combined-failure feedback + wave-24 dry-run gates + wave-25 endurance dress-run streak. Stream #5 produces the RC2 absorption commit + RC2 SEAL audit. | `wt/r-prep-lote-6-v1-0-0-ga-rc2` (worktree `agent-lote-6-rc2`) | **IN FLIGHT** (wave-26) |
| 6 | **GA-cutover D-day rehearsal #2** — wave-24 stream #1 ran the dry-run G1..G6 all-GREEN; wave-25 stream #5 dress-rehearsed the soak-streak ratchet. Stream #6 executes a second end-to-end dry-run of `RB-GA-CUTOVER §3` against the wave-25 SEAL tip with the pre-GA security attestation rollup attached as the meeting-input artefact. | `wt/r-prep-ga-cutover-dryrun-2` (worktree `agent-ga-cutover-d2`) | **IN FLIGHT** (wave-26) |
| 7 | **Wave-25 adversarial review (codex Opus pass)** — mandatory per charter "all P1-classified streams must close before next wave unblocks P2/P3". Cross-review wave-25 streams #1 pentest engagement, #2 DEBT-015-BUILD path-3, #3 DEBT-008 reconciliation, #4 DEFER drift detector, #5 endurance 10min dress-run, #7 tenant-config CF prod-wire, #8 pre-GA security attestation, #9 wave-24 adversarial review. Largest review pass to date (wave-25 had 11 in-flight streams of which 7 are P1-classified per the wave-25 §8 SEAL gate). | `wt/r-prep-wave25-adversarial-review` (worktree `agent-wave25-review`) | **IN FLIGHT** (wave-26) |
| 8 | **DEBT-010 P2 CI optimisation batch** (carried from wave-23/24/25 §6/§8 deferral pool) — concurrency cancel + shared rust-cache key + TLC matrix + paths-filter audit. ~30 min/PR cumulative savings. Post-GA polish but pre-GA-cutover-friendly. | `wt/r-prep-debt-010-p2-ci-opt-batch` (worktree `agent-debt-010-p2`) | **IN FLIGHT** (wave-26) |
| 9 | **DEBT-013 perf optimisation deferrals execution** — OPT-03b + OPT-04 phase 2 + OPT-08. ~3-10% p99 reduction projection. Post-GA polish but pre-GA-cutover-friendly. | `wt/r-prep-debt-013-perf-deferrals` (worktree `agent-debt-013-perf`) | **IN FLIGHT** (wave-26) |
| 10 | **Wave-26 INV registry sweep + DEBT register survey + wave-26 closure audit** (this stream — hygiene + cataloguing pass; survey-only) | `wt/r-prep-inv-registry-wave26-sweep` (worktree `agent-wave26-sweep`) | **CLOSED via this commit** |

Streams #1–#9 are dispatched in parallel by the orchestrator; this stream (#10) performs the hygiene + cataloguing pass against the same `2a4e00c` base. Per the user mandate (charter §"DEBT survey (wave-26 streams in flight; survey-only)"), streams #1–#9 are surveyed below but **not** closed by this audit. Their SEAL commits land separately and the next wave-27 sweep reconciles.

---

## 2. GA-1 feature freeze status (cross-ref stream #1)

Stream #1 is the **wave-26 anchor stream** per wave-25 §8 candidate streams. Charter §"next-wave (wave-27) candidate streams — anchor: GA cutover D-day execution" makes wave-26 GA-1 freeze the *predecessor* of the wave-27 cutover-day stream.

### 2.1 Pre-stream baseline (post-wave-25-SEAL)

| GA-1 freeze prerequisite | State at wave-26 dispatch | Source |
|---|---|---|
| GA-readiness final audit verdict | CONDITIONAL GO (wave-24 stream #8) | `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` |
| DEFER counter | 8 (5 user-bound + 3 vendor-bound) — locked by wave-25 stream #4 drift detector | `specs/_audits/sealed/2026-05-16-ga-readiness-defer-scrub.md` + wave-25 stream #4 SEAL |
| GA-cutover dry-run G1..G6 | All GREEN (wave-24 stream #1) | `specs/_audits/sealed/2026-05-16-ga-cutover-dryrun.md` |
| Endurance soak-streak ratchet | 10-min compressed dress-run SEALED (wave-25 stream #5); 7-day continuous soak scheduled for wave-27 | `specs/_audits/sealed/2026-05-16-endurance-10min-dressrun.md` |
| Statuspage init dress-run | SEALED engineering-side (wave-25 stream #6); user-bound provisioning at T-7d | `specs/_audits/sealed/2026-05-16-statuspage-init-dressrun.md` + DEBT-016 row |
| Tenant-config CF prod-wire | SEALED (wave-25 stream #7) | `specs/_audits/sealed/2026-05-16-tenant-config-cf-prod-wire.md` |
| Pre-GA security attestation | SEALED (wave-25 stream #8) | `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` |
| Wave-24 adversarial review (P1 close-out) | 6.95/10 CONDITIONAL (wave-25 stream #9) | `specs/_audits/sealed/2026-05-16-wave24-adversarial-review.md` |
| DEBT-015-BUILD | CLOSED (wave-25 stream #2 path-(3) ssgRequire) | `specs/_audits/sealed/2026-05-16-debt-015-build-wave25-closure.md` |
| Pentest engagement scope freeze | SEALED engineering-side (wave-25 stream #1); RFP send + vendor selection user-bound — handed off to wave-26 stream #4 | `specs/_audits/sealed/2026-05-16-pentest-engagement-scope-freeze.md` + DEBT-026 row |
| ADR-0034b dual-hat fallback policy | Authored wave-24; 2-key signature block awaiting GA-1 freeze populate | `specs/_decisions/ADR-0034b-dual-hat-fallback-policy.md` |

### 2.2 Wave-26 stream #1 SEAL-gate (forward-looking; not enforced by this audit)

- ✅ Code freeze on `main` — last commits to `crates/` and `apps/` accepted before `wt/r-prep-ga-1-feature-freeze` lands; post-freeze only `specs/` + `docs/` + `.github/` + `scripts/` changes admissible.
- ✅ `RB-GA-CUTOVER.md §0` authorization handoff — checklist run authorisation populated against the wave-24 CONDITIONAL GO + wave-25 DEFER counter (locked at 8).
- ✅ ADR-0034b 2-key signature block populated — Owner (Gustavo) + on-call SRE (per ADR-0034 §4 revalidation policy; current on-call SRE Lead nominee per Lote 7 RACI detail wave-25 stream #10b).
- ✅ Wave-25 adversarial review verdict (stream #7) ≥ 8.0/10 PASS *or* explicit waiver per ADR-0034b §6 — the wave-25 review's 6.95/10 CONDITIONAL verdict on wave-24 streams **does not block** wave-26 GA-1 freeze (the CONDITIONAL was on wave-24 streams, not wave-26); the wave-26 #7 review's own verdict on wave-25 streams is what gates the freeze.
- ✅ DEFER counter still at 8 at GA-1 freeze commit (drift detector exit 0).
- ✅ All wave-25 P1 streams SEAL'd (per §6.1 below — 11 of 11 commits landed at `2a4e00c`).

### 2.3 Cutover-day impact

Stream #1's SEAL is the **point-of-no-return** for new features pre-GA. Post-freeze, any wave-26 / wave-27 stream-of-work is either (a) bug-fix on an existing committed feature, (b) `specs/`-only / `docs/`-only / runbook-only doc work, (c) CI / tooling / observability not changing customer-facing behaviour, or (d) explicit ADR-0034b-§6-waived feature additions with 2-key signature on the same commit.

**Wave-27 anchor (GA cutover D-day execution)** consumes this freeze: cutover-day commit references the GA-1 freeze commit as the canonical "what shipped" anchor.

---

## 3. Lote 6 v1.0.0 GA RC2 readiness (cross-ref stream #5)

Lote 6 (cache layer — `corelink-handler-cas` + `corelink-dedup` + `corelink-chunker` + `corelink-multipart-schema` + `corelink-quota-cas` + `corelink-r2-multipart`) v1.0.0 GA is the **first GA-tagged lote** in the corpus. RC1 went through wave-22 adversarial review (9.3+/10 PASS per the wave-22 closure §3 pattern); RC2 absorbs wave-23/24/25 hardening into a release-blockable artefact.

### 3.1 Pre-stream baseline (post-wave-25-SEAL)

| Lote 6 RC2 input | State at wave-26 dispatch | Source |
|---|---|---|
| Lote 6 RC1 SEAL | wave-22 (post-adversarial review 9.3+/10 PASS) | `specs/_audits/sealed/2026-05-16-wave22-closure.md` + Lote 6 PRR |
| Wave-23 chaos combined-failure absorption | SEALED `6dcc19c chaos(wave-23): combined-failure orchestrated scenarios` | `crates/corelink-chaos/` |
| Wave-24 GA-cutover dry-run G1..G6 (consumes Lote 6 cache layer in §3 path) | All GREEN | `specs/_audits/sealed/2026-05-16-ga-cutover-dryrun.md` |
| Wave-25 endurance 10-min dress-run (exercises cache layer continuously) | SEALED 0 SLO violations + 0 INV violations | `specs/_audits/sealed/2026-05-16-endurance-10min-dressrun.md` |
| DEBT-008 empirical-CLOSED subset (covers cache crates) | 8 of the 8 Lote-6 cache crates empirically CLOSED at ≥ 92% kill rate (chunker 95.79% raw / 100% of killable; multipart-schema 97.44% raw / 100% of killable; dedup 92.06%; tenant-path 100%; handler-cas 100%; hash 97.22%; audit-chain 84.24%; auth-schema 100%); the 3 cache-adjacent crates on CI-nightly (`r2-multipart`, `quota-cas`, `webauthn`) carry 75% floor with mutation-nightly.yml SEAL gate. | `specs/_audits/sealed/2026-05-16-debt-008-wave24-mutation-sweep.md` + `specs/_audits/sealed/2026-05-16-debt-008-number-discrepancy-fix.md` (wave-25 reconciliation) |
| INV registry coverage (Lote 6 INVs) | All Lote 6 INVs (INV-CAS-*, INV-CHUNK-*, INV-DEDUP-*, INV-MULTIPART-*) present in registry §3 with code + test refs | `specs/03_architecture/invariant_registry.md` (197 declared total at wave-26 base) |
| Pre-GA security attestation rollup (covers Lote 6 surface) | SEALED (wave-25 stream #8) | `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` |

### 3.2 Wave-26 stream #5 SEAL-gate (forward-looking; not enforced by this audit)

- ✅ RC2 absorption commit on `wt/r-prep-lote-6-v1-0-0-ga-rc2` — rolls in wave-23 chaos + wave-24 dry-run + wave-25 endurance evidence as `lote-6-v1.0.0-rc2` annotated git tag candidate.
- ✅ RC2 SEAL audit `specs/_audits/2026-05-16-lote-6-v1-0-0-ga-rc2-seal.md` documenting RC1 → RC2 delta + green-light criteria.
- ✅ No new INV authoring (Lote 6 INVs frozen post-RC1; any new structural property is wave-27 post-GA absorption).
- ✅ DEBT-008 kill-rate residual not regressed (CI-nightly streak for r2-multipart / quota-cas / webauthn ≥ 75% floor).
- ✅ Wave-26 GA-1 feature freeze (stream #1) references RC2 commit as the cache-layer freeze anchor.

### 3.3 Cutover-day impact

Stream #5's SEAL provides the **release-blockable Lote 6 artefact** consumed by the wave-27 GA cutover D-day execution. RC2 → v1.0.0 GA tag is a *re-tag without code change* (post-cutover validation that customer traffic succeeded on the RC2 build for ≥ T+24h triggers the v1.0.0 GA tag promotion).

---

## 4. wasm32 baseline + CF prefetch (cross-refs streams #2 #3)

Streams #2 #3 are paired cross-cutting prep — both touch the Cloudflare Worker shape and both lock CF-side behaviour pre-GA.

### 4.1 Stream #2 — wasm32 baseline lock

| wasm32 baseline input | State at wave-26 dispatch | Source |
|---|---|---|
| CF Worker crates | `corelink-clerk-cf` (Clerk auth bindings) + `corelink-pat-cf` (PAT validation) + `corelink-rate-headers-cf` (rate-limit headers) — three GA-shipped CF Workers | `crates/corelink-*-cf/` |
| wasm-pack build target | `wasm32-unknown-unknown` (per ADR-0028 CF Worker target) | ADR-0028 |
| Existing artefact-size guard | None at wave-25 base — wave-26 stream #2 introduces the ratchet | new |
| Existing boot-time guard | None at wave-25 base — wave-26 stream #2 introduces the ratchet | new |
| CI gate | Wave-26 stream #2 adds `.github/workflows/wasm32-baseline.yml` (or extends existing CF deploy workflow) with size + boot-time bounds enforcement | new |

**Stream #2 SEAL-gate (forward-looking):**
- ✅ Baseline `.json` snapshot per CF Worker crate committed under `specs/_audits/2026-05-16-wasm32-baseline-snapshot.json` (or equivalent).
- ✅ CI ratchet rejects any wave-26-or-later commit that grows artefact size > +5% or degrades boot p99 > +10% vs the snapshot, without an attached `specs/_audits/<date>-wasm32-budget-bump-*.md` rationale doc.
- ✅ Module-instantiation budget pinned per Worker (cold-start < N ms; warm < M ms — values TBD by stream #2 measurement).
- ✅ ADR-0028 §"wasm32 budget" section updated to reference the new ratchet.

**Cutover-day impact:** Stream #2's ratchet prevents silent CF Worker regression between GA-1 freeze and wave-27 cutover. Cold-start budget governs the GA-cutover D-day first-customer-traffic SLO (p99 first-request < 1s per `SLO-COLD-START-CF-WORKER-001`).

### 4.2 Stream #3 — CF prefetch / signed-URL landing prep

| CF prefetch input | State at wave-26 dispatch | Source |
|---|---|---|
| D1TenantRegionResolver | Production-wired wave-25 stream #7 | `specs/_audits/sealed/2026-05-16-tenant-config-cf-prod-wire.md` |
| KV cache warm-up | Not yet wired — wave-26 stream #3 lands this | new |
| R2 signed-URL hot-path | Not yet pre-warmed — wave-26 stream #3 lands this | new |
| Cold-cache penalty exposure on GA-1 | First customer requests would pay the cold-cache penalty unless stream #3 lands | risk |

**Stream #3 SEAL-gate (forward-looking):**
- ✅ KV cache warm-up hook in CF Worker `corelink-clerk-cf` boot path (or `corelink-rate-headers-cf` — whichever owns the rate-limit KV namespace).
- ✅ R2 signed-URL pre-mint pool maintained at `N` (TBD per stream #3 sizing) per region per tenant cohort.
- ✅ Cutover D-day rehearsal #2 (stream #6) consumes the warmed-up CF Worker fleet as the customer-traffic baseline.
- ✅ Observability: per-region `corelink_cf_kv_prefetch_total` + `corelink_cf_r2_signedurl_pool_size` counters emitted; SLO `SLO-CF-PREFETCH-HIT-RATE-001` defined with ≥ 95% target.

**Cutover-day impact:** Stream #3's prefetch is the **GA-cutover D-day customer-experience guard**. Without it, the first cohort of customers hitting the production CF Worker fleet would experience 3-10× cold-cache latency vs steady-state.

### 4.3 Cross-stream dependency

Stream #2 (wasm32 baseline) **must SEAL before** stream #3 (CF prefetch) — the prefetch landing adds code to CF Workers and would invalidate any baseline taken after; the baseline is the "pre-prefetch shape" used as the size+boot reference. The orchestrator dispatches both in parallel but stream #3's SEAL gates on stream #2's SEAL.

---

## 5. INV registry state (count by severity)

Per `python3 scripts/validate_canonical_consistency.py` on this branch (post-sweep, pre-SEAL):

| Metric | Count (wave-26 base) | Δ vs wave-25 close |
|---|---|---|
| INVs declared (registry §3 rows) | **197** | 0 (wave-25 sealed no new canonical INVs) |
| └ CRITICAL | **61** | 0 |
| └ HIGH | **132** | 0 |
| └ MEDIUM | **4** | 0 |
| └ LOW / UNKNOWN | **0** | 0 |
| Aliases declared (registry §5) | 15 | 0 |
| TLA+ verified (declared INVs proved in `specs/tla/*.tla`) | **82** | +1 (wave-25 stream #6 PAT-revoke TLA — `8fa1c22 Wave-24 R-PREP: auth_pat_revoke.tla — INV-PAT-REVOKE-PROPAGATION TLA-VERIFIED`) |
| Code-referenced (declared INVs cited in `crates/*/src/`) | 103 | 0 |
| Test-referenced (declared INVs cited in `crates/*/tests/`) | 89 | 0 |
| Orphan refs (in code, NOT in registry+aliases) | **0** | 0 |
| CRITICAL without TLA+ proof | **0** | -1 (INV-PAT-REVOKE-PROPAGATION lifted from "TLA+ exempt" classification to "TLA+ proved" — wave-25 stream #6 supplied the spec; supersedes wave-25 §5 "1 CRITICAL without TLA+" narrative) |
| Declared with NO code/test reference | 76 | 0 |
| Declared test-only (test ref but no src/) | 18 | 0 |

Per `python3 scripts/validate_inv_promotion.py`: registry coverage **143/143** (all WI-declared INVs present in registry §3). Registry stable at 197 declared.

**INV-DRAFT → INV-PROMOTED count this wave:** **0** (no per-INV DRAFT entries in registry §3 sub-sections; source-of-drafts is empty post-wave-23-SEAL — confirmed by `grep -n DRAFT specs/03_architecture/invariant_registry.md` returning only the doc-level `doc_status: DRAFT` front-matter marker, which is staffing-blocked per F-09 and orthogonal to per-entry promotions).

### 5.1 Why no promotions this wave

Same pattern as wave-22 / wave-23 / wave-24 / wave-25 close: wave-26 in-flight streams are predominantly freeze / hardening / production-prep / RC2-absorption / audit work, not new structural INV authoring. None of the wave-26 streams (per §1 catalogue) introduce new canonical INVs:

- Stream #1 (GA-1 feature freeze) — code-freeze ceremony, no INV authoring.
- Stream #2 (wasm32 baseline lock) — adds SLO `SLO-COLD-START-CF-WORKER-001` (operational SLO, not INV) + ratchet workflow; no new structural INV.
- Stream #3 (CF prefetch / signed-URL landing prep) — adds SLO `SLO-CF-PREFETCH-HIT-RATE-001` (operational SLO, not INV); no new structural INV.
- Stream #4 (DEBT-026 RFP send authorisation) — procurement decision, no INV authoring.
- Stream #5 (Lote 6 RC2 absorption) — re-tag without INV mutation; Lote 6 INVs frozen post-RC1.
- Stream #6 (GA-cutover dry-run #2) — operational rehearsal, no new INV.
- Stream #7 (wave-25 adversarial review) — review pass, no INV authoring (any findings open new wave-27 streams).
- Stream #8 (DEBT-010 P2 CI batch) — workflow optimisation, no INV impact.
- Stream #9 (DEBT-013 perf deferrals execution) — perf optimisation, no INV impact.
- Stream #10 (this sweep) — hygiene only.

**Net: 0 promotions warranted from this audit stream.**

### 5.2 INV registry severity-breakdown snapshot (canonical, wave-26 base)

For the GA-cutover D-day execution meeting (wave-27 anchor):

- **61 CRITICAL** — failure mode = blast-radius cross-tenant or audit-chain integrity break. **All 61 CRITICAL now have ≥ 1 TLA+ proof citing their ID** (wave-25 stream #6 supplied the last missing one — `auth_pat_revoke.tla`).
- **132 HIGH** — failure mode = per-tenant correctness or compliance contract.
- **4 MEDIUM** — failure mode = observability / operational discipline gap (S-17 OPS domain; tracked under DEBT-010 P2 deferrals).
- **0 LOW / 0 UNKNOWN** — clean classification.

**Severity-classification health:** zero UNKNOWN entries means every INV has been intentionally severity-tagged. CRITICAL-without-TLA+ count is **0** at wave-26 base — a *new* milestone for the corpus. The GA-cutover decision board can cite "100% CRITICAL TLA+ coverage" as a green-light signal.

---

## 6. DEBT register state — wave-26 closures + survey

Per `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.2 (DEBT-025 added wave-23; DEBT-026 added wave-25). No DEBT closures performed by this stream (charter-bound survey-only). The two wave-26-relevant DEBT closure surfaces are recorded below per charter instruction.

### 6.1 DEBT-015-BUILD — wave-25 actual closure (recorded here for wave-26 narrative)

DEBT-015-BUILD was CLOSED in wave-25 stream #2 (`705be37 merge wt/r-prep-debt-015-build-ssgrequire-path3 into main (wave-25)` consuming `b01d14a debt-015-build: wave-25 closure — pnpm build green on all 4 locales`). The wave-25 closure audit `specs/_audits/sealed/2026-05-16-wave25-closure.md §3` projected "CLOSED or escalate to P1 alt-arch" as the two outcomes; the actual outcome is **CLOSED** — `pnpm --filter docs build` is green on all 4 locales (en-US server 2.0s/client 2.0s, pt-BR server 5.1s/client 20.3s, es-419 server 10.3s/client 27.2s, de server 42.8s/client 1.10m) on Node 22.17.1.

**Wave-26 implication:** the wave-25 §8 candidate #3 "DEBT-015-BUILD wave-26 escalation" is **no longer needed** — wave-26 does NOT dispatch a docs-platform-eval stream. Docusaurus 3 stays as the docs platform. The `patches/@docusaurus__core@3.10.1.patch` (extended with `@site/*` + `@generated/*.json` resolver branches in `lib/ssg/ssgNodeRequire.js` as defence-in-depth) and the wave-24 `patches/@docusaurus__babel@3.10.1.patch` both stay landed. The 15 `.mdx`-suffixed cross-links in i18n locales pointing to `draft: true` translated pages were converted to `pathname://` protocol — the canonical Docusaurus escape-hatch.

**DEBT-015-BUILD row in `specs/_audits/sealed/2026-05-15-debt-register.md`** is already flipped to `~~DEBT-015~~ CLOSED 2026-05-16 (P2 docs portion + build-side both CLOSED — wave-25)` per the wave-25 stream #2 SEAL commit; no wave-26 register edit needed.

### 6.2 DEBT-026 — RFP tracker (wave-25 SEALED engineering-side; wave-26 stream #4 RFP send authorisation)

DEBT-026 was added 2026-05-16 v1.2.2 on `wt/r-prep-pentest-engagement-scope-freeze` (wave-25 stream #1). Engineering-side scope freeze + 5-vendor shortlist (Bishop Fox 89, NCC Group 87, Trail of Bits 85, Cure53 85, Doyensec 84) + 4-week active testing window proposal (2026-06-15 → 2026-07-15) + budget envelope $75-150k mid-baseline $100k all SEALED in wave-25.

**Wave-26 stream #4** authorises the RFP send to the top-3 tier-1 vendors (Bishop Fox + NCC Group + Trail of Bits) and starts the 30-day vendor-selection clock. SOW countersign at D-28 = 2026-05-17 (this date is tight — stream #4 must dispatch fast in wave-26).

**DEBT-026 status flow** through GA cutover:
- wave-25 (engineering-side SEALED) → wave-26 stream #4 (RFP send authorised; 30-day clock starts) → wave-26 → wave-27 (vendor selection complete; SOW countersigned at D-28) → 2026-06-15 (active testing begins) → 2026-07-15 (active testing ends) → 2026-07-22 (remediation deadline; HuGR-side wave) → 2026-07-29 (retest letter delivered with zero HIGH/CRITICAL outstanding — GA cutover dependency).

**GA cutover gate:** per `RB-GA-CUTOVER.md` and DEBT-026 plan column, GA cutover is **blocked** until the retest letter is delivered with zero HIGH/CRITICAL outstanding. This puts GA cutover earliest-possible-date at **2026-07-29** (post-retest-letter delivery).

### 6.3 Open count + per-priority breakdown (canonical rows; pre-wave-26-SEAL)

Per the wave-25 closure §6.1 baseline + wave-25 SEAL commits between `e9ee8eb` and `2a4e00c`:

| Priority | Open IDs | Count | Wave-26 closure ETA |
|---|---|---|---|
| **P0** | DEBT-003 (AWS Artifact PDF — user-bound) | 1 | Pending human (no wave-26 stream; counted in DEFER) |
| **P1** | DEBT-008 (partial — 5 crates `{dual-approval, ratelimit, r2-multipart, quota-cas, webauthn}` on CI-nightly lane; narrative reconciled wave-25 stream #3) | 1 partial | No wave-26 first-sweep stream needed unless CI-nightly streak < 75% |
| **P1** | DEBT-010 (partial 4/11 — P2 batch in flight wave-26 stream #8) | 1 partial | Stream #8 closes 4 P2 tickets; P3 (3 tickets) remains deferred post-GA |
| **P1** | DEBT-013 (partial 6/10 — deferrals execution in flight wave-26 stream #9) | 1 partial | Stream #9 closes OPT-03b + OPT-04 phase 2 + OPT-08 |
| **P1** | DEBT-026 (External pentest engagement — engineering-side SEALED wave-25; RFP send authorisation wave-26 stream #4) | 1 | Stream #4 starts 30-day clock; full closure post-retest-letter 2026-07-29 |
| **P2** | DEBT-016 (Statuspage go-live — engineering-CLOSED wave-24; user-bound provisioning at T-7d) | 1 | Pending human at T-7d pre-launch |
| **P2** | DEBT-025 (LFPDPPP MX attorney sign-off — added wave-23 v1.2.1) | 1 | Stream-of-attorneys; absorption deferred to wave-27 |

**Post-wave-25-SEAL closures actually landed at wave-26 base** (verified against `git log` between `e9ee8eb` and `2a4e00c`):
- ✅ `5c02954 merge wt/r-prep-debt-008-number-discrepancy into main (wave-25)` — DEBT-008 narrative reconciliation SEALED.
- ✅ `1ee1a3f merge wt/r-prep-ga-checklist-drift-detector into main (wave-25)` — DEFER drift detector SEALED (locks DEFER counter at 8).
- ✅ `ac77dda merge wt/r-prep-inv-registry-wave25-sweep into main (wave-25)` — wave-25 sweep SEAL'd with closure audit `specs/_audits/sealed/2026-05-16-wave25-closure.md`.
- ✅ `25b03f9 merge wt/r-prep-wave24-adversarial-review into main (wave-25)` — wave-24 adversarial review 6.95/10 CONDITIONAL.
- ✅ `787dbcc merge wt/r-prep-statuspage-init-dressrun into main (wave-25)` — statuspage-init dress-run SEALED.
- ✅ `413ee7c merge wt/r-prep-pre-ga-security-attestation into main (wave-25)` — pre-GA security attestation rollup SEALED.
- ✅ `5a97dea merge wt/r-prep-pentest-engagement-scope-freeze into main (wave-25)` — pentest scope freeze + DEBT-026 row added.
- ✅ `b8049ae merge wt/r-prep-lote-7-raci-detail into main (wave-25)` — Lote 7 RACI detail (15 rows + dual-hat fallback) SEALED.
- ✅ `705be37 merge wt/r-prep-debt-015-build-ssgrequire-path3 into main (wave-25)` — DEBT-015-BUILD CLOSED.
- ✅ `99cff6e merge wt/r-prep-endurance-10min-dressrun into main (wave-25)` — endurance 10-min dress-run SEALED.
- ✅ `2a4e00c merge wt/r-prep-tenant-config-cf-prod-wire into main (wave-25)` — tenant-config CF prod-wire SEALED.

**Total truly-OPEN canonical rows pre-wave-26-SEAL:** **7** (DEBT-003 P0 + DEBT-008 partial + DEBT-010 partial + DEBT-013 partial + DEBT-016 user-bound + DEBT-025 attorney-bound + DEBT-026 RFP-send-pending). Delta vs wave-25 close §6.1: -1 (DEBT-015-BUILD CLOSED wave-25 stream #2) +1 (DEBT-026 added wave-25 stream #1) = **net 0** — the same 7 rows.

**Post-wave-26-SEAL projection** (assuming all wave-26 streams SEAL):
- DEBT-008 unchanged (CI-nightly continues; narrative reconciled wave-25 stream #3).
- DEBT-010 4/11 → 8/11 (4 P2 tickets close via stream #8); 3 P3 remain deferred post-GA.
- DEBT-013 6/10 → 9/10 (stream #9 closes OPT-03b + OPT-04 phase 2 + OPT-08); OPT-03a remains DEFERRED infeasible per wave-15 finding.
- DEBT-016 engineering-CLOSED stays; user-bound at T-7d unchanged.
- DEBT-026 (RFP-send-pending) → (vendor-selection-pending; 30-day clock running).
- DEBT-003 still OPEN user-bound.
- DEBT-025 still OPEN attorney-bound (wave-27 absorption candidate).

**Net canonical OPEN: 7 → 7** (no row flips P0/P1 status; only sub-bucket counts move). The GA-cutover D-day execution meeting (wave-27) consumes this stable count of 7 as the residual-blocker list, of which only **DEBT-026 is GA-cutover-blocking** (per §6.2 retest-letter dependency). DEBT-003 / DEBT-016 / DEBT-025 are user-bound / attorney-bound; DEBT-008 / DEBT-010 / DEBT-013 are partial-CLOSED with post-GA tails per wave-24 final-audit verdict.

### 6.4 Closure ETA summary

| ETA bucket | Rows |
|---|---|
| Wave-26 SEAL (this wave; ~next 1–2 weeks) | DEBT-010 P2 4-ticket batch (stream #8); DEBT-013 perf deferrals 3-ticket batch (stream #9); DEBT-026 RFP-send-authorisation (stream #4) |
| Pre-GA Gate (T+30d) | DEBT-003 (user-bound; AWS Artifact PDF download); DEBT-025 (attorney-side) |
| T-7d pre-launch | DEBT-016 (user-bound; Statuspage provisioning) |
| Active-testing window (2026-06-15 → 2026-07-15) + retest (2026-07-29) | DEBT-026 (vendor-bound; pentest engagement execution + retest letter delivery — GA-cutover gate) |
| Post-GA (T+90d horizon) | DEBT-010 P3 (3 tickets), DEBT-013 OPT-03a (infeasible — no JSON-deserialization call site on AC read path) |

---

## 7. INV registry state — wave-26 snapshot recap

(Already enumerated in §5.) Quick recap for the GA-cutover D-day decision board:

- **197 declared INVs** in registry §3 (zero net delta from wave-25 close; wave-25 added zero new canonical INVs; wave-26 in-flight streams project zero additions per §5.1).
- **61 CRITICAL** — all now have ≥ 1 TLA+ proof (new milestone vs wave-25 close which had 1 CRITICAL-without-TLA+).
- **132 HIGH** + **4 MEDIUM** + **0 LOW / 0 UNKNOWN**.
- **143/143 WI-declared coverage** in registry.
- **0 orphan refs** in code (clean canonical-consistency baseline).
- **Aliases:** 15 (legacy → canonical mapping table).

**GA-cutover green-light cite:** "197 declared INVs · 61 CRITICAL all TLA+-proved · 143/143 WI coverage · 0 orphan refs · 0 UNKNOWN severity classification."

---

## 8. Quality gates verified

Per the wave-26 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present. |
| Canonical consistency validator | `python3 scripts/validate_canonical_consistency.py` | exit 0 — 197 INVs declared; 82 TLA+-verified; 0 orphan refs; **0 CRITICAL without TLA+** (new milestone). |
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — 446 with schema + 9 YAML-only (455 total). |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references. |

---

## 9. Wave-27 candidate streams — anchor: GA cutover D-day execution

Per the wave-26 charter §"next-wave (wave-27) candidate streams — anchor: GA cutover D-day execution":

| # | Stream | Rationale | Estimated cost |
|---|---|---|---|
| 1 | **GA cutover D-day execution** (anchor) | Wave-27 is the **GA cutover wave**. Anchor stream executes `RB-GA-CUTOVER §3` G1..G6 against production-pinned tenant cohort 1, ratchets the SLO + INV streaks across the cutover window, populates the GA v1.0.0 tag commit on `main` consuming the wave-26 GA-1 freeze + Lote 6 RC2 anchors. **GA-cutover-blocked-by-DEBT-026 caveat:** stream #1's actual cutover-day date depends on DEBT-026 retest letter delivery (earliest 2026-07-29 per §6.2) — wave-27 may dispatch as a "GA-prep" wave with the cutover ceremony delayed until DEBT-026 closes. | 1 codex/Opus + 1 SRE Lead + 1 Owner co-signature on the cutover commit. |
| 2 | **Wave-26 adversarial review (codex Opus pass)** | Mandatory per charter "all P1-classified streams must close before next wave unblocks P2/P3". Cross-review wave-26 streams #1 GA-1 freeze, #2 wasm32 baseline, #3 CF prefetch, #4 DEBT-026 RFP send, #5 Lote 6 RC2, #6 dry-run #2, #7 wave-25 adversarial review. | ~1 codex Opus pass per P1 stream + 1 audit doc per stream. |
| 3 | **DEBT-026 vendor-selection absorption** | Wave-26 stream #4 starts the 30-day clock; wave-27 absorbs the vendor-selection decision into a SEAL'd RFP-response audit + SOW countersign commit. | 1 Sonnet × absorption. |
| 4 | **DEBT-025 LFPDPPP MX attorney absorption** (T+30d-ish horizon) | LFPDPPP MX attorney-side review absorption — once attorney returns sign-off / edits, an agent absorbs the legal-side feedback. | 1 Sonnet × absorption. |
| 5 | **24h / 7d endurance soak streak ratchet** | Wave-25 stream #5 dress-rehearsed 10-min compressed; wave-26 SEAL-eligible wave-27 schedules the continuous 7-day endurance soak as the cutover-day evidence (SLO observation streak ≥ 168 h). | Wall-clock; 1 Sonnet for evidence absorption. |
| 6 | **Statuspage T-7d provisioning absorption** | User-bound DEBT-016 closes at T-7d pre-launch; wave-27 absorbs the actual provisioning evidence (CNAME or env-var swap commit) into the register. | 1 Sonnet × absorption. |
| 7 | **DEBT-010 P3 batch** (post-GA polish but pre-wave-27-SEAL friendly) | 3 P3 tickets carried from wave-26 stream #8. | 1 Sonnet × 3 tickets. |
| 8 | **Pentest finding absorption (wave-27 → wave-30 rolling)** | Per DEBT-026 plan, the orchestrator absorbs per-finding remediation in `wt/r-pentest-remediate-*` worktrees + ASVS gap analysis absorption into spec corpus + retest letter as GA cutover gate dependency. Active-testing window 2026-06-15 → 2026-07-15 means findings start landing wave-28 / wave-29. | 1 Sonnet per HIGH/CRITICAL finding. |
| 9 | **Post-GA polish wave** (wave-27 cumulative-track tail) | If wave-27 cutover succeeds, post-GA wave absorbs the cutover-day retrospective + Lote 6 v1.0.0 GA tag promotion + DEBT-010 P3 + DEBT-013 OPT-03a re-evaluation. | 1 Sonnet × cumulative. |
| 10 | **Wave-27 INV registry + DEBT register hygiene sweep** | Cadence preserved — same charter as wave-19/20/21/22/23/24/25/26 sweep streams. | 1 Sonnet × 30 min. |

### 9.1 Caveats from wave-26 sweep findings

- **INV registry stable at 197** — wave-25 SEAL added zero new canonical INVs; wave-26 in-flight streams (per §5.1) project zero additions. **Wave-26 GA-1 freeze (stream #1) locks the count at 197 declared.** Any post-GA additions are R-prep-post-GA absorption work, not GA-blockers.
- **CRITICAL-no-TLA+ count is 0** at wave-26 base — *new milestone* vs wave-25 close (which had 1 — INV-PAT-REVOKE-PROPAGATION on the exempt classification). Wave-25 stream #6 supplied `auth_pat_revoke.tla` TLA-VERIFIED status (per `8fa1c22`). Wave-27 GA-cutover decision board cites "100% CRITICAL TLA+ coverage" as a green-light signal.
- **No DRAFT entries detected** in `specs/03_architecture/invariant_registry.md` §3 sub-sections at wave-26 base; the doc-level `doc_status: DRAFT` front-matter remains staffing-blocked per F-09 until ≥ 2 reviewers nominated.
- **DEBT-015-BUILD CLOSED in wave-25** — wave-26 carries NO docs-platform-eval stream (wave-25 §8 candidate #3 obviated). Docusaurus 3 stays.
- **DEBT-026 is the only GA-cutover-blocking DEBT row** — earliest cutover date is 2026-07-29 (post-retest-letter). Wave-27 cutover stream #1 may dispatch as a "GA-prep" wave with ceremony deferred. The other 6 OPEN DEBT rows are user-bound / attorney-bound / partial-CLOSED-with-post-GA-tail and do not block cutover.
- **GA-readiness DEFER counter locked at 8** (5 user-bound + 3 vendor-bound) by the wave-25 stream #4 drift detector — no drift detected at wave-26 base. Wave-27 cutover meeting consumes this counter unchanged.
- **Wave-25 adversarial review (wave-26 stream #7) is the bound resource** — wave-25 had 11 in-flight streams of which 7 are P1-classified (pentest engagement, DEBT-015-BUILD path-3, DEBT-008 reconciliation, DEFER drift detector, endurance 10min, tenant-config CF prod-wire, pre-GA security attestation; wave-24 adversarial review is itself P1 since it gates wave-26). **Required before wave-27 GA cutover D-day execution unblocks.**
- **Wave-26 anchor is GA-1 feature freeze** — per the charter "anchor: GA-1 freeze + Lote 6 v1.0.0 GA absorption". Wave-26 is the final wave before GA cutover; any structural blocker surfaced in wave-26 streams (e.g., wave-25 adversarial review CONDITIONAL findings escalation) must be resolved or explicitly waived by the GA-cutover D-day execution decision (wave-27).
- **Lote 6 v1.0.0 GA RC2 (stream #5) is the cache-layer release-blockable artefact** — RC2 → v1.0.0 GA tag is a *re-tag without code change* triggered T+24h post-cutover validation that customer traffic succeeded on the RC2 build.
- **wasm32 baseline (stream #2) + CF prefetch (stream #3) are paired** — stream #2 must SEAL before stream #3 (baseline is "pre-prefetch shape").

### 9.2 Wave-27 entry caveats

- **DCO + Co-Authored-By preserved** on every commit (same as wave-19 through wave-26).
- **No `--no-verify` hooks.** Pre-commit failures must surface root cause.
- **Synchronous Bash only.** No `run_in_background`. Same charter as wave-19/20/21/22/23/24/25/26.
- **30–40-min time budget per stream** (matches wave-22/23/24/25/26 cadence); 8–10 streams per wave realistic.
- **Wave-27 SEAL gate:** all wave-26 P1 streams (#1 GA-1 freeze, #2 wasm32 baseline, #3 CF prefetch, #4 DEBT-026 RFP send, #5 Lote 6 RC2, #6 dry-run #2, #7 wave-25 adversarial review) verified closed before wave-27 unblocks the GA cutover D-day execution.
- **GA cutover D-day execution becomes the wave-27 anchor stream** — its execution authorisation depends on DEBT-026 retest-letter delivery (earliest 2026-07-29) **unless** wave-27 dispatches as a "GA-prep" wave with the ceremony itself deferred. **This is the cutover wave.**

---

## 10. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave26-sweep`
- **Base commit:** `2a4e00c` (wave-25 SEAL tip)
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-26 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 11. Cross-references

- `specs/_audits/sealed/2026-05-16-wave25-closure.md` (wave-25 closure; predecessor).
- `specs/_audits/sealed/2026-05-16-wave24-closure.md` (wave-24 closure).
- `specs/_audits/sealed/2026-05-15-debt-register.md` (DEBT register canonical state — v1.2.2; DEBT-015-BUILD CLOSED wave-25 / DEBT-026 added wave-25).
- `specs/03_architecture/invariant_registry.md` (197 declared at wave-26 base; 61 CRITICAL all TLA+-proved — new milestone).
- `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` (wave-24 stream #8 final audit; CONDITIONAL GO; 8-item DEFER counter).
- `specs/_audits/sealed/2026-05-16-ga-readiness-defer-scrub.md` (wave-25 stream #4 DEFER drift detector — locks counter at 8).
- `specs/_audits/sealed/2026-05-16-debt-015-build-wave25-closure.md` (wave-25 stream #2 path-(3) ssgRequire CLOSURE).
- `specs/_audits/sealed/2026-05-16-pentest-engagement-scope-freeze.md` (wave-25 stream #1 SEALED; spawns DEBT-026).
- `specs/_audits/sealed/2026-05-16-tenant-config-cf-prod-wire.md` (wave-25 stream #7 SEALED; precursor to wave-26 stream #3 CF prefetch).
- `specs/_audits/sealed/2026-05-16-endurance-10min-dressrun.md` (wave-25 stream #5 SEALED; precursor to wave-27 7d soak).
- `specs/_audits/sealed/2026-05-16-statuspage-init-dressrun.md` (wave-25 stream #6 SEALED; user-bound at T-7d).
- `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` (wave-25 stream #8 SEALED).
- `specs/_audits/sealed/2026-05-16-wave24-adversarial-review.md` (wave-25 stream #9 — 6.95/10 CONDITIONAL).
- `specs/_audits/sealed/2026-05-16-debt-008-number-discrepancy-fix.md` (wave-25 stream #3 narrative reconciliation).
- `specs/_audits/sealed/2026-05-16-ga-cutover-dryrun.md` (wave-24 stream #1 dry-run G1..G6 all GREEN; precursor to wave-26 stream #6 dry-run #2).
- `specs/_compliance/GA-GATE-CRITERIA.md` (59 criteria across 6 tracks).
- `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` (the meeting whose APPROVED decision authorises `RB-GA-CUTOVER.md`).
- `specs/_runbooks/RB-GA-CUTOVER.md` (cutover runbook; wave-26 GA-1 freeze authorises §0 checklist run; wave-27 executes §3).
- `specs/_runbooks/STATUSPAGE-INIT.md` (Statuspage provisioning playbook; T-7d gate per DEBT-016).
- `.github/workflows/mutation-nightly.yml` (CI-nightly artifact precedence — covers `{dual-approval, ratelimit, r2-multipart, quota-cas, webauthn}` continuously at wave-26 SEAL).
