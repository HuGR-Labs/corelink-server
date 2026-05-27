# Wave-23 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-readiness rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-23 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave23-sweep`.
> **Base:** `main` @ `043428a` ("merge wt/r-prep-debt-008-mutation-wave22 into main (wave-22)" — wave-22 SEAL tip).
> **Scope:** INV registry hygiene + DEBT register survey (wave-23 in-flight — DO NOT close from this stream) + wave-23 stream catalogue + DEBT-008 wave-22→wave-23 batch state narrative + GA-readiness snapshot post wave-22 streams cataloguing + wave-24 candidate streams + GA cutover dry-run readiness assessment.
> **Cross-ref:** `specs/_audits/2026-05-16-wave22-closure.md` (predecessor), `specs/_audits/2026-05-15-debt-register.md` v1.2.0, `specs/03_architecture/invariant_registry.md`, `specs/_audits/2026-05-16-debt-008-wave22-mutation-sweep.md` (wave-22 expansion baseline).

---

## 1. Wave-23 scope — 10 streams catalogued

Wave-23 is the post-wave-22 R-PREP polish + adversarial-review wave, dispatched on `main` @ `043428a`. Ten parallel streams catalogued (this stream is #10):

| # | Stream | Branch / worktree | Disposition |
|---|---|---|---|
| 1 | **Wave-22 adversarial review** — independent codex/Opus pass over wave-22 closure streams (#1 wave-21 adversarial review absorption, #2 DEBT-008 8-crate, #3 DEBT-015-BUILD, #5 stripe matclock, #6 perf regression, #7 chaos campaign, #8 24h endurance, #9 followups + Lote-7 absorption) | `wt/r-prep-wave22-adversarial-review` (worktree `agent-wave22-review`) | **IN FLIGHT** (wave-23) |
| 2 | **DEBT-008 wave-23 mutation sweep** — continue wave-22 batch: re-sweep verification on `corelink-handler-cas` + `corelink-auth-schema` (PARTIAL post-additions, projected 100 %) + 5 new crates (`corelink-multipart-schema`, `corelink-chunker`, `corelink-r2-multipart`, `corelink-quota-cas`, `corelink-webauthn`) per DEBT register §3 row §51 wave-23 dispatch directive | `wt/r-prep-debt-008-mutation-wave23` (worktree `agent-debt-008-w23`) | **IN FLIGHT** (wave-23) |
| 3 | **DEBT-015-BUILD final closure** — resolve the post-wave-22 residual blocker (webpack server bundle clientModule chunk registry emitting literal `require("@site/docs/*.mdx")` + `require("@generated/docusaurus-plugin-content-docs/default/p/*.json")` strings that fail to externalise in pnpm-isolated layout); fix requires either docs webpack-config override forcing inline `@site/*` + `@generated/*` aliases in server bundle, or invasive `ssgRequire` patch mapping `@site/...` to compiled MDX output under `.docusaurus/docusaurus-plugin-content-docs/default/` | `wt/r-prep-debt-015-build-final` (worktree `agent-debt-015-build-w23`) | **IN FLIGHT** (wave-23) |
| 4 | **Chaos combined-failures scenarios** — extend wave-22 stream #7 chaos campaign harness (8 fail-CLOSED scenarios) with combined-failure matrix (executor-loss × replication-lag × tenant-isolation surface) per `RB-GA-CUTOVER.md` greenlight dashboard demands | `wt/r-prep-chaos-combined-failures` (worktree `agent-chaos-combined`) | **IN FLIGHT** (wave-23) |
| 5 | **Pilot onboarding E2E** — end-to-end pilot signup → DPA exchange → BYOK key bind → first audit log emit dry-run, per `RB-GA-CUTOVER.md` "pilot signups (≥ 3 design-partners)" gate | `wt/r-prep-pilot-onboarding-e2e` (worktree `agent-pilot-onboarding-e2e`) | **IN FLIGHT** (wave-23) |
| 6 | **LFPDPPP MX legal package prep** — draft attorney-ready package (residency annex + retention table + DPA addendum in Spanish) to unblock the user-bound LFPDPPP MX sign-off item gating GA | `wt/r-prep-lfpdppp-mx-legal-package` (worktree `agent-lfpdppp-mx-prep`) | **IN FLIGHT** (wave-23) |
| 7 | **Customer success playbook** — author CS playbook (onboarding ladder + health-score thresholds + escalation matrix) per beta-feedback + pilot-onboarding sister streams (#5 + #8 dependency) | `wt/r-prep-customer-success-playbook` (worktree `agent-cs-playbook`) | **IN FLIGHT** (wave-23) |
| 8 | **Beta feedback triage** — set up beta-feedback ingestion pipeline (GitHub-issue + email + status-page comment routes → triage labels + per-area routing rules); wires into CS playbook (stream #7) | `wt/r-prep-beta-feedback-triage` (worktree `agent-beta-triage`) | **IN FLIGHT** (wave-23) |
| 9 | **Wave-23 P2 cleanup** + **INV-DRAFT promotion sweep** (combined polish bucket) — wave-22 P2 followups + the per-INV DRAFT promotion pass over registry §3.X entries that meet ≥1 ref WI + ≥1 test + ≥1 spec citation gate | `wt/r-prep-wave23-p2-cleanup` (worktree `agent-wave23-cleanup`) + `wt/r-prep-inv-draft-promotion-sweep` (worktree `agent-inv-draft-sweep`) | **IN FLIGHT** (wave-23) |
| 10 | **Wave-23 INV registry sweep + DEBT register survey + wave-23 closure audit** (this stream — hygiene + cataloguing pass; survey-only) | `wt/r-prep-inv-registry-wave23-sweep` (worktree `agent-wave23-sweep`) | **CLOSED via this commit** |

Streams #1–#9 are dispatched in parallel by the orchestrator; this stream (#10) performs the hygiene + cataloguing pass against the same `043428a` base. Per the user mandate (charter §"DEBT survey (wave-23 in flight; survey-only)"), streams #1–#9 are surveyed below but **not** closed by this audit. Their SEAL commits land separately and the next wave-24 sweep reconciles. Note: stream #9 combines two worktrees (`wave23-p2-cleanup` + `inv-draft-promotion-sweep`) for the same charter-mandated "10 streams" budget — same combined-bucket pattern used in wave-22 stream #9.

---

## 2. DEBT-008 progress narrative (wave-22 + wave-23 batch state)

Per `specs/_audits/2026-05-15-debt-register.md` row §51 (DEBT-008 progressive expansion ledger) and `specs/_audits/2026-05-16-debt-008-wave22-mutation-sweep.md` (wave-22 expansion baseline). The wave-23 stream #2 batch continues the cadence established in wave-15 (`corelink-audit-chain`), wave-21 (`corelink-hash`), and wave-22 (`corelink-dedup` + `corelink-tenant-path` empirical CLOSED; `corelink-handler-cas` + `corelink-auth-schema` PARTIAL with projected 100 %).

### 2.1 DEBT-008 closure ledger (cumulative — pre-wave-23-SEAL)

| Crate | Empirical kill % | Closure status | Source wave |
|---|---|---|---|
| `corelink-audit-chain` | **84.24 %** (165 viable; 139 caught; 26 missed → +5 targeted tests; 96 % pop coverage) | **CLOSED** | wave-15 (`wt/debt-008-mutation-full-sweep-v2`) |
| `corelink-hash` | **97.22 %** (35/36; sole remainder is documented-equivalent `(hi << 4) \| lo → (hi << 4) ^ lo` on non-overlapping nibbles) | **CLOSED** | wave-21 (`wt/r-prep-debt-008-mutation-sweep` → `49b1f48`) |
| `corelink-dedup` | **92.06 %** (58/63 viable, 0 missed, 5 timeouts excluded) | **CLOSED** | wave-22 (`wt/r-prep-debt-008-mutation-wave22` → `51d082b` + `043428a`) |
| `corelink-tenant-path` | pre-additions 94.44 % (17/18; 1 timeout on `is_empty → true`) → post-additions **100.00 %** (18/18) | **CLOSED** | wave-22 (`wt/r-prep-tenant-path-uuid-fix` + `wt/r-prep-debt-008-mutation-wave22` joint) |
| `corelink-handler-cas` | pre 57.7 % → +6 targeted tests in `tests/mutation_kills.rs` mapping 1:1 to all 11 surviving mutants; projected 100 % | **PARTIAL** (re-sweep verification → wave-23 stream #2) | wave-22 + wave-23 |
| `corelink-auth-schema` | pre 72.4 % → +10 targeted tests killing all 21 surviving mutants; projected 100 % | **PARTIAL** (re-sweep verification → wave-23 stream #2) | wave-22 + wave-23 |
| `corelink-multipart-schema` (133 mutants estimated) | — | **IN FLIGHT** (wave-23 stream #2) | this wave |
| `corelink-r2-multipart` (150) | — | **IN FLIGHT** (wave-23 stream #2) | this wave |
| `corelink-chunker` (121) | — | **IN FLIGHT** (wave-23 stream #2) | this wave |
| `corelink-quota-cas` (350) | — | **IN FLIGHT** (wave-23 stream #2) | this wave |
| `corelink-webauthn` (369) | — | **IN FLIGHT** (wave-23 stream #2) | this wave |
| `corelink-pat` | CI-nightly matrix (75 % floor) | OPEN-deferred (CI artifact is SEAL gate per `TD-DEBT-008-WAVE-14-EMPIRICAL`) | nightly cron |
| `corelink-clerk` | CI-nightly matrix (75 % floor) | OPEN-deferred (same) | nightly cron |
| `corelink-dual-approval` | CI-nightly matrix (75 % floor) | OPEN-deferred (same) | nightly cron |
| `corelink-ratelimit` | CI-nightly matrix (75 % floor) | OPEN-deferred (same) | nightly cron |

### 2.2 Wave-23 stream #2 SEAL gate

The wave-23 stream #2 SEAL gate (forward-looking; not enforced by this audit) is the union of per-crate empirical baselines:

- ✅ Re-sweep verification on `corelink-handler-cas` + `corelink-auth-schema` confirms post-additions empirical kill rate matches the wave-22 projected 100 % (or documents the residual equivalent-mutation rationale matching `corelink-hash`'s precedent).
- ✅ Each of the 5 new wave-23 crates reaches ≥ 75 % empirical kill rate (CI floor) OR is escalated to a wave-24 follow-on with documented equivalent-mutation analysis.
- ✅ Each crate gets a `tests/mutation_kills.rs` targeted-test module added when missed-mutant analysis surfaces gaps.
- ✅ Per-crate JSON artefact landed at `target/mutants/<crate>.out/mutants.out/` (preserved as wave-23 audit attachments OR digested into `reports/mutation/latest.json` aggregate per `TD-DEBT-008-WAVE-14-EMPIRICAL`).
- ✅ Post-wave-23-SEAL: empirically-closed subset is `{audit-chain, hash, dedup, tenant-path, handler-cas, auth-schema}` + the 5 new wave-23 crates if they SEAL = up to **11 crates** with empirical baseline; CI-nightly continues to cover `{pat, clerk, dual-approval, ratelimit}`.

**No closure performed by this audit stream** (per charter §"DEBT survey (wave-23 in flight; survey-only)"). Verification is forward-looking; actual closure depends on stream #2 SEAL commit landing.

### 2.3 Cross-stream dependencies for wave-23

- **Stream #2 (DEBT-008) ↔ stream #1 (wave-22 adversarial review):** if codex review of wave-22 stream #2 surfaces P0/P1 findings on the wave-22 handler-cas / auth-schema PARTIAL closure rationale (e.g., the +6 / +10 targeted-test additions miss a class of mutants), wave-23 stream #2 expands scope to absorb the findings before continuing to the 5 new crates.
- **Stream #3 (DEBT-015-BUILD) ↔ wave-22 closure:** the wave-22 stream #3 left a NEW same-class residual (server-bundle `@site/*` / `@generated/*` externalisation) that requires Docusaurus-3 webpack-internals familiarity. Stream #3 this wave attempts final closure; if blocked again, DEBT-015-BUILD escalates to a P1 with an explicit alternative-architecture decision (e.g., switch to a different docs SSG entirely).

---

## 3. GA-readiness state — post wave-23 dispatch (pre-SEAL projection)

What's left blocking GA after wave-23 streams complete (cross-ref wave-22 §4):

### 3.1 User-bound items (unchanged from wave-22 §4.1)

| Item | Status | Blocker | Wave-23 unblock vector |
|---|---|---|---|
| LFPDPPP MX attorney sign-off | Pending | Mexican attorney sign-off on residency + retention; no agent can execute the sign-off itself. | Wave-23 stream #6 drafts attorney-ready package (Spanish residency annex + retention table + DPA addendum) — reduces attorney-side prep work, accelerates sign-off cycle. |
| FW-H-* role nominations | Pending | PRR dual-hat row decompositions across S-06 / S-09 / S-13 — Gustavo to onboard / nominate. | None (user-bound). |
| External pentest engagement kickoff | Scoped | Vendor + SOW pending; scope SEALED wave-19. | None (user-bound) — wave-23 has no pentest stream; awaiting wave-22 RFP-prep findings absorption (deferred to wave-24). |
| Pilot signups (≥ 3 design-partners) | Pending | Onboarding flow ready (S-19 SEALED); pilot agreements + DPA signing pending external counterparty. | Wave-23 streams #5 (E2E pilot onboarding dry-run) + #7 (CS playbook) + #8 (beta feedback triage) build the post-signup machinery; signups themselves remain user-bound. |
| AWS Artifact PDF download (DEBT-003 closure) | Pending | Human downloads + `sha256sum` to fill `TBD-on-receipt` in `BYOK-FIPS-ATTESTATION-MATRIX.md`. | None (user-bound). |
| Statuspage `status.corelink.humangr.com` go-live (DEBT-016) | Pending | Operator follows `STATUSPAGE-INIT.md` T-7d pre-launch. | None (user-bound). |
| Docs CI billing reinstatement | Pending | GitHub-billing account issue — out-of-stream resolution. | None (user-bound) — wave-23 stream #3 verifies via local `pnpm build` until billing resolves. |

### 3.2 Agent-closable, post-wave-23 SEAL — projected residual

Assuming all 9 wave-23 in-flight streams SEAL successfully:

| Residual item | Severity | Disposition |
|---|---|---|
| DEBT-008 CI-nightly subset (pat, clerk, dual-approval, ratelimit) | P1 | CI artifact is SEAL gate per `TD-DEBT-008-WAVE-14-EMPIRICAL`; nightly cron `23 5 * * *` produces empirical telemetry; no further wave-24 stream required if CI floor (75 %) holds. |
| DEBT-010 CI optimisation P2/P3 (7 tickets) | P2/P3 | Explicitly deferred post-GA per wave-21/22 §5.2. |
| DEBT-013 perf optimisation deferrals (OPT-03b, OPT-04ph2, OPT-08, OPT-03a infeasible) | P2 | Wave-22 stream #6 tightened regression CI; deferrals themselves remain post-GA. |
| Wave-22 adversarial review findings (this wave stream #1) | TBD | Findings surface during wave-23 SEAL; trigger wave-24 streams if P0/P1 surfaces. |
| Wave-23 codex Opus review (mandatory per charter) | TBD | Required for any P1-classified wave-23 stream (notably #2 DEBT-008 5-new-crate batch, #3 DEBT-015-BUILD final) before wave-24 unblocks any new P2/P3. |
| DEBT-015-BUILD final closure risk | TBD | Stream #3 webpack-internals attempt; if blocked again, escalates to architectural decision (alternative SSG). |

### 3.3 GA gate posture (cross-ref `RB-GA-CUTOVER.md` greenlight dashboard)

- **Spec corpus:** 192 INVs declared, 0 orphan, 0 CRITICAL without TLA+. ✅ GREEN (unchanged from wave-22).
- **Test/code coverage:** 103 src-referenced + 89 test-referenced; 71 forward-looking + 18 test-only by design. ✅ GREEN (unchanged from wave-22).
- **Mutation kill-rate:** {audit-chain 84.24 %, hash 97.22 %, dedup 92.06 %, tenant-path 100.00 %} empirically CLOSED above 75 % floor; handler-cas + auth-schema PARTIAL projected 100 %; 5 additional crates IN FLIGHT (wave-23 stream #2). Post-wave-23-SEAL projection: up to 11 of 15 mutation-tracked crates empirically CLOSED, 4 remain on CI-nightly matrix (75 % floor enforced via `mutation-nightly.yml` aggregate job per `TD-DEBT-008-WAVE-14-EMPIRICAL`). 🟡 YELLOW (stream #2 SEAL will narrow to GREEN-pending-CI-streak).
- **Replication SLO observation streak:** SLO §4.27–§4.29 wiring landed wave-15 DEBT-011; observation streak accumulating; wave-22 stream #8 24h endurance soak rig built; wave-23 stream #5 + #8 wire pilot + beta-feedback telemetry into the streak evidence window. ✅ GREEN.
- **Chaos campaign:** wave-22 stream #7 dispatched 8 fail-CLOSED scenarios; wave-23 stream #4 extends with combined-failure matrix. 🟡 YELLOW pending combined-failure run.
- **DEBT P0 OPEN:** 1 (DEBT-003 user-bound — unchanged). 🟡 YELLOW pending human action.
- **External pentest:** scope SEALED; engagement pending; wave-23 has no pentest stream (RFP prep deferred to wave-24). 🟡 YELLOW pending vendor + SOW.
- **DEBT-015-BUILD:** PARTIAL (wave-22 stream #3 landed (a)+(b)+(c) but introduced new same-class server-bundle residual); IN FLIGHT (wave-23 stream #3 final closure). 🟡 YELLOW pending stream #3 SEAL.
- **Pilot onboarding readiness:** wave-23 stream #5 dry-run + stream #7 CS playbook + stream #8 beta feedback triage. 🟡 YELLOW pending first external pilot signup.
- **LFPDPPP MX attorney package:** wave-23 stream #6 attorney-ready package. 🟡 YELLOW pending sign-off.

**Net GA-Limited gate readiness:** 5 user-bound items (LFPDPPP attorney sign-off, FW-H nominations, pentest engagement, AWS Artifact PDF, Statuspage go-live) + 3 wave-23 in-flight streams (#2 DEBT-008 5-crate + 2 re-sweep, #3 DEBT-015-BUILD final, #4 chaos combined-failures) + 1 wave-22 adversarial review pass (this wave stream #1) gate the GREEN cutover. No new **structural** blockers surfaced wave-23 — all wave-23 items are polish, expansion, pilot-tooling, or campaign-evidence accumulation; no new P0 design-level gaps detected.

### 3.4 GA cutover dry-run readiness assessment (NEW this wave)

Per `RB-GA-CUTOVER.md` greenlight dashboard pre-cutover dry-run checklist, the state-of-machinery for a dry-run rehearsal post-wave-23-SEAL projection:

| Dry-run prerequisite | State post-wave-23-SEAL projection | Readiness |
|---|---|---|
| Chaos campaign harness operable (8 fail-CLOSED scenarios) | wave-22 stream #7 SEAL'd | ✅ READY |
| Combined-failure matrix runnable | wave-23 stream #4 (this wave, IN FLIGHT) | 🟡 PENDING stream #4 SEAL |
| 24h endurance load rig operable | wave-22 stream #8 SEAL'd | ✅ READY |
| Pilot onboarding E2E rehearsable end-to-end | wave-23 stream #5 (this wave, IN FLIGHT) | 🟡 PENDING stream #5 SEAL |
| Beta-feedback ingestion pipeline operable | wave-23 stream #8 (this wave, IN FLIGHT) | 🟡 PENDING stream #8 SEAL |
| CS playbook documented + escalation matrix | wave-23 stream #7 (this wave, IN FLIGHT) | 🟡 PENDING stream #7 SEAL |
| Docs site builds locally (Node 22) | wave-22 stream #3 PARTIAL; wave-23 stream #3 IN FLIGHT | 🟡 PENDING stream #3 SEAL |
| INV registry stable / no orphan refs | 192 declared / 0 orphan; validators exit 0 | ✅ READY |
| DEBT register reconciled (no drift) | 6 canonical OPEN pre-wave-23-SEAL; survey-only this wave | ✅ READY (state is current) |
| Statuspage provisioned | DEBT-016 OPEN (user-bound) | ⛔ BLOCKED on user |
| LFPDPPP MX legal package drafted | wave-23 stream #6 (this wave, IN FLIGHT) | 🟡 PENDING stream #6 SEAL |
| AWS Artifact PDF download + hash | DEBT-003 OPEN (user-bound) | ⛔ BLOCKED on user |

**Net dry-run readiness:** 3 READY + 7 PENDING wave-23 stream SEAL + 2 user-bound blockers. **Dry-run feasibility:** rehearsable as a partial walkthrough now (uses chaos + endurance + INV/DEBT-register state); full end-to-end rehearsal requires wave-23 SEAL for streams #3 + #4 + #5 + #6 + #7 + #8 at minimum. Wave-24 dispatch should explicitly schedule a dry-run rehearsal stream as #1 priority once wave-23 closes; that stream produces evidence for `RB-GA-CUTOVER.md` greenlight dashboard row "GA cutover dry-run complete".

---

## 4. INV registry state (post-wave-23 sweep)

Per `python3 scripts/validate_canonical_consistency.py` on this branch (post-sweep, pre-SEAL):

| Metric | Count (wave-23) | Δ vs wave-22 close |
|---|---|---|
| INVs declared (registry §3 rows) | **192** | 0 |
| └ CRITICAL | **60** | 0 |
| └ HIGH | **128** | 0 |
| └ MEDIUM | **4** | 0 |
| └ LOW / UNKNOWN | **0** | 0 |
| Aliases declared (registry §5) | 13 | 0 |
| TLA+ verified (declared INVs proved in `specs/tla/*.tla`) | **81** | 0 |
| Code-referenced (declared INVs cited in `crates/*/src/`) | 103 | 0 |
| Test-referenced (declared INVs cited in `crates/*/tests/`) | 89 | 0 |
| Orphan refs (in code, NOT in registry+aliases) | **0** | 0 |
| CRITICAL without TLA+ proof | **0** | 0 |
| Declared with NO code/test reference | 71 | 0 |
| Declared test-only (test ref but no src/) | 18 | 0 |

Per `python3 scripts/validate_inv_promotion.py`: registry coverage **143/143** (all WI-declared INVs present in registry §3). No new canonical INVs were added by wave-22 SEAL commits — registry stable at 192 declared since wave-22 close.

**INV-DRAFT → INV-PROMOTED count this wave:** **0** (registry stable; full coverage maintained; no DRAFT-staging entries found in registry §3 — the doc-level `doc_status: DRAFT` in the registry front-matter is staffing-blocked per F-09 audit until ≥ 2 reviewers nominated, separate from any per-entry status; the sibling wave-23 stream `wt/r-prep-inv-draft-promotion-sweep` would surface DRAFT entries if any exist).

### 4.1 Why no promotions this wave

Wave-22 closures landed:
- DEBT-008 wave-22 expansion (4 crates touched: dedup empirical CLOSED, tenant-path empirical CLOSED, handler-cas + auth-schema PARTIAL) — no new INV identifiers (mutation kill-rate is a meta-metric, not an INV).
- DEBT-015-BUILD wave-22 PARTIAL (Docusaurus build progress) — no INV impact.
- Stripe wasm32 MatClock trait expansion — re-uses existing canonical `INV-CLOCK-MONOTONIC-WITHIN-REQUEST` invariant.
- Perf regression CI tightening — no INV impact (CI gate only).
- Chaos campaign harness (8 fail-CLOSED scenarios) — re-uses existing fail-CLOSED invariants (`INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` family) in scenario assertions; no new INVs.
- 24h endurance load harness — re-uses existing SLO IDs (§4.27–§4.29) and existing INVs; no new INVs.
- Wave-21 adversarial review — review-pass, no INV authoring.
- W21-FOLLOWUP-01/02/03 cosmetic absorption — doc polish only; no INV impact.
- Lote 7 framework reviewer roles addendum — governance docs only; no INV impact.

No structural-new INVs added wave-22 → no DRAFT-staging intermediates created → 0 promotions warranted this wave. Same pattern as wave-22 close.

---

## 5. DEBT register survey — pre-wave-23-SEAL state

Per `specs/_audits/2026-05-15-debt-register.md` v1.2.0 (last reconciled in wave-22 close). No DEBT closures performed by this stream (charter-bound survey-only). The state below reflects post-wave-22 canonical OPEN rows; wave-23 in-flight streams have **not yet** flipped to CLOSED in the register.

### 5.1 Open count + per-priority breakdown (canonical rows; pre-wave-23-SEAL)

| Priority | Open IDs | Count | Wave-23 closure ETA |
|---|---|---|---|
| **P0** | DEBT-003 (AWS Artifact PDF — user-bound) | 1 | Pending human (no wave-23 stream) |
| **P1** | DEBT-008 (partial — 4 CI-nightly crates remain; 2 PARTIAL projected closed; 5 in flight via wave-23 stream #2) | 1 partial | Stream #2 SEAL → empirical subset grows 4 → up to 11; PARTIAL re-sweep verifies handler-cas + auth-schema |
| **P1** | DEBT-010 (partial 4/11 — 7 P2/P3 deferred post-GA) | 1 partial | Deferred post-GA |
| **P1** | DEBT-013 (partial 6/10 — 4 explicit deferrals) | 1 partial | Wave-22 stream #6 tightened regression CI; deferrals themselves remain post-GA |
| **P2** | DEBT-015-BUILD (build-side addendum — wave-22 PARTIAL + new server-bundle residual) | 1 | **Stream #3** ETA wave-23 SEAL (final closure attempt) |
| **P2** | DEBT-016 (Statuspage go-live — user-bound) | 1 | Pending human (no wave-23 stream) |

**Post-wave-22-SEAL closures actually landed** (verified against `git log` commits between `bccdd97` and `043428a`):
- ✅ DEBT-008 wave-22 expansion (4 crates: dedup empirical CLOSED, tenant-path empirical CLOSED, handler-cas + auth-schema PARTIAL with +16 targeted tests). Audits: `specs/_audits/2026-05-16-debt-008-wave22-mutation-sweep.md`. Commits `51d082b` + `d7b9eea` + merge `043428a` + `c2fe3aa`.
- ✅ DEBT-015-BUILD wave-22 PARTIAL (`wt/r-prep-debt-015-build-closure` → `f356985`): blockers (a) `draft: true` removal + (b) MDX extensionless cross-links + (c) theme-alias patch APPLIED; new same-class residual (server-bundle `@site/*` / `@generated/*` externalisation) carried forward to wave-23 stream #3.
- ✅ Wave-21 adversarial review (wave-22 stream #1) — CLOSED `caaa537` (9.55/10 PASS).
- ✅ Stripe MatClock wasm32 follow-on — CLOSED `dfe394e` + merge `a7b5927`.
- ✅ Wave-22 24h endurance load harness — CLOSED `4d37030` + merge `b069b20`.
- ✅ Wave-22 chaos engineering campaign harness (8 fail-CLOSED scenarios) — CLOSED `39c98fc` + merge `be00f38`.
- ✅ Wave-22 perf regression CI tightening — CLOSED `6494d9c` + merge `32ba038`.
- ✅ Wave-22 W21-FOLLOWUP-01/02/03 cosmetic absorption — CLOSED `b64a156` + merge `01c0d47`.
- ✅ Wave-22 Lote 7 framework reviewer roles addendum — CLOSED `439eb52` + merge `e2c5579`.
- ✅ Wave-22 closure audit (this stream's predecessor) — CLOSED `db94a55` + merge `68ec153`.

**Total truly-OPEN canonical rows pre-wave-23-SEAL:** 6 (same count as wave-22 close — DEBT-015-BUILD remains OPEN due to new residual; DEBT-008 narrows empirical subset but remains structurally OPEN until all CI-nightly crates have empirical baseline).

**Post-wave-23-SEAL projection** (assuming streams #2 + #3 SEAL): DEBT-008 OPEN subset narrows to `{pat, clerk, dual-approval, ratelimit}` on CI-nightly + 5 new wave-23 crates either CLOSED or escalated; DEBT-015-BUILD flips to CLOSED if stream #3 succeeds, else escalates to P1 with alternative-architecture decision. Net canonical OPEN goes from 6 → 4 (best case) or 6 → 5 (DEBT-015-BUILD escalates).

### 5.2 Closure ETA summary

| ETA bucket | Rows |
|---|---|
| Wave-23 SEAL (next 1–2 weeks) | DEBT-015-BUILD (stream #3 — risk: webpack-internals); DEBT-008 5-new-crate + 2 re-sweep (stream #2 — narrows OPEN subset) |
| Pre-GA Gate (T+30d) | DEBT-003 (user-bound; AWS Artifact PDF download) |
| T-7d pre-launch | DEBT-016 (user-bound; Statuspage provisioning) |
| Post-GA (T+90d horizon) | DEBT-010 P2/P3 (7 tickets), DEBT-013 deferrals (4 tickets) — wave-22 stream #6 already tightened regression CI; deferrals themselves remain post-GA |

---

## 6. Wave-24 candidate streams + caveats from wave-23 findings

Recommended wave-24 dispatch list (ordered by GA-blocker severity, contingent on wave-23 SEAL):

| # | Stream | Rationale | Estimated cost |
|---|---|---|---|
| 1 | **GA cutover dry-run rehearsal** (NEW priority) | Per §3.4 dry-run readiness assessment, post-wave-23-SEAL the machinery (chaos + endurance + pilot E2E + CS playbook + beta-feedback + INV/DEBT register state) becomes fully operable. Wave-24 stream #1 executes the end-to-end dry-run rehearsal and produces evidence for `RB-GA-CUTOVER.md` greenlight dashboard row "GA cutover dry-run complete". | 1 Sonnet × ≤4h rehearsal + evidence absorption. |
| 2 | **Wave-23 adversarial review (codex Opus pass)** | Mandatory per charter §"all P1-classified streams must close before next wave unblocks P2/P3". Cross-review wave-23 streams #2 (DEBT-008 5-new-crate + 2 re-sweep), #3 (DEBT-015-BUILD final closure), #4 (chaos combined-failures), #5 (pilot onboarding E2E), #6 (LFPDPPP MX legal package). | ~1 codex Opus pass + 1 audit doc per stream. |
| 3 | **Wave-22 stream-#1 adversarial findings absorption** | If wave-23 stream #1 (wave-22 adversarial review) surfaces P0/P1 findings on wave-22 closure streams (notably #2 mutation, #3 DEBT-015-BUILD, #7 chaos), dispatch fix agents. | Branch-per-finding; size depends on findings. |
| 4 | **DEBT-008 wave-24 follow-on for any crate not reaching ≥ 75 %** | Crates from wave-23 stream #2 batch that hit < 75 % empirical kill rate need targeted-test additions OR documented equivalent-mutation rationale. | 1 Sonnet per remediation. |
| 5 | **Chaos combined-failures findings absorption** | Wave-23 stream #4 surfaces findings on executor-loss × replication-lag × tenant-isolation surface → dispatch fix agents per finding. | Branch-per-finding. |
| 6 | **24h endurance soak run execution** | Wave-22 stream #8 built the rig; wave-23 didn't run the soak; wave-24 executes the actual 24h soak + captures SLO streak evidence into `RB-GA-CUTOVER.md` greenlight dashboard. | 24h wall-clock + 1 Sonnet for evidence absorption. |
| 7 | **Pentest RFP + vendor shortlist + SOW template** (deferred from wave-22 §7) | User-bound for final vendor selection; agent can draft RFP. | 1 Sonnet × draft RFP + shortlist + SOW template. |
| 8 | **DEBT-010 P2 CI optimisation batch** (concurrency cancel + shared rust-cache key + TLC matrix + paths-filter audit) | Post-GA polish; ~30 min/PR cumulative savings. | 1 Sonnet × 4 P2 tickets. |
| 9 | **DEBT-013 perf optimisation deferrals execution** (OPT-03b + OPT-04ph2 + OPT-08) | Post-GA polish; wave-22 stream #6 already tightened regression CI. | 1 Sonnet × 3 deferrals. |
| 10 | **Wave-24 INV registry + DEBT register hygiene sweep** | Cadence preserved — same charter as wave-19/20/21/22/23 sweep streams. | 1 Sonnet × 30 min. |

### 6.1 Caveats from wave-23 sweep findings

- **No INV registry drift detected this wave.** Registry stable at 192 declared / 143 WI-coverage; no DRAFT promotion warranted from this audit stream (sibling `inv-draft-promotion-sweep` may surface DRAFT entries independently).
- **No DEBT register drift detected this wave.** 6 canonical OPEN rows pre-SEAL; wave-23 streams #2 + #3 in flight will reduce OPEN to 4 or 5 post-SEAL depending on DEBT-015-BUILD final-closure outcome.
- **DEBT-008 wave-22 PARTIAL re-sweep dependency:** wave-23 stream #2 must re-sweep `corelink-handler-cas` + `corelink-auth-schema` BEFORE tackling the 5 new crates. If re-sweep fails to confirm projected 100 %, the targeted-test additions need root-cause analysis (not just delta additions).
- **DEBT-015-BUILD escalation risk:** wave-22 stream #3 hit a new same-class residual (server-bundle externalisation). Wave-23 stream #3 attempts final closure; if blocked again, the row escalates to **P1** with an explicit alternative-architecture decision (e.g., switch from Docusaurus 3 to a different SSG, or invasive runtime patch). Surface this risk in `RB-GA-CUTOVER.md` greenlight dashboard as a watch-item.
- **Wave-22 adversarial review (this wave stream #1) is the bound resource** — codex Opus is the only model capable of independent SOTA-bar review (wave-20 = 9.40/10, wave-21 = 9.55/10). Wave-23 stream #1 requires 1 codex Opus invocation per wave-22 P1 stream (4 minimum: #2 mutation, #3 DEBT-015-BUILD, #7 chaos, #8 endurance).
- **No new structural GA-blocker surfaced wave-23.** All 9 in-flight streams are polish, expansion, pilot-tooling, attorney-package prep, or campaign-evidence-accumulation — none reveal previously-unknown design gaps.
- **GA dry-run dispatch warranted as wave-24 #1.** Post-wave-23-SEAL the machinery is operable end-to-end; failing to rehearse risks discovering integration gaps at actual cutover. The dry-run dispatch should be the #1 priority of wave-24 ahead of even the adversarial review pass.

### 6.2 Wave-24 entry caveats

- **DCO + Co-Authored-By preserved** on every commit (same as wave-19 through wave-23).
- **No `--no-verify` hooks.** Pre-commit failures must surface root cause.
- **Synchronous Bash only.** No `run_in_background`. Same charter as wave-19/20/21/22/23.
- **30–40-min time budget per stream** (matches wave-21/22/23 cadence); 8–10 streams per wave realistic.
- **Wave-24 SEAL gate:** all wave-23 P1 streams (#2 DEBT-008 5-new-crate + 2 re-sweep, #3 DEBT-015-BUILD final, #4 chaos combined-failures) verified closed before wave-24 unblocks any new P2/P3 streams.
- **GA cutover dry-run rehearsal becomes the wave-24 anchor stream** — its findings drive wave-25 dispatch priority.

---

## 7. Quality gates verified

Per the wave-23 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present. |
| Canonical consistency validator | `python3 scripts/validate_canonical_consistency.py` | exit 0 — 192 INVs declared, 0 orphan refs, 0 CRITICAL without TLA+. |
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — 443 with schema + 9 YAML-only (452 total). |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references (264 INV uses; 82 SLO uses; 232 RB uses; 36 ADR uses; 11 FF-HR uses). |

---

## 8. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave23-sweep`
- **Base commit:** `043428a` (wave-22 SEAL tip)
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-23 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 9. Cross-references

- `specs/_audits/2026-05-16-wave22-closure.md` (wave-22 closure; predecessor).
- `specs/_audits/2026-05-16-wave21-closure.md` (wave-21 closure; structural patterns continued).
- `specs/_audits/2026-05-16-wave20-closure.md` (wave-20 closure; baseline cadence).
- `specs/_audits/2026-05-15-debt-register.md` v1.2.0 (DEBT register canonical state; no new changelog entry this wave — survey-only).
- `specs/03_architecture/invariant_registry.md` (stable wave-23 — full coverage maintained at 192 declared / 143 WI-coverage).
- `specs/_audits/2026-05-15-canonical-consistency-baseline.md` (CI ratchet floor; DEBT-004 closure log §3.1).
- `specs/_audits/2026-05-16-debt-008-wave22-mutation-sweep.md` (wave-22 expansion baseline for DEBT-008; sets equivalent-mutation analysis pattern for wave-23 stream #2).
- `specs/_audits/2026-05-16-debt-008-mutation-sweep.md` (wave-21 `corelink-hash` expansion baseline; §11 wave-22 narrative addendum).
- `specs/_audits/2026-05-15-mutation-full-sweep.md` (wave-15 `corelink-audit-chain` empirical baseline; sets methodology continuum).
- `RB-GA-CUTOVER.md` (cutover runbook; greenlight dashboard — dry-run readiness assessment §3.4 anchors wave-24 #1 priority).
- `specs/_audits/2026-05-16-pre-ga-pentest-scope.md` (pentest scope; engagement checklist — RFP prep deferred to wave-24).
- `.github/workflows/mutation-nightly.yml` (CI-nightly artifact precedence; `TD-DEBT-008-WAVE-14-EMPIRICAL` SEAL mechanism — covers `{pat, clerk, dual-approval, ratelimit}` continuously).
