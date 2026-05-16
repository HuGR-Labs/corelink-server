# Wave-25 Closure Audit — 2026-05-16

> **Doc kind:** wave-closure audit / GA-readiness rollup (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-25 hygiene agent (Claude Opus 4.7) — branch `wt/r-prep-inv-registry-wave25-sweep`.
> **Base:** `main` @ `e9ee8eb` ("merge wt/r-prep-pat-clerk-mutation-sweep into main (wave-24)" — wave-24 SEAL tip).
> **Scope:** INV registry hygiene + DEBT register survey (wave-25 in-flight — DO NOT close from this stream) + wave-25 stream catalogue + external pentest engagement state (cross-ref stream #1) + DEBT-015-BUILD path-(3) final state (cross-ref stream #2) + GA-readiness DEFER drift detector (cross-ref stream #4) + INV registry severity-breakdown snapshot + wave-26 candidate streams.
> **Cross-ref:** `specs/_audits/2026-05-16-wave24-closure.md` (predecessor), `specs/_audits/2026-05-15-debt-register.md` v1.2.x, `specs/03_architecture/invariant_registry.md`, `specs/_audits/2026-05-16-ga-readiness-final.md` (wave-24 stream #8 final audit; CONDITIONAL GO), `specs/_audits/2026-05-16-debt-015-build-final.md` (wave-23 root-cause narrowing baseline), `specs/_audits/2026-05-16-pre-ga-pentest-scope.md` (wave-19 SEALED pentest scope).

---

## 1. Wave-25 scope — 10 streams catalogued

Wave-25 is the post-wave-24 GA-freeze-prep wave, dispatched on `main` @ `e9ee8eb` (wave-24 SEAL tip after 9 wave-24 merges: ADR-0034b dual-hat, GA cutover dry-run, statuspage URL substitution, wave-24 INV-registry sweep, GA-readiness final audit, wave-23 adversarial review, DEBT-015-BUILD babel-patch path-(1)/(2) attempt, DEBT-008 wave-24 mutation closure batch, PAT/clerk mutation sweep). Ten parallel streams catalogued (this stream is #10). Per wave-24 §6 candidate streams + §6.2 entry caveats, wave-25 anchors the external pentest engagement scope freeze (stream #1, per wave-24 §6 candidate #1) and adds the GA-readiness DEFER drift detector (stream #4) as a cross-cutting drift gate.

| # | Stream | Branch / worktree | Disposition |
|---|---|---|---|
| 1 | **External pentest engagement scope freeze** (anchor stream per wave-24 §6 candidate #1) — author RFP draft + vendor shortlist + SOW template against the wave-19 SEALED scope (`specs/_audits/2026-05-16-pre-ga-pentest-scope.md`); ratify or amend the scope based on wave-24 dry-run findings + wave-24 GA-readiness final audit DEFER counter. Vendor selection + engagement letter remain user-bound. | `wt/r-prep-pentest-engagement-scope-freeze` (worktree `agent-pentest-engagement-scope`) | **IN FLIGHT** (wave-25) |
| 2 | **DEBT-015-BUILD path-(3) ssgRequire sandbox-eval** — wave-24 stream #3 attempted path-(1) babel-preset `modules: false` patch (PARTIAL per `d91a690 chore(docs): wave-24 babel preset patch + ssgRequire path forward (DEBT-015-BUILD PARTIAL)`); wave-25 stream #2 executes the path-(3) ssgRequire sandbox-eval over the `build/assets/js/<chunkId>.<hash>.js` client chunks — per wave-23 final-audit fix-path order. This is the LAST in-charter path-attempt before alternative-architecture decision per wave-24 §4.3. | `wt/r-prep-debt-015-build-ssgrequire-path3` (worktree `agent-debt-015-w25`) | **IN FLIGHT** (wave-25) |
| 3 | **DEBT-008 number-discrepancy reconciliation** — wave-24 stream #2 SEAL'd 2 batches (`7b10444 wave-24 DEBT-008 mutation closure (chunker + multipart-schema CLOSED; 3 large crates → CI-nightly)` + `c469fe5 chore(debt-008): wave-24 empirical lift for corelink-pat + corelink-clerk`). Cross-wave kill % narrative drifted — chunker/multipart-schema CLOSED but the 3 large crates (`r2-multipart`, `quota-cas`, `webauthn`) were re-classified to CI-nightly instead of empirical first-sweep. Stream #3 reconciles the post-wave-24 number-discrepancy with the wave-23 baseline narrative and updates DEBT-008 row text in the canonical register to reflect the actual final state. | `wt/r-prep-debt-008-number-discrepancy` (worktree `agent-debt-008-numbers`) | **IN FLIGHT** (wave-25) |
| 4 | **GA-readiness DEFER drift detector** — wave-24 stream #8 GA-readiness final audit (`specs/_audits/2026-05-16-ga-readiness-final.md`) issued **CONDITIONAL GO** with 8 DEFER items (5 user-bound + 3 vendor-bound). Stream #4 authors a drift-detector script + CI gate that re-counts the DEFER population each commit and fails the build if the DEFER set grows without an associated `specs/_audits/<date>-defer-add-*.md` evidence doc. Prevents silent regression of the GA-readiness verdict between wave-25 and the cutover meeting. | `wt/r-prep-ga-checklist-drift-detector` (worktree `agent-ga-checklist-drift`) | **IN FLIGHT** (wave-25) |
| 5 | **Endurance 10-minute dress rehearsal** — wave-22 stream #8 built the 24h endurance rig; wave-24 stream #1 dry-run consumed ≥24h streak evidence (`ec074a1 wave-24 r-prep — RB-GA-CUTOVER §3 dry-run (G1..G6 all GREEN)`). Wave-25 stream #5 executes a 10-minute compressed dress rehearsal of the soak-streak ratchet (per wave-24 §6 candidate #6) as the cadence-preserving evidence rotation for the greenlight dashboard between wave-25 and cutover. | `wt/r-prep-endurance-10min-dressrun` (worktree `agent-endurance-dressrun`) | **IN FLIGHT** (wave-25) |
| 6 | **Statuspage init dress rehearsal** — DEBT-016 closed engineering-side wave-24 (`4076391 debt-016: statuspage URL substitution mechanism (engineering-CLOSED)`); user-bound provisioning still T-7d. Wave-25 stream #6 executes a dress-run of the statuspage-init flow against a staging Statuspage tenant (or a mocked HTTP harness if Statuspage org is not yet provisioned) — validates the URL-substitution mechanism end-to-end so the T-7d user-side switch is a pure DNS + ORG-ID swap. | `wt/r-prep-statuspage-init-dressrun` (worktree `agent-statuspage-dressrun`) | **IN FLIGHT** (wave-25) |
| 7 | **Tenant-config CF prod-wire** — wire the tenant-config region resolver (wave-21 9.85/10 adversarial-review highlight per wave-24 §3.2) into the Cloudflare prod binding using the real CF binding pattern (`specs/_audits/2026-05-15-cf-binding-real-pattern.md`). Production wiring layer addition; brings the region resolver from staging-only to GA-cutover-ready. | `wt/r-prep-tenant-config-cf-prod-wire` (worktree `agent-tenant-config-cf-wire`) | **IN FLIGHT** (wave-25) |
| 8 | **Pre-GA security attestation rollup** — consolidate SOC 2 / ISO 27001 / GDPR / LGPD / PCI / CCPA + the wave-24 LFPDPPP MX attorney package + the BYOK 4-provider matrix + the wave-24 chaos combined-failure evidence into a single sign-off-ready pre-GA security attestation document. Cross-references `specs/_compliance/GA-GATE-CRITERIA.md` security track. | `wt/r-prep-pre-ga-security-attestation` (worktree `agent-security-attestation`) | **IN FLIGHT** (wave-25) |
| 9 | **Wave-24 adversarial review (codex Opus pass)** — mandatory per charter "all P1-classified streams must close before next wave unblocks P2/P3". Cross-review wave-24 streams #1 dry-run, #2 DEBT-008 wave-24 batch, #3 DEBT-015-BUILD babel-patch, #6 PAT-revoke TLA+, #8 GA-readiness final audit. **Largest review pass to date** — wave-24 had 9 in-flight streams of which 5 are P1-classified. | `wt/r-prep-wave24-adversarial-review` (worktree `agent-wave24-review`) | **IN FLIGHT** (wave-25) |
| 10 | **Wave-25 INV registry sweep + DEBT register survey + wave-25 closure audit** (this stream — hygiene + cataloguing pass; survey-only) + **Lote 7 RACI detail** (combined-bucket per wave-24 §3.2 pattern — RACI detail rows for Lote 7 (Auth) tenant-isolation matrix) | `wt/r-prep-inv-registry-wave25-sweep` (worktree `agent-wave25-sweep`) + `wt/r-prep-lote-7-raci-detail` (worktree `agent-lote-7-raci`) | **CLOSED via this commit** (combined-bucket) |

Streams #1–#9 are dispatched in parallel by the orchestrator; this stream (#10) performs the hygiene + cataloguing pass against the same `e9ee8eb` base. Per the user mandate (charter §"DEBT survey (wave-25 streams in flight; survey-only)"), streams #1–#9 are surveyed below but **not** closed by this audit. Their SEAL commits land separately and the next wave-26 sweep reconciles. Note: stream #10 combines two worktrees (`inv-registry-wave25-sweep` + `lote-7-raci-detail`) into the same charter-mandated "10 streams" budget — same combined-bucket pattern used in wave-22 stream #9, wave-23 stream #9, wave-24 stream #2.

---

## 2. External pentest engagement state (cross-ref stream #1)

Stream #1 is the **wave-25 anchor stream** per wave-24 §6 candidate streams. Charter §6.2 wave-25 anchor language: *"External pentest engagement scope freeze becomes the wave-25 anchor stream — its scope-freeze decision drives wave-26+ dispatch priority for pentest-finding absorption."*

### 2.1 Pre-stream baseline (post-wave-24-SEAL)

| Pentest baseline artifact | State at wave-25 dispatch | Source |
|---|---|---|
| Wave-19 SEALED pentest scope | Frozen | `specs/_audits/2026-05-16-pre-ga-pentest-scope.md` |
| S-14 BYOK internal pentest | Complete | `specs/_audits/2026-05-14-pentest-s14-byok.md` |
| S-02..S-06 internal pentest baseline | Complete | `specs/_audits/2026-04-30..2026-05-02-pentest-s0X-internal.md` (5 sprints) |
| Wave-24 dry-run findings (drives scope amendment input) | Available | `ec074a1 wave-24 r-prep — RB-GA-CUTOVER §3 dry-run (G1..G6 all GREEN)` + `2128ce3 wave-24 GA-readiness final audit + go/no-go decision board` |
| Vendor shortlist | Not yet authored | — |
| RFP draft | Not yet authored | — |
| SOW template | Not yet authored | — |
| Engagement letter | User-bound (vendor selection + procurement gate) | — |

### 2.2 Wave-25 stream #1 SEAL-gate (forward-looking; not enforced by this audit)

- ✅ Author RFP draft against the wave-19 SEALED scope, incorporating wave-24 dry-run findings (notably the chaos combined-failure surfaces + BYOK 4-provider matrix + pilot-tenant attack surface).
- ✅ Vendor shortlist (≥ 3 candidates) with comparable SOC 2 / ISO 27001 + CREST/OSCP qualifications + sector-experience (multi-tenant SaaS + BYOK).
- ✅ SOW template draft with concrete deliverables (executive summary, technical findings, CVSS scores, remediation guidance, retest scope).
- ✅ Scope-freeze decision: ratify or amend the wave-19 SEALED scope based on wave-24 dry-run + final-audit findings.
- ✅ DEFER counter (drift detector cross-ref stream #4) reflects pentest engagement state.

### 2.3 Post-wave-25-SEAL projection

Stream #1's RFP + shortlist + SOW + scope-ratify-or-amend decision converts the **"pentest vendor-bound"** DEFER row (from wave-24 §11) from **OPEN-vendor-bound (no agent action available)** to **OPEN-user-bound-procurement (vendor selection + engagement letter signature)**. This is the narrowest the DEFER row can be without agent overreach into procurement.

**Cutover-day impact:** the GA-cutover meeting (`GA-GATE-GO-NOGO-TEMPLATE.md`) does NOT require pentest *completion* (which is post-GA per wave-19 scope); it requires pentest *engagement plan ratified*. Stream #1's SEAL puts the engagement plan in place — DEFER counter for this row converts from `8 (5 user-bound + 3 vendor-bound)` to `8 (6 user-bound + 2 vendor-bound)` (one vendor-bound flips to user-bound-procurement, but the row stays OPEN).

---

## 3. DEBT-015-BUILD path-(3) closure verification (cross-ref stream #2)

Stream #2 is the **LAST in-charter path-attempt** before DEBT-015-BUILD escalates to **P1 alternative-architecture** per wave-24 §4.3.

### 3.1 Wave-history narrative (cumulative through wave-25 dispatch)

- **Wave-15 → wave-21**: docs P2 portion CLOSED (Node 22+ engine pin lift; 11 doc files migrated).
- **Wave-22**: 90 docs files normalised to extensionless MDX; theme-alias patch applied; new same-class residual surfaced (server-bundle clientModule chunk-registry literal `require("@site/*.mdx")` + `require("@generated/*.json")` strings failing to externalise in pnpm-isolated layout).
- **Wave-23**: configureWebpack plugin attempted (path-A no effect on 113 + 77 literal-require count); root cause narrowed to `@docusaurus/babel/lib/preset.js` server `modules: 'auto'` vs client `modules: false`. Fix-path order documented: (1) babel-preset patch → (2) custom babel plugin → (3) ssgRequire sandbox-eval → (alt-arch).
- **Wave-24 stream #3** (`d91a690 chore(docs): wave-24 babel preset patch + ssgRequire path forward (DEBT-015-BUILD PARTIAL)`): path-(1) babel-preset patch + path-(2) custom babel plugin both attempted; commit message text indicates **PARTIAL** outcome — neither fully eliminated the literal-require count in the server bundle. The "ssgRequire path forward" notation in the commit message signals that wave-24 explicitly handed off to wave-25 path-(3).
- **Wave-25 stream #2** (this wave, in-flight): path-(3) ssgRequire sandbox-eval over `build/assets/js/<chunkId>.<hash>.js` client chunks.

### 3.2 Wave-25 stream #2 SEAL-gate (forward-looking; not enforced by this audit)

- ✅ ssgRequire sandbox-eval mechanism implemented + applied at SSG render time so the literal `require("@site/X")` / `require("@generated/Y")` server-bundle references are intercepted and resolved via a sandboxed evaluation of the corresponding pre-built client chunk.
- ✅ `pnpm --filter docs build` locally on Node 22 reaches `compiled SUCCESS` for all 113 `@site/*.mdx` routes + 77 `@generated/*.json` references — no `MODULE_NOT_FOUND` errors during SSG.
- ✅ Per-locale verification (en + pt-BR + de + es-419 — same 4-locale matrix as wave-21/22/24 docs touch waves).
- ✅ Spec + reference + INV-promotion validators exit 0 (this audit enforces).
- ✅ Docs CI workflow re-enabled (post-billing-reinstatement) and green.

### 3.3 Wave-25 path-(3) success/failure contingencies

**Path-(3) success (ssgRequire sandbox-eval resolves all literal requires)** → DEBT-015-BUILD flips to **CLOSED** in register; row carries `wave-25 ssgRequire sandbox-eval` as the final closure commit; docs CI gate becomes runnable.

**Path-(3) failure (sandbox-eval cannot externalise reliably or causes SSG performance regression)** → DEBT-015-BUILD escalates to **P1 alternative-architecture decision** per wave-24 §4.3 deadline. **Wave-26 dispatches a docs-platform-eval stream** (Docusaurus 3 → alternatives: Astro Starlight, Mintlify, Next.js + nextra, or invasive runtime-patch). This is the wave-24 §4.3 explicit deadline — wave-25 is the LAST opportunity to close in-place.

**Closure verification (post-wave-25-SEAL, survey-only):** this audit does NOT preempt stream #2's SEAL commit. Once stream #2 SEALs, wave-26 sweep reconciles the DEBT-015-BUILD row state: CLOSED if path-(3) succeeded, or P1 alt-arch escalation if not.

---

## 4. GA-readiness DEFER drift detector (cross-ref stream #4)

Stream #4 addresses a structural risk surfaced by the wave-24 GA-readiness final audit: the **CONDITIONAL GO** verdict depends on a frozen DEFER count of 8 items (5 user-bound + 3 vendor-bound). Without a drift detector, a wave-25 or wave-26 stream could silently add a DEFER row (e.g., a new vendor dependency surfacing during pentest engagement, or a new compliance requirement from LFPDPPP attorney review) and the cutover meeting would only discover the drift in the meeting itself.

### 4.1 Stream #4 SEAL-gate (forward-looking)

- ✅ Author `scripts/validate_ga_defer_drift.py` (or equivalent) that:
  - Parses `specs/_audits/2026-05-16-ga-readiness-final.md` §11 (DEFER counter table) for the baseline DEFER set.
  - Parses `specs/_audits/2026-05-15-debt-register.md` for OPEN rows flagged as `defer-class: user-bound | vendor-bound`.
  - Cross-checks the two for drift (any OPEN DEFER row not in the baseline = drift; any baseline row missing from register = drift).
  - Exits 0 if drift count is 0; exits 1 with a summary diff otherwise.
- ✅ Optional CI gate (`.github/workflows/ga-defer-drift.yml`) that runs the validator on every PR touching `specs/_audits/*` or `specs/_compliance/GA-GATE-*` paths.
- ✅ Companion evidence-doc requirement: any drift-positive addition must ship with `specs/_audits/<date>-defer-add-<row-id>.md` documenting the rationale + classification + ETA.

### 4.2 Post-wave-25-SEAL projection — DEFER counter state

Stream #4's drift-detector locks in the wave-24 GA-readiness final-audit DEFER count of 8 as the cutover-bar; any subsequent wave (including this wave-25) must explicitly document any DEFER additions. **This audit checks for drift at wave-25 dispatch:** no new DEFER rows have been added between wave-24 SEAL (`e9ee8eb`) and wave-25 dispatch — DEFER count remains **8 (5 user-bound + 3 vendor-bound)**.

**Forward-looking projection at wave-25 SEAL:** if stream #1 SEALs (pentest engagement plan ratified), one vendor-bound row flips classification to user-bound-procurement but DEFER count stays 8. If stream #2 succeeds (DEBT-015-BUILD CLOSED), DEFER count drops from 8 → 7 (no impact — DEBT-015-BUILD was not in the 8-DEFER set; it's a separately-tracked P2). If stream #2 fails (alt-arch escalation), a new DEFER row is added (alt-arch decision pending) → DEFER count grows 8 → 9. The drift detector will surface this in the wave-26 sweep regardless.

---

## 5. INV registry state (count by severity)

Per `python3 scripts/validate_canonical_consistency.py` on this branch (post-sweep, pre-SEAL):

| Metric | Count (wave-25 base) | Δ vs wave-24 close |
|---|---|---|
| INVs declared (registry §3 rows) | **197** | 0 (wave-24 sealed no new canonical INVs) |
| └ CRITICAL | **61** | 0 |
| └ HIGH | **132** | 0 |
| └ MEDIUM | **4** | 0 |
| └ LOW / UNKNOWN | **0** | 0 |
| Aliases declared (registry §5) | 15 | 0 |
| TLA+ verified (declared INVs proved in `specs/tla/*.tla`) | **81** | 0 |
| Code-referenced (declared INVs cited in `crates/*/src/`) | 103 | 0 |
| Test-referenced (declared INVs cited in `crates/*/tests/`) | 89 | 0 |
| Orphan refs (in code, NOT in registry+aliases) | **0** | 0 |
| CRITICAL without TLA+ proof | **1** (INV-PAT-REVOKE-PROPAGATION — TLA+ exempt per wave-24 GA-readiness §4.3: sub-second wall-clock obligation, not distributed-consensus property) | 0 |
| Declared with NO code/test reference | 76 | 0 |
| Declared test-only (test ref but no src/) | 18 | 0 |

Per `python3 scripts/validate_inv_promotion.py`: registry coverage **143/143** (all WI-declared INVs present in registry §3). Registry stable at 197 declared.

**INV-DRAFT → INV-PROMOTED count this wave:** **0** (no per-INV DRAFT entries in registry §3 sub-sections; source-of-drafts (`_spec_contract.md` § INV blocks + apps/docs OpenAPI) is empty post-wave-23-SEAL — confirmed by `grep -n DRAFT specs/03_architecture/invariant_registry.md` returning only the doc-level `doc_status: DRAFT` front-matter marker, which is staffing-blocked per F-09 and orthogonal to per-entry promotions).

### 5.1 Why no promotions this wave

Same pattern as wave-22 / wave-23 / wave-24 close: wave-25 in-flight streams are predominantly closure / hardening / production-wiring / audit work, not new structural INV authoring. None of the wave-25 streams (per §1 catalogue) introduce new canonical INVs:

- Stream #1 (pentest engagement) — RFP/SOW are procurement artifacts, not INV-defining.
- Stream #2 (DEBT-015-BUILD path-3) — docs build remediation, no INV impact.
- Stream #3 (DEBT-008 number-discrepancy) — register text reconciliation, no INV impact.
- Stream #4 (DEFER drift detector) — script + CI gate, references existing INVs.
- Stream #5 (endurance 10min dress-run) — operational rehearsal, references existing SLO/INV.
- Stream #6 (statuspage init dress-run) — operational rehearsal, no new INV.
- Stream #7 (tenant-config CF prod-wire) — production wiring of existing region-resolver INVs (§3.27 family).
- Stream #8 (pre-GA security attestation) — consolidates existing compliance INVs.
- Stream #9 (wave-24 adversarial review) — review pass, no INV authoring.
- Stream #10 (this sweep + Lote 7 RACI detail) — RACI rows are operational doc, not invariant-defining.

**Net: 0 promotions warranted from this audit stream.**

### 5.2 INV registry severity-breakdown snapshot (canonical, wave-25 base)

For the GA-cutover meeting:

- **61 CRITICAL** — failure mode = blast-radius cross-tenant or audit-chain integrity break.
- **132 HIGH** — failure mode = per-tenant correctness or compliance contract.
- **4 MEDIUM** — failure mode = observability / operational discipline gap.
- **0 LOW / 0 UNKNOWN** — clean classification.

**Severity-classification health:** zero UNKNOWN entries means every INV has been intentionally severity-tagged. The 4 MEDIUM entries are all operational-discipline class (S-17 OPS domain — §3.27) and are tracked under DEBT-010 P2 deferrals not GA-blockers.

---

## 6. DEBT register survey — pre-wave-25-SEAL state

Per `specs/_audits/2026-05-15-debt-register.md` (last reconciled in wave-23 close — DEBT-025 added 2026-05-16; total canonical OPEN rows surveyed below). No DEBT closures performed by this stream (charter-bound survey-only).

### 6.1 Open count + per-priority breakdown (canonical rows; pre-wave-25-SEAL)

Per the wave-24 closure §8.1 baseline + wave-24 SEAL commits between `33138b5` and `e9ee8eb`:

| Priority | Open IDs | Count | Wave-25 closure ETA |
|---|---|---|---|
| **P0** | DEBT-003 (AWS Artifact PDF — user-bound) | 1 | Pending human (no wave-25 stream; counted in DEFER) |
| **P1** | DEBT-008 (partial — `{dual-approval, ratelimit}` CI-nightly + post-wave-24 reconciliation pending stream #3) | 1 partial | Stream #3 narrative reconciliation; structural empirical-CLOSED subset = `{audit-chain, hash, dedup, tenant-path, handler-cas, auth-schema, chunker, multipart-schema, pat, clerk}` = 10 crates post-wave-24-SEAL |
| **P1** | DEBT-010 (partial 4/11 — 7 P2/P3 deferred post-GA) | 1 partial | Deferred post-GA |
| **P1** | DEBT-013 (partial 6/10 — 4 explicit deferrals) | 1 partial | Wave-22 stream #6 tightened regression CI; deferrals themselves remain post-GA |
| **P2** | DEBT-015-BUILD (path-(1)/(2) attempted wave-24 PARTIAL; path-(3) in flight wave-25 stream #2) | 1 | Stream #2 path-(3) or escalate to P1 wave-26 alt-arch |
| **P2** | DEBT-016 (Statuspage go-live — engineering-CLOSED wave-24; user-bound provisioning at T-7d) | 1 | Pending human at T-7d pre-launch |
| **P2** | DEBT-025 (LFPDPPP MX attorney sign-off — added wave-23 v1.2.1) | 1 | Stream-of-attorneys; agent absorption deferred to wave-26 |

**Post-wave-24-SEAL closures actually landed at wave-25 base** (verified against `git log` between `33138b5` and `e9ee8eb`):
- ✅ `e421a6c merge wt/r-prep-adr-0034b-dual-hat into main (wave-24)` — ADR-0034b dual-hat decomposition SEALED.
- ✅ `3ab6fc2 merge wt/r-prep-ga-cutover-dryrun into main (wave-24)` — GA cutover dry-run G1..G6 all GREEN (per `ec074a1`).
- ✅ `df67e1e merge wt/r-prep-debt-016-statuspage-urls into main (wave-24)` — DEBT-016 engineering-CLOSED (statuspage URL substitution mechanism per `4076391`).
- ✅ `b71a360 merge wt/r-prep-inv-registry-wave24-sweep into main (wave-24)` — wave-24 sweep SEALED with closure audit `specs/_audits/2026-05-16-wave24-closure.md`.
- ✅ `d58734f merge wt/r-prep-wave23-adversarial-review into main (wave-24)` — wave-23 adversarial review 9.20/10 PASS (per `90d9849`).
- ✅ `4a54407 merge wt/r-prep-ga-readiness-final-audit into main (wave-24)` — GA-readiness final audit CONDITIONAL GO verdict authored.
- ✅ `a3b1a20 merge wt/r-prep-debt-015-build-babel-patch into main (wave-24)` — DEBT-015-BUILD path-(1)+(2) attempt PARTIAL; path-(3) handed off to wave-25 stream #2.
- ✅ `513670f merge wt/r-prep-debt-008-mutation-wave24 into main (wave-24)` — chunker + multipart-schema empirically CLOSED; 3 large crates (`r2-multipart`, `quota-cas`, `webauthn`) re-classified to CI-nightly per the wave-24 commit narrative.
- ✅ `e9ee8eb merge wt/r-prep-pat-clerk-mutation-sweep into main (wave-24)` — PAT/clerk combined-bucket empirical lift SEALED.

**Total truly-OPEN canonical rows pre-wave-25-SEAL:** 7 (unchanged from wave-24 close — DEBT-008/010/013 partial + DEBT-003 P0 + DEBT-015-BUILD + DEBT-016 user-bound + DEBT-025 attorney-bound).

**Post-wave-25-SEAL projection** (assuming all wave-25 streams SEAL):
- DEBT-008 narrative reconciled (stream #3 updates register text); empirical subset stays at 10 crates; CI-nightly continues for `{dual-approval, ratelimit, r2-multipart, quota-cas, webauthn}` = 5 crates.
- DEBT-015-BUILD flips to **CLOSED** (path-(3) success) OR escalates to **P1 wave-26 alt-arch** (path-(3) failure).
- DEBT-016 engineering-CLOSED stays; user-bound at T-7d unchanged.
- DEBT-003 still OPEN user-bound.
- DEBT-025 still OPEN attorney-bound (wave-26 absorption candidate).
- DEBT-010 + DEBT-013 still partial deferred post-GA.

**Net canonical OPEN: 7 → 6 (best case, DEBT-015-BUILD flips to CLOSED)** OR **7 → 7 with DEBT-015-BUILD escalated** (worst case — escalation flips priority class P2 → P1).

### 6.2 Closure ETA summary

| ETA bucket | Rows |
|---|---|
| Wave-25 SEAL (this wave; ~next 1–2 weeks) | DEBT-015-BUILD (stream #2 path-3 ssgRequire — risk: sandbox-eval may not externalise reliably); DEBT-008 narrative reconciliation (stream #3) |
| Pre-GA Gate (T+30d) | DEBT-003 (user-bound; AWS Artifact PDF download); DEBT-025 (attorney-side) |
| T-7d pre-launch | DEBT-016 (user-bound; Statuspage provisioning — URL substitution mechanism in place) |
| Post-GA (T+90d horizon) | DEBT-010 P2/P3 (7 tickets), DEBT-013 deferrals (4 tickets) |

---

## 7. Quality gates verified

Per the wave-25 sweep charter:

| Gate | Command | Result |
|---|---|---|
| INV promotion validator | `python3 scripts/validate_inv_promotion.py` | exit 0 — registry coverage 143/143; all WI-declared INVs present. |
| Canonical consistency validator | `python3 scripts/validate_canonical_consistency.py` | exit 0 — 197 INVs declared; 0 orphan refs; 1 CRITICAL without TLA+ (INV-PAT-REVOKE-PROPAGATION — TLA+ exempt per wave-24 §4.3 wall-clock obligation pattern). |
| Spec corpus validator | `python3 scripts/validate_specs.py` | exit 0 — 446 with schema + 9 YAML-only (455 total). |
| Reference validator | `python3 scripts/validate_references.py` | exit 0 — no dangling references (267 INV uses; 82 SLO uses; 234 RB uses; 36 ADR uses; 11 FF-HR uses; 31 SLO definitions; 127 RB definitions; 27 ADR definitions; 199 INV definitions). |

---

## 8. Wave-26 candidate streams

Per the wave-25 charter §"next-wave (wave-26) candidate streams — anchor: GA-1 freeze + Lote 6 v1.0.0 GA absorption":

| # | Stream | Rationale | Estimated cost |
|---|---|---|---|
| 1 | **GA-1 freeze anchor stream** (anchor) | Wave-26 is the **final wave before GA cutover**. Anchor stream executes the GA-1 freeze: code freeze on `main`, Lote 6 v1.0.0 GA absorption commit, RB-GA-CUTOVER §0 checklist run authorization handoff, ADR-0034b 2-key (Owner + on-call SRE) signature block populated against the GA-readiness CONDITIONAL GO verdict from wave-24. | 1 codex/Opus authoring the freeze commit + 2-key sign-off ceremony; 1 Sonnet absorbing wave-25 findings into the freeze. |
| 2 | **Wave-25 adversarial review (codex Opus pass)** | Mandatory per charter "all P1-classified streams must close before next wave unblocks P2/P3". Cross-review wave-25 streams #1 pentest engagement, #2 DEBT-015-BUILD path-3, #3 DEBT-008 reconciliation, #4 DEFER drift detector, #8 pre-GA security attestation, #9 wave-24 adversarial review. | ~1 codex Opus pass per P1 stream + 1 audit doc per stream. |
| 3 | **DEBT-015-BUILD wave-26 escalation** (only if wave-25 stream #2 path-(3) failed) | Alt-arch decision — docs-platform-eval stream: Docusaurus 3 → Astro Starlight / Mintlify / Next.js + nextra / invasive runtime-patch. **Hard wave-24 §4.3 escalation deadline already reached** — wave-26 alt-arch is the LAST resort before GA-Limited launches without the docs CI gate. | 1 codex Opus for alt-arch decision + 1 Sonnet for implementation if Docusaurus stays. |
| 4 | **DEBT-008 wave-26 follow-on** (only if wave-25 stream #3 reconciliation surfaces a < 75% CI-nightly streak for any of `{dual-approval, ratelimit, r2-multipart, quota-cas, webauthn}`) | Targeted-test additions per the wave-15 / wave-21 / wave-22 / wave-23 / wave-24 empirical-closure methodology. | 1 Sonnet per remediation. |
| 5 | **Pentest engagement scope-ratify-or-amend absorption** | Once wave-25 stream #1 SEALs (RFP + shortlist + SOW + scope decision), absorb the decision into `specs/_audits/2026-05-16-pre-ga-pentest-scope.md` v2 (ratified) or v2 (amended). | 1 Sonnet absorption. |
| 6 | **DEBT-025 LFPDPPP MX attorney absorption** (T+30d-ish horizon) | LFPDPPP MX attorney-side review absorption — wave-23 stream #6 drafted the package; once attorney returns sign-off / edits, an agent absorbs the legal-side feedback into the residency annex + retention table + DPA addendum. | 1 Sonnet × absorption. |
| 7 | **24h endurance soak streak ratchet** | Wave-25 stream #5 dress-rehearsed the 10-minute compressed soak; wave-26 schedules a continuous 7-day endurance soak as the cutover-day evidence (SLO observation streak ≥ 168 h). | Wall-clock; 1 Sonnet for evidence absorption. |
| 8 | **DEBT-010 P2 CI optimisation batch** (carried from wave-23/24 §6 — post-GA polish) | ~30 min/PR cumulative savings; concurrency cancel + shared rust-cache key + TLC matrix + paths-filter audit. | 1 Sonnet × 4 P2 tickets. |
| 9 | **DEBT-013 perf optimisation deferrals execution** (carried from wave-23/24 §6 — post-GA polish) | OPT-03b + OPT-04ph2 + OPT-08 remain. | 1 Sonnet × 3 deferrals. |
| 10 | **Wave-26 INV registry + DEBT register hygiene sweep** | Cadence preserved — same charter as wave-19/20/21/22/23/24/25 sweep streams. | 1 Sonnet × 30 min. |

### 8.1 Caveats from wave-25 sweep findings

- **INV registry stable at 197** — wave-24 SEAL added zero new canonical INVs; wave-25 in-flight streams (per §5.1) project zero additions. **Wave-26 GA-1 freeze locks the count at 197 declared.** Any post-GA additions are R-prep-post-GA absorption work, not GA-blockers.
- **No DRAFT entries detected** in `specs/03_architecture/invariant_registry.md` §3 sub-sections at wave-25 base; the doc-level `doc_status: DRAFT` front-matter remains staffing-blocked per F-09 until ≥ 2 reviewers nominated (separate from per-entry status — same caveat as wave-22/23/24).
- **CRITICAL-no-TLA+ count is 1** (INV-PAT-REVOKE-PROPAGATION) — TLA+ exempt per wave-24 GA-readiness §4.3 (wall-clock obligation, not distributed-consensus property). The wave-24 PAT-revoke TLA+ stream (#6) was attempted; the wave-24 GA-readiness final-audit classified it as TLA+-exempt rather than TLA+-required. **No wave-26 follow-on warranted** unless the GA-cutover meeting reverses the exempt classification.
- **DEBT-008 final state is 10 empirically-CLOSED crates + 5 CI-nightly** post-wave-24-SEAL (wave-24 stream #2 + PAT/clerk combined-bucket added pat + clerk to the empirical set; the 3 large crates were re-classified to CI-nightly per the wave-24 commit narrative). **No wave-26 first-sweep stream needed** unless wave-25 stream #3 surfaces a < 75% CI-nightly streak.
- **DEBT-015-BUILD path-(3) is the LAST in-charter attempt** — wave-26 alt-arch is the escalation deadline already triggered by the path-(3) outcome. If path-(3) succeeds (wave-25 stream #2 CLOSED), DEBT-015-BUILD flips to CLOSED and wave-26 needs no follow-on. If path-(3) fails, wave-26 stream #3 is mandatory and escalates DEBT-015-BUILD to P1 with alt-arch decision.
- **GA-readiness DEFER counter locked at 8** (5 user-bound + 3 vendor-bound) by wave-25 stream #4 drift detector. Wave-26 GA-1 freeze cutover meeting consumes this counter as the canonical residual-blocker list.
- **Wave-24 adversarial review (wave-25 stream #9) is the bound resource** — wave-24 had 9 in-flight streams of which 5 are P1-classified (dry-run, DEBT-008 batch, DEBT-015-BUILD babel-patch, PAT-revoke TLA+, GA-readiness final audit). Largest review pass to date. **Required before wave-26 GA-1 freeze** unblocks.
- **Wave-26 anchor is GA-1 freeze + Lote 6 v1.0.0 GA absorption** — per the charter §"next-wave (wave-26) candidate streams — anchor: GA-1 freeze + Lote 6 v1.0.0 GA absorption". Wave-26 is the final wave before GA-cutover; any structural blocker surfaced in wave-25 streams (e.g., DEBT-015-BUILD alt-arch) must be resolved or explicitly waived by the GA-cutover meeting authorization.

### 8.2 Wave-26 entry caveats

- **DCO + Co-Authored-By preserved** on every commit (same as wave-19 through wave-25).
- **No `--no-verify` hooks.** Pre-commit failures must surface root cause.
- **Synchronous Bash only.** No `run_in_background`. Same charter as wave-19/20/21/22/23/24/25.
- **30–40-min time budget per stream** (matches wave-22/23/24/25 cadence); 8–10 streams per wave realistic.
- **Wave-26 SEAL gate:** all wave-25 P1 streams (#1 pentest engagement, #2 DEBT-015-BUILD path-3, #3 DEBT-008 reconciliation, #4 DEFER drift detector, #8 pre-GA security attestation, #9 wave-24 adversarial review) verified closed before wave-26 unblocks the GA-1 freeze.
- **GA-1 freeze becomes the wave-26 anchor stream** — its freeze decision authorizes `RB-GA-CUTOVER.md` §0 checklist run. **This is the last wave before GA cutover.**

---

## 9. Snapshot record

- **Branch:** `wt/r-prep-inv-registry-wave25-sweep`
- **Base commit:** `e9ee8eb` (wave-24 SEAL tip)
- **Sweep date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-25 hygiene agent)
- **Sign-off:** Gustavo Schneiter (final approver, async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>

---

## 10. Cross-references

- `specs/_audits/2026-05-16-wave24-closure.md` (wave-24 closure; predecessor).
- `specs/_audits/2026-05-16-wave23-closure.md` (wave-23 closure; structural patterns continued).
- `specs/_audits/2026-05-16-wave22-closure.md` (wave-22 closure; baseline cadence).
- `specs/_audits/2026-05-15-debt-register.md` (DEBT register canonical state).
- `specs/03_architecture/invariant_registry.md` (197 declared at wave-25 base; severity-classification clean).
- `specs/_audits/2026-05-16-ga-readiness-final.md` (wave-24 stream #8 final audit; CONDITIONAL GO verdict; 8-item DEFER counter).
- `specs/_audits/2026-05-16-ga-final-checklist.md` (operator-runnable boolean checklist; wave-25 stream #4 drift detector references this).
- `specs/_audits/2026-05-16-pre-ga-pentest-scope.md` (wave-19 SEALED pentest scope; wave-25 stream #1 ratifies or amends).
- `specs/_audits/2026-05-16-debt-015-build-final.md` (wave-23 root-cause narrowing baseline; sets fix-path order picked up by wave-24 stream #3 path-(1)/(2) + wave-25 stream #2 path-(3)).
- `specs/_audits/2026-05-16-ga-cutover-dryrun.md` (wave-24 stream #1 dry-run G1..G6 all GREEN; provides scope-amendment input for wave-25 stream #1).
- `specs/_audits/2026-05-15-canonical-consistency-baseline.md` (CI ratchet floor; DEBT-004 closure log).
- `specs/_compliance/GA-GATE-CRITERIA.md` (59 criteria across 6 tracks; wave-25 stream #8 pre-GA security attestation aggregates the security track).
- `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md` (the meeting whose APPROVED decision authorizes `RB-GA-CUTOVER.md`; wave-26 GA-1 freeze populates).
- `specs/_runbooks/RB-GA-CUTOVER.md` (cutover runbook; wave-24 stream #1 dry-run rehearsed §3 G1..G6; wave-26 GA-1 freeze authorizes §0 checklist run).
- `.github/workflows/mutation-nightly.yml` (CI-nightly artifact precedence — covers `{dual-approval, ratelimit, r2-multipart, quota-cas, webauthn}` continuously at wave-25 SEAL).
