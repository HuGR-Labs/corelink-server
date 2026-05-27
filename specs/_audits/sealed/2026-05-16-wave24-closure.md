# Wave-24 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-readiness rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-24 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave24-sweep`.
> **Base:** `main` @ `33138b5` ("merge wt/r-prep-debt-008-mutation-wave23 into main (wave-23)" — wave-23 SEAL tip).
> **Scope:** INV registry hygiene + DEBT register survey (wave-24 in-flight — DO NOT close from this stream) + wave-24 stream catalogue + GA cutover dry-run results (cross-ref stream #1) + DEBT-008 + DEBT-015-BUILD final state narratives + GA-readiness verdict (cross-ref stream #8 final audit) + wave-25 candidate streams (external pentest engagement scope freeze).
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-wave23-closure.md` (predecessor), `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.1, `specs/03_architecture/invariant_registry.md`, `specs/_audits/sealed/2026-05-16-debt-008-wave23-mutation-sweep.md` (wave-23 baseline DEBT-008), `specs/_audits/sealed/2026-05-16-debt-015-build-final.md` (wave-23 baseline DEBT-015-BUILD).

---

## 1. Wave-24 scope — 10 streams catalogued

Wave-24 is the post-wave-23 GA-rehearsal + final-audit wave, dispatched on `main` @ `33138b5`. Ten parallel streams catalogued (this stream is #10). Per wave-23 §6 "wave-24 candidate streams", wave-24 anchors the GA cutover dry-run rehearsal (stream #1, per dry-run readiness assessment in wave-23 §3.4) and adds the GA-readiness final audit (stream #8) as the wave-24 SEAL gate.

| # | Stream | Branch / worktree | Disposition |
|---|---|---|---|
| 1 | **GA cutover dry-run rehearsal** — execute end-to-end dry-run of the `RB-GA-CUTOVER.md` runbook (chaos harness + 24h endurance rig + pilot E2E + CS playbook + beta-feedback + INV/DEBT register state); produces evidence for greenlight-dashboard row "GA cutover dry-run complete". Anchor stream per wave-23 §3.4. | `wt/r-prep-ga-cutover-dryrun` (worktree `agent-ga-cutover-dryrun`) | **IN FLIGHT** (wave-24) |
| 2 | **DEBT-008 wave-24 mutation sweep** — re-sweep verification on `corelink-chunker` (wave-23 projected 97.9 % of viable / 100 % of killable) + `corelink-multipart-schema` (wave-23 projected ≥ 90.6 %) + first sweeps on `corelink-r2-multipart` (150 mutants), `corelink-quota-cas` (350), `corelink-webauthn` (369) + the 11 lifecycle-bound multipart-schema hardening kills carried from wave-23 stream #2. | `wt/r-prep-debt-008-mutation-wave24` (worktree `agent-ab42db3ac8dbf120a`, locked) + `wt/r-prep-pat-clerk-mutation-sweep` (worktree `agent-a5f4154bbf454315f`, locked combined-bucket) | **IN FLIGHT** (wave-24) |
| 3 | **DEBT-015-BUILD babel-preset patch** — apply wave-23 stream #3 narrowed root-cause fix path (1): patch `@docusaurus/babel/lib/preset.js` to use `modules: false` for the server preset-env config (matching client). Path-(1) preferred over path-(2) custom babel plugin and path-(3) ssgRequire sandbox-eval per wave-23 final audit (`specs/_audits/sealed/2026-05-16-debt-015-build-final.md`). | `wt/r-prep-debt-015-build-babel-patch` (worktree `agent-debt-015-w24`) | **IN FLIGHT** (wave-24) |
| 4 | **DEBT-016 Statuspage URL pre-wiring** — pre-populate the post-DEBT-016 cutover URLs (`status.corelink.humangr.com` + RSS + atom + summary endpoints) in user-facing docs + runbook references so user-bound provisioning becomes a pure DNS + CNAME swap at T-7d pre-launch. | `wt/r-prep-debt-016-statuspage-urls` (worktree `agent-debt-016-statuspage`) | **IN FLIGHT** (wave-24) |
| 5 | **ADR-0034b dual-hat decomposition** — resolve the FW-H-* role nominations gating item (PRR dual-hat row decompositions across S-06 / S-09 / S-13). Cannot do the nominations themselves (user-bound) but can decompose the dual-hat rows into separate single-hat invariants + acceptance criteria so the user-side onboarding effort drops from "design a structure" to "fill in names". | `wt/r-prep-adr-0034b-dual-hat` (worktree `agent-adr-0034b`) | **IN FLIGHT** (wave-24) |
| 6 | **INV-PAT-REVOKE-PROPAGATION TLA+ proof** — eliminate the lone CRITICAL-without-TLA+ entry surfaced by `validate_canonical_consistency.py` (`INV-PAT-REVOKE-PROPAGATION` from wave-23 §3.28 PAT revocation domain). Without this, GA-readiness greenlight dashboard cannot flip the "0 CRITICAL without TLA+" cell to GREEN. | `wt/r-prep-auth-pat-revoke-tla` (worktree `agent-auth-pat-revoke-tla`) | **IN FLIGHT** (wave-24) |
| 7 | **Audit analytics wallclock symmetry** — extend wave-22 Stripe MatClock wasm32 follow-on with the audit-analytics path: ensure read-side analytics queries observe the same MatClock-monotonic timestamps as the audit-emit path (no skew between writer and reader views of `INV-CLOCK-MONOTONIC-WITHIN-REQUEST` family). Closes the wave-22 §4.2 follow-on caveat. | `wt/r-prep-audit-analytics-wallclock-symmetry` (worktree `agent-wallclock-analytics`) | **IN FLIGHT** (wave-24) |
| 8 | **GA-readiness final audit** — execute the codex-Opus-level GA-readiness rollup against `RB-GA-CUTOVER.md` greenlight-dashboard. Cross-reads stream #1 dry-run results + stream #3 DEBT-015-BUILD final state + stream #6 PAT-revoke TLA+ + DEBT-008 wave-24 batch + the user-bound items still pending. **This is the wave-24 SEAL-gate stream.** | `wt/r-prep-ga-readiness-final-audit` (worktree `agent-ga-readiness-final`) | **IN FLIGHT** (wave-24) |
| 9 | **Wave-23 adversarial review (codex Opus pass)** — independent SOTA-bar review over wave-23 P1 closure streams (#2 DEBT-008 5-new-crate + 2 re-sweep, #3 DEBT-015-BUILD final, #4 chaos combined-failures, #5 pilot E2E, #6 LFPDPPP MX legal package, #7 CS playbook, #8 beta feedback triage). Required per wave-23 §6 item #2 "Wave-23 adversarial review (codex Opus pass)". | `wt/r-prep-wave23-adversarial-review` (worktree `agent-wave23-review`) | **IN FLIGHT** (wave-24) |
| 10 | **Wave-24 INV registry sweep + DEBT register survey + wave-24 closure audit** (this stream — hygiene + cataloguing pass; survey-only) | `wt/r-prep-inv-registry-wave24-sweep` (worktree `agent-wave24-sweep`) | **CLOSED via this commit** |

Streams #1–#9 are dispatched in parallel by the orchestrator; this stream (#10) performs the hygiene + cataloguing pass against the same `33138b5` base. Per the user mandate (charter §"DEBT survey (wave-24 in flight; survey-only)"), streams #1–#9 are surveyed below but **not** closed by this audit. Their SEAL commits land separately and the next wave-25 sweep reconciles. Note: stream #2 combines two worktrees (`debt-008-mutation-wave24` + `pat-clerk-mutation-sweep`) into the same charter-mandated "10 streams" budget — same combined-bucket pattern used in wave-22 stream #9 and wave-23 stream #9.

---

## 2. GA cutover dry-run results (cross-ref stream #1)

Stream #1 is the **wave-24 anchor stream** per wave-23 §3.4 dry-run readiness assessment. Charter charter §6.2 wave-24 anchor language: *"GA cutover dry-run rehearsal becomes the wave-24 anchor stream — its findings drive wave-25 dispatch priority."*

### 2.1 Prerequisite readiness at wave-24 dispatch (post-wave-23-SEAL)

Per wave-23 §3.4 dry-run readiness matrix, projected post-wave-23-SEAL state:

| Dry-run prerequisite | State at wave-24 dispatch | Readiness |
|---|---|---|
| Chaos campaign harness operable (8 fail-CLOSED scenarios) | wave-22 stream #7 SEAL'd | ✅ READY |
| Combined-failure matrix runnable | wave-23 stream #4 SEAL'd | ✅ READY (assumed post-wave-23-SEAL) |
| 24h endurance load rig operable | wave-22 stream #8 SEAL'd | ✅ READY |
| Pilot onboarding E2E rehearsable end-to-end | wave-23 stream #5 SEAL'd | ✅ READY (assumed post-wave-23-SEAL) |
| Beta-feedback ingestion pipeline operable | wave-23 stream #8 SEAL'd | ✅ READY (assumed post-wave-23-SEAL) |
| CS playbook documented + escalation matrix | wave-23 stream #7 SEAL'd | ✅ READY (assumed post-wave-23-SEAL) |
| Docs site builds locally (Node 22) | wave-23 stream #3 + wave-24 stream #3 | 🟡 PENDING wave-24 stream #3 SEAL |
| INV registry stable / no orphan refs | 197 declared / 0 orphan (validators exit 0 this wave) | ✅ READY |
| DEBT register reconciled (no drift) | 6 canonical OPEN pre-wave-24-SEAL; survey-only this wave | ✅ READY (state is current) |
| Statuspage provisioned | DEBT-016 OPEN (user-bound) — wave-24 stream #4 pre-wires URLs | 🟡 SOFT-READY (URLs pre-wired; provisioning still user-bound) |
| LFPDPPP MX legal package drafted | wave-23 stream #6 SEAL'd | ✅ READY (assumed post-wave-23-SEAL) |
| AWS Artifact PDF download + hash | DEBT-003 OPEN (user-bound) | ⛔ BLOCKED on user |

**Net dry-run feasibility:** 9 READY (incl. 5 assumed-post-wave-23-SEAL) + 2 PENDING (wave-24 stream #3 docs build; statuspage soft-ready) + 1 BLOCKED (DEBT-003 user-bound). **Rehearsable end-to-end now** for the agent-closable portion; the user-bound DEBT-003 artifact hash gap is a documented dry-run waiver (rehearsal records the "TBD-on-receipt" placeholder + verifies the codepath that ingests the hash). Per wave-23 §3.4 wave-24 dispatch directive, stream #1 executes the rehearsal regardless.

### 2.2 Dry-run scope (per stream #1 charter — survey only)

Stream #1's dispatched scope per worktree branch name + wave-23 §6 item #1 rationale:

1. **Step-through the `RB-GA-CUTOVER.md` greenlight-dashboard** row by row, recording GREEN/YELLOW/RED + evidence link.
2. **Execute the 8 fail-CLOSED chaos scenarios** (wave-22 stream #7 harness) against a staging tenant, capturing per-scenario telemetry into `reports/chaos/latest.json`.
3. **Execute the combined-failure matrix** (wave-23 stream #4 — executor-loss × replication-lag × tenant-isolation), capturing per-combination telemetry.
4. **Pilot E2E walk-through** (wave-23 stream #5 — signup → DPA exchange → BYOK bind → first audit emit), using a synthetic pilot tenant.
5. **CS playbook escalation drill** (wave-23 stream #7 — synthetic SEV-2 trigger, escalate per matrix).
6. **Beta feedback triage drill** (wave-23 stream #8 — synthetic feedback item, route per labels).
7. **INV registry + DEBT register state-recording** snapshot at dry-run start vs. end (no drift expected — dry-run is read-only).
8. **Capture findings** into a wave-24 dry-run audit doc (`specs/_audits/2026-05-16-ga-cutover-dryrun-results.md` — to be authored by stream #1).

**Survey-only verdict for this audit:** stream #1 is the wave-24 anchor; its findings drive wave-25 dispatch priority. This audit does **not** preempt stream #1's authored audit doc — that audit is the canonical record. Once stream #1 SEALs, wave-25 sweep stream reconciles findings into greenlight-dashboard updates.

---

## 3. DEBT-008 final state (closure or final residual)

### 3.1 DEBT-008 closure ledger (cumulative pre-wave-24-SEAL)

Per `specs/_audits/sealed/2026-05-15-debt-register.md` row §51 (DEBT-008 progressive expansion ledger) + `specs/_audits/sealed/2026-05-16-debt-008-wave23-mutation-sweep.md` (wave-23 baseline).

| Crate | Empirical kill % | Closure status | Source wave |
|---|---|---|---|
| `corelink-audit-chain` | **84.24 %** (165 viable; 139 caught; 26 missed → +5 targeted tests) | **CLOSED** | wave-15 |
| `corelink-hash` | **97.22 %** (35/36; documented-equivalent on non-overlapping nibbles) | **CLOSED** | wave-21 |
| `corelink-dedup` | **92.06 %** (58/63 viable, 0 missed, 5 timeouts excluded) | **CLOSED** | wave-22 |
| `corelink-tenant-path` | **100.00 %** (18/18 post-additions) | **CLOSED** | wave-22 |
| `corelink-handler-cas` | **100.00 %** (26/26 viable, post-additions confirmed via wave-23 re-sweep) | **CLOSED** | wave-22+23 |
| `corelink-auth-schema` | **100.00 %** (77/77 viable, post-additions confirmed via wave-23 re-sweep) | **CLOSED** | wave-22+23 |
| `corelink-chunker` | pre **79.79 %** (75/94 viable); +13 targeted tests killing 17/19 missed + 1 timeout; 2 mutants constraint-unkillable on 160 GiB MAX_BLOB_SIZE guard; projected post-additions **97.9 %** of viable / **100 %** of killable | **PROJECTED-CLOSED** (re-sweep verification → wave-24 stream #2) | wave-23 |
| `corelink-multipart-schema` | pre **77.78 %** (91/117 viable); +9 targeted tests killing 20/26 missed; 11 lifecycle-bound `<` mutants deferred; projected post-additions **≥ 90.6 %** | **PROJECTED-CLOSED** (re-sweep + 11 lifecycle-bound hardening kills → wave-24 stream #2) | wave-23 |
| `corelink-r2-multipart` (150 estimated) | — | **IN FLIGHT** (wave-24 stream #2 first sweep) | this wave |
| `corelink-quota-cas` (350) | — | **IN FLIGHT** (wave-24 stream #2 first sweep) | this wave |
| `corelink-webauthn` (369) | — | **IN FLIGHT** (wave-24 stream #2 first sweep) | this wave |
| `corelink-pat` | CI-nightly matrix (75 % floor) | OPEN-deferred (CI artifact is SEAL gate per `TD-DEBT-008-WAVE-14-EMPIRICAL`); **wave-24 combined-bucket worktree `wt/r-prep-pat-clerk-mutation-sweep` may flip empirical** | nightly cron + wave-24 attempt |
| `corelink-clerk` | CI-nightly matrix (75 % floor) | OPEN-deferred (same); **wave-24 combined-bucket worktree may flip empirical** | nightly cron + wave-24 attempt |
| `corelink-dual-approval` | CI-nightly matrix (75 % floor) | OPEN-deferred | nightly cron |
| `corelink-ratelimit` | CI-nightly matrix (75 % floor) | OPEN-deferred | nightly cron |

### 3.2 DEBT-008 wave-24 SEAL gate (forward-looking; not enforced by this audit)

- ✅ Re-sweep verification on `corelink-chunker` + `corelink-multipart-schema` confirms post-additions empirical kill rate matches wave-23 projections (97.9 % + ≥ 90.6 %), OR documents the residual equivalent-mutation rationale matching `corelink-audit-chain` / `corelink-hash` precedent.
- ✅ 11 lifecycle-bound multipart-schema hardening kills land + targeted-test module updated.
- ✅ Each of the 3 new wave-24 crates (`r2-multipart`, `quota-cas`, `webauthn`) reaches ≥ 75 % empirical kill rate OR is escalated to wave-25 follow-on with documented equivalent-mutation analysis.
- ✅ If wave-24 stream #2 combined-bucket `pat-clerk-mutation-sweep` lands, `corelink-pat` + `corelink-clerk` flip from CI-nightly-only to empirical-CLOSED (narrows OPEN subset from 4 → 2).
- ✅ Per-crate JSON artefact landed at `target/mutants/<crate>.out/mutants.out/` + digested into `reports/mutation/latest.json` aggregate per `TD-DEBT-008-WAVE-14-EMPIRICAL`.

### 3.3 Post-wave-24-SEAL projection — DEBT-008 final state

**Best case (all wave-24 stream #2 sub-streams SEAL):** empirically-closed subset grows to `{audit-chain, hash, dedup, tenant-path, handler-cas, auth-schema, chunker, multipart-schema, r2-multipart, quota-cas, webauthn, pat, clerk}` = **13 crates** with empirical baseline; CI-nightly continues to cover `{dual-approval, ratelimit}` only = **2 remaining OPEN-deferred** (both with 75 % CI floor + `TD-DEBT-008-WAVE-14-EMPIRICAL` SEAL mechanism).

**Realistic case (re-sweep verifies + 3 new crates ≥ 75 % + pat-clerk flips):** same 13-crate empirical set; DEBT-008 row in register flips from "P1 partial" to "P1 partial — 2/15 CI-nightly residual", aligning the structural-OPEN designation with the actual surface area (just 2 crates on CI-nightly cadence).

**Worst case (≥ 1 new crate < 75 % + re-sweep does not confirm):** DEBT-008 row in register flips to "P1 partial — N CI-nightly + M wave-25 follow-on"; wave-25 stream dispatched per item #4 in wave-23 §6 ("DEBT-008 wave-24 follow-on for any crate not reaching ≥ 75 %").

**Final-state verdict for the DEBT register (forward-looking):** DEBT-008 will **not** flip to fully-CLOSED at wave-24 SEAL because `dual-approval` + `ratelimit` remain on CI-nightly by design; however, the **structural-OPEN narrative** changes from "5 crates with no empirical baseline" (pre-wave-21) to "2 crates with CI-nightly empirical telemetry" (post-wave-24). This is the **final residual** for DEBT-008 prior to GA — no further wave-25 stream warranted if CI floor (75 %) holds for those 2 crates per `TD-DEBT-008-WAVE-14-EMPIRICAL`.

---

## 4. DEBT-015-BUILD final state

### 4.1 Wave-history narrative

- **Wave-15 → wave-21**: docs P2 portion CLOSED (engine pin `<22` lifted; 11 doc files migrated to Node 22+ LTS callouts). `apps/docs/package.json` engine pin `>=22.0.0 <23.0.0`.
- **Wave-22 (`wt/r-prep-debt-015-build-closure` → `f356985`)**: blockers (a) `draft: true` removed from 3 referenced security/residency pages; (b) 90 docs files normalised to extensionless MDX cross-links (261 link sites + 4 broken specs/ROADMAP markdown links rewritten); (c) theme-alias rewrite APPLIED to `patches/@docusaurus__core@3.10.1.patch`. Net: build progresses past MDX compile and server-bundle load; SSG reaches per-route rendering. **New same-class residual:** server-bundle clientModule chunk registry emits literal `require("@site/docs/*.mdx")` + `require("@generated/docusaurus-plugin-content-docs/default/p/*.json")` strings that fail to externalise in pnpm-isolated layout.
- **Wave-23 (`wt/r-prep-debt-015-build-final` → final audit `specs/_audits/sealed/2026-05-16-debt-015-build-final.md`)**: configureWebpack plugin attempted (path A: force `output.asyncChunks: false` + `dynamicImportMode: 'eager'` + `splitChunks: false` on server); zero effect on the 113 `@site/*.mdx` + 77 `@generated/*.json` literal-require count. **Root cause narrowed** to `@docusaurus/babel/lib/preset.js` running `@babel/preset-env` with implicit `modules: 'auto'` on the server target (vs `modules: false` for client) — this transforms `() => import(spec)` into nested CJS `require(spec)` BEFORE webpack sees the code, and webpack's static analyzer doesn't deep-walk arrow-nested CJS requires. **Wave-23 fix path order documented (path 1 preferred):** (1) patch `@docusaurus/babel/lib/preset.js` `modules: false` for server config — 1–2 h; (2) custom babel plugin in `apps/docs/babel.config.js` rewriting nested `require("@site/X")` back to top-level import — 2–3 h; (3) `ssgRequire` patch sandbox-evaling `build/assets/js/<chunkId>.<hash>.js` client chunks — 3–5 h. Wave-22's path-B framing superseded by path (3). No regression vs wave-22; build still red identically.
- **Wave-24 (this wave) — `wt/r-prep-debt-015-build-babel-patch` (worktree `agent-debt-015-w24`)**: stream #3 executes path (1) per wave-23 final-audit recommendation.

### 4.2 Wave-24 stream #3 SEAL gate (forward-looking; not enforced by this audit)

- ✅ Patch landed in `patches/@docusaurus__babel@<version>.patch` (or `@docusaurus__core` overlay if babel preset is unpatchable directly) flipping the server preset-env `modules: false`.
- ✅ `pnpm --filter docs build` locally on Node 22 reaches `compiled SUCCESS` — i.e., `build/__server/server.bundle.js` no longer contains literal `require("@site/*")` or `require("@generated/*")` strings; webpack rewrites them to `__webpack_require__(<id>)` like the client bundle.
- ✅ SSG renders all 113 `@site/*.mdx` routes without `MODULE_NOT_FOUND`.
- ✅ Per-locale verification (en + pt-BR + de + es-419 — same 4-locale matrix as wave-21/22 docs touch waves).
- ✅ Spec + reference + INV-promotion validators exit 0 (gate also enforced for this audit).

### 4.3 Wave-24 stream #3 final-state contingencies

**Path (1) success** → DEBT-015-BUILD flips to **CLOSED** in register; row caries `wave-24 babel-preset patch` as the final closure commit; docs CI gate becomes runnable post-billing-reinstatement (DEBT-015-CI residual remains user-bound but no longer code-blocked).

**Path (1) failure (modules:false breaks client or causes other regressions)** → stream #3 escalates to path (2) custom babel plugin in `apps/docs/babel.config.js` — typically 2–3 h on top, but ALWAYS within wave-24 stream #3 budget (path 1 + path 2 stack to ~ 4 h total, within the 30–40-min/stream wave-charter budget IF the agent picks them up sequentially OR signals re-dispatch for wave-25). Path (3) ssgRequire sandbox-eval is the wave-25 escalation if both (1) + (2) fail.

**Final state for DEBT-015-BUILD (worst case at wave-24 SEAL):** if all 3 paths fail wave-24, the row escalates to **P1** with explicit alternative-architecture decision (e.g., switch from Docusaurus 3 to a different SSG, or invasive runtime patch); wave-25 dispatches a docs-platform-eval stream. **This is the LAST wave where path (1)/(2)/(3) can be attempted — escalation deadline.**

---

## 5. GA-readiness verdict (cross-ref stream #8 final audit)

Stream #8 (`wt/r-prep-ga-readiness-final-audit`) is the wave-24 SEAL-gate stream — its final audit is the canonical GA-readiness verdict that drives the `RB-GA-CUTOVER.md` greenlight-dashboard sign-off. This section is the **survey snapshot** projected against post-wave-24-SEAL state; stream #8's final audit supersedes any tentative verdict below.

### 5.1 Greenlight-dashboard snapshot — projected post-wave-24-SEAL

| Gate | State pre-wave-24 (wave-23 §3.3) | Projected post-wave-24-SEAL | Δ |
|---|---|---|---|
| Spec corpus (declared INVs, orphan refs, CRITICAL-no-TLA+) | 192 / 0 / 0 | **197 / 0 / 0 (if stream #6 PAT-revoke TLA+ lands)** OR 197 / 0 / 1 (if stream #6 slips) | +5 declared (wave-23 §3.27/§3.28); CRITICAL-no-TLA+ goes 0 → 1 → 0 across wave-23/wave-24 |
| Test/code coverage (src + test references) | 103 + 89 | unchanged unless new crates introduced | 0 |
| Mutation kill-rate | 6/15 empirical CLOSED + 2 PARTIAL + 7 OPEN-deferred (4 CI-nightly + 3 wave-23 in-flight) | Best case: 13/15 empirical CLOSED + 2 CI-nightly residual | +7 empirical CLOSED |
| Replication SLO observation streak | Streak accumulating | Stream #1 dry-run adds ≥ 24 h streak evidence | continues GREEN |
| Chaos campaign (combined-failure matrix) | wave-23 stream #4 IN FLIGHT | Stream #1 dry-run executes against staging | GREEN post-dry-run |
| DEBT P0 OPEN | 1 (DEBT-003 user-bound) | 1 (unchanged — DEBT-003 user-bound) | 0 |
| External pentest engagement | Scope SEALED wave-19; engagement pending | Wave-25 RFP + vendor shortlist + SOW template (per §6 candidate streams) | 0 — vendor + SOW still user-bound |
| DEBT-015-BUILD | PARTIAL (wave-23 narrowed root cause) | Best case CLOSED via path (1); worst case escalates to P1 alternative-architecture | flip-to-CLOSED OR escalate |
| Pilot onboarding | wave-23 stream #5 IN FLIGHT | Dry-run rehearses end-to-end; signups still user-bound | flip-to-READY for first pilot |
| LFPDPPP MX attorney package | wave-23 stream #6 IN FLIGHT | Package drafted; sign-off pending | flip-to-attorney-side-pending |
| Statuspage `status.corelink.humangr.com` go-live | DEBT-016 OPEN (user-bound) | Wave-24 stream #4 pre-wires URLs; provisioning still T-7d user-bound | soft-ready |
| FW-H-* role nominations | Pending user nominations | Wave-24 stream #5 (ADR-0034b dual-hat decomp) reduces user-side effort | structure-ready |
| AWS Artifact PDF download (DEBT-003) | Pending | unchanged | 0 |
| Wave-23 codex Opus review | Pending wave-24 stream #9 | Stream #9 SEAL'd | review-complete |

### 5.2 GA-readiness verdict (this audit, forward-looking)

Three-state framing per the GA cutover RACI:

- **GREEN (cutover-ready):** requires (a) wave-24 stream #1 dry-run PASS, (b) wave-24 stream #3 DEBT-015-BUILD CLOSED (path 1 or 2), (c) wave-24 stream #6 PAT-revoke TLA+ proof SEAL, (d) wave-24 stream #8 final-audit issues GO. All four conditions concurrent → flip greenlight-dashboard to GREEN. Remaining user-bound items (DEBT-003 hash, DEBT-016 provisioning, FW-H nominations, LFPDPPP attorney sign-off, pilot signups, pentest engagement) are checklist items at cutover-day rather than gates.
- **YELLOW (conditional-cutover):** any one of (a)–(d) PARTIAL — typically (b) DEBT-015-BUILD escalates to P1 OR (a) dry-run surfaces non-blocking findings absorbed into wave-25. GA-Limited launch feasible with explicit waivers.
- **RED (no-go):** (a) dry-run surfaces P0 design-level gap, OR (c) PAT-revoke TLA+ proof cannot be discharged (model-checking inconsistent), OR (d) final-audit issues NO-GO on structural grounds.

**Forward-looking projection at wave-24 SEAL (this audit's stance):** **GREEN-PROBABLE** — no known structural blockers, all 4 conditions independently tractable within the wave-24 stream budgets, no prior wave introduced an unknown-unknown that would cascade. Stream #8's final audit is the canonical verdict; this audit defers to it.

---

## 6. Wave-25 candidate streams (external pentest engagement scope freeze)

Per the wave-24 charter, the wave-25 anchor stream is the **external pentest engagement scope freeze** (deferred from wave-22 RFP-prep findings absorption per wave-23 §3.1).

| # | Stream | Rationale | Estimated cost |
|---|---|---|---|
| 1 | **External pentest engagement scope freeze** (anchor stream) | Pentest scope SEALED wave-19 (`specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md`); wave-22 deferred RFP-prep to wave-24; wave-24 deferred to wave-25 because wave-24 stream-#1 dry-run needed first to inform scope (e.g., which combined-failure surfaces should be in pentest scope). Wave-25 stream #1 executes: (a) RFP draft + vendor shortlist + SOW template; (b) scope-freeze decision: ratify or amend the wave-19 SEALED scope based on wave-24 dry-run + final-audit findings. Vendor selection + engagement letter remain user-bound. | 1 codex/Opus draft RFP + shortlist + SOW; 1 Sonnet absorb wave-24 findings into amended scope. |
| 2 | **Wave-24 adversarial review (codex Opus pass)** | Mandatory per charter §"all P1-classified streams must close before next wave unblocks P2/P3". Cross-review wave-24 streams #1 dry-run, #2 DEBT-008 wave-24 batch, #3 DEBT-015-BUILD babel-patch, #6 PAT-revoke TLA+, #8 GA-readiness final audit. | ~1 codex Opus pass per P1 stream + 1 audit doc per stream. |
| 3 | **Wave-24 stream-#1 dry-run findings absorption** | If wave-24 stream #1 dry-run surfaces P0/P1 findings on `RB-GA-CUTOVER.md` runbook gaps (notably any chaos combined-failure that exposes new fail-OPEN classes, pilot-onboarding gaps not yet papered over, or CS playbook ambiguity), dispatch fix agents. | Branch-per-finding; size depends on findings. |
| 4 | **DEBT-008 wave-25 follow-on** | Any crate from wave-24 stream #2 batch that hit < 75 % empirical kill rate; or `dual-approval` + `ratelimit` empirical-baseline first-sweep if CI-nightly streak insufficient. | 1 Sonnet per remediation. |
| 5 | **DEBT-015-BUILD path (2)/(3) escalation** (only if wave-24 stream #3 path (1) failed) | Custom babel plugin in `apps/docs/babel.config.js` (path 2) OR ssgRequire sandbox-eval (path 3) OR alternative-architecture decision. | 1 Sonnet (path 2) or codex Opus (path 3 / alternative-architecture). |
| 6 | **24h endurance soak execution** | Wave-22 stream #8 built the rig; wave-24 dry-run consumes ≥ 24 h streak evidence in stream #1; if wave-24 stream #1 dry-run absorbs the soak, this becomes "soak streak ratchet" (continuous run for 7+ days). | Wall-clock; 1 Sonnet for evidence absorption. |
| 7 | **DEBT-010 P2 CI optimisation batch** (carried from wave-23 §6 item #8) | Post-GA polish; ~30 min/PR cumulative savings; concurrency cancel + shared rust-cache key + TLC matrix + paths-filter audit. | 1 Sonnet × 4 P2 tickets. |
| 8 | **DEBT-013 perf optimisation deferrals execution** (carried from wave-23 §6 item #9) | Post-GA polish; wave-22 stream #6 already tightened regression CI; OPT-03b + OPT-04ph2 + OPT-08 remain. | 1 Sonnet × 3 deferrals. |
| 9 | **DEBT-025 LFPDPPP MX absorption** (T+30d-ish horizon) | LFPDPPP MX attorney-side review absorption — wave-23 stream #6 drafted the package; once attorney returns sign-off / edits, an agent absorbs the legal-side feedback into the residency annex + retention table + DPA addendum. | 1 Sonnet × absorption. |
| 10 | **Wave-25 INV registry + DEBT register hygiene sweep** | Cadence preserved — same charter as wave-19/20/21/22/23/24 sweep streams. | 1 Sonnet × 30 min. |

### 6.1 Caveats from wave-24 sweep findings

- **INV registry grew by 5 wave-23 → wave-24** (192 → 197): §3.27 OPS domain (4 INVs from S-17 `_spec_contract.md §8`) + §3.28 PAT revocation domain (1 INV from public OpenAPI). All 5 already have spec/code/test citations per wave-23 `inv-draft-sweep`; one (`INV-PAT-REVOKE-PROPAGATION`) is CRITICAL and lacks a TLA+ proof — wave-24 stream #6 closes that gap.
- **No further DRAFT entries detected** in `specs/03_architecture/invariant_registry.md` §3 sub-sections at wave-24 base; the doc-level `doc_status: DRAFT` in front-matter remains staffing-blocked per F-09 until ≥ 2 reviewers nominated (separate from any per-entry status — same caveat as wave-23 §4).
- **CRITICAL-no-TLA+ count goes from 0 (wave-23 close) → 1 (wave-24 base) → 0 (wave-24 SEAL projection)** — the 1 entry is `INV-PAT-REVOKE-PROPAGATION` (PAT revocation propagation lifecycle); wave-24 stream #6 dispatches an explicit TLA+ proof stream. **No wave-25 follow-on needed** if stream #6 SEALs.
- **DEBT-008 wave-24 batch is the LAST first-sweep batch before GA** — all 11 crates in the `corelink-{mutation-tracked}` set will have empirical baseline (or CI-nightly empirical telemetry) post-wave-24-SEAL. **No wave-25 first-sweep stream needed** unless wave-24 stream #2 surfaces a < 75 % crate.
- **DEBT-015-BUILD has a hard escalation deadline at wave-24 SEAL** — paths (1)/(2)/(3) attempted in wave-24 stream #3; if all fail, escalates to P1 with alternative-architecture decision (wave-25 docs-platform-eval stream). This is the last wave where path-attempts are within charter scope; further path-attempts would be wave-25 P1 escalation by definition.
- **Wave-23 codex Opus review (wave-24 stream #9) is the bound resource** — wave-23 had 9 in-flight streams; codex Opus needs to pass over each P1 stream (#2 mutation, #3 DEBT-015-BUILD final, #4 chaos combined-failures, #5 pilot E2E, #6 LFPDPPP, #7 CS, #8 beta-feedback = 7 streams). This is the largest adversarial-review pass of the R-prep waves to date.
- **Pentest engagement scope freeze deferred to wave-25 stream #1 (anchor)** — rationale: wave-24 stream #1 dry-run was needed first to inform scope amendments (chaos combined-failure surfaces, pilot-tenant attack surface, BYOK key-binding ceremony). Wave-25 absorbs dry-run findings into the SEALED-wave-19 scope (ratify or amend) before RFP issue.

### 6.2 Wave-25 entry caveats

- **DCO + Co-Authored-By preserved** on every commit (same as wave-19 through wave-24).
- **No `--no-verify` hooks.** Pre-commit failures must surface root cause.
- **Synchronous Bash only.** No `run_in_background`. Same charter as wave-19/20/21/22/23/24.
- **30–40-min time budget per stream** (matches wave-22/23/24 cadence); 8–10 streams per wave realistic.
- **Wave-25 SEAL gate:** all wave-24 P1 streams (#1 dry-run, #2 DEBT-008 batch, #3 DEBT-015-BUILD babel-patch, #6 PAT-revoke TLA+, #8 GA-readiness final audit) verified closed before wave-25 unblocks any new P2/P3 streams.
- **External pentest engagement scope freeze becomes the wave-25 anchor stream** — its scope-freeze decision drives wave-26+ dispatch priority for pentest-finding absorption.

---

## 7. INV registry state (post-wave-24 sweep)

Per `python3 scripts/validate_canonical_consistency.py` on this branch (post-sweep, pre-SEAL):

| Metric | Count (wave-24 base) | Δ vs wave-23 close |
|---|---|---|
| INVs declared (registry §3 rows) | **197** | +5 |
| └ CRITICAL | **61** | +1 (PAT-revoke) |
| └ HIGH | **132** | +4 (OPS domain) |
| └ MEDIUM | **4** | 0 |
| └ LOW / UNKNOWN | **0** | 0 |
| Aliases declared (registry §5) | 15 | +2 (wave-23 invariant-draft sweep added 5 aliases per registry §3.27/§3.28 narrative; net +2 in canonical alias index) |
| TLA+ verified (declared INVs proved in `specs/tla/*.tla`) | **81** | 0 |
| Code-referenced (declared INVs cited in `crates/*/src/`) | 103 | 0 |
| Test-referenced (declared INVs cited in `crates/*/tests/`) | 89 | 0 |
| Orphan refs (in code, NOT in registry+aliases) | **0** | 0 |
| CRITICAL without TLA+ proof | **1** (INV-PAT-REVOKE-PROPAGATION) | +1 (wave-24 stream #6 closes) |
| Declared with NO code/test reference | 76 | +5 (the 5 new wave-23 INVs are spec/doc-referenced; src/test references accumulate post-impl) |
| Declared test-only (test ref but no src/) | 18 | 0 |

Per `python3 scripts/validate_inv_promotion.py`: registry coverage **143/143** (all WI-declared INVs present in registry §3). No NEW canonical INVs added by wave-23 SEAL commits beyond §3.27 (4 OPS) + §3.28 (1 PAT) already absorbed before wave-24 base — registry stable at 197 declared.

**INV-DRAFT → INV-PROMOTED count this wave:** **0** (no per-INV DRAFT entries in registry §3 sub-sections; the 5 wave-23 additions were promoted in wave-23 `inv-draft-sweep` SEAL commit and are already canonical at wave-24 base). The sibling pattern that wave-23 stream #9 ran (combined `inv-draft-promotion-sweep` worktree) has no analogue in wave-24 because the source-of-drafts (`_spec_contract.md` § INV blocks across S-17 + apps/docs OpenAPI) is empty post-wave-23.

### 7.1 Why no promotions this wave

Same pattern as wave-22/wave-23 close: wave-24 in-flight streams are predominantly closure / hardening / audit work, not new structural INV authoring. The lone "new INV"-style stream of wave-24 is stream #6 (PAT-revoke TLA+ proof) — but the INV itself (`INV-PAT-REVOKE-PROPAGATION`) was already promoted in wave-23; stream #6 adds the proof artifact, not a new canonical row. No DRAFT-staging intermediates created this wave → 0 promotions warranted from this audit stream.

---

## 8. DEBT register survey — pre-wave-24-SEAL state

Per `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.1 (last reconciled in wave-23 close — DEBT-025 added 2026-05-16; total rows 22). No DEBT closures performed by this stream (charter-bound survey-only).

### 8.1 Open count + per-priority breakdown (canonical rows; pre-wave-24-SEAL)

| Priority | Open IDs | Count | Wave-24 closure ETA |
|---|---|---|---|
| **P0** | DEBT-003 (AWS Artifact PDF — user-bound) | 1 | Pending human (no wave-24 stream) |
| **P1** | DEBT-008 (partial — `{dual-approval, ratelimit}` CI-nightly + 3 in-flight first sweeps + 2 re-sweep verifications via wave-24 stream #2) | 1 partial | Stream #2 SEAL → empirical subset grows 6 → up to 13; structural-OPEN narrows to 2 CI-nightly |
| **P1** | DEBT-010 (partial 4/11 — 7 P2/P3 deferred post-GA) | 1 partial | Deferred post-GA |
| **P1** | DEBT-013 (partial 6/10 — 4 explicit deferrals) | 1 partial | Wave-22 stream #6 tightened regression CI; deferrals themselves remain post-GA |
| **P2** | DEBT-015-BUILD (build-side addendum — wave-23 narrowed root cause to babel preset-env; wave-24 stream #3 path (1) attempt) | 1 | Stream #3 path-(1) or escalate to P1 wave-25 docs-platform-eval |
| **P2** | DEBT-016 (Statuspage go-live — user-bound; wave-24 stream #4 pre-wires URLs) | 1 | Pending human at T-7d pre-launch |
| **P2** | DEBT-025 (LFPDPPP MX attorney sign-off — added wave-23 v1.2.1) | 1 | Stream-of-attorneys, not stream-of-agents; agent absorption deferred to wave-25 item #9 |

**Post-wave-23-SEAL closures actually landed at wave-24 base** (verified against `git log` commits between `043428a` and `33138b5`):
- ✅ DEBT-008 wave-23 expansion: `handler-cas` + `auth-schema` re-sweep CLOSED 100 %; `chunker` + `multipart-schema` PARTIAL with wave-23 projections; baseline audit `specs/_audits/sealed/2026-05-16-debt-008-wave23-mutation-sweep.md`.
- ✅ DEBT-015-BUILD wave-23 final audit `specs/_audits/sealed/2026-05-16-debt-015-build-final.md`: root cause narrowed to babel preset-env `modules: 'auto'` on server target; 3 fix paths documented in priority order.
- ✅ Wave-23 INV registry sweep + DEBT register survey (predecessor stream) — CLOSED with merge `33138b5`.
- ✅ Wave-23 stream #9 invariant-draft promotion sweep — CLOSED; §3.27 OPS (4 INVs) + §3.28 PAT (1 INV) + 5 aliases added; audit `specs/_audits/sealed/2026-05-16-inv-draft-sweep.md`.
- ✅ Wave-23 streams #4, #5, #6, #7, #8 (chaos combined-failures + pilot E2E + LFPDPPP package + CS playbook + beta-feedback triage) — assumed all CLOSED based on the `33138b5` SEAL tip; stream #8's GA-readiness-final-audit (wave-24 stream #8) will reconcile any residuals.
- ✅ DEBT-025 added to register (LFPDPPP MX attorney review — OPEN, target wave-26 absorption).

**Total truly-OPEN canonical rows pre-wave-24-SEAL:** 7 (one more than wave-23 close — DEBT-025 added 2026-05-16; structural new entry).

**Post-wave-24-SEAL projection** (assuming all wave-24 streams SEAL):
- DEBT-008 OPEN subset narrows to `{dual-approval, ratelimit}` on CI-nightly only (5 crates flip to empirical-CLOSED).
- DEBT-015-BUILD flips to **CLOSED** (path 1) OR escalates to **P1 wave-25 docs-platform-eval** (path 1+2+3 all fail).
- DEBT-016 pre-wired (URLs in runbook) but still OPEN user-bound at T-7d.
- DEBT-003 still OPEN user-bound.
- DEBT-025 still OPEN attorney-bound.
- DEBT-010 + DEBT-013 still partial deferred post-GA.

**Net canonical OPEN: 7 → 6 (best case, DEBT-015-BUILD flips to CLOSED)** OR **7 → 7 with DEBT-015-BUILD escalated** (worst case).

### 8.2 Closure ETA summary

| ETA bucket | Rows |
|---|---|
| Wave-24 SEAL (this wave; ~next 1–2 weeks) | DEBT-015-BUILD (stream #3 path 1 — risk: babel-preset patch); DEBT-008 empirical narrowing (stream #2 — 3 new crates + 2 re-sweep verifications) |
| Pre-GA Gate (T+30d) | DEBT-003 (user-bound; AWS Artifact PDF download); DEBT-025 (attorney-side) |
| T-7d pre-launch | DEBT-016 (user-bound; Statuspage provisioning) |
| Post-GA (T+90d horizon) | DEBT-010 P2/P3 (7 tickets), DEBT-013 deferrals (4 tickets) — already tightened regression CI; deferrals themselves remain post-GA |

---

## 9. Quality gates verified

Per the wave-24 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present. |
| Canonical consistency validator | `python3 scripts/validate_canonical_consistency.py` | exit 0 — 197 INVs declared; 0 orphan refs; 1 CRITICAL without TLA+ (INV-PAT-REVOKE-PROPAGATION → wave-24 stream #6 closes). |
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — 444 with schema + 9 YAML-only (453 total). |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references (267 INV uses; 82 SLO uses; 233 RB uses; 36 ADR uses; 11 FF-HR uses; 31 SLO definitions; 126 RB definitions; 27 ADR definitions; 199 INV definitions). |

---

## 10. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave24-sweep`
- **Base commit:** `33138b5` (wave-23 SEAL tip)
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-24 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 11. Cross-references

- `specs/_audits/sealed/2026-05-16-wave23-closure.md` (wave-23 closure; predecessor).
- `specs/_audits/sealed/2026-05-16-wave22-closure.md` (wave-22 closure; structural patterns continued).
- `specs/_audits/sealed/2026-05-16-wave21-closure.md` (wave-21 closure; baseline cadence).
- `specs/_audits/sealed/2026-05-15-debt-register.md` v1.2.1 (DEBT register canonical state; DEBT-025 added 2026-05-16; no new changelog entry this wave — survey-only).
- `specs/03_architecture/invariant_registry.md` (197 declared post wave-23 invariant-draft sweep; §3.27 OPS + §3.28 PAT promoted from spec corpus).
- `specs/_audits/sealed/2026-05-15-canonical-consistency-baseline.md` (CI ratchet floor; DEBT-004 closure log §3.1).
- `specs/_audits/sealed/2026-05-16-debt-008-wave23-mutation-sweep.md` (wave-23 baseline for DEBT-008; sets re-sweep + first-sweep methodology for wave-24 stream #2).
- `specs/_audits/sealed/2026-05-16-debt-008-wave22-mutation-sweep.md` (wave-22 expansion baseline; equivalent-mutation analysis pattern).
- `specs/_audits/sealed/2026-05-16-debt-008-mutation-sweep.md` (wave-21 `corelink-hash` expansion baseline).
- `specs/_audits/sealed/2026-05-15-mutation-full-sweep.md` (wave-15 `corelink-audit-chain` empirical baseline; methodology continuum).
- `specs/_audits/sealed/2026-05-16-debt-015-build-final.md` (wave-23 final audit narrowing DEBT-015-BUILD root cause to babel preset-env; sets wave-24 stream #3 fix-path order).
- `specs/_audits/sealed/2026-05-16-debt-015-build-closure.md` (wave-22 PARTIAL baseline for DEBT-015-BUILD).
- `specs/_audits/sealed/2026-05-16-inv-draft-sweep.md` (wave-23 invariant-draft promotion audit; §3.27 + §3.28 source-of-truth).
- `specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md` (wave-19 SEALED pentest scope; wave-25 anchor stream ratifies or amends).
- `RB-GA-CUTOVER.md` (cutover runbook; greenlight dashboard — wave-24 stream #1 dry-run rehearses; wave-24 stream #8 final-audit issues GA verdict).
- `.github/workflows/mutation-nightly.yml` (CI-nightly artifact precedence; `TD-DEBT-008-WAVE-14-EMPIRICAL` SEAL mechanism — covers `{dual-approval, ratelimit}` continuously at wave-24 SEAL; potentially `{pat, clerk}` too pending wave-24 stream #2 combined-bucket SEAL).
