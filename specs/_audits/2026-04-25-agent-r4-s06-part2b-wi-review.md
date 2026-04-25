---
id: "AUDIT-2026-04-25-AGENT-R4-S06-PART2B"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
created: "2026-04-25"
reviewer: "Agent R4 (Claude Opus 4.7, 1M context — round 4 part 2b focused)"
scope: "Lote 10.6 — Sprint S-06 Part 2b (WI-S06-006 TLA+ CI gate + WI-S06-007 PRR ship gate)"
sprint_contract: "specs/04_sprints/S06/_spec_contract.md v1.1.0"
calibration_baselines:
  - "specs/_audits/2026-04-25-agent-r4-s06-part1-wi-review.md (8.13/10 aggregate; P0-1 LIKE→json_each highest leverage)"
  - "specs/_audits/2026-04-25-agent-r4-s05-part2-wi-review.md (8.05/10)"
  - "WI-S04-003 best-in-class 8.6"
files_reviewed:
  - "specs/04_sprints/S06/work_items/WI-S06-006-tla-ci-gate-property-test-100k-race.md (429 lines)"
  - "specs/04_sprints/S06/work_items/WI-S06-007-dash-gc-rb-dry-runs-prr-ship-gate.md (585 lines)"
cross_references:
  - "specs/04_sprints/S06/_spec_contract.md v1.1.0 §6 DoD; §10.s06.4 30d TLA+ verde; §19 Waiver policy"
  - "specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md (FROZEN; 91 lines design content)"
  - "specs/03_architecture/invariant_registry.md §3.17 (23 INV-GC-* rows promovidas preemptivamente)"
  - "specs/04_sprints/S05/work_items/WI-S05-006-sweeper-rb-fm-060-prr-ship-gate.md (precedent staffing)"
  - "specs/_audits/2026-04-25-agent-r4-s06-part1-wi-review.md (existing P0-1..P0-6)"
---

# Agent R4 — Lote 10.6 S-06 Part 2b (WI-S06-006 + WI-S06-007) WI Review

> **Reviewer**: Agent R4 (independent SOTA reviewer; ruthless, technical, no diplomacy).
> **Calibration target**: User directive "average não serve. SOTA puro 9-10". Best in program: WI-S04-003 = 8.6. S-06 part 1 average 8.13.

---

## Veredito Geral

WI-006 + WI-007 are **the two governance / ship-gate WIs of S-06** and they are written with a clear awareness of the Lote 10.4bis + 10.5bis lessons (Crypto SME mandatory non-waivable, validate_inv_promotion.py CI gate, 4-tier P0/P1/P2/P3 incident classification, sprint contract drift defense, ADR design content not rubber-stamp, production rollout gradual 10/50/100). The core scaffolding is correct — `tla-ci-gate.yml` workflow has the right shape (path-based PR scope filter on `crates/corelink-gc`/`gc_correctness.tla`/`migrations/**blob_meta**`/`migrations/**ac_meta**`; TLC v1.8.0 pinned; `0 errors found` + invariant-name greps; status-check + branch-protection enforcement; ADR + Architect + Crypto SME override discipline); the property test 100k race specifies deterministic seeds (`generate_random_interleaving(iter)` keyed on iter index), reproducible failures (eprintln of mark_started_at_ms / ac_created_at_ms on violation), and an unambiguous "violations == 0" assertion that maps to TLA+ `InvGCReRefProtected`. The 30d sustained verde gate has its own workflow file (`tla-30d-sustained.yml`, separate from per-PR `tla-ci-gate.yml`); a metric `corelink.ci.tla.30d_sustained_verde` (gauge boolean) is emitted; the cumulative INV §3.17 promotion list in WI-007 §1.7 is enumerated WI-by-WI; ADR-0042 was verified to exist at `specs/03_architecture/adrs/ADR-0042-gc-worker-scheduler.md` (FROZEN; 91 lines; substantive Decision matrix + Rejected alternatives + Consequences — **NOT** rubber-stamp procedural like ADR-0040 was flagged in S-05 part 2). Sprint contract §19 was verified to no longer carry the 95% TLA+ verde waiver — only refcount drift, mark-budget extension for >5M-blob tenants, and customer-opt-in <72h grace are waivable, and **TLA+ CI verde sustained**, **property test 100k**, **chaos 30d zero violations**, **RB-FM-300 + RB-FM-404 dry-runs** are explicitly non-waivable. WI-007 §30 sign-off table correctly grades Crypto SME as `**MANDATORY** (non-waivable)` row 13, with row 12 AppSec as `**mandatory emphatic**` for TLA+ obligation alignment — this is a **defensible split of responsibility**, but see P1-3 below for a row-12 vs row-13 emphatic-vs-mandatory inversion concern.

But seven defect classes hold this pair short of the **9-10 SOTA bar** and below the WI-S04-003 ceiling. **First (P0, the highest-leverage one): WI-006 §1's property test 100k race code does NOT inherit the WI-S06-003 P0-1 LIKE→json_each defect because the property test does not publish any SQL — `execute_gc_scenario(scenario)` is described as "Reuses real Rust impl from WI-S06-002 (mark) + WI-S06-003 (sweep)".** This is the right architectural choice (the property test should exercise the production code path), BUT it makes the property test **silently inherit the bug** unless WI-006 explicitly pins that the SQL exercised by the property test is the **fixed json_each EXISTS check**, not the broken LIKE check. As currently specified, if WI-S06-003 §6.1.2 ships with the `LIKE '%' || $2 || '%'` defect, the property test 100k race will **PASS GREEN** (because BLAKE3-256 hex digests at fixed length 64 chars never collide via substring under the JSON wrapper as long as the fixture generator only emits hex digests as values — no parent_digest or short-digest fields), giving **false confidence** that INV-GC-004 holds. The property test needs to be **explicit** that it cross-validates the json_each idiom AND that the fixture generator includes adversarial inputs (digests as substring-prefixes of other values, JSON envelope mutation, schema-evolution scenario with `blob_refs` wrapped in metadata object) so the test would FAIL if the LIKE defect ever returned. This is a hard cross-WI dependency: WI-006 must explicitly require WI-S06-003 §6.1.2 to land the json_each fix BEFORE the property test runs in CI, OR WI-006 must instrument its own SQL EXISTS check (parallel implementation; cross-validates the production code via differential testing). The spec as written gives neither; this is a load-bearing gap.

**Second (P0): WI-007 PRR sign-off staffing is a near-verbatim copy of S-05 part 2's 10-of-13-unfilled defect.** §30 Sign-off table shows: Owner/Final Approver = `_pending_` (Gustavo, both), SRE Lead = `_staffing-blocked; ADR-0034 waiver via Architect compensation_`, Security Lead/Engineer×2/QA/Compliance/Privacy/Architect/AppSec/Crypto SME = all `_TBD; **mandatory**_` (or emphatic / non-waivable). That is **10-of-13 still TBD** at spec time, with one explicit ADR-0034 waiver for SRE Lead. This is the **exact** carry-forward defect S-04 part 2 audit and S-05 part 2 audit both flagged as a persistent program defect. The two-week-advance Crypto SME booking lesson from Lote 10.4bis (R-008 risk register row in WI-007) acknowledges the constraint but does not commit a **booking date** or an **escalation path** if the booking slips inside the sprint window. WI-007 §29 Review Checkpoints schedules `D+6 PRR meeting` with no per-role pre-booking schedule — so on D+6 the program will discover whether the 13 sign-offs are bookable, by which time WIs-001..006 are SEALED and the sprint window is consumed. This is a **process-time-budgeting** defect that S-05 audits already flagged and Lote 10.5bis lessons already coded; the spec did not absorb the lesson. The fix is concrete: add a §29.X "Sign-off booking calendar" subsection committing each role's calendar slot (e.g., Crypto SME D-14 advance booking; AppSec/Architect D-7; Privacy/Compliance D-5; Engineer×2/QA D-3; Security Lead D+2 same as PRR), and an escalation path if any booking slips (Architect compensation per ADR-0034 OR sprint-extension via ADR), AND a CI gate `validate_signoff_calendar.py` that fails if any TBD slot is within D-3 of PRR.

