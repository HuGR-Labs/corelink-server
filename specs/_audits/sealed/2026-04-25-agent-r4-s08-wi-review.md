---
type: audit
title: Agent R4 (Opus 4.7) review of S-08 WIs
date: 2026-04-25
reviewer: Agent R4 (Claude Opus 4.7)
sprint: S-08
target: 6 WIs
---

# Agent R4 review of Sprint S-08 (Lote 10.8) WIs

## Aggregate score: 8.0 / 10

S-08 sits at the threshold of the rigorous bar but does not clear it. The lessons from Lote 10.7bis are mostly absorbed (P0-2 phantom column rejection, P0-6 strict-< predicate, P0-7 5-tier canonical, P0-9 primary_region routing, R5 P0-3 `worker::send_future`, alarm re-arm AT START, audit fail-closed, D1 batch ≤250, CHECK inline) and the LGPD humane response framing is consistently applied. However: (a) WI-S08-003 introduces a 6th `X-Rate-Limit-Type` discriminator (`bandwidth_quota`) that contradicts the canonical 5-discriminator enumeration in WI-S08-005 and sprint contract §5 R-S08-8; (b) WI-S08-005 systematically misattributes its multi-signal trigger pattern to "Lote 10.7bis P0-9 race-aware" — that lesson was about DO routing via primary_region, not signal aggregation; (c) the per-PAT cap = 10× tenant refill_rate formula is logically incompatible with the per-tenant bucket binding at 1× (WI-S08-001) — the design works only if "10×" is reinterpreted as "10× per-PAT-typical-share", but the spec doesn't acknowledge this; (d) WI-S08-006 advertises "8 alert rules" but the YAML enumerates 14; (e) WI-S08-001 and WI-S08-006 both claim 13 sign-offs while framework §33.5.4.3 caps HIGH_RISK at 10–12. Pattern signal: spec absorbed the *names* of Lote 10.7bis lessons but not always their *semantics*; first-principles cross-WI integration was not run as a final sanity pass before sealing.

## Per-WI scores

