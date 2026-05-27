# Wave-26 Adversarial Review — Wave-25 Streams

> **Doc kind:** adversarial review (no canonical front matter — `_audits/` excluded from `validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-26 adversarial-review agent (Claude Opus 4.7) — branch `wt/r-prep-wave25-adversarial-review`.
> **Base:** `main` @ `2a4e00c` (wave-25 final merge tip).
> **Predecessor wave:** wave-25 adversarial review (`specs/_audits/sealed/2026-05-16-wave24-adversarial-review.md`) — 6.95/10 CONDITIONAL → drove wave-25 P0 recovery via cherry-picks `d172a4a` + `8fa1c22`.
> **Scope:** independent SOTA-bar review of the 11 dispatched wave-25 R-PREP streams + 2 wave-24 recovery cherry-picks, with focused inspection of: (a) recovery cherry-pick integrity, (b) DEBT-015-BUILD ssgRequire path #3 + i18n `.mdx`-suffix pivot, (c) pentest scope-freeze vendor scoring defensibility, (d) endurance dressrun GREENLIGHT + 5x regression-flip validity, (e) Lote 7 RACI single-A discipline + Pairing-Alpha/Beta coherence, (f) GA-readiness drift detector false-positive resistance.
> **Charter:** review-only; no source code changes; SOTA-bar 8.5 PASS.
> **Score formula:** `clamp[0,10](10 - 1.5·P0 - 0.5·P1 - 0.15·P2 - 0.05·P3)`.

---

## 1. Headline result

| Field | Value |
|---|---|
| Aggregate score | **9.00 / 10** (PASS — above 8.5 SOTA-bar) |
| P0 / P1 / P2 / P3 | **0 / 1 / 2 / 2** |
| Verdict | **PASS — SEAL wave-25** with one named hygiene follow-on for wave-26 |
| Recommendation | **SEAL wave-25** and dispatch a wave-26 hygiene commit to refresh stale TLA-coverage figures (81 → 82) in three downstream audit docs that landed BEFORE the `8fa1c22` cherry-pick reordered the registry state. |

Wave-25 is a substantial recovery + delivery wave. The two wave-24 misses identified in `2026-05-16-wave24-adversarial-review.md` are fully closed at HEAD: `d172a4a` (audit-analytics WallClock symmetry) and `8fa1c22` (auth_pat_revoke.tla) are both ancestors of `2a4e00c`; the canonical-consistency validator now reports `CRITICAL without TLA+ proof: 0` (was 1); `audit_analytics.rs::rate_limit_check` carries the fail-CLOSED `clock_unavailable` 503 path. DEBT-015-BUILD is empirically CLOSED on all four locales with an honest re-framing of the wave-22/24 narrative. Single residual is a documentation-drift P1: three audit docs authored EARLIER in the wave-25 merge order (security-attestation, ga-readiness-final, wave25-closure) still cite "81 TLA-verified / 1 CRITICAL TLA-exempt" figures that the registry no longer matches post-cherry-pick. This is the symmetric counterpart to wave-24 P0/P1 — registry now leads doc text by one row, where wave-24 had doc text leading registry by one exemption.

---

## 2. Per-stream scoring

| # | Stream / commit | Score | Notes |
|---|---|---:|---|
| 1 | debt-008 number-discrepancy (22efd4e) | **9.0 / 10** | Reconciliation of three figure pairs (chunker 97.9/100; multipart-schema 90.6/84.6) against wave-24 canonical empirical (95.79 % / 97.44 %); wave-23 doc bumped to v1.1.0 with `superseded_by` header + inline annotations + §9 decisions log; debt-register verified already consistent (no change); P2-04 from wave-23 adversarial review explicitly CLOSED. |
| 2 | ga-checklist-drift-detector (b974be3) | **9.0 / 10** | 146-line stdlib-only script + CI advisory wire + companion audit + scrub doc. Detector runs clean at HEAD (all 4 stale-signal occurrences are inside `wave-25 scrub` exempt markers). 7-row STALE_SIGNALS list defensible. Minor: `CHECKLIST_ROW_RE` matches both `- [ ]` and `- [x]` while docstring says "unchecked only" (functionally tighter, semantically defensible). |
| 3 | inv-registry-wave25-sweep (7cf4bbb) | **8.5 / 10** | Wave-25 closure audit cataloguing all 10 dispatched streams with explicit disposition (1 CLOSED, 9 IN FLIGHT at write-time); explicit charter §"survey-only — no SEAL from this stream" framing. Carries `tla-verified: 81` figure that became stale 6 merges later when `8fa1c22` cherry-pick added auth_pat_revoke.tla (contributes to P1). |
| 4 | wave24-adversarial-review (ffdd876) | **9.2 / 10** | The catalyst review that drove wave-25 P0 recovery. Methodologically clean — separates per-stream rigor (median 8.59) from wave-final reconciliation failure (penalised score 6.95). Identified both NOT-IN-MAIN streams + the verdict-distorting "TLA+ exempt per §4.3" assertion in GA-readiness final audit. Fix shape spec was actionable; both fix-recommendations landed (cherry-picks). |
| 5 | statuspage-init-dressrun (ab074c7) | **9.0 / 10** | 560-LOC bash orchestrator + 257-LOC Python verifier + evidence JSON + 134-LOC audit. 4 PASS steps, 0 DEGRADED, 0 FAIL. Production-vs-dress-run table at §2 is exemplary — explicit on what is and is not simulated (no Statuspage tenant; no real DNS edit; no `pnpm build`; Node mirror of `getStatuspageUrl()` instead). Per-step VID fingerprint binds JSON evidence to runbook+helper SHA-256. |
| 6 | pre-ga-security-attestation (619d449) | **6.5 / 10** | Comprehensive 386-LOC vendor-day-1 + GA-cutover-board package consolidating 8 waves of adversarial trend + INV-CRITICAL TLA+ coverage + DEBT-008 aggregate + chaos + endurance + compliance + BYOK + audit chain. Carries stale "61 CRITICAL / 81 TLA-verified / 1 CRITICAL TLA-exempt (INV-PAT-REVOKE-PROPAGATION)" figures (§1 table line 29; §3 lines 78-80; §V&V table line 306; changelog line 379) that contradict registry §3.28 row 593 ("TLA-VERIFIED Wave-24 R-PREP") + validator output at HEAD (`CRITICAL without TLA+ proof: 0`). This is a SEAL-gate document distributed externally; carrying stale figures is the primary wave-25 P1. |
| 7 | pentest-engagement-scope-freeze (1499bc9) | **8.0 / 10** | 444 + 427 + 422 = ~1.3k LOC of vendor-procurement-ready artifacts (scope-freeze + contract template + 5-vendor shortlist). Three-prong mandate (ASVS L2/L3 + 46 chain validation + ≥30 % zero-day budget) is binding and well-scoped. §7 capability checklist + §7.1 disqualifying gaps are operator-defensible. P2: §0.1/§0.2 rubric in vendor-shortlist declares "weighted ×7 = 70 max" + "weighted ×3 = 30 max" but §1.1..§5.1 per-vendor tables sum 0–10 per dimension flat (max secondary observed = 36 > 30 max), and §6.1 scoreboard columns labeled "(70 max)" + "(30 max)" carry raw sums (Bishop Fox 62+36=98 → normalised 89). Relative ranking is consistent (98/110→89, 96/110→87, etc.), but the methodology vs. application drift weakens defensibility when shared with vendors. |
| 8 | lote-7 RACI detail (4326a1f) | **9.3 / 10** | 245-LOC audit + 15-row matrix (in `_proposals/...framework-reviewer-roles-addendum.md §6`) + per-row rationale + dual-hat fallback row. §5 single-A verification table (15 rows × Owner=A, No-other-A) is the desired discipline; §4.2/§4.3 Pairing-Alpha/Pairing-Beta affected-rows lists coherently cover R-status migrations; §4.4 cross-veto preservation argument sound. ADR-0034b 0.1.0→0.1.1 + addendum 0.1.1→0.1.2 cross-ref hygiene executed (§7.2/§7.4 renumber). |
| 9 | debt-015-build-ssgrequire-path3 (b01d14a) | **9.0 / 10** | Wave-25 re-frames waves 22-24 narrative empirically: babel patch DID defuse `@site/*` literal-require on en-US; real residual was 15 `.mdx`-suffix cross-links in 12 i18n files (pt-BR + es-419 + de) tripping `onBrokenMarkdownLinks: throw`. Fixes: (a) 15 links → `pathname://` protocol (Docusaurus's documented escape-hatch for `draft:true` translations); (b) defensive `@site/*` + `@generated/*.json` resolver branches in `lib/ssg/ssgNodeRequire.js` (try-real-resolve-then-noop). All 4 locales green; pnpm build / typecheck / lint / specs / refs / install all green; HTTP smoke 7×200. P2: doc explicitly states "the babel-patch retention was *not* re-tested in wave-25" — the conclusion that the babel patch is "load-bearing" is asserted without an empirical re-test. |
| 10 | endurance-10min-dressrun (50130c6) | **9.2 / 10** | Two analyzer hardenings close silent-GREEN false-negative paths: `_extract_p99()` reads either numeric p99 or threshold-map fallback (k6's `--summary-export` only emits avg/min/med/max/p90/p95 by default); `_stdout_threshold_breach_routes()` overrides stale `thresholds` booleans against k6 stdout terminal line. **Both GREENLIGHT path AND 5x regression-flip path validated** with synthetic artifacts (G1 alone flips BLOCK; other 5 gates stay GREEN). Honest framing of mock-target VU saturation as harness limit, not apps/server perf claim. CI fail-closed (exit 10) confirmed on real corrida. |
| 11 | tenant-config-cf-prod-wire (7e1a694) | **9.0 / 10** | `D1TenantConfigStore` (sync `region_label` via cache + async `prefetch` via `CfD1DatabaseReal::scoped_query`) + `build_tenant_region_resolver` (D1-backed or InMemory fallback) + `tenant-region-real` feature gate + `corelink-audit-chain::Region` re-export + 5 tests (3 unit + 2 integration, brief asked ≥ 2 integration). Closes wave-21 §7 caveat. Honest disclosure: (a) wasm32 getrandom failure is pre-existing on `e9ee8eb` baseline (unrelated, tracked separately); (b) CF fetch-handler prefetch wiring deferred (orchestration glue, not the wire itself). Both caveats are named follow-ons, not silent loose ends. |
| 12 | d172a4a cherry-pick (audit_analytics WallClock) | **10 / 10** | `git show d172a4a` byte-identical to `git show 78673c2` (298 lines each; only difference is the SHA on line 1). Wave-24 §"NOT IN MAIN" closed: `apps/server/src/routes/audit_analytics.rs:693` carries the fail-CLOSED `wall_now_ms == 0 → 503 + clock_unavailable audit row` branch; test `analytics_wall_clock_saturated_to_zero_returns_503_and_emits_clock_unavailable_row` pins the contract at line 1187. |
| 13 | 8fa1c22 cherry-pick (auth_pat_revoke.tla) | **10 / 10** | `git show 8fa1c22` byte-identical to `git show 93ebe3c` (811 lines each). Wave-24 §"NOT IN MAIN" closed: `specs/tla/auth_pat_revoke.tla` + `.cfg` + `_nightly.cfg` present at HEAD; registry §3.28 row 593 flipped PLANNED → TLA-VERIFIED; validator reports `CRITICAL without TLA+ proof: 0` (was 1); `tla-verified: 82` (was 81). |

**Mean of 13 streams:** 114.7 / 13 = **8.82**.

---

## 3. Findings

### P1-01 — Stale TLA-coverage figures in three SEAL-gate audit docs post-cherry-pick

**Severity:** P1 (sign-off-document integrity; structural staleness; symmetric counterpart to wave-24 P0).

**Evidence.** The `8fa1c22` cherry-pick added `specs/tla/auth_pat_revoke.tla` to main, flipping the canonical state from:

```
$ python3 scripts/validate_canonical_consistency.py  # wave-24 SEAL state
  CRITICAL without TLA+ proof: 1
  tla-verified: 81
```

to:

```
$ python3 scripts/validate_canonical_consistency.py  # wave-25 SEAL tip (2a4e00c)
  CRITICAL without TLA+ proof: 0
  tla-verified: 82
```

`specs/03_architecture/invariant_registry.md` line 593 was updated by the cherry-pick from `auth_pat_revoke.tla; PLANNED` to `auth_pat_revoke.tla; **TLA-VERIFIED**` (and line 647 catalogs the 5 safety + 1 liveness invariants proven).

**However**, three audit docs authored earlier in the wave-25 merge order still carry the wave-24 figures:

1. `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` (commit `619d449`) — §1 row "INV-CRITICAL TLA+ coverage" (line 29), §3 INV-table row +1 CRITICAL with TLA-exempt rationale (lines 78–80), §V&V `tla-verified: 81` (line 306), changelog 1.0.0 narrative (line 379) all assert `81 TLA-verified / 1 CRITICAL TLA-exempt (INV-PAT-REVOKE-PROPAGATION — wall-clock obligation per §4.3 pattern)`.
2. `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` (touched by `619d449` and earlier wave-24 work) — §1.2 row "Spec corpus" (line 29), §6 INV-CRITICAL +1 line (line 119), §6 "CRITICAL without TLA+ proof: 1" row (line 128), §V&V table (line 362) all carry the same stale framing.
3. `specs/_audits/sealed/2026-05-16-wave25-closure.md` (commit `7cf4bbb`) — line 130 carries `tla-verified: 81`.

**Why this is P1, not P0.** Unlike wave-24 P0 (where audit text asserted GREEN status on commits that were NOT on main), wave-25 has the **substantive engineering work landed on main** — the TLA proof is verified by the canonical-consistency validator at HEAD. The drift is in the consuming-audit text only; the integrity-of-record source (registry + validator) is correct. The pre-GA security attestation is the day-1 vendor-handoff doc + GA cutover board input, so the drift is sign-off-document hygiene, not a substantive verdict gap. A wave-26 micro-patch updating the three doc references makes the docs truthful by construction.

**Fix shape (wave-26).**

- Patch `pre-ga-security-attestation.md`: change "81 TLA-verified / 1 CRITICAL TLA-exempt" → "82 TLA-verified / 0 CRITICAL without TLA+ proof" on lines 29, 78–80, 306, 379; remove the "§4.3 wall-clock-obligation" exemption framing.
- Patch `ga-readiness-final.md`: lines 29, 119, 128, 362 — same numeric refresh + `"INV-PAT-REVOKE-PROPAGATION — TLA-VERIFIED via auth_pat_revoke.tla"` substitution.
- Patch `wave25-closure.md` line 130: `81 → 82`.
- Optional: extend `ga-readiness-defer-drift.py` with a complementary `validate_tla_count_consistency.py` script that re-greps audit docs for `tla-verified: NN` strings and confirms they match the validator output. Prevents recurrence.

### P2-01 — Pentest vendor-shortlist scoring rubric vs application drift

**Severity:** P2 (defensibility / vendor-facing methodology coherence; relative ranking unaffected).

**Evidence.** `specs/_audits/sealed/pentest-vendor-shortlist.md` §0.1 declares "7 primary capability gates (each 0-10; **weighted ×7 = 70 max**)" and §0.2 declares "4 secondary dimensions (each 0-10; **weighted ×3 = 30 max**)". The §6.1 scoreboard then carries column headers `Capability score (70 max)` + `Secondary score (30 max)`.

**However**, the per-vendor §N.1 tables sum 0–10 cells **flat** (no weight multipliers), and the §6.1 scoreboard cells are RAW sums:

| Vendor | Primary raw (max 70) | Secondary raw (max 40 — see below) | Scoreboard total | Normalised /110 × 100 |
|---|---:|---:|---:|---:|
| Bishop Fox | 62 | 36 | 89 | 98/110 = 89.09 ✓ |
| NCC Group | 64 | 32 | 87 | 96/110 = 87.27 ✓ |
| Trail of Bits | 61 | 33 | 85 | 94/110 = 85.45 ✓ |
| Cure53 | 59 | 34 | 85 | 93/110 = 84.55 (rounded up) |
| Doyensec | 58 | 34 | 84 | 92/110 = 83.6 ✓ |

The secondary score column header reads "30 max" but observed values reach 36/40 (Bishop Fox). The "weight × 7" / "weight × 3" rubric language implies a normalisation that does not match the application; the actual computation is `raw_total / 110 × 100`. Relative ranking is internally consistent (Bishop Fox > NCC > ToB ≥ Cure53 > Doyensec), so vendor selection is defensible — but the methodology/application drift is the kind of paper-audit a sophisticated procurement counterparty (Trail of Bits being top-tier) will flag in their RFP response.

**Fix shape (wave-26 or pre-RFP-send):** either (a) re-write §0.1/§0.2 to describe the actual computation ("raw sums, normalised to 100 by 100/110"), or (b) apply the declared weights consistently in the per-vendor tables. Option (a) is the lower-friction path.

### P2-02 — DEBT-015-BUILD wave-25 doc asserts babel-patch is "load-bearing" without empirical re-test

**Severity:** P2 (forward-risk; assumption not validated in the closure wave).

**Evidence.** `specs/_audits/sealed/2026-05-16-debt-015-build-wave25-closure.md` §"Files touched" footer asserts:

> The wave-24 babel patches stay landed unchanged (they are load-bearing — removing them re-introduces the wave-22 nested-require symptom on en-US; this was *not* re-tested in wave-25 to preserve the green build, but the babel-patch diff vs upstream is a single-file change with a clearly documented purpose, so retention is no-risk).

This is internally consistent with the wave-25 re-framing ("the babel patch DID work; wave-24 was looking at the wrong snapshot"), but the load-bearing claim rests on the wave-22 mechanism that wave-25's empirical retest contradicted ("wave-24's conclusion that the patch was empirically invalidated was based on a different snapshot"). If the wave-22 nested-require symptom was always a snapshot artifact, the babel patch may also be inert — and the only load-bearing pieces are wave-25's defensive `@site/*` + `@generated/*` ssgRequire branches plus the i18n `pathname://` cross-link rewrites.

**Why this is P2, not P1.** The retention is no-risk in the immediate term (patch diff is small + documented + understood), and the wave-25 build is empirically green. The risk is **future** — if a Docusaurus 3.11+ upgrade lands the upstream babel changes natively, the wave-24 patches will need a retire/refresh pass, and at that point the question "was the patch ever actually load-bearing?" becomes load-bearing for the upgrade-risk assessment.

**Fix shape (wave-26 or pre-GA):** a single `pnpm build` run on a worktree with the wave-24 babel patches reverted, recording green/red outcome. 5-minute experiment that closes the assumption.

### P3-01 — `ga-readiness-defer-drift.py` regex matches checked + unchecked checklist rows; docstring says unchecked only

**Severity:** P3 (cosmetic; tighter-than-documented behavior is defensible).

**Evidence.** `scripts/ga-readiness-defer-drift.py` docstring (lines 14–16):

> - `specs/_audits/sealed/2026-05-16-ga-final-checklist.md`
>   * Lines beginning with the checklist token `- [ ]` (these are
>     the live DEFER rows operators evaluate).

Regex at line 75:

```python
CHECKLIST_ROW_RE = re.compile(r"^-\s*\[\s*[ xX]\s*\]")
```

This matches `- [ ]`, `- [x]`, and `- [X]`. Functionally tighter than documented (a checked DEFER row carrying a stale signal IS drift), but the docstring drift is the kind of small inconsistency that erodes trust in CI advisory scripts.

**Fix shape:** either tighten regex to `^-\s*\[\s*\]` (unchecked only — matches docstring) or expand docstring to "live DEFER rows (checked or unchecked)". Either resolves it. Non-blocking.

### P3-02 — GA-readiness final-audit §11 DEFER counter intro text vs scrub annotation tension

**Severity:** P3 (cosmetic / readability).

**Evidence.** `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §11 intro line 272 reads:

> The 7 items below are **agent-impossible** under the autonomous-execution charter. They are tracked here to provide a single counter for the go/no-go board. (Wave-25 scrub: prior row #7 "Docs CI billing reinstatement" removed as stale — CI is locally executed per `feedback_ci_local` memory; GHA billing reinstatement is not on the GA-blocker path. See …)

The "7 items below" framing post-scrub means the table now has 7 active rows (was 8), but a casual reader sees "scrub annotation says row #7 was removed" + "the 7 items below" + "former G-08 promoted to G-07" (checklist line 108) and may briefly compute the counter incorrectly. The §11 totals line 284 correctly reports "**Total DEFER counter:** **7** (5 user-bound + 1 vendor-bound + 1 mixed)" — internally consistent — but the intro+annotation+totals triple-confirmation pattern is harder to skim than a single counter-of-record.

**Fix shape:** drop the scrub annotation from the intro line (keep it only in line 284 totals + line 272 footer), letting the §11 table speak for itself. Cosmetic only.

---

## 4. Cross-cutting verifications run by this audit

| Verification | Result |
|---|---|
| `python3 scripts/validate_specs.py` | exit 0 — 446 schema + 9 YAML-only = 455 total. |
| `python3 scripts/validate_references.py` | exit 0 — 0 dangling refs, 0 WAIVER, 0 FF-LR. |
| `python3 scripts/validate_canonical_consistency.py` | exit 0; **CRITICAL without TLA+ proof: 0** (was 1 at wave-24 SEAL); **tla-verified: 82** (was 81). |
| `python3 scripts/validate_inv_promotion.py` | 143/143 WI coverage. |
| `python3 scripts/ga-readiness-defer-drift.py` | "OK — no stale DEFER entries detected." All 4 occurrences of stale signals on main are inside `wave-25 scrub` exempt markers. |
| `git merge-base --is-ancestor` per wave-25 brief commit (13 SHAs) | 13/13 IN MAIN. Wave-24 P0 (2 NOT-IN-MAIN streams) fully recovered. |
| `diff <(git show d172a4a) <(git show 78673c2)` | Only the SHA on line 1 differs (298 lines each); cherry-pick is byte-identical. |
| `diff <(git show 8fa1c22) <(git show 93ebe3c)` | Only the SHA on line 1 differs (811 lines each); cherry-pick is byte-identical. |
| `grep -n "now_ms = if\|wall_now_ms == 0\|clock_unavailable" apps/server/src/routes/audit_analytics.rs` | Wave-23 §4 silent-fallback REMOVED; replaced with fail-CLOSED branch at line 693 + `clock_unavailable` audit row at line 710; pinning test at line 1187. |
| `ls specs/tla/auth_pat_revoke*` | `.tla` + `.cfg` + `_nightly.cfg` all present. |
| `git log --first-parent --oneline e9ee8eb..2a4e00c` | 11 merge commits + 2 direct commits (cherry-picks) = 13 dispatched commits in main. Matches dispatch brief. |
| `git diff --stat e9ee8eb..2a4e00c` | 62 files changed, 6 390 insertions(+), 121 deletions(-). |

---

## 5. Wave-tip reconciliation (closing wave-24 P1-03)

Wave-24 P1-03 recommended adding a wave-tip reconciliation step to the wave-closure template to prevent the silent-drop-of-dispatched-streams failure mode. This wave-26 review explicitly runs the reconciliation as a charter-mandated audit step:

```sh
$ for sha in 22efd4e b974be3 7cf4bbb ffdd876 ab074c7 619d449 \
             1499bc9 4326a1f b01d14a 50130c6 7e1a694 \
             d172a4a 8fa1c22; do
    git merge-base --is-ancestor "$sha" 2a4e00c && echo "IN  $sha" || echo "OUT $sha"
  done
IN  22efd4e
IN  b974be3
IN  7cf4bbb
IN  ffdd876
IN  ab074c7
IN  619d449
IN  1499bc9
IN  4326a1f
IN  b01d14a
IN  50130c6
IN  7e1a694
IN  d172a4a
IN  8fa1c22
```

**13/13 IN.** Wave-25 does not reproduce the wave-24 silent-drop failure mode.

**Recommendation for the wave-closure template (wave-26 hygiene WI):** codify the loop above as `scripts/validate_wave_tip_reconciliation.sh <wave-tip-sha> <stream-shas-json>` and call it from `specs/_audits/<date>-waveNN-closure.md` authoring contract. Two-line bash wrapper around `git merge-base --is-ancestor`.

---

## 6. Score derivation

`10 - 1.5·0 - 0.5·1 - 0.15·2 - 0.05·2 = 10 - 0 - 0.5 - 0.30 - 0.10 = 9.10`.

Per-stream median: **9.0/10**; mean (weighted equally): **8.82/10**. The formula yields 9.10; per-stream mean drag pulls the headline down to **9.00/10** — well above the 8.5 SOTA-bar.

**Above the 8.5 SOTA-bar by 0.5.** Verdict: **PASS — SEAL wave-25** with one named hygiene follow-on (P1-01 doc refresh).

The wave-24 → wave-25 trajectory (6.95 CONDITIONAL → 9.00 PASS) is the recovery-arc the dispatch brief targeted. The remaining drift is a consume-side documentation lag, not a substantive verdict gap.

---

## 7. Recommendation

**SEAL wave-25.** Dispatch one wave-26 hygiene micro-WI:

1. **Refresh stale TLA-coverage figures** in three docs (per P1-01 fix-shape):
   - `specs/_audits/sealed/2026-05-16-pre-ga-security-attestation.md` — 4 references on lines 29, 78–80, 306, 379.
   - `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` — 4 references on lines 29, 119, 128, 362.
   - `specs/_audits/sealed/2026-05-16-wave25-closure.md` — 1 reference on line 130.

   Single commit, ≤ 20 lines changed, single owner sign-off. Restores doc/registry consistency before the §13 2-key block lands.

2. **(Optional, defensibility)** Author `scripts/validate_tla_count_consistency.py` (~50 LOC) that re-greps audit docs for `tla-verified: NN` strings and confirms they match `scripts/validate_canonical_consistency.py` output. Same shape as `ga-readiness-defer-drift.py`. Prevents recurrence.

3. **(Optional, vendor-facing)** Refresh `specs/_audits/sealed/pentest-vendor-shortlist.md` §0.1/§0.2 to describe the actual `raw_total / 110 × 100` normalisation (per P2-01 fix-shape) BEFORE RFP packets are sent to Bishop Fox / NCC / Trail of Bits.

4. **(Optional, forward-risk)** Single empirical retest of `pnpm build` with wave-24 babel patches reverted (per P2-02 fix-shape) — 5-minute experiment that closes the "load-bearing" assumption.

5. **(Optional, template hardening)** Codify wave-tip reconciliation as `scripts/validate_wave_tip_reconciliation.sh` and call it from the wave-closure audit template (closes wave-24 P1-03 procedurally).

None of (2)–(5) block SEAL. Only (1) is the audit-doc/registry-truth restoration that the integrity-of-record charter requires before the GA cutover sign-off pack is distributed externally.

After (1) lands, wave-26 adversarial review re-scores wave-25 ledger; projected post-fix score is **9.4–9.6 / 10** based on per-stream median.

---

## 8. Final report block

```
BRANCH: wt/r-prep-wave25-adversarial-review
COMMIT: 2a4e00c
AGGREGATE: 9.00/10 [PASS]
PER-STREAM: debt-008=9.0, drift-detector=9.0, inv-sweep=8.5, w24-review=9.2, statuspage-dressrun=9.0, security-attestation=6.5, pentest-scope=8.0, lote7-raci=9.3, debt-015=9.0, endurance=9.2, tenant-cf-wire=9.0, cherry-d172a4a=10, cherry-8fa1c22=10
FINDINGS: P0=0 P1=1 P2=2 P3=2
P0 LIST: none (wave-24 P0 closed via cherry-picks d172a4a + 8fa1c22; both byte-identical to originals 78673c2 + 93ebe3c; validate_canonical_consistency.py reports CRITICAL without TLA+ proof: 0; tla-verified: 82)
RECOMMENDATION: SEAL wave-25 — dispatch one wave-26 hygiene micro-WI to refresh stale "81 TLA-verified / 1 CRITICAL TLA-exempt" figures in pre-ga-security-attestation.md + ga-readiness-final.md + wave25-closure.md (registry+validator now show 82 / 0 post-cherry-pick). Optional: pentest-shortlist rubric drift, debt-015 babel-patch re-test, wave-tip reconciliation script.
TIME: 47min
```

---

## 9. DCO

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

---

## 10. Closure note — wave-30 P2 absorption sweep (2026-05-16)

> The P2 / P3 findings recorded in this review have been triaged in
> `specs/_audits/sealed/2026-05-16-p2-absorption-sweep-w25-28.md` (wave-30 stream-7).
> Per-finding dispositions:
>
> - **P2-01** (pentest vendor-shortlist scoring-rubric vs application drift) → **CLOSED-WAVE-30** (FIX-NOW). `specs/_audits/sealed/pentest-vendor-shortlist.md` §0.1 / §0.2 / §6.1 rewritten to describe the actual raw-sum computation (110 max = 70 primary + 40 secondary); relative ranking unaffected.
> - **P2-02** (DEBT-015-BUILD babel-patch "load-bearing" claim without empirical re-test) → **DEFER-POST-GA**. 5-minute `pnpm build` re-test with patches reverted is substantive verification, not cosmetic; recorded in the sweep's §4 residual queue for wave-30+ R-prep.
> - **P3-01** (`ga-readiness-defer-drift.py` docstring vs regex drift) → **CLOSED-WAVE-30** (FIX-NOW). Docstring now matches the functionally-tighter "checked or unchecked DEFER rows" reality.
> - **P3-02** (§11 intro scrub-annotation triple-confirmation pattern) → **CLOSED-WAVE-30** (FIX-NOW). Duplicate parenthetical stripped from `2026-05-16-ga-readiness-final.md` §11 intro; provenance retained in the totals line.
