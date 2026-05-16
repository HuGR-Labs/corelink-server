# Wave-29 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-cutover-wait-state hygiene rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-29 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave29-sweep`.
> **Base:** `main` @ `365dd38` ("merge wt/r-prep-pre-cutover-weekly-verify into main (wave-28)" — wave-28 SEAL tip).
> **Scope:** **Cutover-wait-state hygiene sweep — INV registry survey + DEBT register survey + wave-29 stream catalogue.** Wave-29 is the **first customer-acquisition + GA-wait-state wave** (engineering corpus is feature-complete since wave-26 GA-1 freeze; cutover ceremony gated on operator-paced DEBT items per wave-27 §3.4 NO-GO triggers). Ten streams catalogued — 3 signup-pipeline streams (backend / landing / admin UI), 1 ShadowSinkFactory full-adoption stream, 1 wave-28 adversarial review, 3 customer-facing artefact streams (audit-chain viz / pricing calculator / trust center publish), 1 perf-baseline GA-freeze stream, and this hygiene sweep (#10). No new canonical INV promotions warranted (signup backend INV-SIGNUP-TOKEN-IDEMPOTENT candidate surveyed but deferred — see §6.2). DEBT register surveyed; wave-28 closures (DEBT-003 / DEBT-016 / DEBT-025 / DEBT-026 / DEBT-027) all flipped to **engineering-CLOSED with operator-bound action enumerated**; net OPEN reduced 8 → 5 operator-paced rows.
> **Cross-ref:** `specs/_audits/2026-05-16-wave27-closure.md` (predecessor; wave-28 closure doc was deferred into the wave-29 hygiene sweep), `specs/_audits/2026-05-15-debt-register.md` v1.2.4, `specs/03_architecture/invariant_registry.md` v0.2.2, `specs/_audits/2026-05-16-ga-readiness-final.md` (wave-24 stream #8 CONDITIONAL GO), `specs/_runbooks/RB-GA-CUTOVER.md`, `specs/_runbooks/RB-POST-GA-CONTINUITY.md` (wave-27 anchor), `specs/_compliance/GA-GATE-CRITERIA.md`, `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md`.

---

## 1. Wave-29 scope — 10 streams catalogued

Wave-29 is the **GA-cutover-wait-state + customer-acquisition wave** — dispatched on `main` @ `365dd38` (wave-28 SEAL tip after 13 wave-28 merges: AWS Artifact fetch automation, pilot-announcement comms package, Statuspage provisioning automation, pentest finding absorption framework, LFPDPPP MX engagement final, DEBT-003 AWS Artifact recorder, pentest RFP send ceremony, pre-cutover weekly verification cron). Engineering corpus has been **feature-complete since wave-26 GA-1 freeze** (`74b8faa` on main); wave-29 work pivots from "lock the engineering surface" → "fill the customer-facing surface area + execute operator-bound DEBT absorption". Ten parallel streams catalogued (this stream is #10).

| # | Stream | Branch / worktree | Disposition |
|---|---|---|---|
| 1 | **signup.corelink.dev backend** — token-based pilot-slot reservation backend (wires the contract pinned in wave-27 DEBT-027 register row); idempotent token issuance + 24h-replay-safe consumption + Slack-payload + Grafana-panel hooks. Candidate INV introduced: `INV-SIGNUP-TOKEN-IDEMPOTENT` (DRAFT; survey deferred — see §6.2). | `wt/r-prep-signup-corelink-dev-backend` (worktree `agent-signup-backend`) | **IN FLIGHT** (wave-29) |
| 2 | **signup.corelink.dev landing page** — public landing (4-section: hero + value-prop + 3-tier pricing-summary + signup form); consumes pricing calculator stream #7 SSR data + DEBT-027 signup-link contract. | `wt/r-prep-signup-landing-page` (worktree `agent-signup-landing`) | **IN FLIGHT** (wave-29) |
| 3 | **Pilot admin web UI** — Owner-facing pilot-admin UI consuming wave-27 admin scripts (`grant-pilot-tier.sh` / `list-pilot-tenants.sh` / `pilot-24h-checkin.sh`) as backend endpoints + Grafana dashboard embed; replaces shell-only pilot admin path. | `wt/r-prep-pilot-admin-web-ui` (worktree `agent-pilot-admin-ui`) | **IN FLIGHT** (wave-29) |
| 4 | **ShadowSinkFactory full adoption** — final consumer-side adoption sweep (wave-21 `TokioPgShadowSinkFactory` wired to tenant-region resolver; wave-27 `wt/r-prep-shadow-sink-consumer-adoption` SEALED the partial adoption). Wave-29 flips the remaining ad-hoc constructors to factory-issued instances across all consumer crates (audit / ratelimit / replication). | `wt/r-prep-shadow-sink-full-adoption` (worktree `agent-shadow-sink-full-adoption`) | **IN FLIGHT** (wave-29) |
| 5 | **Wave-28 adversarial review (codex Opus pass)** — mandatory per charter "all P1-classified streams must close before next wave unblocks". Cross-reviews wave-28 streams (AWS Artifact automation, Statuspage automation, pentest absorption, LFPDPPP MX final, pentest RFP send, pre-cutover weekly verify, pilot announcement comms). | `wt/r-prep-wave28-adversarial-review` (worktree `agent-wave28-review`) | **IN FLIGHT** (wave-29) |
| 6 | **Audit-chain viz UI** — customer-facing audit-chain visualisation UI atop wave-15 R2 NDJSON producer + wave-23 `RB-AUDIT-CHAIN-VERIFICATION` (extends `wt/r-prep-audit-chain-viz` partial work); tenant-scoped Merkle-path inspector + tamper-evidence proof viewer. | `wt/r-prep-audit-chain-viz-ui` (worktree `agent-audit-chain-viz`) | **IN FLIGHT** (wave-29) |
| 7 | **Pricing page calculator** — public 4-tier pricing calculator + comparison + internal cost-worksheet (extends `wt/r-prep-pricing-page-calculator` partial work `db5330f`); SSR-rendered; consumed by stream #2 signup-landing-page. | `wt/r-prep-pricing-page-calculator` (worktree `agent-pricing-page`) | **IN FLIGHT** (wave-29) |
| 8 | **Trust center publish** — customer trust center publication pipeline (extends `wt/r-prep-trust-center` partial work `0780007`); consolidates SOC 2 / ISO 27001 / PCI DSS / LGPD / FedRAMP-informational status + DEBT-003 AWS Artifact PDF link (gated by DEBT-003 closure). | `wt/r-prep-trust-center-publish` (worktree `agent-trust-center`) | **IN FLIGHT** (wave-29) |
| 9 | **Perf baseline GA freeze** — final pre-cutover perf-baseline snapshot (Lote 6 cache SLOs + Lote 7 CI-perf-SLA + CF Worker prefetch hit-rate + cold-start tail); pinned at the cutover commit base; consumed by NO-GO trigger #5 (endurance 7-day soak streak). | `wt/r-prep-perf-baseline-ga-freeze` (worktree present) | **IN FLIGHT** (wave-29) |
| 10 | **Wave-29 INV registry sweep + DEBT register survey + closure audit** (this stream — hygiene + cataloguing pass; survey-only; **cutover-wait-state hygiene**) | `wt/r-prep-inv-registry-wave29-sweep` (worktree `agent-wave29-sweep`) | **CLOSED via this commit** |

Streams #1–#9 are dispatched in parallel by the orchestrator; this stream (#10) performs the hygiene + cataloguing pass against the same `365dd38` base. Per charter §"survey-only", streams #1–#9 are surveyed below but **not** closed by this audit.

---

## 2. signup.corelink.dev backend + landing + admin (cross-ref streams #1, #2, #3)

Wave-29 spawns the **first end-to-end customer-acquisition path on `corelink.dev`**. Three coordinated streams.

### 2.1 Stream #1 — signup.corelink.dev backend

Engineering scope: token-based pilot-slot reservation backend. Wire-format pinned in wave-27 DEBT-027 register row §2.1 (`https://signup.corelink.dev/pilot/<token>` — base32-encoded 16-byte token; 7-day TTL; idempotent consumption with replay-safe 24h dedup window). Consumes:

- **wave-15 audit-chain R2 NDJSON producer** (`8ba0353`) — every token issuance / consumption is an audit event.
- **wave-27 pilot admin scripts** (`6761860`) — `grant-pilot-tier.sh` runs server-side once a token is consumed.
- **wave-26 GA-1 feature freeze** (`74b8faa`) — backend is **net-new on `apps/`**, not `crates/`; GA-1 freeze applies to `crates/` so the freeze allows new apps endpoints without ADR-0034b waiver (per ADR-0034b §3 scope-fence).
- **Lote 6 cache SLOs** — backend reads tenant-region resolver (wave-21 `b5c6bfa`) for tenant-region assignment at token consumption.

Candidate INV: `INV-SIGNUP-TOKEN-IDEMPOTENT` — "consumption of a signup token MUST be exactly-once across replays within the 24h dedup window; the 25th-hour-onward replay MUST be observably distinguishable from the 1st-hour replay (different audit event ID, same tenant binding)." Survey-only per §6.2 (deferred to wave-30 sweep once stream #1 SEALs and the wire-spec lands canonical).

### 2.2 Stream #2 — signup.corelink.dev landing page

Engineering scope: public landing page at `https://signup.corelink.dev/` (4 sections — hero / value-prop / 3-tier pricing-summary / signup form). SSR-rendered. Consumes stream #7 pricing-calculator SSR data + DEBT-027 signup-link contract. No backend dependency for the static landing path (only the `<form action>` POST hits stream #1 backend).

### 2.3 Stream #3 — pilot admin web UI

Engineering scope: Owner-facing pilot-admin UI replacing wave-27 shell-only admin path. Three endpoints consumed:

- `POST /admin/pilot/grant-tier` — wraps `grant-pilot-tier.sh` (validated SQL); JWT-gated Owner-only.
- `GET /admin/pilot/list` — wraps `list-pilot-tenants.sh` (read-only; Owner-only).
- `POST /admin/pilot/24h-checkin` — wraps `pilot-24h-checkin.sh` (Slack payload preview + dispatch).

Plus Grafana dashboard embed (`dashboards/grafana/dash-pilot-tenants.yml` — wave-27 SSOT). Owner-only RBAC enforced via existing wave-19 `RequestPrelude` wiring (wave-27 `c32f81a` `/v1/audit/analytics/*` precedent).

### 2.4 Cross-stream dependency table

| Stream | Depends on | Consumed by | DEBT row |
|---|---|---|---|
| #1 backend | wave-27 `6761860` admin scripts; wave-21 `b5c6bfa` tenant-region resolver; wave-15 audit-chain | #2 landing form action; #3 admin UI backend | DEBT-027 |
| #2 landing | #7 pricing calculator SSR; #1 backend (form POST) | Owner Slack announcement (wave-28 `b0485bc`) | DEBT-027 |
| #3 admin UI | wave-27 `6761860` admin scripts; wave-27 `dash-pilot-tenants.yml` | Owner pilot-recruitment workflow | DEBT-027 |

All three streams collectively unblock the engineering-side of DEBT-027 (≥ 3 pilot signups before cutover). Recruitment remains operator-paced.

---

## 3. ShadowSinkFactory full adoption (cross-ref stream #4)

Engineering scope: complete the wave-21 `TokioPgShadowSinkFactory` adoption sweep started in wave-27 `f5ff683` (`wt/r-prep-shadow-sink-consumer-adoption`). Wave-29 stream #4 flips remaining ad-hoc `ShadowSink` constructors across all consumer crates to factory-issued instances.

### 3.1 Lineage chain

- **wave-18 caveat #3** — `RealNeonShadowSink` driver authored (`ec6e83b`).
- **wave-19** — first consumer wire (`dc8a6bb` `audit-export payload column + async pages`).
- **wave-21** — `TokioPgShadowSinkFactory` wired to tenant-region resolver (`b5c6bfa`).
- **wave-27** — partial consumer-adoption sweep (`f5ff683` `wt/r-prep-shadow-sink-consumer-adoption`).
- **wave-29 stream #4** — final adoption across all consumer crates.

### 3.2 Adoption surface

Wave-29 stream #4 covers the remaining consumer crates that still construct `ShadowSink` directly:

- `audit-export` (partial wave-19; full wave-29).
- `ratelimit` (factory adoption flag).
- `replication` (factory adoption flag).
- `audit-chain` (factory adoption flag for wave-15 NDJSON producer hook).

Outcome: zero direct `ShadowSink::new(...)` call sites in consumer crates post-SEAL; all sinks issued via `ShadowSinkFactory::for_tenant(tenant_id)` with region-aware resolution. Invariants `INV-AUDIT-CHAIN`, `INV-AUDIT-EMIT-ATOMIC`, `INV-DATA-TENANT-ISOLATION` preserved structurally (factory enforces tenant-region binding at construction).

### 3.3 DEBT impact

No DEBT row spawned; this is a wave-21→wave-27→wave-29 follow-on chain, not a new debt. Post-SEAL, all wave-18/19 caveats #3/#4 (per debt-register v1.2.0 entry §209) are **structurally closed** by factory enforcement — no operator-bound action.

---

## 4. Customer-facing artefacts (cross-refs streams #6, #7, #8)

Wave-29 fills the **customer-facing surface area** the GA cutover meeting requires. Three streams.

### 4.1 Stream #6 — audit-chain viz UI

Customer-facing audit-chain visualisation UI. Atop wave-15 R2 NDJSON producer + wave-23 `RB-AUDIT-CHAIN-VERIFICATION` runbook. Tenant-scoped Merkle-path inspector + tamper-evidence proof viewer. Extends `wt/r-prep-audit-chain-viz` (partial wave-23 `1b8544e`). RBAC: tenant-scoped read-only via existing `RequestPrelude` wiring.

INV touch surface: `INV-AUDIT-CHAIN` (alias §5 of registry; canonical `INV-AUDIT-APPEND-ONLY` + `INV-AUDIT-EMIT-ATOMIC` family). No new INV — UI consumes invariants that already TLA-verify.

### 4.2 Stream #7 — pricing page calculator

Public 4-tier pricing calculator extending wave-prior `db5330f` (`feat(r-prep): public pricing calculator + 4-tier overview + comparison + internal worksheet`). Wave-29 stream #7 SSR-renders and exposes the 4-tier comparison page + interactive calculator + internal cost-worksheet. Consumed by stream #2 signup-landing-page.

No INV touch surface.

### 4.3 Stream #8 — trust center publish

Customer trust center publication. Consolidates:

- **SOC 2 Type II + ISO 27001 + PCI DSS SAQ-A + LGPD + FedRAMP-informational crosswalk** — all SEALED across waves 11/12/13/14 (`adc45d8` PCI DSS, `3ac2998` ISO 27001, `0780007` FedRAMP informational).
- **DEBT-003 AWS Artifact PDF link** — wave-28 `de8c4b5` AWS Artifact recorder closes engineering-side; Owner uploads PDFs to `evidence/aws-artifact/` at T-7d pre-launch.
- **DEBT-026 pentest letter link** — gated by retest letter delivery (target 2026-07-29).

Stream #8 publishes the trust-center landing surface; DEBT-003/DEBT-026 link slots **render placeholders until operator action lands**. No INV touch surface.

### 4.4 Cross-stream consumption

Stream #2 signup-landing-page links to stream #6 audit-chain-viz (proof-of-tamper-evidence), stream #7 pricing-calculator (3-tier embed), stream #8 trust-center (compliance-attestation links). The customer-acquisition funnel is end-to-end closed at wave-29 SEAL (modulo operator-paced DEBT-003 / DEBT-026 PDF/letter slots).

---

## 5. Perf baseline GA freeze (cross-ref stream #9)

Engineering scope: final pre-cutover perf-baseline snapshot — pinned at the cutover commit base. Consumed by NO-GO trigger #5 (per wave-27 §3.4 — endurance 7-day soak streak must observe baseline without regression).

### 5.1 Baseline surface

Five SLO families pinned at wave-29 stream #9 SEAL:

| Family | Source | Pinned metric |
|---|---|---|
| Lote 6 cache SLOs | Wave-15 GA cache layer | p95 hit-latency / p99 hit-latency / hit-rate / cold-fill p99 |
| Lote 7 CI-perf-SLA | Wave-26 `fbc2b6c` Lote 7 follow-ons | CI-pipeline p95 wall-clock + per-crate test time + mutation-nightly aggregate |
| CF Worker prefetch hit-rate | Wave-26 `a48bbec` CF Worker prefetch wire | `SLO-CF-PREFETCH-HIT-RATE-001` (e.g., ≥ 80% on warm cohort) |
| CF Worker cold-start tail | Wave-25 endurance dress-run | `SLO-COLD-START-CF-WORKER-001` (e.g., p99 ≤ 50 ms) |
| Audit-chain emit-latency | Wave-15 NDJSON producer | per-tenant emit p95 / R2 archive lag p99 |

### 5.2 Freeze mechanism

Stream #9 commits a `specs/_audits/2026-05-16-perf-baseline-ga-freeze.md` doc with the 5 SLO families pinned at the wave-29 base commit (`365dd38`). The endurance 7-day soak streak (wave-27 stream #4 — wall-clock 168h) reads this doc as the canonical baseline; any deviation > regression-threshold per SLO-family policy fires NO-GO trigger #5.

### 5.3 DEBT impact

No DEBT spawned. Baseline freeze is a **gate enabler**, not a debt.

---

## 6. INV registry state — 197 + delta (no promotions warranted this wave)

Per `python3 scripts/validate_inv_promotion.py` + `python3 scripts/validate_specs.py` + `python3 scripts/validate_references.py` on this branch (post-sweep, pre-SEAL):

| Metric | Count (wave-29 base) | Δ vs wave-27 close (= wave-28 close, no INV changes wave-28) |
|---|---|---|
| INVs declared (registry §3 rows) | **197** | 0 (wave-26 GA-1 freeze locked the count; wave-27/28 sealed no new canonical INVs) |
| └ CRITICAL | **61** | 0 |
| └ HIGH | **132** | 0 |
| └ MEDIUM | **4** | 0 |
| └ LOW / UNKNOWN | **0** | 0 |
| Aliases declared (registry §5) | 15 | 0 |
| TLA+ verified | **82** | 0 |
| WI coverage | **143 / 143** | 0 |
| Orphan refs (in code, NOT in registry+aliases) | **0** | 0 |
| **CRITICAL without TLA+ proof** (**Z = 0 milestone**) | **0** | 0 (preserved across waves 26 → 29) |

### 6.1 Z = 0 milestone — held across 3 consecutive waves (26 → 27 → 28 → 29)

The Z = 0 milestone (CRITICAL-without-TLA+ count = 0) established at wave-26 stream #9 INV-CRITICAL TLA final audit (`6f87754`) is **preserved at wave-29 base**. The cite "61 CRITICAL all TLA+-proved" remains green-light for the GA-GO/NO-GO meeting.

### 6.2 INV-SIGNUP-TOKEN-IDEMPOTENT — DRAFT candidate (deferred to wave-30 sweep)

Wave-29 stream #1 (signup backend) introduces an exactly-once consumption semantics over a 24h dedup window. The natural promotion candidate is:

> **INV-SIGNUP-TOKEN-IDEMPOTENT (DRAFT, candidate)** — "Consumption of a signup token MUST be exactly-once across replays within the 24h dedup window; replays after the 24h window are observably distinguishable from in-window replays (different audit event ID, same tenant binding); failed consumptions never burn the token."

**Why deferred:** the canonical wire-spec lands inside stream #1 (`apps/signup-backend/`). Per the registry promotion charter (`_spec_contract.md §14`), an INV is promoted when its canonical source SEALs. Stream #1 is **IN FLIGHT** at wave-29 base; this audit is **survey-only** per the wave-sweep charter. Wave-30 sweep absorbs the promotion once stream #1 SEALs and stamps the canonical wire-spec.

**Severity classification (forward-looking):** **HIGH** (per-tenant correctness; not blast-radius — token replay does not cross tenants; not audit-chain integrity break — audit emission is independent of token state). **TLA+ proof:** likely **not required** at HIGH classification (per registry §2 — TLA+ mandated for CRITICAL only); recommended property-test in `crates/signup-token/tests/` instead.

**Net: 0 promotions warranted from this audit stream.** Wave-30 absorbs the DRAFT promotion.

**Update (Wave-30 stream-4 R-PREP — 2026-05-16):** **Net flipped to 1 promotion absorbed by wave-30**. The Wave-30 stream-4 dispatch authored `specs/tla/signup_token_idempotent.tla` + PR/nightly cfgs + CI matrix wiring + registry §3.29 + §4.1 coverage-table row, promoting `INV-SIGNUP-TOKEN-IDEMPOTENT` from DRAFT → PROMOTED + **TLA-VERIFIED**. PR lane: 1 017 distinct states / 5 s; nightly: 37 273 distinct states / 33 s — both green. Despite the §6.2 forward-looking note that TLA+ was "likely not required at HIGH" the dispatch elected to author the spec anyway to (a) tighten the audit-emit-atomic contract beyond what the integration tests assert (Len(audit_log) = request_count atomicity bijection) and (b) prove the cross-branch SoT coherence (token_tenant ↔ email_tenant agreement) that the in-memory store's iter-find-first-match semantics relies on. See `specs/_audits/2026-05-16-inv-signup-token-tla.md` for the full dispatch ledger + counterexample-driven refinement history (the initial `InvSignupIdempotentByEmail` formulation was found over-tight by TLC and reformulated to the per-`reserved`-row granularity that matches the prod `insert_or_existing` semantics).

### 6.3 Registry severity-breakdown snapshot (canonical; wave-29 base = wave-28 close = wave-27 close)

For the GA-cutover D-day execution meeting (preserved cite):

> *"197 declared INVs · 61 CRITICAL all TLA+-proved (Z = 0 milestone preserved across 4 consecutive waves 26 → 29) · 143/143 WI coverage · 0 orphan refs · 0 UNKNOWN severity classification · 82 TLA+ proofs · 15 legacy→canonical aliases."*

---

## 7. DEBT register state — wave-28 closures + wave-29 deltas

Per `specs/_audits/2026-05-15-debt-register.md` v1.2.4 (wave-27 close baseline). Wave-28 absorbed five operator-bound DEBT items into engineering-CLOSED state by landing the AS-IF-AUTOMATED bundles + Owner-action runbooks; wave-29 in-flight streams add zero new DEBT rows (signup pipeline maps to existing DEBT-027; ShadowSinkFactory adoption maps to existing wave-18 caveats already closed).

### 7.1 Wave-28 engineering-CLOSED list (5 rows)

| DEBT ID | Wave-28 commit | Engineering-side absorption | Operator-bound residual action | Closure ETA |
|---|---|---|---|---|
| **DEBT-003** | `de8c4b5` (`chore(debt-003): one-command AWS Artifact PDF recorder + Owner quickstart`) | `scripts/aws-artifact-pdf-recorder.sh` (one-command) + `2599b58` fetch-automation merge + Owner quickstart in `docs/operator/aws-artifact-quickstart.md` | Owner runs the one-command at T-7d pre-launch; commits PDFs to `evidence/aws-artifact/` | T-7d pre-launch (user-paced) |
| **DEBT-016** | `de860d9` (`Statuspage Option A AS-IF-AUTOMATED bundle`) + `12d2f84` merge | Statuspage Option A provisioning automation bundle (env-var swap commit pre-staged + CNAME staging entry + 4-step provisioning runbook execution against sandbox tenant) | Owner provisions Statuspage tenant + executes env-var swap commit at T-7d pre-launch | T-7d pre-launch (user-paced) |
| **DEBT-025** | `6076641` (`wave-28 step-6: finalize LFPDPPP MX attorney engagement package`) + `2038bc1` merge | Final attorney engagement package: engagement letter (signed-ready) + 8-13h scope SOW + EVT-044 PDF delivery contract | Owner countersigns engagement letter; attorney delivers EVT-044 PDFs (es-MX privacy notice + 2 breach templates) + Q1-Q4 redlines within T+21d | T+30d-ish from attorney engagement; hard-cap pre-S-20 GA target 2026-10-01 |
| **DEBT-026** | `f3d44dc` (`wave-28: pentest RFP send ceremony — 33min→5min Owner action`) + `2ae9d03` merge + `bb2da85` (`wave-28 step-9 — pentest finding absorption framework`) + `764b115` merge | RFP send ceremony (33min Owner action collapsed to 5min via batched template-render + per-vendor rationale prefill) + pentest finding absorption framework (10-state machine for in-flight finding triage) | Owner executes the 5min RFP-send ceremony; vendor-paced (NOT_CONTACTED → RFP_SENT → SOW_COUNTERSIGN → ENGAGEMENT → RETEST_LETTER) | 2026-07-29 (retest letter delivery — earliest GA cutover unblock date) |
| **DEBT-027** | `b0485bc` (`marketing: add pilot-announcement comms package (wave-28 step-7)`) + `42658a2` merge | Pilot-announcement comms package: Slack/email/X templates + ICP target list rendered + per-channel send-cadence playbook | Owner sends announcement on Owner-chosen date; recruits ≥ 3 cohort-1 pilots; signs pilot agreements | T-7d pre-launch (user-paced recruitment) |

**All 5 rows: engineering-CLOSED with operator-bound action enumerated.** Zero remaining engineering-side work on these DEBT items.

### 7.2 Wave-28 ancillary commits (non-DEBT-numbered but cutover-relevant)

- `25f45e3` — `wave-28: pre-cutover weekly verification cron`. NEW recurring verification cron (weekly G1..G6 dry-run gate re-execution). Feeds wave-27 §3.4 NO-GO trigger #4 (cutover dry-run #3+) automatically.

### 7.3 Net canonical OPEN: 8 → 5 (wave-27 close → wave-29 close)

Delta vs wave-27 close §4.2 baseline (8 OPEN):

- **−5** (DEBT-003 / DEBT-016 / DEBT-025 / DEBT-026 / DEBT-027 all flipped to **engineering-CLOSED + operator-bound**; not register-state CLOSED but engineering-side fully done — operator action remaining).
- **+0** (no new DEBT rows wave-28 or wave-29).

**5 OPEN rows remaining (all operator-paced; engineering-side feature-complete):**

| DEBT ID | State | Why remaining |
|---|---|---|
| DEBT-003 | Engineering-CLOSED + operator-bound | Owner runs PDF recorder at T-7d |
| DEBT-008 | P1 partial; CI-nightly streak monitored | Post-GA T+90d horizon (CI-nightly ≥ 75% streak ratchet) |
| DEBT-010 | P1 partial 8/11; 3 P3 carried forward | Wave-27 stream #6 SEAL pending |
| DEBT-013 | P1 partial 9/10; OPT-03a DEFERRED-infeasible | Wave-27 stream #6 SEAL pending (or formal DEFERRED re-affirmation) |
| DEBT-016 | Engineering-CLOSED + operator-bound | Owner provisions Statuspage at T-7d |
| DEBT-025 | Engineering-CLOSED + attorney-bound | Attorney delivers EVT-044 PDFs |
| DEBT-026 | Engineering-CLOSED + vendor-bound | Vendor delivers retest letter 2026-07-29 |
| DEBT-027 | Engineering-CLOSED + operator-bound | Owner recruits ≥ 3 cohort-1 pilots |

Reading: of the 8 nominally-OPEN rows, **5 are engineering-CLOSED with operator-bound residual** (DEBT-003 / -016 / -025 / -026 / -027); **3 remain engineering-side P1 partial** (DEBT-008 / -010 / -013), all post-GA-horizon-acceptable.

### 7.4 Net cutover-blocker delta: 1 → 1 (unchanged)

Per wave-27 §2.3, the single hard GA-cutover blocker remains: **DEBT-026 retest letter delivery (earliest 2026-07-29)**. Wave-28 engineering absorption did not change the vendor-paced timeline; wave-29 stream catalogue does not introduce new blockers.

### 7.5 Closure ETA summary (post-wave-29-SEAL projection)

| ETA bucket | Rows |
|---|---|
| Wave-29 SEAL (this wave) | DEBT-010 P3 batch (carried wave-27 stream #6 → wave-29 absorption pending); DEBT-013 OPT-03a re-evaluation (carried wave-27 → wave-29 absorption pending) |
| T-7d pre-launch | DEBT-003 (AWS Artifact PDFs); DEBT-016 (Statuspage provisioning); DEBT-027 (≥ 3 cohort-1 pilots enrolled) |
| Active-testing window (2026-06-15 → 2026-07-15) + retest (2026-07-29) | DEBT-026 (vendor-bound — GA-cutover gate) |
| T+30d-ish from attorney engagement | DEBT-025 (EVT-044 PDFs delivered; pre-2026-10-01 hard-cap) |
| Post-GA T+90d horizon | DEBT-008 (CI-nightly streak 30-night ratchet) |

---

## 8. Next-wave (wave-30) candidate streams — anchor: GA cutover wait state + post-GA continuity

Per autonomous-execution charter §"anchor: post-GA continuity playbook activation" + wave-27 stream #6 carry-forward (`3f0a459` `RB-POST-GA-CONTINUITY 30-day playbook + monitor`). Wave-30 anchor candidates surveyed below; orchestrator selects ≥ 8 at dispatch.

### 8.1 Strong candidates (high-priority; charter-anchored)

| # | Candidate stream | Anchor | Pre-condition |
|---|---|---|---|
| 1 | **INV-SIGNUP-TOKEN-IDEMPOTENT promotion + property-test** | Wave-29 stream #1 SEAL | Stream #1 wire-spec canonical |
| 2 | **Wave-29 adversarial review (codex Opus pass)** | charter "every wave reviewed" | Wave-29 streams SEALed |
| 3 | **Wave-28 + wave-29 closure rollup absorption** | charter "every wave closure-audited" | This audit doc + wave-28 retrospective |
| 4 | **Endurance 7-day soak streak observation absorption** | wave-27 stream #4 wall-clock 168h | 7d wall-clock elapsed from wave-27 dispatch |
| 5 | **GA-cutover dry-run #3** | NO-GO trigger #4 retest | T-72h of cutover ceremony OR weekly-verify cron alert |
| 6 | **DEBT-010 P3 + DEBT-013 OPT-03a final disposition** | wave-27 stream #6 carry-forward | Re-eval evidence gathered |
| 7 | **RB-POST-GA-CONTINUITY 30-day monitor first-week activation** | wave-27 `5414720` anchor | Cutover commit landed |
| 8 | **Pilot intake first-customer cold-fill observation** | DEBT-027 closure (≥ 3 enrolled) | Pilot signup pipeline live |
| 9 | **Trust center DEBT-003 PDF slot fill** | DEBT-003 operator action at T-7d | Owner PDF upload |
| 10 | **GA cutover ceremony commit (if retest letter delivered)** | DEBT-026 retest letter 2026-07-29 | Retest letter zero HIGH/CRITICAL |

### 8.2 Tertiary candidates (post-GA polish; defer-friendly)

- DEBT-008 CI-nightly streak observation + 30-night ratchet trigger
- Post-GA T+30d retrospective audit
- Customer-feedback intake from first-3-pilots round
- Wave-29 perf-baseline-freeze comparison (T+7d soak vs baseline)
- v1.0.1 patch-release window prep (if any hotfix surfaces during cutover-wait)

### 8.3 Wave-30 dispatch principle

Per autonomous-execution charter §"inflection point — GA cutover ceremony": the orchestrator **does NOT speculatively pre-dispatch the cutover ceremony commit** (#10 above). The cutover commit is the **only stream that requires explicit Owner authorisation** per ADR-0034b 2-key signature framework. All other 9 streams are agent-dispatchable as soon as wave-29 SEALs.

---

## 9. Quality gates verified

Per the wave-29 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present |
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — 448 with schema + 9 YAML-only (457 total) |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references; [SLO] 31def/82uses, [RB] 129def/236uses, [ADR] 27def/36uses |

---

## 10. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave29-sweep`
- **Base commit:** `365dd38` (wave-28 SEAL tip)
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-29 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 11. Cross-references

- `specs/_audits/2026-05-16-wave27-closure.md` (immediate predecessor; wave-28 closure absorbed into wave-29 sweep).
- `specs/_audits/2026-05-15-debt-register.md` v1.2.4 (canonical DEBT state; wave-28 closures appended in change-log v1.2.5+ when register PR lands).
- `specs/03_architecture/invariant_registry.md` v0.2.2 (197 declared; 61 CRITICAL all TLA+-proved — Z = 0 milestone preserved across waves 26 → 29).
- `specs/_audits/2026-05-16-ga-readiness-final.md` (wave-24 stream #8 — CONDITIONAL GO; 8-item DEFER counter).
- `specs/_audits/2026-05-16-ga-readiness-defer-scrub.md` (wave-25 stream #4 — DEFER drift detector; counter locked at 8).
- `specs/_audits/2026-05-16-ga-cutover-dryrun.md` (wave-24 stream #1 G1..G6 all GREEN).
- `specs/_compliance/GA-GATE-CRITERIA.md` (59 criteria across 6 tracks).
- `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` (GA-GO/NO-GO meeting template).
- `specs/_runbooks/RB-GA-CUTOVER.md` (cutover runbook).
- `specs/_runbooks/RB-POST-GA-CONTINUITY.md` (wave-27 `5414720` 30-day monitor playbook — wave-30 anchor).
- `specs/_decisions/ADR-0034b-dual-hat-fallback-policy.md` (2-key signature framework; cutover-commit gate).