**Third (P0): WI-007 cumulative INV §3.17 promotion claims "~22 INVs" but the registry actually contains 23 INV-GC-* rows promovidas preemptivamente.** WI-007 §1.7, §6.1.8, §10.s06.007.7, §15 chaos #12, §28 R-010 all reference `~22 INVs §3.17`. The actual count in `invariant_registry.md §3.17` (lines 331-353) is **23 rows** (verified by direct enumeration). The discrepancy is small but material because **`validate_inv_promotion.py` is a CI gate** that enforces WI-declared ↔ registry alignment; if the script counts WI-declared promotions and asserts equality with registry rows, then 22-vs-23 will FAIL CI. This is exactly the rubber-stamp prevention pattern Lote 10.4bis introduced — the gate works **as designed**, and the spec's count is wrong. Either drop a duplicate (P1-1 from part 1 already proposed merging `INV-GC-MARK-STARTED-AT-IMMUTABLE` + `INV-GC-MARK-STARTED-AT-ATOMIC`, which would give 22 — that is likely the intended state) OR update WI-007 to claim 23. The spec as written is internally inconsistent.

**Fourth (P0): RB-FM-300 / 404 / 305 dry-run targets are claimed but the 5/30/60-min sustainability / replicability is not benchmarked.** WI-007 §6.1.4 specifies "RB-FM-300: detection ≤ 5min / remediation ≤ 30min / customer notif ≤ 1h"; "RB-FM-404: detection ≤ 1min / remediation ≤ 30min"; "RB-FM-305: detection via storage growth metric / manual trigger via admin API stub". §14.s06.007.3 codifies "3 RB dry-runs ≤ 5/30/60 min targets". But: (a) the dry-run is run **once** per RB; "≤ 5 min" detection is unverifiable from a single sample (could be 4.9 min one run, 5.5 min the next; need ≥3 runs per RB to establish p95 / sustainability); (b) the chaos PR injection details are not specified (which exact refcount-drift magnitude triggers the SEV-2 alert? the threshold is 0.1% global / 1% per-tenant per WI-S06-005 / sprint contract — at what magnitude does the dry-run inject?); (c) RB-FM-305 detection "via storage growth metric + sweeper tick rate alert" with no time bound — the sweeper-tick-stale alert fires after `> 1h` per §6.1.5 alert rules, which is **far** above any "≤ 5 min" implicit detection target. The dry-runs as specified will produce post-mortems but those post-mortems **cannot validate sustainability** (the customer-trust claim "we caught and remediated within 30 min" needs ≥ 3 reproducible runs to be defensible to a Compliance/Privacy/AppSec reviewer at PRR). Fix: each RB dry-run must be specified as **≥ 3 independent runs with documented seed/timing variance**, and the SEV-2 alert latency for refcount-drift detection must be cross-checked against the WI-S06-005 alert configuration (no spec was reviewed in this audit but flag for cross-WI verification).

**Fifth (P0): the 30d sustained chaos test under load is "extended 30d staging com synthetic workload (Bazel + multipart) production-like" but the 4h-under-1k-QPS chaos test is conflated with the 30d sustained chaos test.** Sprint contract §6 DoD specifies TWO separate tests: "Chaos test: rodar GC durante write load 1k QPS por 4h → zero falso positivo" AND completeness criteria §10.s06.2 "Zero blobs reachable deletados em 30d staging sustained". WI-007 §1.5 conflates: "GC + write load 1k QPS sustained 4h; INV-GC-001/004 violations = 0. Extended 30d staging com synthetic workload (Bazel + multipart) production-like. Pause clock on P0/P1 incidents (4-tier classification Lote 10.4bis lesson)." The 4h-1kQPS test is a **separable, repeatable acceptance criterion** that should run as a CI integration test (not a 30d staging observation period). The 30d sustained test is a **post-sprint observation period** concurrent with S-07/S-08 (per sprint contract §13). WI-007 §6.1.6 only lists "30d sustained chaos test" as one in-scope item; the 4h-1kQPS gate is missing as an explicit Gherkin scenario / DoD checkbox. Fix: split into `10.s06.007.5a` (4h-1kQPS chaos zero violations; pre-merge gate, repeatable) and `10.s06.007.5b` (30d sustained chaos zero violations; post-sprint observation gate; pause-clock-on-P0/P1).