- WI-S08-001: 8.4 / 10 (cleanest application of P0-7 + P0-9 + R5 P0-3; small INV §3.X placeholder; 13 sign-offs over upper bound)
- WI-S08-002: 8.2 / 10 (NAT-aware threshold well-justified; humane appeal flow rigorous; LGPD trail solid; ipnet crate dependency unverified vs CF Workers WASM target)
- WI-S08-003: 7.4 / 10 (correctly REJECTS phantom column + race-aware strict-<, but introduces 6th `bandwidth_quota` discriminator + per-PAT 10× tenant rate logical contradiction left unresolved + sprint contract correction "queued for tris" instead of fixing now)
- WI-S08-004: 8.1 / 10 (LGPD framing strongest of the 6; calibration target sound; but score arithmetic in narrative §3 (lines 313–315) leaks model reasoning trace; metric dependencies on `corelink_exec_*` undefined upstream)
- WI-S08-005: 7.6 / 10 (multi-signal trigger correct pattern but misattributed to P0-9 ten times; FM-251 mis-cited as "rate FP" — actual canonical is "credential stuffing"; PAT-CIRCUIT-BREAKER-001 doesn't exist — canonical is PAT-CIRCUIT-001; scope-creep absorbed camada 4 GlobalCircuit beyond sprint contract WI table mapping)
- WI-S08-006: 7.7 / 10 (8-vs-14 alert count discrepancy; 13-sign-off claim exceeds framework §33.5.4.3 upper bound; otherwise solid PRR consolidation, RB-FM-250 dry-run pass criteria realistic, RBAC + cost regression gate well-specified)

## P0 findings (catastrophic; block ship)

1. **WI-S08-003 introduces phantom 6th discriminator `bandwidth_quota`** breaking cross-WI canonical enumeration. WI-S08-003 line 41 (header), line 246 (Persona 3), line 396 (middleware comment), line 564 (Gherkin), line 730 (API contract) all use `X-Rate-Limit-Type: bandwidth_quota`. WI-S08-005 lines 41/65/171/173/291/582 enumerate exactly 5 discriminators (`tenant_quota|per_ip|per_pat|over_quota|global_circuit_open`) per sprint contract §5 R-S08-8. WI-S08-006 dashboard panel `sli-distinction` line 67–68 maps over-quota = `per_ip + per_pat + over_quota` — `bandwidth_quota` would not flow into either counter and is silently dropped from SLI accounting. **Fix**: either subsume bandwidth under `over_quota` (preferred — bandwidth excedence is also "over-plan" semantically), update WI-S08-003 + 4 Gherkin scenarios + middleware comment to use `over_quota`, OR formally add `bandwidth_quota` as 6th discriminator updating sprint contract §5 R-S08-8 + WI-S08-005 enum + WI-S08-006 panel + within-quota/over-quota mapping. Pick one before sealing.

2. **WI-S08-005 misattributes "multi-signal trigger" to Lote 10.7bis P0-9 race-aware lesson** (10+ occurrences: line 29, 41, 136, 194, 230, 309, 336, 468, 584, 649, 741, 783, 787). P0-9 lesson per `specs/_audits/sealed/2026-04-25-agent-r4-s07-wi-review.md` line 316 is specifically "DO singleton `quota-<tenant_id>` cross-region inconsistency" — i.e., DO routing via `tenant.primary_region` to avoid 50–100ms RTT. The race-aware strict-< predicate is P0-6 (Eviction reachable check), correctly applied in WI-S08-003. The multi-signal trigger pattern is sprint-contract-§15-R-S08-004 driven; the framework lesson lineage is being fabricated to lend authority. **Fix**: replace all "Lote 10.7bis P0-9 race-aware lesson absorbed" with "sprint contract §15 R-S08-004 + standard circuit-breaker hysteresis pattern" or cite an appropriate lineage. This matters because future review cycles will trust the citation chain; broken citations corrupt institutional memory.

3. **Per-PAT cap = 10× tenant refill_rate is logically incompatible with camada 1 binding at 1× tenant** (WI-S08-003 §3.1 lines 91, 120, 194–202, 224; sprint contract §5 R-S08-3). All PAT requests pass through the per-tenant DO RateLimiter (WI-S08-001 camada 1) which caps at 1× tenant refill. A single PAT cannot consume 10× tenant rate because it would already be 429'd by camada 1 long before camada 3 kicks in. The narrative line 224 hand-waves "5–20 PATs sharing tenant rate; one PAT typically 5–10%" — implying the 10× cap is meant as "10× per-PAT-typical-share" not "10× tenant total". **Fix**: clarify formula explicitly. Either (a) PAT cap = `tenant_refill_rate / N` where N is target PAT-count-per-tenant (e.g., cap = 0.1× tenant for "expected ≥10 PATs"), with detection threshold "any single PAT > X% of last-24h tenant traffic"; or (b) keep 10× as misuse alarm threshold but make clear it's never enforcing — only flagging — because camada 1 enforces. Current spec implies enforcement which is incoherent.

## P1 findings (high quality; address in bis)

1. **WI-S08-006 alert count: claims "8 alert rules canonical" but YAML enumerates 14** (lines 250–364). Title (line 28), §6.1.2 (line 488), §10.s08.006.15 (line 689), Gherkin scenario line 595–598 ("8 distinct rules with multiple conditions; 14 unique conditions") — the spec acknowledges the discrepancy but doesn't reconcile it: 14 distinct YAML alert rule IDs exist. Sprint contract §6 DoD lists 3 alert examples. **Fix**: either re-organize the 14 conditions into 8 grouped rules, OR update sprint contract + WI title to "14 alerts canonical" with clear SEV breakdown.

2. **PRR sign-off counts inconsistent + exceed framework §33.5.4.3 upper bound** (10–12 for HIGH_RISK). Per-WI: WI-S08-001 = 13 (line 520, 530), WI-S08-002 = 11, WI-S08-003 = 12, WI-S08-004 = 12, WI-S08-005 = 12, WI-S08-006 = 13 (line 425, 502, 612–615, 681). Framework `00_framework.md` line 2127 explicit: "Total mínimo: 10–12" for HIGH_RISK. WI-S08-001 includes Crypto SME advisory as a 13th sign-off and WI-S08-006 includes both Crypto SME advisory + AppSec separately. **Fix**: either reduce to ≤12 per WI (consolidate Crypto SME advisory into Architect or AppSec), OR explicitly call out advisory roles as non-counting in framework + amend §33.5.4.3 to allow advisory roles beyond the 12-cap.

3. **FM-251 misattribution in WI-S08-005 + WI-S08-006**. WI-S08-005 line 245, 273 cites `FM-251 (rate limit FP)`; failure_modes.md line 176 actually defines FM-251 as "Credential stuffing / brute force". The "rate-limit false-positive" semantic doesn't have a canonical FM-ID — closest match is FM-201 ("Config change causa rate-limit drop"). **Fix**: either correct citation to FM-201 / FM-255 / new FM proposal, OR add FM-NEW for "Rate limit false-positive (legitimate within-quota gets 429)".

4. **PAT-CIRCUIT-BREAKER-001 doesn't exist** (WI-S08-005 line 273 cites `resilience_patterns.md PAT-CIRCUIT-BREAKER-001`). Canonical name per resilience_patterns.md line 126 is `PAT-CIRCUIT-001`. WI-S08-005 also cites PAT-RATE-LIMIT-001 correctly elsewhere — this is a copy-paste error. **Fix**: replace `PAT-CIRCUIT-BREAKER-001` → `PAT-CIRCUIT-001`.

5. **WI-S08-004 metric dependencies undefined upstream**. Lines 358–361 reference `corelink_exec_cpu_seconds_total`, `corelink_exec_wallclock_seconds_total`, `corelink_egress_bytes_total`, `corelink_concurrent_exec_count` — none of these metrics are defined in any spec under `/specs/`. RBE/exec sprint (S-17 per file listing) presumably emits them but no cross-reference. **Fix**: add explicit upstream dependency to WI-S08-004 §18 ("S-17 RBE exec sprint provides corelink_exec_* metrics"); add a reasoned fallback if upstream lags (e.g., score with whatever metrics S-09 provides; defer cpu/exec features until S-17 ships).

6. **Sprint contract correction queued instead of executed in `bis`** (WI-S08-003 §1.1 line 170, §9.13 line 624). The phantom column `tenant_quota.bytes_used` (sprint contract §5 R-S08-5) is correctly REJECTED in the WI but the WI defers fixing the sprint contract itself to "tris cycle". This is a hand-wave: the sprint contract is canonical input; if it's wrong, the tris cycle hasn't fixed it; merge would seal a contradiction. **Fix**: amend sprint contract §5 R-S08-5 in this same Lote 10.8bis cycle; don't kick the can.

7. **Missing FM-401 cross-reference attribution**. WI-S08-005 line 273 cites FM-401 as "thundering herd" — failure_modes.md line 209 actually defines FM-401 as "Thundering herd em cache miss" (correct semantic) — but WI-S08-005's mitigation is multi-signal trigger for circuit breaker, which is a different mitigation than singleflight. The FM-401 citation is decorative not driven. **Fix**: drop the citation OR explicitly explain why circuit breaker mitigates thundering herd (it doesn't; PAT-SINGLEFLIGHT-001 does — different problem space).

