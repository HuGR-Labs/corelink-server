# WP-10 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)
**Date:** 2026-08-26
**Previous Verdict:** ❌ FAIL (9 BLOCKING, 11 HIGH, 7 MEDIUM)
**New Verdict:** ❌ **FAIL — 8 NEW ISSUES (4 BLOCKING, 2 HIGH, 2 MEDIUM) + 1 REGRESSION**

---

## Executive summary

Iter 1 review found 9 BLOCKING + 11 HIGH + 7 MEDIUM; WP-10 author applied fixes. **Most fix-application was correct** (k6 script rewritten cleanly, runbook path moved to `specs/_runbooks/RB-DEVENV-*.md`, §6 became a derivation, "byte-identical" → SHA-256, `docker kill` → `stress-ng`, M1–M7 fully addressed, H3/H4/H8/H9/H11 properly reflected). **But 4 of the 9 BLOCKING fixes were applied against CITATIONS THAT DO NOT EXIST IN HEAD** — the iter 1 review cited a doc-comment or a migration header but the WP-10 author trusted it without verification, and HEAD proves them wrong:

1. `apps/signup-worker/src/lib/d1.ts::seedTenantEntitlements` does NOT seed `runners_entitlement` (the function's own doc-comment, dated 2026-08-02, says it was REMOVED for that purpose — "It silently reverted an owner-ratified decision").
2. `runners_entitlement` columns are `(max_concurrency, max_vcpu_h)` per migration 0070/0072; `max_vcpu` (instantaneous cores) does not exist.
3. The "free + runner-promo" dogfood tier is structurally impossible: `runners_entitlement` is the cap the gate CHECKS — writing a row = granting capacity. Free signup = no row = reject. The whole §3.1 column is broken.
4. `RB-CHAOS-CATALOG.md` has 8 existing experiments (R2/D1/Neon latency etc.), and the WP-10 claim "this WP contributes the 5 DevEnv rows" → "8/8 chaos expts" violates the gate math (5 DevEnv rows + 8 existing = 13 expts, but GA-GATE-O10 says "8 of 8").

Iter 1's iter 1 review also missed the OKF wiki gap (0 DevEnv concepts exist, WP-10 still creates none), the GA-GATE-O06 count drift (gate says "47", repo has 55, WP-10 says "58" — all three wrong), and the chaos-catalog scope mismatch (RB-CHAOS-CATALOG.md mandates staging-only at GA; WP-10 §4.3 designs production-shape OOM/stress-ng injection). Plus a k6 script regression (the `http.delete` 3-arg call is fine in modern k6, but the `socket.on('message')` assertion in `wsStorm` is still a no-op check — passes even when the subprotocol was stripped, the original B2/B4 surface still partially leaks).

**The single biggest miss:** iter 1 trusted `seedTenantEntitlements` without re-reading the function's doc-comment, which explicitly documents the 2026-08-02 removal. A 30-second `rg` would have caught it.

---

## ✅ ITER 1 FIXES — VERIFICATION

| Iter 1 Issue | Fix Applied? | Verdict | Evidence |
|--------------|--------------|---------|----------|
| B1 (runbook dir) | ✅ `specs/_runbooks/RB-DEVENV-*.md` | **PASS** | §5.3 lines 412-425 |
| B2 (k6 7 defects) | ⚠️ Most fixed, see N1 | **PARTIAL** | Lines 193-359 |
| B3 (launch-day-projection.rs) | ✅ §4.4 added | **PASS** | Lines 372-380 |
| B4 (WS subprotocol assert) | ⚠️ `concurrentDevEnv` asserts; `wsStorm` does NOT | **PARTIAL** | Lines 297-307 vs 340-359 |
| B5 (GA-gate derivation) | ✅ §6 derivation page | **PASS** | §6 lines 488-547 |
| B6 (pricing page ownership) | ✅ "runner-tier pricing addendum" | **PASS** | §2 line 33, §6.1 line 508 |
| B7 (docker kill → stress-ng) | ✅ §4.3 row Container OOM uses `stress-ng` | **PASS** | Line 366 |
| B8 (D1 + CF quota cross-link) | ✅ §6.3 cites upstream owner | **PASS** | Lines 530-536 |
| B9 (byte-identical → SHA-256) | ✅ §3.2 row 8 | **PASS** | Line 69 |
| H1 (runbook frontmatter) | ✅ §5.4 has 11-field frontmatter | **PASS** | Lines 432-446 |
| H2 (custom metrics + Prometheus + cogs) | ✅ `Counter`/`Trend`/`Gauge`/`recordClassA`/`recordClassB` | **PASS** | Lines 223-229, 197 |
| H3 (`run_devenv_load.sh`) | ✅ Appendix B declares the wrapper | **PASS (declared)** | Line 612 |
| H4 (cache vs runner axis) | ✅ §2 + §6.1 disambiguated | **PASS** | Line 33, 508 |
| H5 (≤2h/day dogfood) | ✅ All 5 rows = 2h | **PASS** | Lines 48-52 |
| H6 (launch-day-projection citation) | ✅ §4.4 + §6.2 row | **PASS** | Lines 372-380, 524 |
| H7 (CF Container + DO cap) | ✅ §4.1 footer | **PASS** | Line 135 |
| H8 (tier-promo row) | ⚠️ Cited but **citation is wrong** (N2) | **REGRESSION** | Line 56 |
| H9 (CHANGELOG gate) | ✅ §6.2 row | **PASS** | Line 520 |
| H10 (OpenAPI auth gate) | ⚠️ Tracked but **decision not made** (N8) | **PARTIAL** | Line 409 |
| H11 (print-ready checklist) | ✅ Appendix D | **PASS** | Lines 646-675 |
| M1 (renumber §8) | ✅ §8 + §9 + §10 | **PASS** | Line 568, 583 |
| M2 (NPS plan) | ✅ `marketing/lighthouse-kit/` | **PASS** | Line 579 |
| M3 (iOS version) | ✅ "iOS 17+ Safari" | **PASS** | Lines 52, 70, 662 |
| M4 (rapid start/stop math) | ✅ "50 warm start/stop cycles" | **PASS** | Line 131 |
| M5 (blast radius / rollback) | ✅ §4.3 has 5 columns | **PASS** | Lines 364-370 |
| M6 (doc_status enforcement) | ✅ §5.1 mandates `doc_status: ACTIVE` | **PASS** | Line 390 |
| M7 (chaos catalog link) | ✅ §4.3 header cites `RB-CHAOS-CATALOG.md` | **PASS** | Line 362 |

**Summary:** 23/27 fixes fully applied; 2 partial (B2/B4, H10); 1 regression (H8 — cited wrong function); 1 cross-WP leak (B5 only fixed at the WP-10 level, not at the GA-GATE-CRITERIA level — see N5).

---

## 🔴 NEW BLOCKING ISSUES (Missed in Iteration 1)

### N1. **`seedTenantEntitlements` does NOT seed `runners_entitlement` — the iter 1 fix cited a function whose own doc-comment (dated 2026-08-02) documents its removal**
- **Location:** §3.1 "Dogfood Tier" column (lines 48-52) + §3.1 footer line 56
- **Problem:** Iter 1 review (H8) prescribed: "cite `apps/signup-worker/src/lib/d1.ts::seedTenantEntitlements`". The WP-10 author trusted the citation and wrote: "*free* cache + *runner-promo* (`runners_entitlement` row seeded with `max_concurrency=1, max_vcpu=2`) via `apps/signup-worker/src/lib/d1.ts::seedTenantEntitlements`". **The function's own doc-comment (HEAD) says this is wrong** — the `runners_entitlement` row was removed from `seedTenantEntitlements` on 2026-08-02 because "writing the row is granting the capacity" and "free signup → install the public GitHub App → boxes spawn on our Cloudflare account" leaked compute. The actual current path is `apps/signup-worker/src/webhooks/stripe.ts` (Stripe webhook, gated by payment — `INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, created_at_ms, max_vcpu_h)`).
- **Why it slipped through:** The iter 1 reviewer cited the function name from memory; the WP-10 author did not `rg` to verify. The citation looks plausible (the function exists, the migration is at 0070, the column exists) but the FUNCTION IS NOT THE ONE THAT WRITES THE ROW ANYMORE.
- **Impact:** A dogfood engineer reading §3.1 will (a) ask the team to invoke a function that no longer writes the row, (b) expect a `max_vcpu=2` column that doesn't exist (see N2), (c) not realise the runner-promo requires a Stripe test-mode checkout (or manual `INSERT` via `d1.py` for the staging env), and (d) be confused why "free + runner-promo" is a contradiction (see N3). The whole dogfood program cannot start without resolving this.
- **Fix:** Replace the §3.1 footer + "Dogfood Tier" column with a CORRECT escalation:
  > **Dogfood tier:** All dogfooders start on `free` cache + ZERO `runners_entitlement` row (the default per `seedTenantEntitlements` policy of 2026-08-02). To bootstrap runner capacity for dogfood, the corelink-runners TL runs a one-time `INSERT INTO runners_entitlement (tenant_id, max_concurrency, plan, max_vcpu_h) VALUES (?, 1, 'dogfood-promo', 100)` per dogfooder via `apps/signup-worker/scripts/d1.py` (or the Stripe test-mode webhook `apps/signup-worker/src/webhooks/stripe.ts`). The row is read by `runner_concurrency_for_tenant()` (per `marketing/corelink-feature-catalog.html` card "runner entitlement axis (separate from cache tier)"). Escalation to higher quota: TL request → corelink-runners TL. **Do not modify `seedTenantEntitlements`** — it deliberately omits `runners_entitlement`.

### N2. **`max_vcpu=2` field does not exist on `runners_entitlement` — column is `max_vcpu_h` (monthly hours), not instantaneous cores**
- **Location:** §3.1 footer line 56
- **Problem:** `runners_entitlement` columns per migration 0070 (`max_concurrency`, `plan`, `created_at_ms`) + migration 0072 (`max_vcpu_h`) + the `runner_concurrency_for_tenant()` introspection contract in `marketing/corelink-feature-catalog.html`. **There is no `max_vcpu` column.** `max_vcpu_h` is a MONTHLY CEILING (denominated in vCPU-hours, the same unit as the `$0.30/vCPU-h` overage rate that the 2026-08-02 changelog entry introduced). Iter 1's H8 prescribed `max_concurrency=1, max_vcpu=2` — `max_vcpu=2` is fabricated.
- **Why it slipped through:** The "max_vcpu" name looks right (sounds like instantaneous cores), but the billing model is hourly. The iter 1 reviewer and the WP-10 author both did not open `migrations/d1/0072_runners_entitlement_max_vcpu_h.sql` to verify.
- **Impact:** A TL executing the `INSERT` would fail with `no such column: max_vcpu`. Even if it succeeded, the dogfooder would be told "you have 2 cores" but actually be metered on a 2 vCPU-hour monthly budget (which is ~7 minutes of 16-vCPU compute, or ~30 minutes on 4-vCPU — far below dogfood needs).
- **Fix:** Replace `max_concurrency=1, max_vcpu=2` with `max_concurrency=1, max_vcpu_h=100` (or larger; matches the existing 100/240/600h tiers per the 2026-08-02 changelog). Confirm in `migrations/d1/0072_runners_entitlement_max_vcpu_h.sql` that the column is nullable + signed + named exactly `max_vcpu_h`.

### N3. **`free + runner-promo` is structurally contradictory — `runners_entitlement` is the cap the gate CHECKS, not a promo overlay**
- **Location:** §3.1 column 5 (all 5 rows) + §3.1 footer + Appendix D
- **Problem:** `runner_concurrency_for_tenant()` semantics (per the `runner entitlement axis` card in `marketing/corelink-feature-catalog.html`): "absent concurrency ⇒ reject, absent vcpu_h ⇒ wall-off (intentional asymmetry)". Translation: a tenant with NO `runners_entitlement` row CANNOT mint a runner, period. There is no "free tier with a runner-promo overlay" — the row is the entitlement. Iter 1's H8 accepted "free cache + 1-tenant Runner promo" as a coherent state, but the actual system has no such state.
- **Why it slipped through:** The iter 1 review framed the tier as "free cache + promo creds" (a billing model) but the actual system uses `runners_entitlement` (a capacity grant). The 2026-08-02 doc-comment in `seedTenantEntitlements` is explicit: "Runners is a SEPARATE PAID axis from the cache tier (Option B, 2026-06-13) — chosen precisely because the alternative 'leaked compute a cache-only tenant never bought'."
- **Impact:** Every dogfooder row in §3.1 is contradictory. The first dogfooder to hit "tier limit reached" will get a 429 from `runner_concurrency_for_tenant()` and a P1 will be filed against a non-bug.
- **Fix:** Rename column to **"Runner Entitlement"** with values like `dogfood-promo (1, 100h)` or `dogfood-promo (1, 50h)`. Add an "Escalation" column for higher quota. Replace the footer with the N1 fix. Remove Appendix D's `Tier: free+runner-promo` line; replace with `Entitlement: dogfood-promo (1 concurrent / 100h monthly)`.

### N4. **§6.1 chaos row math: "5 DevEnv rows" added to existing 8 violates GA-GATE-O10's "8 of 8" assertion**
- **Location:** §6.1 line 506: "`GA-GATE-CRITERIA.md:GA-GATE-O10` (8/8 chaos expts; this WP contributes the 5 DevEnv rows)"
- **Problem:** `RB-CHAOS-CATALOG.md` has 8 fixed experiments (R2 GET latency, D1 query latency, Neon query latency, plus 5 more per §1 of the runbook; "Distinct count = 8 — AC gate ≥ 8 satisfied"). WP-10 §4.3 has 5 DevEnv failure-injection rows (OOM, network partition, disk full, DO hibernation, concurrent snapshots). If WP-10 adds 5 to the catalog, the total becomes 13, but GA-GATE-O10's threshold is "at least 8 of 8 executed" — the 8 denominator is now wrong (it would read "8 of 13" if read literally, breaking the gate's assertion).
- **Why it slipped through:** The WP-10 author treated chaos-catalog additions as additive without updating the gate's denominator, and iter 1 review did not check the gate's arithmetic.
- **Impact:** At T-7, the SRE Lead ticks GA-GATE-O10 by reading `RB-CHAOS-CATALOG.md`'s last 30 days of execution. If the catalog has 13 experiments, the gate literally says "8 of 8" but the file shows 13 — the gate criterion's "8 of 8" string is a stale constant in `GA-GATE-CRITERIA.md` and will need to be updated to "X of 13" once the 5 DevEnv rows land. WP-10 has not raised this as a cross-WP coord.
- **Fix:** Either (a) extend the chaos catalog such that the 5 DevEnv rows REPLACE 5 of the existing rows (net 8 total — but 5 of the original 8 are latency-injection; the DevEnv rows are OOM/partition/disk/etc. and don't overlap), or (b) raise a cross-WP issue that GA-GATE-O10 needs its denominator raised to "X of 13" before the DevEnv rows ship, or (c) restructure WP-10 §4.3 to contribute 0 NEW rows and instead exercise 3-5 of the existing 8 against a DevEnv-shaped surface (not great — the DevEnv failure modes are categorically different from latency). Recommended: (b), and explicitly own the gate-criteria patch in §6.1.

---

## 🟠 NEW HIGH SEVERITY ISSUES

### N5. **GA-GATE-O06 "All 47 runbooks" is stale on three counts — gate says 47, repo has 55, WP-10 says 58. WP-10 acknowledges the drift but does not own the fix.**
- **Location:** §5.3 line 427 + §6.1 line 504
- **Problem:** `ls specs/_runbooks/RB-*.md | wc -l` = **55** (verified). `GA-GATE-CRITERIA.md:GA-GATE-O06` says "All **47** runbooks" (stale by 8). WP-10 §5.3 says "**58** existing runbooks" (off by 3 — author may have counted `STATUSPAGE-INIT.md` + `ONCALL-ESCALATION-MATRIX.md` + `rb-storage-fallback.md`, but those are not `RB-*.md` and should be excluded). The gate's count and the WP-10's count disagree, and neither matches reality. The WP-10 footnote "*O06 will need a baseline update as the count grows*" is a deferral that the campaign plan has not tracked.
- **Why it slipped through:** The iter 1 review said "match the 58 existing runbooks" (B1 fix). The author copy-pasted the 58 from iter 1. Neither party ran `ls` to verify. A `validate_slo_runbook_coverage.py` run at T-7d will silently fail because the gate's regex matches `RB-*.md` (55 files) but the gate's "47" is hard-coded.
- **Impact:** GA-GATE-O06 flips to `NOT_READY` at T-7d on a count mismatch, not a quality issue. A staging GHA job needs to be added or the gate text needs to be re-parameterised ("ALL RB-*.md are ACTIVE or FROZEN") before T-0.
- **Fix:** Replace the 47/58 numbers in WP-10 with the verified count (55) and raise a cross-WP issue: "GA-GATE-O06 should read 'all `RB-*.md` files in `specs/_runbooks/`' (replacing the hard-coded 47) to be maintainable." Add the re-parameterised gate text to §6.1 + a §6.2 quality gate row that asserts `ls specs/_runbooks/RB-*.md | wc -l >= 55` (and 62 after WP-10 lands the 7 DevEnv runbooks).

### N6. **OKF wiki has zero DevEnv concepts — iter 1 review raised it, WP-10 has no remediation in the deliverable list**
- **Location:** §5.1 user guide subtree + §5.3 runbooks
- **Problem:** `rg -i "devenv|runner_devenv|wave-devenv|runner-dev" docs/knowledge/` returns 0 matches (verified: 0 DevEnv concepts in the entire 157-concept wiki). Iter 1 review (cross-WP coord §8 of the review) flagged: "Open a cross-WP issue for OKF wiki — add DevEnv concepts (none exist today)." **WP-10 has no entry in its DoD for "create 5+ OKF concepts for the DevEnv surface"**, and §5.1 (user guide) + §5.3 (runbooks) are not grounded in any wiki citation. The campaign will launch with a code-grounded architecture wiki that has zero representation of the launch surface — a 6-month-from-now engineer asking "how does DevEnv work" gets 0 OKF hits, only the runbooks + WP files.
- **Why it slipped through:** The iter 1 review raised the gap in cross-WP coord (B5 + cross-WP section), not as a B-level issue, so it was not added to the "must fix" list. The WP-10 author treated it as out-of-scope.
- **Impact:** OKF wiki's drift gate (C5) stays green (no stale concept), but the "coverage" gate degrades — `marketing/lighthouse-kit/`, `corelink-feature-catalog.html`, and the runbooks all reference the DevEnv surface without any wiki concept anchoring the code. A reviewer 3 months from now cannot `python3 scripts/okf_context.py --file <devenv-route>` and get any context.
- **Fix:** Add to §5.1 (or a new §5.5) a deliverable: "**OKF wiki coverage** — author ≥ 5 DevEnv concepts to `docs/knowledge/surfaces/dev-env/` (one per major concept: lifecycle, WS subprotocol forwarding, snapshot content-hash, runner-vs-cache axis, tenant quota). Each concept's `source_files` MUST cite the actual implementing file + line range. Cross-referenced from each of the 7 RB-DEVENV-*.md runbooks."

---

## 🟡 NEW MEDIUM SEVERITY ISSUES

### M8. **k6 `wsStorm` function does not assert WS subprotocol forwarding (B4 only partially fixed)**
- **Location:** §4.2 `wsStorm` function, lines 340-359
- **Problem:** `concurrentDevEnv` (lines 297-307) was fixed to assert `socket.on('message', (msg) => check(socket, { 'vnc subprotocol forwarded': () => msg && msg.length > 0 }))`. **`wsStorm` (lines 353-358) only checks `ws open`** — the `socket.on('message', ...)` is missing, so a noVNC server that strips the `binary` subprotocol and falls back to base64 (silently) would pass the wsStorm check while breaking every real noVNC client. The original B4 said "Add an explicit VNC `subprotocols: ['binary']` parameter to `ws.connect`" — that part is fixed, but the assertion that the subprotocol was actually FORWARDED is only in `concurrentDevEnv`, not `wsStorm`.
- **Fix:** Add a `socket.on('message', (msg) => check(socket, { [`${ep.name} msg non-empty`]: () => msg && msg.length > 0 }))` to each `ws.connect` in `wsStorm` (mirror lines 303-306 of `concurrentDevEnv`).

### M9. **k6 `http.delete` 3-arg signature works, but the `null` body argument is misleading and inconsistent with `http.post` usage in the same script**
- **Location:** §4.2 line 333: `http.delete(\`${TARGET_HOST}/v1/customer/devenv\`, null, { headers })`
- **Problem:** k6's `http.delete(url, [body], [params])` accepts 3 args, so it parses, but the `null` body is unusual and reads like a port from the legacy `http.del(url, body)` API. Other `http.post` calls in the script use 2 args (URL + JSON body, no separate params) and put headers in a third arg, but `http.delete` here puts headers in the third arg too. Consistency nit, not a runtime bug.
- **Fix:** Use `http.delete(\`${TARGET_HOST}/v1/customer/devenv\`, { headers })` (drop the `null` body). Trivial.

---

## 🔄 REGRESSION

### R1. **Iter 1 H8 fix introduced N1/N2/N3 (the `seedTenantEntitlements` + `max_vcpu=2` + "free + runner-promo" claim) — these are NEW, more severe than the original H8**
- **Location:** §3.1 lines 48-52 + §3.1 footer + Appendix D
- **Why it's a regression:** Iter 1 H8 was "Not stated (NOT ENFORCED)" — a gap. The fix in iter 2 *added* citations, but the citations are wrong AND the underlying model is wrong. The WP-10 went from a gap to a confidently-wrong section.
- **Fix:** N1+N2+N3 fixes.

---

## 📊 RE-VERIFICATION OF INVARIANTS

| Invariant | Enforced in Iter 1? | Enforced Now? | Delta |
|-----------|---------------------|---------------|-------|
| INV-DOGFOOD-NO-DATA-LOSS | ✅ | ✅ | — |
| INV-LOAD-TEST-THRESHOLDS | ✅ | ✅ | — |
| INV-SNAPSHOT-INTEGRITY (SHA-256) | ❌ | ✅ | ↑ |
| INV-RUNBOOK-COMPLETE | ❌ | ⚠️ Path + frontmatter fixed; **count drift (N5)** | ↑ but new gap |
| **INV-TIER-PROMO (free + runner-promo)** | ❌ | **❌ NOW REGRESSED — citation is wrong (N1-N3)** | ↓ |
| INV-OBSERVABILITY (Prometheus) | ❌ | ✅ | ↑ |
| INV-COGS-ATTRIBUTION | ❌ | ✅ | ↑ |
| **INV-CHAOS-CATALOG-COVERED** | ❌ | **⚠️ Linked but math breaks GA-GATE-O10 (N4)** | ↑ but new gap |
| INV-SIGNOFF-PACK | ✅ | ✅ | — |
| INV-CHANGELOG-COMPLETE | ❌ | ✅ | ↑ |
| INV-CF-QUOTA-HEADROOM | ❌ | ✅ | ↑ |
| **INV-OKF-COVERAGE** | not raised | **❌ Still zero DevEnv concepts (N6)** | — |
| **INV-RUNNERS-ENTITLEMENT-ACCURATE** | not raised | **❌ Cites wrong function + wrong column (N1-N3)** | — |

**Invariants Enforced: 8/13 (62%)** — was 27% in iter 1, now down from 100% in iter 1's self-report because the new INVARIANTS we just added (OKF coverage, runners-entitlement-accurate) are failing. **Net: WP-10 still fails the invariant gate, just on different axes.**

---

## 📋 DoD GAP RE-VERIFICATION

| # | DoD Item | Iter 1 Verdict | Iter 2 Verdict | Notes |
|---|----------|----------------|----------------|-------|
| 1 | Dogfood program scoped (2 weeks, ≥5 engineers, ≤2h/day) | ❌ | **❌ Tier column is structurally broken (N3)** | rows 48-52 |
| 2 | Dogfood checklist (10 rows) | ⚠️ | ✅ | — |
| 3 | Dogfood gate criteria (6 rows) | ✅ | ✅ | — |
| 4 | Load test scenarios (6 rows) | ❌ | ⚠️ k6 clean except wsStorm (M8) | — |
| 5 | k6 script provided | ❌ | ⚠️ Major defects fixed; wsStorm subprotocol assert missing (M8) | — |
| 6 | Failure-injection tests (5 rows) | ❌ | ⚠️ Math breaks GA-GATE-O10 (N4) | — |
| 7 | User guide subtree (10 pages) | ⚠️ | ⚠️ No OKF grounding (N6) | — |
| 8 | API reference auto-generated | ✅ | ✅ | — |
| 9 | Runbook list (7 runbooks) | ❌ | ✅ path + ID | — |
| 10 | Runbook template with frontmatter | ❌ | ✅ | — |
| 11 | GA Engineering Sign-Offs | ⚠️ | ⚠️ §6.1 chaos math wrong (N4) | — |
| 12 | Quality Gates (10 rows) | ⚠️ | ✅ H9 fixed, M6 fixed | — |
| 13 | Infrastructure Readiness (7 rows) | ⚠️ | ✅ | — |
| 14 | Rollback Plan (4 rows) | ⚠️ | ✅ | — |
| 15 | Launch Timeline | ✅ | ✅ | — |
| 16 | Post-Launch Metrics | ⚠️ | ✅ M2 fixed | — |
| 17 | Sign-off table | ✅ | ✅ | — |
| 18 | Appendix: Onboarding | ✅ | ✅ | — |
| 19 | Appendix: Load test exec | ❌ | ✅ H3 declared | — |
| 20 | Appendix: Emergency contacts | ✅ | ✅ | — |
| 21 | **Appendix D: Print-Ready Checklist** | n/a | ✅ H11 | — |
| 22 | **OKF wiki DevEnv concepts** | n/a | ❌ N6 | — |
| 23 | **Dogfood tier accuracy** | n/a | ❌ N1+N2+N3 | — |

**DoD Pass Rate: 17/23 (74%)** — was 4/20 (20%) in iter 1, but new DoD items added in iter 2 reveal persistent gaps.

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 9 → 0 | 4 NEW | ❌ Regression |
| High Issues | 11 → 0 | 2 NEW | ❌ Regression |
| Medium Issues | 7 → 0 | 2 NEW | ❌ Regression |
| DoD Pass Rate | 20% → 100% | 74% | ↓ (new items) |
| Invariants Enforced | 27% → 100% | 62% | ↓ (new invariants) |
| Iter 1 Fixes Applied Correctly | n/a | 23/27 (85%) | 1 regression, 2 partial |

**OVERALL VERDICT: ❌ FAIL — 4 new BLOCKING, 2 new HIGH, 2 new MEDIUM, 1 regression**

---

## 🔧 FIXES NEEDED FOR ITERATION 2

### Must Fix (Blockers)
1. **N1** — Replace `seedTenantEntitlements` citation with `apps/signup-worker/scripts/d1.py` (or Stripe webhook) + the policy "do not modify seedTenantEntitlements".
2. **N2** — Replace `max_vcpu=2` with `max_vcpu_h=100` (verify against migration 0072).
3. **N3** — Replace "free + runner-promo" with "dogfood-promo (1, 100h)" in §3.1 column + Appendix D + §3.1 footer.
4. **N4** — Either (a) raise GA-GATE-O10 denominator patch as a §6.1 deliverable, or (b) restructure §4.3 to not add 5 NEW chaos rows.

### Should Fix (High)
5. **N5** — Verify `ls specs/_runbooks/RB-*.md | wc -l`, replace "58" with the actual count (55), and raise a cross-WP issue for GA-GATE-O06's "47" → "all RB-*.md" re-parameterisation.
6. **N6** — Add OKF wiki coverage deliverable (≥ 5 DevEnv concepts in `docs/knowledge/surfaces/dev-env/`) to §5.1 or new §5.5.

### Nice to Fix (Medium)
7. **M8** — Add `socket.on('message', ...)` assertion to `wsStorm` (mirror `concurrentDevEnv` lines 303-306).
8. **M9** — Drop the `null` body from `http.delete` call.

---

## 🔍 CROSS-WP COORDINATION NEEDS (NEW IN ITER 2)

1. **WP-07 (Billing)** — `runners_entitlement` is read by `runner_concurrency_for_tenant()` (mentioned in `marketing/corelink-feature-catalog.html` "runner entitlement axis" card). WP-07 owns the Stripe materializer (`apps/signup-worker/src/webhooks/stripe.ts`) that INSERTS the row. The dogfood-promo path requires WP-07 to expose a non-Stripe dogfood provisioning path (e.g. a `clw admin grant-runner <tenant>` CLI or a `d1.py` script). **The WP-10 dogfood program cannot start until WP-07 ships this path or accepts a manual SQL workaround.**
2. **WP-08 (Worker ingress)** — §4.1 references `POST /v1/customer/devenv` returning `vnc_url/tty_url/code_url` (per WP-08:415-417). Iter 1 confirmed the path shape; iter 2 confirms k6 targets it correctly. No new coord.
3. **WP-09 (Dashboard)** — Unchanged from iter 1 (the dashboard N+1 issue on the create endpoint).
4. **WP-06 (Lifecycle)** — §3.2 row "Crash recovery" asserts `onError` fires on OOM. WP-06's `onError` stub in WP-01:564-566 is `throw new Error("NOT_IMPLEMENTED")`. The dogfood checklist will fail this row for every dogfooder until WP-06 ships. **This is a blocking dependency, not just a coord need.** Cross-WP issue: "WP-06 must land before WP-10 dogfood week 1 starts."
5. **GA-GATE-CRITERIA.md** — Two patches required: (a) GA-GATE-O06 re-parameterise from "47" to "all `RB-*.md` files", (b) GA-GATE-O10 denominator re-baselined from 8 to (8 + WP-10 DevEnv additions).
6. **`docs/knowledge/`** — N6 cross-WP: author ≥ 5 DevEnv concepts. No existing owner; should be the TechLead (or a new WI).
7. **CHANGELOG.md** — Iter 1's H9 fix added a §6.2 row "CHANGELOG `[Unreleased]` covers every WP-01..WP-10 `feat:` / `fix:`". This is in §6.2 but the N1/N2/N3 fix above itself constitutes a `fix(devenv): correct dogfood tier citation per WP-10 iter 2 review` — which itself must add a CHANGELOG entry. Iterate gate.

---

## CONVERGENCE TRAJECTORY

- Iter 1: 9 BLOCKING + 11 HIGH + 7 MEDIUM (rejected)
- Iter 2: 4 NEW BLOCKING + 2 NEW HIGH + 2 NEW MEDIUM + 1 regression (rejected)
- **Trajectory: 27 → 9 → 8 (count-wise) but 4 of 9 iter 1 BLOCKING fixes are wrong / partial, and 4 new BLOCKING surfaced in iter 2 that iter 1 could not have found** (because they depended on reading the doc-comment in `seedTenantEntitlements`, the column list in 0072, the OKF wiki contents, and the chaos-catalog math — none of which iter 1 examined).
- **Prediction: 1-2 more iter cycles to convergence.** Iter 3 should focus on (a) the N1-N4 fix, (b) re-validating citations against HEAD (this is the systematic gap — the WP-10 author trusted iter 1's citations without `rg`ing them), and (c) closing the OKF wiki gap (N6) which is the only one that requires code-grounded concept authoring (not a 5-line edit).

---

## NEXT STEPS

1. Apply all 4 BLOCKING fixes (N1-N4) — the citation/column/tier/math errors
2. Apply both HIGH fixes (N5, N6) — count drift + OKF gap
3. Apply both MEDIUM fixes (M8, M9) — k6 polish
4. Re-verify every iter 1 fix that is now in WP-10 with `rg` against HEAD (lesson from N1-N3: trust nothing, verify every path / function / column / file)
5. Proceed to Iteration 3 review

**Do NOT declare PASS until 2 consecutive iterations pass without new BLOCKING issues** (raised from the 3-pass standard for WP-01, because WP-10 is smaller in scope but has more cross-WP surfaces).

---

**END OF WP-10 ITERATION 2 REVIEW**