**Sixth (P0): override discipline for `tla-ci-gate.yml` is specified as "ADR + Architect + Crypto SME signoff" but the GitHub Actions / branch-protection mechanics for the override path are not pinned.** WI-006 §1 invariant 2 says: "TLC red → CI fails → merge blocked; reviewer can override only via ADR + Architect+Crypto SME sign-off (consistente Lote 10.4bis ADR governance lesson)". §9.4: "Override via ADR + Architect + Crypto SME signoff (Lote 10.4bis governance)". §6.1.7: "Override discipline: ADR + Architect + Crypto SME sign-off mandatory pre-override". But none of these specify the **GitHub-mechanism by which the override is recognized**: is it (a) a CODEOWNERS rule requiring 2 approving reviews from `@architect-team` AND `@crypto-sme-team`? (b) a branch protection bypass via admin manual? (c) a labeled PR (`tla-override-approved`) that switches off the required status check? (d) a separate workflow that posts a synthetic green status check after detecting an ADR commit + 2 sign-off git-trailers? Without one of these mechanisms specified, the "override discipline" is an aspiration. Worse, the most permissive default in GitHub is **"admins can bypass branch protection"** which is the rubber-stamp regression Lote 10.4bis was supposed to prevent. Fix: pin one mechanism (recommended: CODEOWNERS rule + signed git-trailer `Tla-Override-ADR: ADR-XXXX` validated by a workflow that asserts the ADR file exists at the canonical path AND has `doc_status: ACCEPTED` + Architect + Crypto SME approvals in the ADR sign-off block); explicitly disable admin-bypass on `main` branch protection. Add a chaos test (currently §6.1.12 #3 "CI override attempt without ADR → blocked") that verifies admins cannot bypass without the ADR + sign-offs.

**Seventh (P0): TLC v1.8.0 download from `github.com/tlaplus/tlaplus` is NOT integrity-verified — no SHA-256 checksum pin.** WI-006 §1 yaml: `wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar`. Supply-chain risk: if the GitHub release artifact is replaced (compromised maintainer key, GitHub account takeover, MITM on a CI runner), the CI gate runs **a malicious TLC** that reports `0 errors found` for any input. The defense-in-depth claim is voided silently. This is the same defect class as the cargo-deny / cargo-audit pinning that Lote 10.4bis covered for Rust deps, and it is **load-bearing** because TLC IS the formal verification baseline. Fix: pin SHA-256 of the v1.8.0 jar in the workflow; verify checksum before invocation; fail CI if checksum drift. Add to ADR-0042 (or a new ADR-0043) the policy "TLC version + SHA pinned; bumps require ADR + Architect + Crypto SME signoff". Add a chaos test "TLC binary tamper detection" that mutates a byte and asserts CI fails on checksum mismatch.

**Beyond the seven P0s:** WI-006 chaos suite §6.1.12 has 10 scenarios (meets HIGH_RISK ≥ 10 floor exactly; no margin). WI-007 chaos suite §15 has 12 scenarios (exceeds floor — appropriate for ship gate). Risk register: WI-006 §28 has 12 rows (meets ≥ 10 floor); WI-007 §28 has 12 rows. 13-row sign-off table itemized in both; Crypto SME MANDATORY EMPHATIC in WI-006 §30 (correct — TLA+ obligation); Crypto SME MANDATORY non-waivable in WI-007 §30 row 13 (correct). The compact §9-32 form IS used in WI-006 (line 329 onwards) — this is an information-density tradeoff, but for a CI-gate WI it is defensible (WI-S05-003 also compressed). WI-007 uses the full SOTA layout. The "DSR erasure interaction tested" DoD item from sprint contract §6 is captured in WI-007 §10.s06.007.14 + §17 ST-021 (3h budget) — captured but understaffed (DSR erasure interaction with grace bypass is the cripto+privacy boundary that needs Privacy + Crypto SME independent review, easily 8-16h not 3h). Customer-comm pre-ship is captured (§13 SLA addendum + release notes + safety doc). Production rollout gradual 10/50/100 captured in §15.x and §29 D+9/+11/+13 timeline. Cost gate ±10% captured in §6.1.10 + §22. The pair earns a **part 2b average of 8.0/10**, materially below part 1's 8.13 and below the WI-S04-003 8.6 ceiling.

The single highest-leverage Lote 10.6bis action **for this pair** is **the sign-off booking calendar (P0-W7-2)** — it costs ~4h to add to the spec, it closes a persistent program defect that has carried through S-04 + S-05, and it is a precondition for the 13-sign-off ceremony to be schedulable rather than aspirational. The single highest-leverage **technical** action is **TLC SHA-256 pinning (P0-W6-7)** — 1h spec edit + 30 min CI workflow update — closes a supply-chain hole in the formal verification baseline.

**Aggregate Part 2b score: 8.0/10.**

---

## Per-WI Numerical Score

| WI | Score | Cripto | Complete | Clarity | SOTA | Internal | Prior-WI | Customer | TLA+ | Verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| WI-S06-006 | **8.0** | 8.0 | 8.0 | 8.5 | 8.5 | 7.5 | 7.5 | 7.5 | 8.5 | pass-with-fixes (P0) |
| WI-S06-007 | **8.0** | 7.5 | 8.5 | 8.0 | 8.5 | 7.5 | 7.5 | 8.5 | 8.0 | pass-with-fixes (P0) |

**Average: 8.0/10.** (Below S-06 part 1's 8.13; below S-05 part 2's 8.05 by a hair; well below WI-S04-003's 8.6 ceiling.)

Breakdown:
- **Cripto rigor**: WI-006 leads on TLA+ obligation alignment (TLC pinned-version, invariant-name asserts, override discipline ADR+Architect+Crypto SME); held back by TLC SHA pinning gap (P0-W6-7) and property-test inheritance of upstream LIKE defect (P0-W6-1).
- **Completeness**: WI-007 has 12 Gherkin scenarios + 14 DoD checkboxes (most thorough in S-06); WI-006 compresses §9-32 (defensible but reduces information density).
- **Clarity**: Both WIs are operationally readable. WI-006 publishes the yaml and the Rust property test code inline (cleanest cripto-evidence in the pair).
- **SOTA-adherence**: 13-row sign-off + per-role checklist + 4-tier P0/P1/P2/P3 + Crypto SME mandatory + validate_inv_promotion CI gate + production rollout gradual all present in WI-007. WI-006 has 5 metrics + cargo-audit/deny + cost gate per-PR.
- **Internal consistency**: WI-006 has the property-test-inheritance gap (P0-W6-1); WI-007 has the 22-vs-23 INV-count mismatch (P0-W7-3) and the 4h-vs-30d chaos test conflation (P0-W7-5).
- **Prior-WI consistency**: ADR-0042 verified to exist + FROZEN + design content substantive (NOT rubber-stamp like ADR-0040 was flagged). Sprint contract §19 verified — 95% TLA+ verde waiver REMOVED (good). RB-FM-300/404/305 cross-references intact.
- **Customer-facing readiness**: WI-007 §3 personas + §1.10 customer comm artifacts (SLA addendum + release notes + safety doc). WI-006 is invisible to customer (CI infrastructure) — defensible.
- **TLA+ alignment**: WI-006 explicitly cites `gc_correctness.tla` + `InvGCReRefProtected` + `InvGCNeverDeleteReachable` (correct). The TLA+ ↔ Rust action mapping table from part 1 P0-3 is **NOT** added in WI-006 — that is a missed opportunity, since WI-006 is the natural home for the TLA+ ↔ Rust contract documentation.

---

## P0 Findings (must-fix before Lote 10.6bis SEAL)

### P0-W6-1 — WI-S06-006 property test 100k race silently inherits WI-S06-003 §6.1.2 LIKE→json_each defect

**Severity**: P0 cripto-load-bearing (compounds part 1 P0-1). **WI**: WI-S06-006 §1 (property test code), §6.1.4-5, §10.s06.006.7.

The property test `prop_gc_004_race_mark_update_ar_100k` calls `execute_gc_scenario(scenario)` which is described as "Reuses real Rust impl from WI-S06-002 (mark) + WI-S06-003 (sweep)". If WI-S06-003 §6.1.2 ships with the broken `LIKE '%' || $2 || '%'` SQL EXISTS check (part 1 P0-1), the property test will pass green for 100k iterations because BLAKE3-256 hex digests at fixed length 64 chars + JSON-array-of-strings wrapper never collide via substring under benign fixture generators. The test gives **false confidence** that INV-GC-004 holds while the production SQL is broken.

The defense-in-depth claim "TLA+ + property test 100k + chaos sob load = 3 layers" is voided silently because layer 2 (property test) is testing the same broken SQL as layer 3 (chaos under load); only TLA+ is actually orthogonal — and TLA+ does not exercise the SQL string at all (it formalizes the algorithm, not the implementation).

**Fix** (Lote 10.6bis):

(a) Hard cross-WI dependency: WI-006 §18 Dependencies must list "Hard: WI-S06-003 §6.1.2 SQL EXISTS check uses `json_each(a.blob_refs)` idiom (part 1 P0-1 fix landed); blocking precondition for property test CI run".

(b) Augment property test fixture generator (`generate_random_interleaving`) to include adversarial inputs that would FAIL the LIKE check but pass json_each:
- `blob_refs` JSON envelope mutation: wrap in `{"refs": [...], "metadata": {"parent_digest": $other_digest}}`.
- Short-digest substring scenarios (16-char prefix in a metadata field that contains digest D as substring).
- Schema-evolution scenario: `blob_refs` evolves to JSON object — assert SQL fails to compile (compile-time check) OR returns 0 protected for all candidates (semantic check).

(c) Add explicit cross-validation property test `prop_inv_gc_004_json_blob_refs_evolution` (mentioned in part 1 P0-1 fix; lift up into WI-006 §1 explicitly).

(d) Add Crypto SME review checkpoint: "the property test must be run against a deliberately-broken LIKE-substring impl as a negative control before the SEAL — verify the test FAILS the broken impl (proves test has discriminating power)".

Estimated effort: 2h spec rewrite + 6h fixture generator + 4h Crypto SME differential review = ~12h.

### P0-W6-2 — WI-S06-006 TLC v1.8.0 download is not SHA-256 integrity-verified (supply-chain hole in formal verification baseline)

**Severity**: P0 supply-chain. **WI**: WI-S06-006 §1 yaml.

Current:
```yaml
- name: Install TLC (TLA+ tools)
  run: |
    wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
    echo "TLA_HOME=$(pwd)/tla2tools.jar" >> $GITHUB_ENV
```

If the GitHub release artifact is replaced (maintainer key compromise, GitHub account takeover, CDN MITM on a CI runner), CI runs a malicious TLC that reports `0 errors found` for any input — the **formal verification baseline is silently voided**. Same defect class as cargo-deny pinning (Lote 10.4bis closed for Rust deps).

**Fix** (Lote 10.6bis):

(a) Pin SHA-256 of v1.8.0 jar:
```yaml
- name: Install TLC v1.8.0 (pinned SHA-256)
  run: |
    wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
    echo "<sha256>  tla2tools.jar" | sha256sum -c -
    echo "TLA_HOME=$(pwd)/tla2tools.jar" >> $GITHUB_ENV
```

(b) Fail CI hard on checksum mismatch.

(c) Add ADR-0043-tla-tools-version-pinning: "TLC version + SHA pinned; bumps require ADR + Architect + Crypto SME signoff" — OR augment ADR-0042 with this clause.

(d) Add chaos test #11 "TLC binary tamper detection": mutate a byte; assert CI fails on checksum mismatch.

(e) Cache the verified jar in GitHub Actions cache keyed by SHA to avoid re-downloading every PR (cost optimization).

Estimated effort: 1h spec edit + 30min workflow update + 2h ADR drafting + 1h chaos test = ~5h.

### P0-W6-3 — WI-S06-006 override discipline (ADR + Architect + Crypto SME) GitHub-mechanism not pinned

**Severity**: P0 governance correctness. **WI**: WI-S06-006 §1 invariant 2, §6.1.7, §9.4, §15 (chaos #3).

Override path is described prose-only ("ADR + Architect+Crypto SME sign-off"). No GitHub branch-protection mechanism specified: CODEOWNERS rule? Labeled-PR bypass? Synthetic green status check after ADR commit + git-trailer validation? Admin manual bypass (the rubber-stamp default Lote 10.4bis was supposed to prevent)?

Without a pinned mechanism, "override discipline" is aspirational. Worst-case default in GitHub is admins-can-bypass-branch-protection — **the exact rubber-stamp regression** Lote 10.4bis flagged.

**Fix** (Lote 10.6bis):

(a) Pin one mechanism. Recommended: CODEOWNERS rule + signed git-trailer + workflow validator:
- `.github/CODEOWNERS` — `crates/corelink-gc/** @humangr-labs/architect-team @humangr-labs/crypto-sme-team`.
- `gc_correctness.tla` modifications require **2 approving reviews** (1 architect + 1 crypto-sme group).
- Override workflow `tla-override-validate.yml` parses PR body / commit trailers for `Tla-Override-ADR: ADR-XXXX`; asserts the ADR file exists at canonical path with `doc_status: ACCEPTED` AND has Architect + Crypto SME approvals in ADR sign-off block; only then posts a synthetic green status check that satisfies the required `tla-ci-gate` check.
- **Disable admin-bypass on `main`** branch protection (`enforce_admins: true`).

(b) Document the override mechanism in §9 design decisions OR ADR-0042 addendum.

(c) Add chaos test "Admin attempts force-push without ADR override" → blocked by `enforce_admins: true`. Augment §6.1.12 #3 to be specific.

(d) Add Crypto SME review checkpoint: "verify branch-protection settings on `main` enforce non-bypassable status check + 2-team CODEOWNERS rule".

Estimated effort: 3h spec rewrite + 3h CODEOWNERS + override workflow + 2h Crypto SME review = ~8h.

### P0-W7-1 — WI-S06-007 PRR sign-off staffing 10-of-13 unfilled = persistent program defect carry-forward from S-04/S-05

**Severity**: P0 process risk. **WI**: WI-S06-007 §30 Sign-off (HIGH_RISK 13).

§30 status:
- 1-2 Owner / Final Approver (Gustavo): `_pending_`
- 3 SRE Lead: `_staffing-blocked; ADR-0034 waiver via Architect compensation_`
- 4-13 (Security Lead, Engineer×2, QA, Compliance, Privacy, Architect, AppSec, Crypto SME): all `_TBD_`

That is **10 TBD slots + 2 pending self-roles + 1 ADR-0034 SRE waiver = 13 unbooked sign-offs at spec time**. Identical to the S-05 part 2 audit finding (and S-04 part 2 before it). Lote 10.4bis lessons explicitly called out 40-80h Crypto SME advance booking — WI-007 §28 R-008 acknowledges this as a risk mitigation but no concrete booking calendar is committed.

§29 schedules `D+6 PRR meeting` with no per-role pre-booking — the program will discover on D+6 whether 13 sign-offs are bookable, by which time WIs-001..006 are SEALED and the sprint window is consumed.

**Fix** (Lote 10.6bis):

(a) Add §29.1 "Sign-off Booking Calendar" subsection committing each role's calendar slot:
| Role | Booking lead-time | PRR slot | Escalation path |
|---|---|---|---|
| Crypto SME (row 13, MANDATORY non-waivable) | D-14 advance | D+6 PRR + D+5 pre-review (8h) | If unbookable: sprint extension via ADR; NO ADR-0034 waiver allowed (non-waivable) |
| AppSec (row 12, mandatory emphatic) | D-7 | D+6 + D+4 pre-review | ADR-0034 path with Security Lead compensation |
| Architect (row 11, mandatory) | D-7 | D+6 | ADR-0034 path; Crypto SME co-sign acceptable |
| Privacy (row 10, mandatory) | D-5 | D+6 | DPO interim acceptable per ADR-0017 |
| Compliance (row 9, mandatory) | D-5 | D+6 | ADR-0034 path |
| Engineer×2, QA (rows 5-7) | D-3 | D+6 | Owner + 1 peer acceptable |
| Security Lead (row 4) | D-3 | D+6 | ADR-0034 path |
| Product (row 8) + Owner/Final Approver (rows 1-2) | self-scheduled | D+6 + D+8 ship decision | n/a |
| SRE Lead (row 3) | already waived ADR-0034 | n/a | Architect compensation |

(b) CI gate `validate_signoff_calendar.py` (extends Lote 10.4bis CI gate pattern): fails if any TBD slot is within D-3 of PRR; warns at D-7.

(c) Update §28 R-008 to "M H HIGH H" exposure (was M L MEDIUM L) — staffing risk is **detection-low / impact-high / probability-medium** based on S-04 + S-05 precedent.

(d) Add Sub-task ST-022 "Sign-off booking calendar maintenance + escalation triggering" 4h.

(e) Sprint contract §11 dependencies update: "Hard: Crypto SME D-14 advance booking confirmed; failure to book triggers sprint extension via ADR not waiver".

Estimated effort: 4h spec rewrite + 2h CI gate + 1h R-008 update = ~7h. (Lowest-cost highest-leverage fix in this pair.)

### P0-W7-2 — WI-S06-007 cumulative INV §3.17 promotion count mismatch (~22 claimed vs 23 actual rows)

**Severity**: P0 CI-gate-blocking. **WI**: WI-S06-007 §1.7, §6.1.8, §10.s06.007.7, §15 chaos #12, §28 R-010.

WI-007 claims `~22 INVs §3.17`. Direct enumeration of `invariant_registry.md §3.17` lines 331-353 = **23 rows**:

| WI source | INVs claimed | INVs in registry §3.17 |
|---|---|---|
| WI-S06-001 | 5 | 5 (IDEMPOTENT-RERUN, SINGLE-RUNNING-PER-TENANT-REGION, PHASE-MONOTONIC, MARK-STARTED-AT-IMMUTABLE, DEGRADE-MODE-PROBE-PER-BATCH) |
| WI-S06-002 | 5 | 5 (MARK-STARTED-AT-ATOMIC, REACHABLE-SET-COMPLETE, MARK-TENANT-SCOPED, MARK-PHASE-BUDGETED, MARK-D1-BOUNDED-BATCH) |
| WI-S06-003 | 4 | 4 (SWEEP-AUDIT-FAIL-CLOSED, SWEEP-IDEMPOTENT, SWEEP-TENANT-SCOPED, GRACE-RESPECTED) |
| WI-S06-004 | 4 | 4 (PHYSICAL-DELETE-IDEMPOTENT, GRACE-BOUNDARY-STRICT, R2-D1-ORDERING, DSR-BYPASS-AUTHORIZED) |
| WI-S06-005 | 2 | 2 (RECONCILE-AUTO-FIX-BOUNDED, RECONCILE-AUDIT-FAIL-CLOSED) |
| WI-S06-006 | 3 | 3 (CI-GATE-ENFORCED, PROPERTY-TEST-CROSS-VALIDATED, 30D-SUSTAINED-VERIFICATION) |
| **Total** | **23** | **23** |

WI-007 §1 enumerates the same 23 IDs but the cumulative summary says "~22". `validate_inv_promotion.py` will fail CI if it does strict count alignment.

This intersects with part 1 P1-1 (merge `INV-GC-MARK-STARTED-AT-IMMUTABLE` + `INV-GC-MARK-STARTED-AT-ATOMIC` into composite). If P1-1 is applied → 22 rows, matches "~22" claim. If P1-1 is rejected → 23 rows; WI-007 must be updated to 23 + sprint contract §6 + WI-006 §12 + WI-007 §1.7 / §6.1.8 / §10.s06.007.7.

**Fix** (Lote 10.6bis):

(a) Decide P1-1 (merge IMMUTABLE + ATOMIC). Recommended: **merge** (they are not orthogonal — IMMUTABLE is a property of the timestamp; ATOMIC is the capture mechanism; both are needed and they should be a single composite INV per part 1 P1-1 fix).

(b) Update WI-007 §1.7 to enumerate the merged composite + post-merge count = 22.

(c) Update §6.1.8 + §10.s06.007.7 + §15 chaos #12 + §28 R-010 + DoD checklist to consistent count.

(d) Run `validate_inv_promotion.py` locally pre-PR; CI gate green.

Estimated effort: 1h spec rewrite (depends on P1-1 decision; could be done in 30min if P1-1 already applied) = ~1h.

### P0-W7-3 — WI-S06-007 conflates 4h-1kQPS chaos test with 30d sustained chaos test (sprint contract DoD §6 requires both)

**Severity**: P0 DoD coverage gap. **WI**: WI-S06-007 §1.5, §6.1.6, §10.s06.007.5, §15 chaos #6.

Sprint contract §6 DoD specifies TWO separable tests:
1. "Chaos test: rodar GC durante write load 1k QPS por 4h → zero falso positivo" — pre-merge gate, repeatable acceptance criterion.
2. §10.s06.2 "Zero blobs reachable deletados em 30d staging sustained" — post-sprint observation period concurrent with S-07/S-08 (per sprint contract §13).

WI-007 §1.5 conflates: "GC + write load 1k QPS sustained 4h; INV-GC-001/004 violations = 0. Extended 30d staging com synthetic workload (Bazel + multipart) production-like."

The 4h-1kQPS test is missing as an explicit DoD checkbox (only §10.s06.007.5 "30d sustained chaos zero violations" is listed); the Gherkin scenario "30d sustained chaos zero violations" only describes the 30d test.

**Fix** (Lote 10.6bis):

(a) Split DoD into:
- `10.s06.007.5a` 4h-under-1kQPS chaos zero violations (pre-merge gate; CI-runnable; repeatable; ≥3 runs).
- `10.s06.007.5b` 30d sustained staging chaos zero violations (post-sprint observation; pause-clock-on-P0/P1).

(b) Add Gherkin scenario "4h-1kQPS chaos zero violations" with explicit setup (1k QPS write load via REAPI ByteStream Write fixture; 4h duration; INV-GC-001 + INV-GC-004 violation counter == 0; ≥3 independent runs with seed variance documented).

(c) §15 chaos #6 split: "30d chaos violation injected → SEV-0; ship blocked" + new "4h-1kQPS chaos violation injected → SEV-0; merge blocked".

(d) §17 ST-008 split into ST-008a (4h-1kQPS suite, 4h budget) + ST-008b (30d sustained workload, 6h budget).

Estimated effort: 2h spec rewrite + 2h Gherkin authoring = ~4h.

### P0-W7-4 — RB-FM-300/404/305 dry-runs not specified as ≥3-run sustainability validation

**Severity**: P0 customer-trust evidence rigor. **WI**: WI-S06-007 §1.3, §6.1.4, §10.s06.007.3, §14.s06.007.3.

Each dry-run is specified as a single run with target latencies (≤5/30/60 min for RB-FM-300; ≤1/30 min for RB-FM-404; manual trigger for RB-FM-305). A single sample cannot establish p95 / sustainability — could be 4.9 min on this run, 5.5 on the next. The customer-trust claim "we caught and remediated within 30 min" needs ≥3 runs to be defensible to Compliance / Privacy / AppSec at PRR.

Additional gaps:
- Chaos PR injection magnitudes not specified (which exact refcount-drift % triggers the SEV-2 alert? sprint contract is 0.1% global / 1% per-tenant — at what magnitude does dry-run inject?).
- RB-FM-305 detection "via storage growth metric + sweeper tick rate alert" — but the sweeper-tick-stale alert fires after `> 1h` (per §6.1.5 / §15 chaos #5), which is **far above** the implicit "≤ 5 min" detection target for RB-FM-300.

**Fix** (Lote 10.6bis):

(a) Each RB dry-run = ≥ 3 independent runs with documented seed variance + p95 latency reported. Update §6.1.4 to "≥ 3 runs per RB; report p50, p95, max; sustainability claim requires p95 ≤ target".

(b) Pin chaos PR injection magnitudes:
- RB-FM-300: inject refcount drift = 0.5% per-tenant (above SEV-2 threshold 0.1%, below SEV-1 threshold 1%).
- RB-FM-404: inject UpdateActionResult fired at exactly `mark_started_at_ms + 1ms` (boundary case; protected_re_ref expected).
- RB-FM-305: inject GC paused 7d (cron disabled) + 100 GiB orphan accumulation.

(c) Cross-check RB-FM-305 detection latency vs sweeper-tick-stale alert (`> 1h`); if "≤ 5 min" target conflicts, document which alert provides the 5-min detection (not the sweeper-tick alert; possibly the SLO-FRESH-GC sustained metric).

(d) Add Sub-tasks ST-004b/005b/006b "RB dry-run runs 2 + 3 + variance analysis" 3h each = 9h additional.

Estimated effort: 2h spec rewrite + 9h additional dry-run runs (already in sprint timeline; just rebudgeted) = ~11h spec + run.

### P0-W6-W7-1 — Cross-WI: WI-006 30d sustained verde gate workflow `tla-30d-sustained.yml` design unspecified beyond filename

**Severity**: P0 governance gate design. **WIs**: WI-006 §1 invariant 4, §6.1.8 (30d sustained verification); WI-007 §1.4 (30d gate consumption).

WI-006 §6.1.8: "30d sustained verification: GitHub Actions history check; CI workflow `tla-30d-sustained.yml` daily runs aggregate verde 30d." — only the filename is given. Critical questions unanswered:
- How does the workflow query the past 30 days of CI runs? `gh api` for `tla-ci-gate.yml` workflow run history?
- What constitutes "verde"? All `tla-ci-gate` runs `success`? What about cancelled / skipped runs? Forced re-runs?
- What is the cadence? Daily at 00:00 UTC? Cron + manual trigger?
- How is the 30-day window computed — last 30 calendar days, last 30 successful runs, last 30 unique PR merges?
- What happens when the gate transitions from RED to GREEN — does the 30d clock reset (Lote 10.4bis pause-clock-on-P0/P1 lesson)?
- What is the output? A single boolean metric `corelink.ci.tla.30d_sustained_verde`? What if it's currently FALSE — does this block S-20 GA promotion?

WI-007 §1.4 consumes the gate: "CI history checked daily via `tla-30d-sustained.yml` (WI-S06-006). All 30 days verde required pre-S-20 GA promotion." This is the consumer side; the producer (WI-006) under-specifies.

**Fix** (Lote 10.6bis):

(a) WI-006 §6.1.8 expand to:
- Workflow runs daily at 06:00 UTC (post-overnight property test) + manual trigger.
- Query: `gh api repos/$REPO/actions/workflows/tla-ci-gate.yml/runs?per_page=100&created=>=$THIRTY_DAYS_AGO` — paginate; aggregate.
- Verde definition: ALL non-cancelled / non-skipped runs in 30d window have `conclusion=='success'`. Cancelled runs ignored. Skipped runs (PR didn't touch GC code) ignored.
- 30d window computed as last 30 calendar days from the workflow run's `now()`.
- P0/P1 incident: if any TLC red sustained > 4h or property-test-violation incident in the window → 30d clock RESET (clock starts from incident resolution timestamp). 4-tier classification per Lote 10.4bis.
- Output: emit `corelink.ci.tla.30d_sustained_verde` (gauge boolean) + `corelink.ci.tla.30d_window_days` (gauge int = days-since-clock-reset).
- S-20 GA promotion gate: requires gauge == TRUE for ≥ 30d.

(b) Publish workflow yaml inline in WI-006 §1 (parallel to `tla-ci-gate.yml` already published).

(c) Add Gherkin scenario "30d clock reset on P0/P1" to WI-006 §8 (only "30d sustained verde gate" is currently specified; the reset semantics are missing).

(d) Cross-link WI-007 §1.4 to specific WI-006 sub-section.

Estimated effort: 4h spec rewrite + 2h workflow yaml authoring = ~6h.

---

## P1 Findings

### P1-W6-1 — WI-S06-006 TLA+ ↔ Rust action mapping table not added (missed opportunity)

**Severity**: P1 documentation completeness. Part 1 P0-3 fix proposed adding the mapping table to WI-S06-002 §1; WI-006 (the formal-verification CI-gate WI) is the natural canonical home for the table (it documents the cross-validation contract between TLA+ obligations and Rust impl). Currently WI-006 §1 cites obligations by name (`InvGCReRefProtected`, `InvGCNeverDeleteReachable`) but does not enumerate the action ↔ impl mapping.

**Fix**: add the 4-row action mapping table from part 1 P0-3 fix (d) to WI-006 §1 OR §9 design decisions. Cross-link from WI-002 §1.

Estimated effort: 1h.

### P1-W6-2 — WI-S06-006 chaos suite (10 scenarios) hits HIGH_RISK floor with no margin

§6.1.12 lists exactly 10 chaos scenarios. WI-006 is governance + CI-gate; HIGH_RISK floor is ≥10. Compare: WI-007 has 12 (margin = 2). Adding 2 more chaos scenarios for margin (e.g., "TLC binary tamper detection" from P0-W6-2 + "30d clock reset on P0/P1" from P0-W6-W7-1) lifts margin and exercises the new fixes.

Estimated effort: 2h.

### P1-W7-1 — WI-S06-007 DSR erasure interaction tested 3h budget understaffs Privacy + Crypto SME independent review

§17 ST-021 budgets 3h for "DSR erasure interaction test (S-11 forward stub)". DSR erasure interaction with grace bypass is the cripto+privacy boundary that needs Privacy + Crypto SME independent review on:
- Bypass authorization (DSR signal authentication; INV-GC-DSR-BYPASS-AUTHORIZED).
- Cross-tenant impact (bypass affects only single tenant; INV-TENANT-ISOLATION preserved).
- Audit emission (immediate physical-delete still audit-emits; INV-OBS-AUDIT-CHAIN-INTEGRITY).
- LGPD Art. 16 retention conflict resolution (sprint contract R-S06-12).

3h is implausible for these four checks + integration test stub; minimum 8h Privacy review + 8h Crypto SME review (parallelizable).

**Fix**: rebudget ST-021 to 16h (Privacy 8h + Crypto SME 8h pair-review). Or split into ST-021a (engineer integration stub 3h) + ST-021b (Privacy + Crypto SME independent review 16h).

Estimated effort: spec rewrite 30min (rebudget); actual review hours flow into total PERT.

### P1-W7-2 — WI-S06-007 row-12 AppSec "MANDATORY EMPHATIC for TLA+ obligation alignment" vs row-13 Crypto SME "MANDATORY non-waivable" — assignment inversion

§30 row 12 AppSec: "_TBD; **mandatory emphatic** — TLA+ obligation alignment_". Row 13 Crypto SME: "_**MANDATORY** (non-waivable; TLA+ formal verification + property test 100k race + INV-GC-004 strict `<` semantics)_".

TLA+ obligation alignment is the **Crypto SME** lane (cryptographic / formal-verification expertise) — not AppSec (application security; threat modeling; STRIDE). AppSec's "emphatic" review surface should be the audit fail-closed boundary, the multi-tenant SQL injection prevention (clippy + sqlx prepared), and the override-discipline supply-chain (TLC SHA pinning P0-W6-2). The current assignment partially overlaps but the wording inverts the canonical lane split.

**Fix**: row 12 AppSec emphatic = "audit fail-closed + multi-tenant strict + supply-chain TLC pinning"; row 13 Crypto SME mandatory non-waivable = "TLA+ formal verification + property test 100k cross-validation + INV-GC-004 strict `<` semantics + json_each EXISTS check semantic equivalence".

Estimated effort: 30min spec edit.

### P1-W7-3 — WI-S06-007 PRR "rubber-stamp prevention CI gate validates non-empty entries" mechanism unspecified

§1.6 + §6.1.7: "rubber-stamp prohibido (Lote 10.4bis lesson)" + "CI gate validates entries". §10.s06.007.6: "PRR 13 sign-offs collected; rubber-stamp prevented". But: the CI gate name, the validation rule, and the failure mode are not specified.

Compare Lote 10.4bis lesson absorbed in WI-007 §9.7 = `validate_inv_promotion.py` has a clear name + behavior. The rubber-stamp gate needs equivalent rigor: e.g., `validate_prr_signoff.py` reads `specs/_audits/2026-XX-XX-s06-prr-meeting.md`, asserts each of 13 rows has non-empty "Evidence:" + "Verified:" + "Concerns:" fields, fails CI on any blank row.

**Fix**: §6.1.7 expand to specify gate name + validation rule + failure mode. Add Sub-task ST-022 "validate_prr_signoff.py CI gate" 3h.

Estimated effort: 1h spec + 3h CI gate implementation.

### P1-W6-3 — WI-S06-006 §10.s06.006.7 "TLA+ ↔ Rust alignment cross-validation" DoD criterion unmeasurable

§10.s06.006.7: "TLA+ ↔ Rust alignment cross-validation". No measurable criterion (test count? coverage %? equivalence assertion?).

**Fix**: rewrite to "Property test `prop_tla_rust_alignment` runs ≥ 1000 TLA+ scenarios → executes against Rust impl → asserts identical final state; CI nightly green".

Estimated effort: 30min.

---

## P2 Findings

### P2-W6-1 — WI-006 §1 yaml uses `wget` (no retry on transient network failure)
Replace with `curl --retry 3 --fail -L` or `actions/cache@v4` keyed by SHA. Estimated effort: 15min.

### P2-W7-1 — WI-007 §13 artifacts uses `2026-XX-XX` placeholders for RB dry-run reports / PRR meeting notes / SLO compliance reports
These will be created during the sprint; placeholder OK at spec time but flag for sprint execution: replace with actual ISO date. Estimated effort: trivial during execution.

### P2-W7-2 — WI-007 §22 cost analysis uses linear PERT × $100/hr → $8.5k one-time + $1k/yr maintenance
Reasonable rough-order-of-magnitude. Note: missing fixture-prep cost (WI-006 nightly property test ~$1/dia = $365/yr accounted; 30d staging "~$100 cost" understated — 30d × 5 regions × D1 ops + R2 ops at 1k QPS sustained will exceed $100; flag for review).

### P2-W7-3 — WI-007 §27 Knowledge Transfer (3h tech talk + workshop + 10-question onboarding test)
Adequate content; note that "How CoreLink reclaims storage safely" customer doc is also a competitive-advantage reveal — WI-006 + WI-007 together publicly disclose the TLA+ + property-test 100k + audit-fail-closed defense-in-depth strategy. Same observation as part 1 P2-4 — consider holding the customer-facing safety doc until S-20 GA or BYOK (S-14).

### P2-W7-4 — WI-007 §6.1.13 "Property test 100k tenant isolation cumulative (sprint contract DoD §6)"
Sprint contract §6 DoD specifies "property test em Rust cobrindo race Mark+UpdateActionResult 100k iterations" — cross-tenant isolation is implicit (tenant-scoped strict per Lote 10.4bis). WI-006 §6.1.11 lists `prop_gc_004_race_mark_update_ar_100k` + `prop_gc_001_reachable_never_deleted_100k` + `prop_tla_rust_alignment` (3 property tests). WI-007 §6.1.13 adds a fourth "tenant isolation cumulative" — verify this is incremental over WI-006's three OR clarify it's a wrapper that runs all four. Estimated effort: 30min clarification.

### P2-W7-5 — WI-007 §15 chaos #11 "CI ship-gate workflow regression → CI red"
The "ship-gate workflow" `gc-ship-gate.yml` is listed in §13 artifacts but its design is not published. Compare to WI-006 publishing `tla-ci-gate.yml` inline. Estimated effort: 2h yaml authoring.

### P2-W6-2 — WI-006 §22 cost analysis "TLC ~$0.000001 per PR × 50 PRs/dia = ~$0.05/dia = $18/yr"
Cost computation assumes GitHub Actions free-tier minutes. For private repo (which `corelink-server` likely is), GitHub Actions minutes are billed at $0.008/min for ubuntu-latest; TLC ≤ 5 min → $0.04/PR; × 50 PRs/day = $2/day = $730/yr (40× the claimed figure). Estimated effort: 30min recompute.

---

## Cross-WI Consistency Check (WI-006 ↔ WI-007 + Lote 10.5bis lessons preservation)

| Check | Status | Notes |
|---|---|---|
| **WI-006 `tla-ci-gate.yml` produces** → **WI-007 §1.4 consumes 30d verde gate** | OK BUT under-specified | Producer/consumer wired; gate workflow design under-specified per P0-W6-W7-1. |
| **WI-006 property test 100k race** → **WI-007 §6.1.13 cumulative tenant isolation** | PARTIAL | Cross-tenant property test added in WI-007 but relationship to WI-006's three property tests not clarified. |
| **WI-006 override discipline ADR + Architect + Crypto SME** → **WI-007 §30 Crypto SME MANDATORY non-waivable** | OK | Consistent on Crypto SME role; WI-007 explicit non-waivable; WI-006 specifies override path. P0-W6-3 holds: GitHub mechanism not pinned. |
| **WI-006 30d sustained verde gate** → **WI-007 §1.4 + §10.s06.007.4** | OK semantically; under-specified | Sprint contract §10.s06.4 alignment confirmed; gate design needs P0-W6-W7-1 fix. |
| **Sprint contract §6 DoD chaos 4h-1kQPS + §10.s06.2 30d sustained** | INCONSISTENT (P0-W7-3) | WI-007 §1.5 conflates; needs split. |
| **Sprint contract §10.s06.4 30d TLA+ verde** → **WI-007 §1.4 + WI-006 §6.1.8** | OK | Both WIs reference; gate design under-specified. |
| **Sprint contract §19 waiver — 95% TLA+ verde REMOVED** | OK | Verified §19 (lines 300-314) — TLA+ green sustained, property test 100k, grace 72h, chaos 30d, RB-FM-300/404 dry-runs ALL non-waivable. Lote 10.4bis sprint contract drift defense ABSORBED. |
| **Lote 10.4bis 4-tier P0/P1/P2/P3 incident classification** | PARTIAL | WI-007 §2.7 enumerates 4 tiers correctly; WI-006 doesn't propagate the 4-tier to its CI gate (e.g., TLC red sustained > 4h is P0 reset clock per WI-007 R-007 vs WI-006 §1 invariant 5 implies single-event reset). Cross-link needed. |
| **Lote 10.4bis validate_inv_promotion.py CI gate** | OK | WI-007 §6.1.8 + §10.s06.007.7 + §15 chaos #12 + §28 R-010 reference; INV count discrepancy (P0-W7-2) breaks the gate. |
| **Lote 10.4bis Crypto SME mandatory non-waivable for cripto WIs** | OK | WI-006 §16 + §30 + §28 + WI-007 §30 row 13 emphasize. Row-12-vs-row-13 minor inversion (P1-W7-2). |
| **Lote 10.4bis sign-off rubber-stamp prevention** | UNDER-SPECIFIED (P1-W7-3) | Mechanism not pinned; CI gate name not given. |
| **Lote 10.4bis production rollout gradual** | OK | WI-007 §1.11 + §15.x + §29 D+9/+11/+13 timeline (10/50/100). |
| **Lote 10.4bis ADR design content not rubber-stamp** | OK | ADR-0042 verified at canonical path; FROZEN; 91 lines; substantive Decision matrix + Rejected alternatives + Consequences. NOT procedural. |
| **Lote 10.5bis canonical_bytes layouts / partial UNIQUE / clippy lint TenantCtx-only / audit_outbox = WI-S01-004** | OK | Inherited from part 1; ship gate WI does not regress. |
| **Lote 10.5bis preemptive INV §3.17 promotion** | OK BUT count off | 23 rows promovidas preemptivamente; WI-007 says 22 (P0-W7-2 discrepancy). |
| **Lote 10.5bis sprint contract drift defense** | OK | §19 waivers reduced; non-waivable items explicit. |

---

## Verdict per WI

### WI-S06-006 — `8.0 / 10` — **GO-WITH-FIXES (P0)**

Strong CI-gate design with TLC v1.8.0 pinned + invariant-name asserts + path-based PR scope + override discipline ADR + Architect + Crypto SME signoff. Property test 100k race correctly cites `InvGCReRefProtected`. Held back by: property test inheriting WI-S06-003 LIKE defect silently (P0-W6-1, the highest-leverage technical fix in the pair); TLC SHA-256 not pinned (P0-W6-2 supply-chain hole); override mechanism prose-only not pinned (P0-W6-3); 30d sustained verde gate workflow design under-specified (P0-W6-W7-1); TLA+ ↔ Rust action mapping not added (P1-W6-1, missed opportunity). With P0 fixes, projected score → 9.0.

### WI-S06-007 — `8.0 / 10` — **GO-WITH-FIXES (P0)**

Most thorough ship-gate WI in the program (12 Gherkin + 14 DoD + 21 sub-tasks + 12 risk + 12 chaos). ADR-0042 verified substantive; sprint contract §19 waiver discipline held; Crypto SME MANDATORY non-waivable correctly graded. Held back by: 10-of-13 sign-off staffing TBD (P0-W7-1, persistent program defect carry-forward from S-04/S-05); INV count 22 vs 23 (P0-W7-2 CI-gate-blocking); 4h-vs-30d chaos test conflation (P0-W7-3 DoD coverage gap); RB dry-runs single-sample (P0-W7-4 sustainability evidence rigor); DSR erasure 3h understaff (P1-W7-1); rubber-stamp gate mechanism unspecified (P1-W7-3); row-12-vs-row-13 emphatic-vs-mandatory inversion (P1-W7-2). With P0 fixes, projected score → 8.8.

---

## Aggregate Part 2b Score

**8.0 / 10.**

Below S-06 part 1 (8.13), comparable to S-05 part 2 (8.05), well below WI-S04-003 ceiling (8.6). The pair is **GO-WITH-FIXES for Lote 10.6bis**; no WI is REJECT.

---

## Lote 10.6bis P0 Fix Plan — Part 2b additions

Augments the existing part 1 fix plan (P0-1..P0-6, P1-1..P1-5, ~68h budget). Part 2b adds:

| ID | Title | WI | Severity | Effort | Owner |
|---|---|---|---|---|---|
| **P0-W6-1** | Property test 100k race must NOT silently inherit WI-S06-003 LIKE defect; add fixture adversarial inputs + cross-validation against deliberately-broken impl | WI-S06-006 | P0 cripto-load-bearing | ~12h | Engineer + Crypto SME |
| **P0-W6-2** | TLC v1.8.0 SHA-256 integrity-verify in CI workflow; add ADR or amend ADR-0042 | WI-S06-006 + ADR | P0 supply-chain | ~5h | Engineer + Architect |
| **P0-W6-3** | Pin GitHub branch-protection / CODEOWNERS / override-workflow mechanism for ADR + Architect + Crypto SME override path; disable admin-bypass on `main` | WI-S06-006 + repo settings | P0 governance | ~8h | Architect + Engineer |
| **P0-W7-1** | Sign-off booking calendar §29.1 + CI gate `validate_signoff_calendar.py`; commit 13 calendar slots; close 10-of-13-TBD persistent program defect | WI-S06-007 + ADR | P0 process | ~7h | Architect + SRE Lead |
| **P0-W7-2** | INV §3.17 count alignment 22 vs 23 — apply part 1 P1-1 merge OR update WI-007 to 23 | WI-S06-007 + WI-S06-001 + registry | P0 CI-gate-blocking | ~1h | Architect |
| **P0-W7-3** | Split DoD into 4h-1kQPS chaos (pre-merge gate) + 30d sustained chaos (post-sprint observation); add Gherkin + sub-tasks | WI-S06-007 + sprint contract | P0 DoD coverage | ~4h | Engineer + QA |
| **P0-W7-4** | RB-FM-300/404/305 dry-runs: ≥3 runs per RB + p95 latency + chaos PR injection magnitudes pinned + RB-FM-305 detection-latency cross-check | WI-S06-007 | P0 evidence rigor | ~11h spec + execute | Engineer + SRE |
| **P0-W6-W7-1** | 30d sustained verde gate workflow `tla-30d-sustained.yml` design (cadence, query mechanism, P0/P1 clock reset, gauge emission); publish yaml inline | WI-S06-006 + WI-S06-007 | P0 governance gate | ~6h | Engineer + Architect |
| **P1-W6-1** | TLA+ ↔ Rust action mapping table (lift from part 1 P0-3 fix; canonical home WI-S06-006 §1) | WI-S06-006 | P1 docs | ~1h | Architect |
| **P1-W6-2** | Add 2 chaos scenarios for margin (TLC tamper + 30d clock reset) | WI-S06-006 | P1 chaos margin | ~2h | Engineer |
| **P1-W7-1** | Rebudget ST-021 DSR erasure interaction 3h → 16h (Privacy 8h + Crypto SME 8h independent) | WI-S06-007 | P1 staffing realism | ~30min spec | Architect |
| **P1-W7-2** | Row-12 AppSec emphatic vs row-13 Crypto SME mandatory non-waivable lane assignment fix | WI-S06-007 §30 | P1 sign-off taxonomy | ~30min | Architect |
| **P1-W7-3** | `validate_prr_signoff.py` CI gate (rubber-stamp prevention; equivalent rigor to validate_inv_promotion.py) | WI-S06-007 + tooling | P1 governance gate | ~4h | Architect + Engineer |
| **P1-W6-3** | §10.s06.006.7 measurable criterion (≥1000 scenarios + identical final state assertion) | WI-S06-006 | P1 DoD measurability | ~30min | Architect |

**Total Lote 10.6bis Part 2b P0 fix budget**: ~54h spec + execute time.
**Combined Lote 10.6bis P0 fix budget (Part 1 + Part 2b)**: ~122h ≈ 2 weeks one engineer + Architect + Crypto SME pair-review on P0-1 + P0-W6-1 + P0-W6-3.

**Highest-leverage single fixes**:
1. **P0-1** (Part 1; SQL EXISTS LIKE→json_each) — closes the cripto-load-bearing defect class. Without P0-1, P0-W6-1 (property test inheritance) also fails silently.
2. **P0-W7-1** (sign-off booking calendar) — lowest-cost (~7h) fix that closes a persistent S-04/S-05 program defect.
3. **P0-W6-2** (TLC SHA pinning) — lowest-effort (~5h) fix that closes a supply-chain hole in the formal verification baseline.

**Crypto SME mandatory non-waivable** for: P0-1 (json_each semantic equivalence), P0-W6-1 (property test cross-validation against broken-impl negative control), P0-W6-2 (TLC supply-chain ADR + bump policy), P0-W6-3 (override mechanism Crypto SME co-sign workflow), P0-W7-1 (D-14 advance booking calendar).

---

## Reviewer Notes (process)

- WI-006 read in full (429 lines verified).
- WI-007 read in full (585 lines verified).
- ADR-0042 read in full (91 lines; FROZEN; substantive design content verified — NOT rubber-stamp procedural).
- Sprint contract §19 verified — 95% TLA+ verde waiver REMOVED; non-waivable items: TLA+ verde sustained, property test 100k, grace 72h, chaos 30d, RB-FM-300/404.
- Invariant registry §3.17 enumerated — 23 INV-GC-* rows promovidas preemptivamente (lines 331-353).
- WI-S05-006 (S-05 ship gate) sign-off table cross-checked — same 10-of-13-TBD pattern in §30; persistent program defect.
- Part 1 audit P0/P1 list cross-referenced to ensure non-overlap with Part 2b findings.
- **Single most dangerous defect in this pair**: P0-W6-1 (property test silently inheriting WI-S06-003 LIKE defect) — compounds part 1 P0-1 — voids the defense-in-depth claim.
- **Single highest-leverage fix in this pair**: P0-W7-1 (sign-off booking calendar) — ~7h, closes S-04/S-05 carry-forward defect.
- **Aggregate part 2b score 8.0/10** — below part 1's 8.13; pair is GO-WITH-FIXES; not REJECT.

**Fim audit Agent R4 Lote 10.6 S-06 Part 2b.**