8. **WI-S08-004 narrative §3 leaks model reasoning trace** (lines 313–315). Verbatim text: "Hmm wait — example needs higher.\n\nActually rebuilding: …". This is conversational scratchpad left in production spec — sloppy. Persona 3 also reaches score 0.40 = Noop tier in the original calculation, and the rebuilt Persona 3 reaches 0.596 = SilentDowngrade only because exec is 50× baseline. The DoD §6 calibration target ≥80% TP becomes harder to verify without precommitted thresholds. **Fix**: remove lines 313–315 reasoning leak; precommit calibrated thresholds; document expected score range per persona post-staging-calibration.

9. **`chrono::tomorrow_at_utc_midnight()` lesson lineage unverified**. Cited in WI-S08-003 line 188, 222, 635, 811; WI-S08-004 line 679; WI-S08-005 line 191, 593, 647, 783 as "Lote 10.5bis chrono crate `tomorrow_at_utc_midnight()` lesson". S-05 work items + S-05 sprint contract reference Lote 10.5bis P0 fixes (BLAKE3 recalibration, partial UNIQUE, canonical_bytes 102) — none mention `tomorrow_at_utc_midnight`. The function name itself is plausible (chrono crate has midnight helpers) but the *lesson attribution* is fabricated. **Fix**: cite chrono crate API directly (e.g., `chrono::Utc::now().date_naive().succ_opt().unwrap().and_hms_opt(0,0,0)` pattern) without spurious Lote 10.5bis attribution.

10. **WI-S08-002 ipnet crate dependency on CF Workers WASM target unverified**. Line 369 references `ipnet::IpNet::from_str` for CIDR parsing. CF Workers Rust runs in V8 isolate via wasm-bindgen; ipnet crate purely-Rust should compile fine, but no WASM-compatibility check is documented. **Fix**: add 30-min spike to verify ipnet compiles + works in CF Workers; OR use a documented-compatible alternative; OR limit ipnet usage to admin endpoints (Worker context is fine; not edge ruleset evaluation context).

