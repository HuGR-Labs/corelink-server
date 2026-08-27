# BRIEFING REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 9 BLOCKING, 4 HIGH, 3 MEDIUM**

---

## 🔴 BLOCKING Issues

### B1. **MAJOR FACTUAL ERROR: clw binary location stated WRONG**

- **Location:** §4.1 line 88 + §4.3 line 136 + §10.3 lines 436, 439 + §6.6 line 242 + §9 line 387 + §13.3 line 514
- **Problem:** The briefing states the `clw` binary is in `corelink-runners/crates/clw-*`. This is **FALSE**. Verified by `ls /Users/gustavoschneiter/Documents/HuGR/corelink-runners/crates/` and `ls /Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/`. The `clw-*` crates are in `corelink-workspaces/crates/`, NOT `corelink-runners/crates/`.
- **Impact:** If a new session follows this briefing, ALL clw verification commands will fail. The "Common Lies" table itself is lying. This undermines the entire briefing's credibility.
- **Fix:** Correct all references to use `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/`.

### B2. **§4.1 line 88: Wrong description of corelink-runners**

- **Problem:** Table row says corelink-runners contains "clw workspace binary source" — completely wrong. The runner repo contains: `corelink-runner`, `corelink-cloud-engine`, `corelink-fabric`, `corelink-fabric-api`, `corelink-fabric-server`, `corelink-runners-contracts`, `corelink-check-exec-server`, `corelink-cli`, plus `deploy/cloudflare/src/` (Container DO patterns).
- **Fix:** Replace with accurate description: "Cloudflare Container DO patterns (RunnerContainer, CheckHostContainer), spawn-Worker, CloudflareEngine, fabric, runner orchestration"

### B3. **§4.1 line 89: Wrong description of corelink-workspaces**

- **Problem:** Table row says "**NOT THIS** — the CLI binary. Different scope" — but the briefing just established in §4.1 line 88 that corelink-runners has the CLI. The contradiction makes the briefing incoherent. In reality, `corelink-workspaces/crates/clw-cli/` IS the CLI binary location.
- **Fix:** Replace with: "clw CLI binary (clw snapshot/hydrate/run/ls/rm/prune/erase) + clw-types (frozen contract) + all clw-* libraries. This is where the `clw` command comes from."

### B4. **§6.6 line 242: Wrong path in example**

- **Problem:** "Read `corelink-runners/crates/clw-cli/src/main.rs` to verify" — wrong path. Should be `corelink-workspaces/crates/clw-cli/src/main.rs`.
- **Fix:** Correct path.

### B5. **§10.1 line 404: grep command wrong path**

- **Problem:** Not a path issue, but `grep -n "clw " WP-XX_*.md` will hit "clw " in many false-positive contexts (like "clw types"). Should be more specific.
- **Fix:** Use `grep -n "clw snapshot\|clw hydrate\|clw run" WP-XX_*.md` to target actual CLI invocations.

### B6. **§10.3 lines 436, 439: Wrong paths for clw verification**

- **Problem:** Both grep commands point to `corelink-runners/crates/clw-cli/src/main.rs`. Wrong repo.
- **Fix:** Change to `corelink-workspaces/crates/clw-cli/src/main.rs`.

### B7. **§9 line 387: Wrong "lesson learned"**

- **Problem:** "The `clw-*` crates are in `corelink-runners/crates/`, not `corelink-workspaces`" — FALSE. The opposite is true.
- **Fix:** Correct to: "The `clw-*` crates are in `corelink-workspaces/crates/`, not `corelink-runners`."

### B8. **§13.3 line 514: Path reference ambiguous/wrong**

- **Problem:** "Read the source (`clw-cli/src/main.rs`...)" — doesn't specify the absolute path. A new session would not know where to look.
- **Fix:** Use absolute path: `/Users/gustavoschneiter/Documents/HuGR/corelink-workspaces/crates/clw-cli/src/main.rs`

### B9. **Version header claims "3-iteration reviewed" — false**

- **Problem:** Line 3: "**Version:** 1.0 (SOTA, 3-iteration reviewed)". This is iteration 1. The "3-iteration reviewed" claim is false.
- **Fix:** Change to "**Version:** 1.0 (DRAFT, iteration 1 of review)".

---

## 🟠 HIGH Issues

### H1. **Self-contradictory across sections**

- **Problem:** §4.1 line 88 implies clw is in corelink-runners. §4.3 line 136 says clw is in corelink-runners. But the actual repo structure has clw in corelink-workspaces. The briefing contradicts the filesystem.
- **Fix:** After B1-B3 fixes, briefing will be self-consistent. Verify by reading end-to-end.

### H2. **"Common Lies" table itself contains a lie**

- **Problem:** The "Common Lies" table is supposed to list things the WPs get wrong. But one of its entries is itself wrong (line 136). This destroys trust in the entire table.
- **Fix:** Verify every entry in "Common Lies" against HEAD before declaring the briefing done.

### H3. **Missing: how to identify which WPs have been reviewed**

- **Problem:** The briefing says "Audit 10 WPs" but doesn't tell a new session which WPs have already been reviewed (the existing REVIEW_*.md files) and which might be fully converged.
- **Fix:** Add a section "Checking Review Status" with commands to list existing reviews.

### H4. **No warning about trusting prior reviews**

- **Problem:** A new session might assume that since the WPs have REVIEW_*.md files, they are "done". But prior reviews (including the ones this briefing is based on) may have errors. The briefing should explicitly warn against this.
- **Fix:** Add a prominent warning at the top: "DO NOT TRUST EXISTING REVIEWS. They are part of the audit corpus. Read them, but verify every claim with HEAD."

---

## 🟡 MEDIUM Issues

### M1. **Briefing is 590 lines — could be more concise**

- **Problem:** Some redundancy between §6 patterns and §14 catalog. A new session reading this will take 30+ min to absorb.
- **Fix:** Consolidate §6 and §14. Keep §6 (patterns to look for) and remove §14 (historical catalog is reference, move to appendix).

### M2. **No explicit "you are a reviewer" framing**

- **Problem:** The briefing reads as documentation, not as instructions to an agent. A new session might read it passively.
- **Fix:** Add a "YOUR ROLE" section at the top making clear: "You are a code reviewer. Your job is to find and fix bugs. You are not reading for pleasure."

### M3. **"Convergence Rule" is in §5.2 but the convergence mechanism isn't explained**

- **Problem:** Says "2 consecutive clean iterations = PASS" but doesn't explain how to KNOW when an iteration is "clean". Does 1 MEDIUM issue count as clean?
- **Fix:** Clarify: "Clean = 0 BLOCKING AND 0 HIGH. MEDIUM issues are allowed and don't block convergence."

---

## 📊 SCORECARD

| Category | Score |
|----------|-------|
| Blocking Issues | 9 (NEEDS FIXING) |
| High Issues | 4 |
| Medium Issues | 3 |
| Factual Accuracy | ❌ FAILED (clw location wrong in 7 places) |
| Internal Consistency | ❌ FAILED (contradictions across sections) |
| Self-Awareness | ⚠️ PARTIAL (version label false) |
| Usefulness | ⚠️ POTENTIAL (if BLOCKING fixed) |

---

## 🔧 FIX PRIORITY

### Must Fix Immediately
1. **B1-B8**: All clw location references (8 places) — single highest-priority fix
2. **B9**: Version label

### Should Fix
1. H1-H4: Internal consistency + reviewer framing

### Nice to Fix
1. M1-M3: Conciseness, role framing, convergence clarification

---

**Next Step:** Apply all BLOCKING + HIGH fixes. After fix, run iteration 2.