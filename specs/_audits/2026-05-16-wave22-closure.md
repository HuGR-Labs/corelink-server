# Wave-22 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-readiness rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-22 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave22-sweep`.
> **Base:** `main` @ `bccdd97` ("merge wt/r-prep-tenant-config-region-resolver into main (wave-21)" — wave-21 SEAL tip).
> **Scope:** INV registry hygiene + DEBT register survey (wave-22 in-flight — DO NOT close from this stream) + wave-22 stream catalogue + DEBT-008 wave-22 8-crate batch state + DEBT-015-BUILD closure verification + GA-readiness snapshot post wave-21 streams cataloguing + wave-23 candidate streams.
> **Cross-ref:** `specs/_audits/2026-05-16-wave21-closure.md` (predecessor), `specs/_audits/2026-05-15-debt-register.md` v1.2.0, `specs/03_architecture/invariant_registry.md`, `specs/_audits/2026-05-16-debt-008-mutation-sweep.md` (wave-21 expansion baseline).

---

## 1. Wave-22 scope — 10 streams catalogued

Wave-22 is the post-wave-21 R-PREP hardening wave, dispatched on `main` @ `bccdd97`. Ten parallel streams catalogued (this stream is #10):

| # | Stream | Branch / worktree | Disposition |
|---|---|---|---|
| 1 | **Wave-21 adversarial review** — independent codex/Opus pass over wave-21 closure streams (#1 RLS WITH CHECK, #3 TLA FT-7 CRITICAL, #2 mutation expansion, #4 Node 22 ESM, plus #5–#8 secondary streams) | `wt/r-prep-wave21-adversarial-review` (worktree `agent-wave21-review`) | **IN FLIGHT** (wave-22) |
| 2 | **DEBT-008 wave-22 8-crate mutation sweep** — empirical mutation testing across `corelink-multipart-schema`, `corelink-r2-multipart`, `corelink-handler-cas`, `corelink-quota-cas`, `corelink-dedup`, `corelink-chunker`, `corelink-auth-schema`, `corelink-webauthn` (per DEBT register §3 row §51 wave-22 dispatch directive) | `wt/r-prep-debt-008-mutation-wave22` (worktree `agent-debt-008-w22`) | **IN FLIGHT** (wave-22) |
| 3 | **DEBT-015-BUILD closure** — close the inline build-side addendum on DEBT-015 (unstub `draft: true` security/residency pages, normalise MDX cross-link extensions, re-verify Node 22 `pnpm build`; apply `prism-include-languages` theme-alias rewrite to `patches/@docusaurus__core@3.10.1.patch` if it resurfaces) | `wt/r-prep-debt-015-build-closure` (worktree `agent-debt-015-build`) | **IN FLIGHT** (wave-22) |
| 4 | **Tenant-path UUID compile fix** — `Uuid::now_v7` compile error on `corelink-tenant-path` blocking DEBT-008 mutation sweep expansion (per DEBT register §3 row §51 caveat "after `Uuid::now_v7` compile fix") | `wt/r-prep-tenant-path-uuid-fix` (worktree `agent-tenant-path-uuid-fix`) | **IN FLIGHT** (wave-22) |
| 5 | **Stripe matclock trait expansion** — wallclock trait coverage on stripe wasm32 surface continuing wave-20 `wt/r-prep-stripe-wasm32-clock-trait` + wave-21 wallclock cross-route unification (`4303576`) | `wt/r-prep-stripe-matclock-trait` (worktree `agent-stripe-matclock`) | **IN FLIGHT** (wave-22) |
| 6 | **Perf regression CI tighten** — tighten the perf regression CI gating per DEBT-013 perf-followup-tickets.md; reduce false-positive flake rate on p99 tail-latency criteria | `wt/r-prep-perf-regression-ci-tighten` (worktree `agent-perf-regression-ci`) | **IN FLIGHT** (wave-22) |
| 7 | **Chaos campaign dispatch** — pre-GA chaos campaign per `RB-GA-CUTOVER.md` greenlight dashboard (executor failure-injection matrix; tail-latency under partition; replication SLO breach detection) | `wt/r-prep-chaos-campaign` (worktree `agent-chaos-campaign`) | **IN FLIGHT** (wave-22) |
| 8 | **24h endurance load setup** — 24h endurance soak rig setup for pre-GA SLO observation streak validation (k6 + Grafana dashboards + replication-lag panels) | `wt/r-prep-24h-endurance-load-setup` (worktree `agent-24h-endurance`) | **IN FLIGHT** (wave-22) |
| 9 | **Wave-21 followups + Lote-7 framework absorption** (combined polish bucket) — wave-21 P2 followups + lote-7 framework reviewer staffing absorption + secondary doc-fix backlog | `wt/r-prep-w21-followups-p2` (worktree `agent-w21-followups`) + `wt/r-prep-lote-7-framework-absorption` (worktree `agent-lote-7-absorption`) | **IN FLIGHT** (wave-22) |
| 10 | **Wave-22 INV registry sweep + DEBT register survey + wave-22 closure audit** (this stream — hygiene + cataloguing pass; DEBT-008 expanded closure verification + DEBT-015-BUILD closure verification) | `wt/r-prep-inv-registry-wave22-sweep` (worktree `agent-wave22-sweep`) | **CLOSED via this commit** |

Streams #1–#9 are dispatched in parallel by the orchestrator; this stream (#10) performs the hygiene + cataloguing pass against the same `bccdd97` base. Per the user mandate (charter §"DEBT survey (wave-22 in flight; DO NOT close from this stream)"), streams #1–#9 are surveyed below but **not** closed by this audit. Their SEAL commits land separately and the next wave-23 sweep reconciles.

---

## 2. DEBT-008 wave-22 expanded closure — verification

Per `specs/_audits/2026-05-15-debt-register.md` row §51 (DEBT-008 wave-22 dispatch directive), the wave-22 stream #2 batch targets eight additional crates beyond the wave-21 `corelink-hash` closure (which itself reached **97.22 %** empirical kill rate per `specs/_audits/2026-05-16-debt-008-mutation-sweep.md`).

### 2.1 DEBT-008 closure ledger (cumulative — pre-wave-22-SEAL)

| Crate | Empirical kill % | Closure status | Source |
|---|---|---|---|
| `corelink-audit-chain` | 84.24 % (165 viable; 139 caught; 26 missed → +5 targeted tests; 96 % pop coverage) | **CLOSED** (wave-15, `wt/debt-008-mutation-full-sweep-v2`) | `specs/_audits/2026-05-15-mutation-full-sweep.md` |
| `corelink-hash` | 97.22 % (35/36; sole remainder is documented-equivalent `(hi << 4) \| lo → (hi << 4) ^ lo` on non-overlapping nibbles) | **CLOSED** (wave-21, `wt/r-prep-debt-008-mutation-sweep` → `49b1f48`) | `specs/_audits/2026-05-16-debt-008-mutation-sweep.md` |
| `corelink-pat` | CI-nightly matrix (75 % floor) | OPEN-deferred (CI artifact is SEAL gate per `TD-DEBT-008-WAVE-14-EMPIRICAL`) | `.github/workflows/mutation-nightly.yml` |
| `corelink-clerk` | CI-nightly matrix (75 % floor) | OPEN-deferred (CI artifact is SEAL gate) | same |
| `corelink-dual-approval` | CI-nightly matrix (75 % floor) | OPEN-deferred (CI artifact is SEAL gate) | same |
| `corelink-ratelimit` | CI-nightly matrix (75 % floor) | OPEN-deferred (CI artifact is SEAL gate) | same |
| `corelink-multipart-schema` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |
| `corelink-tenant-path` | — (gated on `Uuid::now_v7` compile fix from wave-22 stream #4) | **IN FLIGHT** (wave-22 streams #2 + #4) | this stream |
| `corelink-r2-multipart` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |
| `corelink-handler-cas` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |
| `corelink-quota-cas` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |
| `corelink-dedup` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |
| `corelink-chunker` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |
| `corelink-auth-schema` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |
| `corelink-webauthn` | — | **IN FLIGHT** (wave-22 stream #2) | this stream |

### 2.2 DEBT-008 expanded closure — wave-22 SEAL gate

The wave-22 stream #2 SEAL gate is the union of per-crate empirical baselines:

- ✅ Each of the 8 wave-22 crates reaches ≥ 75 % empirical kill rate (CI floor) OR is escalated to a wave-23 follow-on with documented equivalent-mutation analysis (matching `corelink-hash`'s `(hi << 4) \| lo` rationale).
- ✅ Each crate gets a `tests/mutation_kills.rs` targeted-test module added when missed-mutant analysis surfaces gaps.
- ✅ Per-crate JSON artefact landed at `target/mutants/<crate>.out/mutants.out/` (preserved as wave-22 audit attachments OR digested into `reports/mutation/latest.json` aggregate per `TD-DEBT-008-WAVE-14-EMPIRICAL`).
- ✅ Post-wave-22-SEAL: empirically-closed subset is `{audit-chain, hash, multipart-schema, tenant-path, r2-multipart, handler-cas, quota-cas, dedup, chunker, auth-schema, webauthn}` (= 11 crates with empirical baseline; CI-nightly continues to cover {pat, clerk, dual-approval, ratelimit}).

**No closure performed by this audit stream** (per charter §"DEBT survey... DO NOT close from this stream"). Verification is forward-looking; actual closure depends on stream #2 SEAL commit landing.

### 2.3 Pre-condition: tenant-path UUID compile fix (wave-22 stream #4)

The `corelink-tenant-path` mutation sweep is blocked on the `Uuid::now_v7` compile error flagged in DEBT register row §51. Wave-22 stream #4 (`wt/r-prep-tenant-path-uuid-fix`) targets this fix; stream #2 absorbs the tenant-path mutation sweep once #4 SEALs. Cross-stream dependency tracked here so the next wave's hygiene pass reconciles correctly.

---

## 3. DEBT-015-BUILD closure verification

Per `specs/_audits/2026-05-15-debt-register.md` row §71 — DEBT-015 P2 docs portion CLOSED in wave-21 (`wt/r-prep-debt-015-node22-esm` → `8ab8786`); the build-side residual is carried as an inline addendum **DEBT-015-BUILD**, with closure gate:

> Build-side CLOSED gate is green `pnpm build` on Node 22 with engine pin honoured.

### 3.1 Pre-condition checklist (per row §71)

| Pre-condition | Status (pre-wave-22-SEAL) | Closure stream |
|---|---|---|
| Unstub or remove `draft: true` from referenced security/residency pages (`explanation/security/audit-chain.mdx`, `explanation/security/byok.mdx`, `explanation/residency/lgpd-brazil.mdx`) | IN FLIGHT (stream #3) | `wt/r-prep-debt-015-build-closure` |
| Normalise MDX cross-link extensions to extensionless form | IN FLIGHT (stream #3) | same |
| Re-run `pnpm build` on Node 22 (engine pin `>=22.0.0 <23.0.0` already lifted wave-21) | IN FLIGHT (stream #3) | same |
| Theme-alias rewrite on `patches/@docusaurus__core@3.10.1.patch` IF `prism-include-languages` resurfaces | Contingent on previous step output | same |
| Docs CI billing block (GitHub-billing account issue) | External — out-of-stream | Operator |

### 3.2 Wave-22 stream #3 SEAL gate (DEBT-015-BUILD closure verification criteria)

The wave-22 stream #3 closure verification (to be performed by the wave-23 hygiene sweep) requires:

- ✅ `pnpm --filter @corelink/docs build` exits 0 on Node 22.x with `--config.engine-strict=true`.
- ✅ Zero `draft: true`-induced broken-link errors.
- ✅ All MDX cross-links extensionless (auditable via `rg "\(\.+\/[^)]+\.mdx?\)" apps/docs/docs/`).
- ✅ Patch file `patches/@docusaurus__core@3.10.1.patch` either unmodified OR includes documented theme-alias rewrite with cross-ref to upstream Docusaurus issue.
- ✅ Engine pin `apps/docs/package.json#engines.node` remains at `>=22.0.0 <23.0.0`.

**No closure performed by this audit stream** (per charter §"DO NOT close from this stream"). DEBT-015-BUILD remains OPEN-IN-FLIGHT pending stream #3 SEAL.

---

## 4. GA-readiness state — post wave-22 dispatch (pre-SEAL projection)

What's left blocking GA after wave-22 streams complete (cross-ref wave-21 §5):

### 4.1 User-bound items (unchanged from wave-21 §5.1)

| Item | Status | Blocker |
|---|---|---|
| LFPDPPP MX attorney sign-off | Pending | Mexican attorney sign-off on residency + retention; no agent can execute. |
| FW-H-* role nominations | Pending | PRR dual-hat row decompositions across S-06 / S-09 / S-13 — Gustavo to onboard / nominate. |
| External pentest engagement kickoff | Scoped | Vendor + SOW pending; scope SEALED wave-19. |
| Pilot signups (≥ 3 design-partners) | Pending | Onboarding flow ready (S-19 SEALED); pilot agreements + DPA signing pending external counterparty. |
| AWS Artifact PDF download (DEBT-003 closure) | Pending | Human downloads + `sha256sum` to fill `TBD-on-receipt` in `BYOK-FIPS-ATTESTATION-MATRIX.md`. |
| Statuspage `status.corelink.dev` go-live (DEBT-016) | Pending | Operator follows `STATUSPAGE-INIT.md` T-7d pre-launch. |
| Docs CI billing reinstatement | Pending | GitHub-billing account issue — out-of-stream resolution. |

### 4.2 Agent-closable, post-wave-22 SEAL — projected residual

Assuming all 9 wave-22 in-flight streams SEAL successfully:

| Residual item | Severity | Disposition |
|---|---|---|
| DEBT-008 CI-nightly subset (pat, clerk, dual-approval, ratelimit) | P1 | CI artifact is SEAL gate per `TD-DEBT-008-WAVE-14-EMPIRICAL`; nightly cron `23 5 * * *` produces empirical telemetry; no further wave-23 stream required if CI floor (75 %) holds. |
| DEBT-010 CI optimisation P2/P3 (7 tickets) | P2/P3 | Explicitly deferred post-GA per wave-21 §5.2. |
| DEBT-013 perf optimisation deferrals (OPT-03b, OPT-04ph2, OPT-08, OPT-03a infeasible) | P2 | Wave-22 stream #6 tightens regression CI; deferrals themselves remain post-GA. |
| Wave-21 adversarial review findings (stream #1) | TBD | Findings surface during wave-22 SEAL; trigger wave-23 streams if P0/P1 surfaces. |
| Wave-22 codex Opus review (mandatory per charter) | TBD | Required for any P1-classified wave-22 stream before wave-23 unblocks any new P2/P3. |

### 4.3 GA gate posture (cross-ref `RB-GA-CUTOVER.md` greenlight dashboard)

- **Spec corpus:** 192 INVs declared, 0 orphan, 0 CRITICAL without TLA+. ✅ GREEN.
- **Test/code coverage:** 103 src-referenced + 89 test-referenced; 71 forward-looking + 18 test-only by design. ✅ GREEN.
- **Mutation kill-rate:** {audit-chain 84.24 %, hash 97.22 %} empirically CLOSED above 75 % floor; 8 additional crates IN FLIGHT (stream #2). Post-wave-22-SEAL projection: 10 of 14 mutation-tracked crates empirically CLOSED, 4 remain on CI-nightly matrix (75 % floor enforced via `mutation-nightly.yml` aggregate job per `TD-DEBT-008-WAVE-14-EMPIRICAL`). 🟡 YELLOW (stream #2 will green to GREEN-pending-CI-streak).
- **Replication SLO observation streak:** SLO §4.27–§4.29 wiring landed wave-15 DEBT-011; observation streak accumulating; wave-22 stream #8 (24h endurance) extends evidence window. ✅ GREEN.
- **Chaos campaign:** dispatched wave-22 stream #7. 🟡 YELLOW pending campaign run.
- **DEBT P0 OPEN:** 1 (DEBT-003 user-bound). 🟡 YELLOW pending human action.
- **External pentest:** scope SEALED; engagement pending. 🟡 YELLOW pending vendor + SOW.
- **DEBT-015-BUILD:** IN FLIGHT (stream #3). 🟡 YELLOW pending stream #3 SEAL.

**Net GA-Limited gate readiness:** 3 user-bound items (LFPDPPP, FW-H nominations, AWS Artifact PDF) + 3 wave-22 in-flight streams (#2 mutation 8-crate, #3 DEBT-015-BUILD, #7 chaos campaign) + 1 wave-21 adversarial review pass (stream #1) gate the GREEN cutover. No new **structural** blockers surfaced wave-22 — all wave-22 items are either polish, expansion, or campaign-run evidence accumulation; no new P0 design-level gaps detected.

---

## 5. INV registry state (post-wave-22 sweep)

Per `python3 scripts/validate_canonical_consistency.py` on this branch (post-sweep, pre-SEAL):

| Metric | Count (wave-22) | Δ vs wave-21 close |
|---|---|---|
| INVs declared (registry §3 rows) | **192** | +1 (wave-21 commits introduced 1 new canonical INV) |
| └ CRITICAL | **60** | 0 |
| └ HIGH | **128** | +1 |
| └ MEDIUM | **4** | 0 |
| └ LOW / UNKNOWN | **0** | 0 |
| Aliases declared (registry §5) | 13 | 0 |
| TLA+ verified (declared INVs proved in `specs/tla/*.tla`) | **81** | +5 (DEBT-014 FT-6/FT-7/FT-8/FT-9 TLA+ followups SEALed `437d3f1`) |
| Code-referenced (declared INVs cited in `crates/*/src/`) | 103 | 0 |
| Test-referenced (declared INVs cited in `crates/*/tests/`) | 89 | 0 |
| Orphan refs (in code, NOT in registry+aliases) | **0** | 0 |
| CRITICAL without TLA+ proof | **0** | 0 |
| Declared with NO code/test reference | 71 | +1 (matches new declared INV) |
| Declared test-only (test ref but no src/) | 18 | 0 |

Per `python3 scripts/validate_inv_promotion.py`: registry coverage **143/143** (all WI-declared INVs present in registry §3). No INV DRAFT entries with sufficient coverage surfaced for promotion in this sweep window (wave-21 commits added 1 canonical INV directly to registry §3 — landed via `wt/r-prep-debt-014-tla-specs` TLA spec coverage extension, no DRAFT staging intermediate).

**INV-DRAFT → INV-PROMOTED count this wave:** **0** (registry stable; full coverage maintained; +1 canonical addition landed via wave-21 stream directly without DRAFT staging).

### 5.1 Why no promotions this wave

Wave-21 closures landed:
- 4 TLA+ specs (FT-6/7/8/9) via `wt/r-prep-debt-014-tla-specs` → registry §4 TLA-verified count +5 (FT-7 covered two existing CRITICAL invariants newly proved).
- Audit-export emit-discipline (wave-20 rolled-forward) + Neon shadow RLS WITH CHECK + tenant config region-resolver + wallclock cross-route unification — all re-using existing canonical INVs (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER, INV-AUTH-SCHEMA-RLS-DEFAULT-ON, INV-AUDIT-CHAIN-EMIT-ORDERED, INV-CLOCK-MONOTONIC-WITHIN-REQUEST).
- DEBT-008 wave-21 expansion (`corelink-hash`) — no new INV identifiers (mutation kill-rate is a meta-metric, not an INV).
- Secrets-X false-positive fix — tooling-only; no INV impact.

The +1 declared INV addition is structural (registry §3 row added in `wt/r-prep-debt-014-tla-specs` audit doc trail, matching TLA spec extension) rather than DRAFT-promoted; full coverage maintained at 143/143.

---

## 6. DEBT register survey — pre-wave-22-SEAL state

Per `specs/_audits/2026-05-15-debt-register.md` v1.2.0 (last reconciled in wave-21 close). No DEBT closures performed by this stream (charter-bound survey-only). The state below reflects post-wave-21 canonical OPEN rows; wave-22 in-flight streams have **not yet** flipped to CLOSED in the register.

### 6.1 Open count + per-priority breakdown (canonical rows; pre-wave-22-SEAL)

| Priority | Open IDs | Count | Wave-22 closure ETA |
|---|---|---|---|
| **P0** | DEBT-003 (AWS Artifact PDF — user-bound) | 1 | Pending human (no wave-22 stream) |
| **P1** | DEBT-008 (partial — 4 CI-nightly crates remain; 8 in flight via stream #2) | 1 partial | Stream #2 SEAL → empirical subset grows 2 → 10 |
| **P1** | DEBT-010 (partial 4/11 — 7 P2/P3 deferred post-GA) | 1 partial | Deferred post-GA |
| **P1** | DEBT-013 (partial 6/10 — 4 explicit deferrals) | 1 partial | Stream #6 tightens regression CI (does not close deferrals) |
| **P2** | DEBT-015-BUILD (build-side addendum on DEBT-015) | 1 | **Stream #3** ETA wave-22 SEAL |
| **P2** | DEBT-016 (Statuspage go-live — user-bound) | 1 | Pending human (no wave-22 stream) |

**Post-wave-21-SEAL closures actually landed** (verified against `git log` commits between `30e5f66` and `bccdd97`):
- ✅ DEBT-014 FT-6/FT-7/FT-8/FT-9 — fully CLOSED `437d3f1` (wave-21 stream #3).
- ✅ DEBT-015 P2 docs portion — CLOSED `8ab8786` (wave-21 stream #4); build-side residual carried as DEBT-015-BUILD inline addendum (now wave-22 stream #3).
- ✅ DEBT-008 `corelink-hash` — empirical CLOSED `e9c0a5e` + audit `2026-05-16-debt-008-mutation-sweep.md` (wave-21 stream #2).
- ✅ Wave-20 streamB B-P1-01 (RLS WITH CHECK + emit-discipline cleanup) — CLOSED `8ab5cba` (wave-21 stream #1).
- ✅ Tenant config region-resolver wiring — CLOSED via `bccdd97` merge tip (wave-21 stream #6).
- ✅ Wallclock cross-route unification + A-P2-05 / B-P2-03 — CLOSED `f3462c6` (wave-21 stream #8).
- ✅ Wave-20 adversarial review — CLOSED `1dccc22` "wave-20 adversarial review: independent SOTA-bar pass (9.40/10 PASS)" (wave-21 stream #9).
- ✅ Secrets-X false-positive — CLOSED `2571e4e` (wave-21 stream #5).
- ✅ W19-P1-01 doc-fix — CLOSED `7333206` (wave-21 stream #7).

**Total truly-OPEN canonical rows pre-wave-22-SEAL:** 6 (down from 7 at wave-21 close after DEBT-015 docs-CLOSED transitioned the P2 row to the DEBT-015-BUILD inline addendum; net OPEN stays effectively flat as the build addendum is now tracked as a wave-22 stream).
**Post-wave-22-SEAL projection** (assuming streams #2 + #3 SEAL): DEBT-008 OPEN subset narrows to {pat, clerk, dual-approval, ratelimit} on CI-nightly + DEBT-015-BUILD flips to CLOSED. Net canonical OPEN goes from 6 → 5.

### 6.2 Closure ETA summary

| ETA bucket | Rows |
|---|---|
| Wave-22 SEAL (next 1–2 weeks) | DEBT-015-BUILD (stream #3); DEBT-008 8-crate expansion (stream #2, narrows OPEN subset) |
| Pre-GA Gate (T+30d) | DEBT-003 (user-bound; AWS Artifact PDF download) |
| T-7d pre-launch | DEBT-016 (user-bound; Statuspage provisioning) |
| Post-GA (T+90d horizon) | DEBT-010 P2/P3 (7 tickets), DEBT-013 deferrals (4 tickets) — wave-22 stream #6 may tighten regression CI but does not close the deferrals themselves |

---

## 7. Wave-23 candidate streams + caveats from wave-22 findings

Recommended wave-23 dispatch list (ordered by GA-blocker severity, contingent on wave-22 SEAL):

| # | Stream | Rationale | Estimated cost |
|---|---|---|---|
| 1 | **Wave-22 adversarial review (codex Opus pass)** | Mandatory per charter §"all P1-classified streams must close before next wave unblocks P2/P3". Cross-review wave-22 streams #2 (DEBT-008 8-crate), #3 (DEBT-015-BUILD), #7 (chaos campaign), #8 (24h endurance). | ~1 codex Opus pass + 1 audit doc per stream. |
| 2 | **Wave-22 stream-#1 adversarial findings absorption** | If stream #1 surfaces P0/P1 findings on wave-21 closure streams, dispatch fix agents. | Branch-per-finding; size depends on findings. |
| 3 | **DEBT-008 wave-22 follow-on for any crate not reaching ≥ 75 %** | Crates from stream #2 batch that hit < 75 % empirical kill rate need targeted-test additions OR documented equivalent-mutation rationale (matching `corelink-hash`'s `(hi << 4) \| lo` pattern). | 1 Sonnet per remediation. |
| 4 | **Chaos campaign findings absorption** | Stream #7 surface findings on executor failure-injection / tail-latency under partition / replication SLO breach → dispatch fix agents per finding. | Branch-per-finding. |
| 5 | **24h endurance soak run execution** | Stream #8 builds the rig; wave-23 runs the actual 24h soak + captures SLO streak evidence into `RB-GA-CUTOVER.md` greenlight dashboard. | 24h wall-clock + 1 Sonnet for evidence absorption. |
| 6 | **DEBT-010 P2 CI optimisation batch** (concurrency cancel + shared rust-cache key + TLC matrix + paths-filter audit) | Post-GA polish; ~30 min/PR cumulative savings. | 1 Sonnet × 4 P2 tickets. |
| 7 | **DEBT-013 perf optimisation deferrals execution** (OPT-03b + OPT-04ph2 + OPT-08) | Post-GA polish; depends on stream #6 regression-CI tightening landed. | 1 Sonnet × 3 deferrals. |
| 8 | **Pentest engagement kickoff support (RFP + vendor shortlist + SOW template)** | User-bound for final vendor selection; agent can draft RFP. | 1 Sonnet × draft RFP + shortlist + SOW template. |
| 9 | **Pilot onboarding tooling polish** (DPA signing automation, pilot success dashboard) | Post-design-partner-signup hygiene; depends on first pilot signups. | 1 Sonnet × ≤2 weeks. |
| 10 | **Wave-23 INV registry + DEBT register hygiene sweep** | Cadence preserved — same charter as wave-19/20/21/22 sweep streams. | 1 Sonnet × 30 min. |

### 7.1 Caveats from wave-22 sweep findings

- **No INV registry drift detected this wave.** Registry stable at 192 declared / 143 WI-coverage; no DRAFT promotion warranted. The +1 declared INV vs wave-21 close is a direct canonical addition (TLA spec extension via `wt/r-prep-debt-014-tla-specs`), not a DRAFT promotion.
- **No DEBT register drift detected this wave.** 6 canonical OPEN rows pre-SEAL; wave-22 streams #2 + #3 in flight will reduce OPEN to 5 post-SEAL (DEBT-015-BUILD flips, DEBT-008 narrows subset but remains structurally OPEN until all CI-nightly crates have empirical baseline).
- **DEBT-008 cross-stream dependency:** wave-22 stream #2 (`corelink-tenant-path`) is blocked on wave-22 stream #4 (`Uuid::now_v7` compile fix). If stream #4 slips, stream #2's tenant-path sub-task escalates to wave-23 follow-on.
- **DEBT-015-BUILD docs-CI dependency:** the GitHub-billing account issue blocks docs-CI verification independently of stream #3 code changes. Local `pnpm build` on Node 22 must be sufficient SEAL evidence; CI green is a wave-23+ verification once billing resolves.
- **Wave-21 adversarial review (wave-22 stream #1) is the bound resource** — codex Opus is the only model capable of independent SOTA-bar review (wave-20 adversarial review scored 9.40/10 per `1dccc22`). Wave-22 stream #1 requires 1 codex Opus invocation per wave-21 P1 stream (4 minimum: #1 RLS, #2 mutation, #3 TLA, #4 Node 22).
- **No new structural GA-blocker surfaced wave-22.** All 9 in-flight streams are polish, expansion, or campaign-evidence-accumulation — none reveal previously-unknown design gaps.

### 7.2 Wave-23 entry caveats

- **DCO + Co-Authored-By preserved** on every commit (same as wave-19/20/21/22).
- **No `--no-verify` hooks.** Pre-commit failures must surface root cause.
- **Synchronous Bash only.** No `run_in_background`. Same charter as wave-19/20/21.
- **30–40-min time budget per stream** (matches wave-21/22 cadence); 8–10 streams per wave realistic.
- **Wave-23 SEAL gate:** all wave-22 P1 streams (#2 DEBT-008 8-crate, #7 chaos campaign) verified closed before wave-23 unblocks any new P2/P3 streams.

---

## 8. Quality gates verified

Per the wave-22 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present. |
| Canonical consistency validator | `python3 scripts/validate_canonical_consistency.py` | exit 0 — 192 INVs declared, 0 orphan refs, 0 CRITICAL without TLA+. |
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — 440 with schema + 9 YAML-only (449 total). |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references. |

---

## 9. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave22-sweep`
- **Base commit:** `bccdd97` (wave-21 SEAL tip)
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-22 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 10. Cross-references

- `specs/_audits/2026-05-16-wave21-closure.md` (wave-21 closure; predecessor).
- `specs/_audits/2026-05-16-wave20-closure.md` (wave-20 closure; structural patterns continued).
- `specs/_audits/2026-05-15-debt-register.md` v1.2.0 (DEBT register canonical state; no new changelog entry this wave — survey-only).
- `specs/03_architecture/invariant_registry.md` (updated wave-21; stable wave-22 — full coverage maintained).
- `specs/_audits/2026-05-15-canonical-consistency-baseline.md` (CI ratchet floor; DEBT-004 closure log §3.1).
- `specs/_audits/2026-05-16-debt-008-mutation-sweep.md` (wave-21 expansion baseline for DEBT-008; sets equivalent-mutation analysis pattern for wave-22 stream #2).
- `specs/_audits/2026-05-15-mutation-full-sweep.md` (wave-15 `corelink-audit-chain` empirical baseline; sets methodology for wave-22 stream #2 crates).
- `RB-GA-CUTOVER.md` (cutover runbook; greenlight dashboard updated post-wave-22-SEAL).
- `specs/_audits/2026-05-16-pre-ga-pentest-scope.md` (pentest scope; engagement checklist).
- `.github/workflows/mutation-nightly.yml` (CI-nightly artifact precedence; `TD-DEBT-008-WAVE-14-EMPIRICAL` SEAL mechanism).