11. **SLI distinction mapping incomplete in WI-S08-006 §6.1.1 panel**. Line 67–68: within-quota series counts `tenant_quota + global_circuit_open` (line 581 confirms). Over-quota series counts `per_ip + per_pat + over_quota`. If `bandwidth_quota` becomes a 6th discriminator (P0-1 fix), it must be added to over-quota series. If `bandwidth_quota` is subsumed under `over_quota`, no change needed. Decision blocks dashboard implementation.

12. **WI-S08-005 R-S08-4 scope expansion not documented in sprint contract §12 PERT**. Sprint contract line 151 has "WI-S08-005 | Response code types + RFC 9331 headers | 8h". WI-S08-005 absorbs camada 4 global circuit breaker + multi-signal + hysteresis + RFC 9331 fixture parser, revising estimate to ~19h (line 678). This is a 2.4× scope creep relative to sprint contract estimate; WI Change Log line 783 acknowledges it. **Fix**: amend sprint contract §12 to reflect actual 19h estimate; the buffer (3 days) absorbs it but reasoning should be transparent for future sprint planning calibration.

13. **Reference in WI-S08-001 §9.X to "INV §3.X registry placeholder"** instead of canonical §3.12 (where INV-RATE-LIMIT-PROPORTIONALITY actually lives in invariant_registry.md line 163). Lines 97, 401 (twice in WI-S08-001), line 700 in WI-S08-006. **Fix**: replace `registry §3.X` → `registry §3.12`.

## P2 findings (polish; address opportunistically)

1. **WI-S08-004 §10.s08.5 attribution stretch**. Line 180: "5min aggregation window canonical (sprint contract §10.s08.5 spirit absorbed; calendar-aligned reset)". Sprint contract §10.s08.5 actually says "Monthly reset determinístico: bandwidth quota reset em 1º dia UTC do mês seguinte; não em sliding window" — about monthly, not 5min. "Spirit absorbed" hand-wave is weak. **Fix**: cite a real source for 5min aggregation choice (PagerDuty industry standard) or move the attribution to the standard observability_model.md if it documents 5min canonical.

2. **WI-S08-001 §22 cost analysis numbers unrealistic**. Line 467: "TCO 12m: 5 regions × 1k tenants × 1k requests/sec × 86400 × 365 × $0.0000005 = ~$78840/yr". 1k tenants × 1k req/sec = 10^6 RPS aggregate per region × 5 regions = 5×10^6 RPS sustained — that's higher than most CDNs. Multiplying by 31.5M sec/yr = 1.58×10^14 ops × $5×10^-7 = $7.88×10^7 ≈ $79M/yr — not $79k. Off by 1000×. **Fix**: recompute; intent was probably "1k req/sec aggregate" or per-tenant much lower.

3. **WI-S08-004 sign-off line 800-810 has 12 entries but Final Approver counts as 1-2 = 2 entries; total = 13 not 12**. Same off-by-one in WI-S08-001 (line 522–534), WI-S08-006 (line 850–862). Cosmetic but compounds with P1 finding 2.

4. **Inconsistent header format across WIs**. WI-S08-001/002/003 use long single-paragraph titles in §0 row "Título"; WI-S08-006's title spans 3 paragraphs effectively. **Fix**: enforce title max 250 chars or move dense detail to §1 Intent.

