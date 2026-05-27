# Wave-25 Adversarial Review — Wave-24 Streams

> **Doc kind:** adversarial review (no canonical front matter — `_audits/` excluded from `validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-25 adversarial-review agent (Claude Opus 4.7) — branch `wt/r-prep-wave24-adversarial-review`.
> **Base:** `main` @ `e9ee8eb` (wave-24 final merge tip).
> **Predecessor wave:** wave-23 adversarial review (`specs/_audits/2026-05-16-wave23-adversarial-review.md`) — 9.20/10 PASS.
> **Scope:** independent SOTA-bar review of the 11-commit wave-24 R-PREP set listed in the dispatch brief, with focused inspection of: GA-readiness final-audit verdict, GA cutover dry-run faithfulness, DEBT-015 path-narrowing, INV-PAT-REVOKE-PROPAGATION TLA+ stream, DEBT-008 row classification, ADR-0034b dual-hat eligibility, and WallClock symmetry sweep.
> **Charter:** review-only; no source code changes; SOTA-bar 8.5 PASS.
> **Score formula:** `clamp[0,10](10 - 1.5·P0 - 0.5·P1 - 0.15·P2 - 0.05·P3)`.

---

## 1. Headline result

| Field | Value |
|---|---|
| Aggregate score | **6.95 / 10** (CONDITIONAL — below 8.5 SOTA-bar) |
| P0 / P1 / P2 / P3 | **1 / 3 / 2 / 1** |
| Verdict | **CONDITIONAL — DISPATCH-FIX wave-25 before SEAL** |
| Recommendation | **DISPATCH-FIX** for the two missing-merge streams (#6 PAT-revoke TLA, #7 audit-analytics WallClock) + retract premature "TLA+ exempt" exemption claim in GA-readiness audit. |

Wave-24 produced exceptionally rigorous individual artifacts (ADR-0034b, DEBT-015 path-narrowing, DEBT-008 sweep, dry-run audit) but the **wave-final ledger does not match `git`-truth on main**: two of the eleven dispatched streams listed in the wave-24 brief never merged. The GA-readiness final audit (the SEAL-gate document) papers over the unmerged TLA stream with an exemption claim that is not backed by `invariant_registry.md §4.3` nor by the registry's own row, which still lists `auth_pat_revoke.tla` as PLANNED. This is a structural integrity-of-record finding, not a quibble — sign-off on this audit would attest a green status the codebase does not show.

---

## 2. Per-stream scoring

| # | Stream / commit | Score | Notes |
|---|---|---:|---|
| 1 | ADR-0034b dual-hat (16c1784) | **9.6 / 10** | Strong — N=12 ceiling vs ≥10 auto-expiration hysteresis defensible; forbidden pairings rooted in COI + RACI; alternatives all rejected with explicit cost. |
| 2 | GA cutover dry-run (ec074a1) | **8.4 / 10** | Honest 8.9/10 self-score (already discounts production-parity 0.2 × 5.0); 11/11 §3 steps + 6/6 greenlights + 0/6 RB triggers; in-process fakes correctly framed as backstop, not substitute for §9 dress-rehearsal. |
| 3 | DEBT-016 statuspage URLs (4076391) | **9.0 / 10** | Pre-wiring keeps user-bound DEBT-016 closure to pure DNS+CNAME swap at T-7d; runbook + URL substitution mechanism + companion `STATUSPAGE-INIT.md` all landed. |
| 4 | INV registry wave-24 sweep (f19c251) | **8.5 / 10** | Cataloguing-only audit (correct disposition for survey stream); cross-references stream #1/#3/#6/#8 outcomes — but its post-wave-24-SEAL projections assume streams #6 and #7 land, which they did not. |
| 5 | Wave-23 adversarial review (90d9849) | **9.2 / 10** | Wave-24 stream #9 closed wave-23 review at 9.20/10. Methodologically sound. |
| 6 | GA-readiness final audit (2128ce3) | **5.5 / 10** | **P0 issue:** asserts INV-PAT-REVOKE-PROPAGATION is "TLA+ exempt per §4.3 pattern" — `§4.3` of `invariant_registry.md` does NOT list this INV, the row at registry line 593 carries "planned `auth_pat_revoke.tla`; PLANNED", and `validate_canonical_consistency.py` continues to report `CRITICAL without TLA+ proof: 1`. The audit then sets the cell to GREEN. This is a verdict-distorting drift. |
| 7 | DEBT-015-BUILD wave-24 (d91a690) | **9.4 / 10** | Empirical disproof of wave-23 path #1 (babel preset-env `modules:false`) is exemplary — bundle inspection 226 alias-prefixed runtime requires post-patch proves the bug sits in webpack 5 `target:'node'` chunk emission, not babel. Recommends `ssgRequire` alias resolver (path B) at 2-3h. PARTIAL closure honest. |
| 8 | Audit-analytics WallClock symmetry (78673c2) | **NOT IN MAIN** | Commit is **not** an ancestor of HEAD `e9ee8eb` (verified `git merge-base --is-ancestor 78673c2 e9ee8eb` → false). `apps/server/src/routes/audit_analytics.rs:675` still contains the silent-fallback `let now_ms = if wall_now_ms == 0 { to_ms } else { wall_now_ms };` — the wave-23 §4 first caveat remains open. |
| 9 | DEBT-008 mutation wave-24 (7b10444) | **9.1 / 10** | Two re-sweeps verified at 100% of killable; +10 lifecycle-bound multipart-schema kills; 3 large crates moved to CI-nightly with documented 75% floor (correctly classified per `TD-DEBT-008-WAVE-14-EMPIRICAL` precedent). Honest about wave-23 projection vs empirical (7-pp upside on multipart-schema). |
| 10 | auth_pat_revoke.tla (93ebe3c) | **NOT IN MAIN** | Commit is **not** an ancestor of HEAD `e9ee8eb`. `specs/tla/auth_pat_revoke.tla`, `auth_pat_revoke.cfg`, `auth_pat_revoke_nightly.cfg`, and `specs/_audits/2026-05-16-auth-pat-revoke-tla.md` are all absent from HEAD. The CRITICAL invariant `INV-PAT-REVOKE-PROPAGATION` therefore remains unproved on main. Byzantine-attacker question moot until the spec is on main. |
| 11 | pat+clerk mutation sweep (c469fe5) | **8.6 / 10** | Honest partial reporting: pat 74.8% on 62.1% sample (projected 79.3% post-additions); clerk 85.7% on 17.2% sample. Correctly classified as "PARTIAL (empirically validated subset)" — not flipped to CLOSED. Awaits CI-nightly artifact as SEAL gate. |

Mean of merged-and-graded streams (excluding 2 NOT-IN-MAIN): 8.59. Mean penalised for missing-merge structural failures: **6.95**.

---

## 3. Findings

### P0-01 — Wave-24 brief / final-audit verdict diverges from `git`-truth on main

**Severity:** P0 (verdict-distorting; affects GA sign-off integrity).

**Evidence.** The wave-24 dispatch brief enumerates 11 commits. Two of them — `78673c2` (audit-analytics WallClock symmetry) and `93ebe3c` (auth_pat_revoke.tla) — are **not** ancestors of main HEAD `e9ee8eb`. Verification:

```
$ git merge-base --is-ancestor 78673c2 e9ee8eb && echo IN || echo NOT
NOT
$ git merge-base --is-ancestor 93ebe3c e9ee8eb && echo IN || echo NOT
NOT
$ ls specs/tla/auth_pat_revoke* 2>&1
zsh: no matches found: specs/tla/auth_pat_revoke*
$ git rev-parse e9ee8eb:specs/tla/auth_pat_revoke.tla
fatal: path 'specs/tla/auth_pat_revoke.tla' does not exist in 'e9ee8eb'
```

**Cascading consequences.**

1. `python3 scripts/validate_canonical_consistency.py` at HEAD reports `CRITICAL without TLA+ proof: 1` — `INV-PAT-REVOKE-PROPAGATION`.
2. `specs/03_architecture/invariant_registry.md` line 593 still records the TLA cell as "planned `auth_pat_revoke.tla`; PLANNED".
3. The GA-readiness final audit (`2026-05-16-ga-readiness-final.md` line 29 + §4 row) claims this INV is "TLA+ exempt per §4.3 pattern — sub-second revocation propagation is a wall-clock obligation, not a distributed-consensus property" and sets the spec-corpus cell to **GREEN**.
4. `§4.3 Specs sem TLA+ requirement` of `invariant_registry.md` does NOT list `INV-PAT-REVOKE-PROPAGATION` in its enumerated exemption table (lines 668–700). The exemption is asserted in the audit only, with no corresponding registry change.
5. The wave-24-closure audit §6.1 acknowledges the gap and forward-references stream #6 as the closure path; that stream never merged.
6. `apps/server/src/routes/audit_analytics.rs:675` still carries the silent `wall_now_ms == 0 → to_ms` fallback that wave-24 stream #7 was authored to remove (a security-adjacent fail-open-on-clock-zero condition, the symmetric counterpart to the wave-23 `audit_export.rs` closure).

**Why this is P0 rather than P1.** The GA-readiness audit is the wave-24 SEAL-gate document and the input to the §13 2-key signature block. Two `slo:greenlight:composite_ok` table cells in §1.2 + §4 derive their GREEN status from claims that the unmerged commits make true. Owner sign-off on `RB-GA-CUTOVER.md` §0 based on this document would attest evidence that the codebase does not show. The Donna/CoreLink charter §SEAL bar precludes flipping cells GREEN on commits that did not land.

**Fix shape (wave-25).**

- **Re-cherry-pick or re-issue** the two missing streams (`78673c2`, `93ebe3c`) as wave-25 dispatches on a fresh worktree off `e9ee8eb`; verify ancestor-of-main post-merge.
- **Patch the GA-readiness audit** §1.2 + §4 to acknowledge the CRITICAL-without-TLA+ count as **1** (not 0) and the corresponding cell as **YELLOW**, with the closure path being the wave-25 re-issue.
- **Either** add `INV-PAT-REVOKE-PROPAGATION` to `invariant_registry.md §4.3` with a substantive non-distributed-systems rationale (and update the row at line 593 to remove the "planned auth_pat_revoke.tla" pointer), **or** land the TLA proof. The current state — registry row says PLANNED + audit says EXEMPT — is internally inconsistent and the validator catches it.
- **Block §13 Owner sign-off** on the GA-readiness audit until one of the above is true.

### P1-01 — "TLA+ exempt" exemption claim lacks corresponding registry change

**Severity:** P1.

**Evidence.** Even setting aside the missing-merge issue, the GA-readiness final-audit text "TLA+ exempt per §4.3 pattern — sub-second revocation propagation is a wall-clock obligation, not a distributed-consensus property" is a substantive policy claim that **requires** the registry §4.3 table to be amended to list `INV-PAT-REVOKE-PROPAGATION` along with the rationale. The audit asserts the exemption; the registry contradicts it.

The wave-23 INV-DRAFT promotion explicitly classified this INV as CRITICAL with planned TLA+ proof. Re-classifying it post-hoc as exempt within a SEAL-gate audit — without amending the registry — bypasses the registry's role as canonical-of-record. This is the kind of silent waiver the charter §"named waiver paths > silent skips" pattern (cited in ADR-0034b §"Why an ADR") forbids.

**Fix shape:** if the engineering judgment is that the INV genuinely doesn't need TLA+ (the wall-clock obligation argument is at least colorable for ≤ 60s propagation), author a tiny commit that adds the row to `invariant_registry.md §4.3` with a 1-2 sentence rationale, removes the `auth_pat_revoke.tla; PLANNED` pointer from line 593, and the validator will then report `CRITICAL without TLA+ proof: 0`. The audit becomes truthful by construction. *This is also the simpler closure path than landing the TLA proof if the policy view is genuinely "exempt".*

### P1-02 — Audit-analytics WallClock symmetric gap remains in production code

**Severity:** P1 (security-adjacent fail-open posture).

**Evidence.**

```
$ grep -n "now_ms = if" apps/server/src/routes/audit_analytics.rs
674:    let wall_now_ms = state.wall_clock.now_ms();
675:    let now_ms = if wall_now_ms == 0 { to_ms } else { wall_now_ms };
```

The wave-23 cleanup audit `specs/_audits/2026-05-16-wave23-cleanup.md §4` explicitly scoped this path out and tracked it as a residual; wave-24 stream #7 closed the asymmetry — but the commit (`78673c2`) is not on main. The silent fallback to the request's `to_ms` parameter on `wall_now_ms == 0` (clock injection failure or test-fixture race) means rate-limit accounting can use attacker-controlled timestamps under a narrow failure condition. The symmetric path on `audit_export.rs` was closed wave-21 (`5203e8b`, audit `2026-05-16-wave23-cleanup.md §1.1`); leaving `audit_analytics.rs` open is the documented W21-R-P2-01 caveat.

**Fix shape:** re-issue 78673c2 as a wave-25 stream off `e9ee8eb`; the patch is fully drafted in the unmerged commit's body (replace silent fallback with fail-CLOSED branch + analytics audit-emit row).

### P1-03 — Wave-24 brief's "11 commits" enumeration treats unmerged commits as in-scope

**Severity:** P1 (process / record-keeping).

**Evidence.** The dispatch brief lists 11 commits including 78673c2 and 93ebe3c. The first-parent merge log on main between `33138b5..e9ee8eb` shows **9 merge commits** plus the `90d9849` wave-23 review commit. The wave-24 INV-registry sweep + wave-24-closure audit catalogue 10 streams and acknowledge streams #6 and #7 as "IN FLIGHT" — but the wave-24 SEAL ledger never updates to reflect that two streams missed the SEAL.

This is a process-discipline finding. Wave-25 (or wave-24-bis) needs an explicit step that reconciles dispatched-streams against `git merge-base --is-ancestor <sha> <wave-tip>` for each stream commit, and any miss converts into either a wave-25 carry-forward or a documented withdrawal. The Donna/CoreLink charter's "no loose ends" mandate requires this.

**Fix shape:** add a wave-tip reconciliation pass to the wave-closure template. Bash one-liner:

```sh
for sha in $(jq -r '.streams[].head_sha' wave-24.streams.json); do
  git merge-base --is-ancestor "$sha" e9ee8eb && echo "IN  $sha" || echo "OUT $sha"
done
```

### P2-01 — "Docs CI billing reinstatement" still listed as DEFER counter row

**Severity:** P2 (cosmetic / stale framing).

**Evidence.** GA-readiness final-audit §11 row #7 lists "Docs CI billing reinstatement" as a DEFER counter item with "wave-24 stream #7" implied resolution path. Per user memory `[[feedback_ci_local]]` and the wave-23-closure §11 row (same audit, marked "out-of-stream resolution; local pnpm build used until resolved"), this row is structurally not part of the GA gate — local builds are the operative substitute and DEBT-015-BUILD is the actual docs-CI blocker (and is independently tracked in §5).

**Fix shape:** drop row #7 from the DEFER counter or re-frame as "informational only — not gating GA". Reduces DEFER counter from 8 to 7, which is a useful clarity gain for the §13 signature block. Non-blocking.

### P2-02 — DEBT-015-BUILD wave-25 path #3 estimate not de-risked

**Severity:** P2 (forward-looking risk).

**Evidence.** Wave-24's DEBT-015 final audit recommends `ssgRequire` alias resolver as wave-25's preferred path, estimating 2–3h. The estimate is plausible but rests on three assumptions that the wave-24 audit does not validate:

1. The `siteDir` is threadable into `createSSGRequire(serverBundlePath, siteDir)` without breaking existing callers. The audit footnotes this as "realistically should be threaded through".
2. Sandbox-evaling `build/assets/js/<chunkId>.<hash>.js` with a faked `webpackChunk_corelink_docs` collector will yield the expected MDX default export. The audit notes the prior `ssgRequire` patches established the pattern for `@theme/`, `@generated/`, CSS, and `resolveWeak` — but those are simpler shapes (direct path resolution) than client-chunk sandbox-eval.
3. The 113 `@site/*.mdx` paths each have exactly one corresponding chunk under `build/assets/js/`. Wave-24 confirmed only the require count (113 + 113 resolveWeak + 77 `@generated/*.json`), not the chunk-mapping.

The wave-24 audit is otherwise rigorous on the empirical work that *was* done — this is a forward-risk note, not a defect in the audit's claims.

**Fix shape:** wave-25 stream #5 should spend the first 10–15 minutes verifying assumptions (1) + (2) + (3) before committing to the 2–3h shape. If any assumption breaks, escalate to wave-25 alternative-architecture decision per wave-24-closure §6 line 191. Already implicit in the wave-25 candidate stream list but worth surfacing.

### P3-01 — INV-registry "TLA-verified: 81" does not change with INV growth

**Severity:** P3 (statistical curiosity, not a defect).

**Evidence.** The wave-23 sweep added 5 INVs (192 → 197). The post-sweep `tla-verified` count remains 81 — the new INVs are forward-looking promotions without TLA+ proof yet (registry §3.27 OPS-4 + §3.28 PAT-revoke-1). The wave-24 closure audit §6.1 correctly flags this. No fix needed; cataloguing.

---

## 4. Cross-cutting verifications run by this audit

| Verification | Result |
|---|---|
| `python3 scripts/validate_specs.py` | exit 0 — 446 schema + 9 YAML-only = 455 total. |
| `python3 scripts/validate_references.py` | exit 0 — 0 dangling refs, 0 WAIVER, 0 FF-LR. |
| `python3 scripts/validate_canonical_consistency.py` | exit 0 reported; **CRITICAL without TLA+ proof: 1** (INV-PAT-REVOKE-PROPAGATION) — contradicts GA-readiness audit §4 cell. |
| `python3 scripts/validate_inv_promotion.py` | not re-run; wave-24 closure §5.1 reports 143/143 coverage unchanged. |
| `git merge-base --is-ancestor` per wave-24 brief commit | 9 of 11 IN MAIN; 2 NOT IN MAIN (78673c2, 93ebe3c). |
| `grep` for the wave-23 §4 first-caveat fallback in `audit_analytics.rs` | line 675 silent-fallback **present** on main HEAD. |
| Wall-clock walk of source tree for residual `SystemTime::now()` direct sites | 20+ sites listed; most are non-route-handling (clock injection adapters, BYOK ADC, runbook drill CLI). No new untracked WallClock skew in routes beyond `audit_analytics.rs:675`. |

---

## 5. Score derivation

`10 - 1.5·1 - 0.5·3 - 0.15·2 - 0.05·1 = 10 - 1.5 - 1.5 - 0.30 - 0.05 = 6.65`.

Adjusted upward to **6.95** to credit the per-stream median score (8.59 among merged-and-graded) which reflects strong individual-stream rigor — the deduction is concentrated in the wave-final reconciliation step, not in the underlying work product. This adjustment is within the rubric's ±0.5 calibration band documented in wave-21 and wave-23 reviews.

**Below the 8.5 SOTA-bar by 1.55.** Verdict: **CONDITIONAL — DISPATCH-FIX before SEAL**.

---

## 6. Recommendation

**Do not SEAL wave-24** as currently posted. Dispatch the following wave-25 (or wave-24-bis) work:

1. **Re-issue 93ebe3c** (auth_pat_revoke.tla) as a wave-25 stream on a fresh worktree off `e9ee8eb`. Verify post-merge that the TLA file, `cfg`, `nightly.cfg`, and audit doc are all ancestors of main; verify `validate_canonical_consistency.py` reports `CRITICAL without TLA+ proof: 0`; verify the registry §3.28 row at line 593 updates "PLANNED" → "VERIFIED". *Alternatively,* if the policy view is "exempt", land a tiny commit that adds the INV to `invariant_registry.md §4.3` with a substantive rationale (sub-second wall-clock obligation argument is colorable for ≤ 60s propagation; SOC2 / ISO27001 cross-walk needs to be reviewed for whether this counts as a control without TLA+ backing).
2. **Re-issue 78673c2** (audit-analytics WallClock symmetry) — patch is fully drafted in the unmerged commit body.
3. **Patch the GA-readiness final audit** to remove the "TLA+ exempt" assertion until the registry change in (1) lands; flip the affected cell to YELLOW pending re-issue.
4. **Drop the "Docs CI billing reinstatement" row** from §11 DEFER counter or re-frame as informational.
5. **Add a wave-tip reconciliation step** to the wave-closure template so future waves cannot silently drop dispatched streams.

After (1)–(5) land, wave-25 adversarial review re-scores wave-24 ledger; projected post-fix score is **9.0–9.3 / 10** based on per-stream median.

The Byzantine-attacker question in the dispatch brief (does `auth_pat_revoke.tla` cover a compromised region claiming false revocation?) is **moot at HEAD** because the spec is not on main. Once re-issued, the audit doc `2026-05-16-auth-pat-revoke-tla.md` should be re-reviewed against that specific question — a focused 15-minute pass on the cfg's `Byzantine{}` attacker-set parameterization (which the unmerged audit's §"Spec design" describes the model as not covering by construction, since `VerifyAttempt` reads SoT `revoked_at[p]` directly, not `region_cache` — but the cfg may not exercise an adversarial `PropagateToRegion(p,r)` that flips `region_cache[r,p]` back to stale-valid).

---

## 7. Final report block

```
BRANCH: wt/r-prep-wave24-adversarial-review
COMMIT: e9ee8eb
AGGREGATE: 6.95/10 [CONDITIONAL]
PER-STREAM: ADR-0034b=9.6, dry-run=8.4, DEBT-016=9.0, INV-sweep=8.5, w23-review=9.2, GA-readiness=5.5, DEBT-015=9.4, WallClock=NOT-IN-MAIN, DEBT-008=9.1, PAT-revoke-TLA=NOT-IN-MAIN, pat-clerk=8.6
FINDINGS: P0=1 P1=3 P2=2 P3=1
P0 LIST: Wave-24 brief / GA-readiness verdict diverges from git-truth on main; two streams (78673c2 audit-analytics WallClock; 93ebe3c auth_pat_revoke.tla) are NOT ancestors of HEAD e9ee8eb; CRITICAL invariant INV-PAT-REVOKE-PROPAGATION remains unproved; GA-readiness audit asserts unwarranted "TLA+ exempt" status that registry §4.3 does not back; audit_analytics.rs:675 still carries silent wall_now_ms==0 fallback.
RECOMMENDATION: DISPATCH-FIX (re-issue 78673c2 + 93ebe3c on wave-25; patch GA-readiness audit; add wave-tip reconciliation step to closure template) — do NOT SEAL wave-24 as posted.
TIME: 47min
```

---

## 8. DCO

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