5. **WI-S08-005 §13 metric naming**: `corelink.rate_limited_within_quota_total` (with dot) vs Prometheus convention `corelink_rate_limited_within_quota_total` (with underscore). All other metric names in §6.1.13 follow Prometheus convention. Same drift on `corelink.global_circuit.state` etc. **Fix**: choose one (Prometheus-canonical: underscores, since that's how OTel-to-Prom conversion exposes them).

6. **Migration file naming convention `00X_*.sql` placeholder** (WI-S08-001 line 412–413; WI-S08-002 line 572; WI-S08-003 line 661; WI-S08-004 line 662; WI-S08-005 line 631). Real migration numbers must be assigned + sequential per ADR-0036. **Fix**: assign actual numbers in coordination with S-07 migrations + S-08 cumulative.

7. **WI-S08-002 audit fail-closed Gherkin scenario** line 499–506: "transaction ABORTED (rollback): D1 row deleted; CF List entry removed". CF List push is API call, not transactional with D1 INSERT — rollback semantics need careful design (compensating delete on CF API). **Fix**: clarify saga vs 2PC pattern; CF List remove may itself fail leaving partial state; idempotent retry pattern documented.

## Cross-WI integration issues

1. **Discriminator enumeration drift** (P0 above): WI-S08-003 vs WI-S08-005 vs WI-S08-006 must agree on exactly 5 (or 6 if `bandwidth_quota`) discriminators. WI-S08-005's `RateLimitType::counts_against_sli()` enumeration is the authoritative API surface; all 429 emitters in camadas 1–4 must map cleanly into one of the enumerated variants; SLI dashboard must total-sum to within-quota + over-quota with no leakage.

2. **WI-S08-006 §6.1.4 RB-FM-250 + WI-S08-002 sustained-abuse detection both target DDoS**. WI-S08-002 §6.1.6 sustained-abuse triggers SEV-2 with humane SLA; WI-S08-006 RB-FM-250 dry-run pass criteria are SLO ≥ 99.0% non-attacker + recovery ≤ 15min. The dry-run must validate WI-S08-002's automated detection actually engages without auto-applying — the runbook + dry-run script need to explicitly assert "0 auto-applied blocks during dry-run; admin manual approval path exercised".

3. **ADR-0020 95–100% transition window**: WI-S08-003 §3.4 line 167 says S-08 owns "100% storage hard-block + 95-100% transition window monitored (alert SEV-3 customer notification SEV-2 internal)". WI-S08-006 dashboard panel `camada-3-storage-utilization` line 137–151 has alerts at storage-95pct (SEV-3) and storage-100pct (SEV-2). Consistent. But the SEV-3 customer notification path (during 95–100% window) requires S-13 (or staging stub); the dashboard alerts emit visibility but customer email notification requires S-13 sealing. Cross-cutting dependency well-flagged but worth highlighting in WI-S08-003 §18 and WI-S08-006 §18.

4. **PAT cap formula consistency** (WI-S08-001 §6.1.4 vs WI-S08-003 §6.1.6). WI-S08-001 defines `refill_rate_for_tier` returning (sustained, burst). WI-S08-003 defines `pat_rate_cap_for_tier` = `tenant_refill * 10.0`. Per WI-S08-001 §6.1.4: free 10 RPS / solo 50 / team 200 / business 1000 / enterprise 10000. So WI-S08-003 cap: free 100 RPS / solo 500 / team 2000 / business 10000 / enterprise 100000 (line 200 confirms). At enterprise tier, single PAT cap = 100000 RPS — but tenant aggregate at enterprise is also 10000 RPS. PAT cap > tenant cap by 10× at every tier. Cross-WI logic conflict (P0 #3 above).

5. **Multi-signal trigger threshold sources**. WI-S08-005 §6.1.6 thresholds: error_5xx > 0.5, p99 > 5×SLO, do_error > 0.3. SLO baseline lives in slo_catalog.md. WI-S08-005 should cite the exact SLO §x.y for "5×SLO p99 latency" so the threshold is auditable + adjustable when SLO baseline shifts. Missing.

6. **WI-S08-006 PRR ship gate enforce reference order**: §6.1.4 line 502 says "6 WIs SEALED individual PRRs (WI-S08-001..005 + this WI)". Correct enumeration. §10.s08.006.11 line 685 same. PRR evaluator script `scripts/prr_ship_gate_s08.py` (artifact line 716) must implement the gate; not visible whether the script logic was specified — but for spec-first stage, having the artifact path is sufficient.

## Strengths

- **LGPD humane response framing strongest of any sprint reviewed so far**. WI-S08-002 + 004 + 005 + 006 consistently enforce: NEVER auto-apply blocks (CIDR), NEVER auto-suspend (abuse), customer self-service inspection + appeal endpoints with 24h SLA, audit log every decision, AutoSuspendForbidden runtime canary alert at SEV-1. Compliance + Privacy emphatic sign-offs across the board. This is materially better than NativeLink/BuildBuddy where rate-limit responses are opaque.
- **Race-aware strict-< predicate correctly inherited from S-06 INV-GC-004 + S-07 WI-S07-002**. WI-S08-003 §6.1.3 implements it cleanly with `total_committed < self.max_storage_bytes` and overshoot computation. The lesson lineage P0-6 is correctly cited (vs the misattribution in WI-S08-005).
- **TenantCtx-only enforcement (Lote 10.4bis) consistently applied** across all 6 WIs; cross-tenant injection chaos test scenarios in WI-S08-001 §6.1.12.7 + WI-S08-003 §6.1.13.12 + WI-S08-004 §6.1.12.10.
- **Size-proportional reservation TTL (Lote 10.7bis R5 P0-2)** correctly implemented in WI-S08-003 §3.1.6 with 60s minimum + 7d cap + multipart 160 GiB scenario validated in chaos test §6.1.13.5.
- **Cost analysis included per-WI** with TCO 12m projections + cost-saved-by-defense framing. Best practice (modulo arithmetic error in P2 #2).
- **5-tier canonical taxonomy (P0-7)** consistently applied across WI-S08-001 refill_rate_for_tier + WI-S08-003 pat_rate_cap_for_tier + WI-S08-004 abuse baselines.
- **Audit fail-closed (Lote 10.6bis)** + fail-closed bias toward over-counting decision in WI-S08-001 §6.1.8 + WI-S08-003 §6.1.9 — correct trade-off (charge tenant for legitimate use; reconcile catches drift; never silently miss audit).
- **CF Workers Rust runtime (Lote 10.7bis R5 P0-3)** `worker::send_future()` correctly cited; no `tokio::spawn` slip in any of the 6 WIs.
- **WI-S08-002 NAT-aware threshold (anonymous 10 RPS / authenticated 1000 RPS)** is well-justified vs corporate proxy + mobile CGNAT scenarios; sprint contract §15 R-S08-006 risk explicitly mitigated.

## Verdict

**APPROVED CONDITIONALLY** for `Lote 10.8bis` cycle, contingent on:

1. **P0-1 (discriminator drift)** resolved: pick `over_quota` subsumes-bandwidth, OR add `bandwidth_quota` as 6th canonical discriminator with sprint contract + WI-S08-005 enum + WI-S08-006 panel updates. Single decision; ~2h spec rewrite.
2. **P0-2 (P0-9 misattribution)** resolved: replace 10+ "Lote 10.7bis P0-9 race-aware" citations in WI-S08-005 with sprint contract §15 R-S08-004 + standard hysteresis pattern citation. ~30min spec rewrite.
3. **P0-3 (PAT cap formula contradiction)** resolved: clarify whether 10× is enforcement (impossible per camada 1) or detection threshold (alarm-only, not 429). ~1h spec rewrite + Gherkin scenario clarification + sprint contract §5 R-S08-3 amendment.
4. **P1 sweep**: address findings 1–7 minimum (alert count consistency, sign-off cap compliance, FM-251 + PAT-CIRCUIT name corrections, S-17 metric dependency, sprint contract phantom column fix in same cycle, FM-401 attribution, narrative reasoning leak). ~6h aggregate.
5. **Cross-WI #4 PAT cap consistency** addressed jointly with P0-3.

Estimated `Lote 10.8bis` effort: **~12h** (within standard bis budget; comparable to S-07 Lote 10.7bis ~58h was elevated; this one is lighter because the canonical absorptions mostly worked). Post-bis projected aggregate: **~9.0/10**, achieving SOTA bar crossing analogous to S-06 (8.13 → 9.1 post-bis). Crypto SME EMPHATIC needed for P0-3 PAT cap clarification (race-correctness + multi-camada coherence). Architect mandatory for P0-1 discriminator decision. Compliance + Privacy sign-offs preserved (LGPD framing already strong).

**Reasoning for not "REJECTED"**: the P0 findings are bounded scope (discriminator decision is binary; P0-9 misattribution is mechanical search-replace; PAT cap is single-paragraph clarification). The structural quality is high — Lote 10.7bis lessons are mostly genuinely absorbed (not just name-dropped); LGPD framing is the strongest of any sprint to date; TenantCtx + audit fail-closed + race-aware predicate + 5-tier canonical + size-proportional TTL all solid. The defects are real but addressable in 12h. If they were architectural (e.g., D1 atomic CAS chosen over DO), this would be REJECTED.

**Reasoning for not "APPROVED" outright**: 8.0/10 is on the rigorous-bar threshold per the brief (≥8.0). At 8.0 with 3 P0s + 13 P1s, the work has not yet earned unconditional ship. Pattern from prior reviews shows S-06 needed bis to cross 9.0; S-07 needed bis to cross 8.5. S-08 has analogous potential but requires the bis cycle.
