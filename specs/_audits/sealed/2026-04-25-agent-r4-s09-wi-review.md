---
type: audit
title: Agent R4 (Opus 4.7) review of S-09 WIs
date: 2026-04-25
reviewer: Agent R4 (Claude Opus 4.7)
sprint: S-09
target: 7 WIs
---

# Agent R4 review of Sprint S-09 (Lote 10.9) WIs

## Aggregate score: 7.6/10

The 7 WIs are dense, well-structured, and absorb most Lote 10.4bis/10.6bis/10.7bis/10.8bis/tris lessons explicitly. However the cycle introduces several **first-principles errors** (P0 math, canonical drift, factual platform claims) that the absorbed-lesson discipline did not catch — the same pattern S-08 tris discovered in S-08 bis. Lowest scorer is WI-S09-002 (R2 platform feature error + IPv6 redaction code/narrative inconsistency); highest is WI-S09-004 (clean fail-CLOSED execution).

## Per-WI scores

| WI | Score | Rationale |
|---|---|---|
| WI-S09-001 (Analytics Engine + cardinality) | 7.5/10 | Strong structure, fail-OPEN distinction clear; metric naming convention violates Prom canonical (dots vs underscores); INV §3.13 placement wrong; histogram bucket cardinality math omitted. |
| WI-S09-002 (Logpush + R2 + Loki + PII) | 6.8/10 | tipo-driven `redact!` macro elegant; R2 storage class transitions (`InfrequentAccess`, `Archive`) are not Cloudflare R2 features (factual error); IPv6 redaction code outputs `2001:db8:0:0::/64` while narrative claims `2001:db8::/64`; schema regex mismatch. |
| WI-S09-003 (OTLP + W3C + sampling + exemplars) | 8.0/10 | W3C 55-char traceparent math correct; sampling discipline solid; cost analysis admits its own arithmetic error mid-paragraph (line 480-486) without resolving it; minor INV positioning issue. |
| WI-S09-004 (CloudEvents + audit chain) | 8.2/10 | Strongest WI; fail-CLOSED canonical exemplar correctly distinguished; BLAKE3 chain math sound; CloudEvents specversion="1.0.2" disagrees with canonical `observability_model.md §7.1` (says "1.0"); event type pattern `io.corelink.*` disagrees with canonical `dev.hugr.corelink.*`; INV §3.14 placement wrong. |
| WI-S09-005 (12 Grafana dashboards) | 7.5/10 | Lote 10.8-tris file-count discipline cleanly absorbed; 12 dashboards in WI **disagree with canonical `observability_model.md §10` list** (canonical has DASH-GLOBAL-PRODUCT, DASH-TENANT, DASH-EXEC, DASH-SUPPLY-CHAIN, DASH-SECURITY, DASH-SLO-CATALOG; WI has DASH-AUTH, DASH-BILLING, DASH-RATE-LIMIT, DASH-DEDUP, DASH-CHAOS, DASH-SLO). |
| WI-S09-006 (Multi-burn-rate alerts + PagerDuty) | 7.6/10 | Sloth math (14.4x/6x) correct per Google SRE Workbook Ch 5; **5 PagerDuty services contradicts sprint contract §5.5 R-S09-14 which mandates 3** (staging, prod-us, prod-eu); auto-quarantine recording rule itself has cardinality concern (per-alertname series) without bound discussion. |
| WI-S09-007 (Synthetic canary + runbook dry-runs) | 6.8/10 | Runbook dry-run absorption strong; **canary loop math is wrong**: WI claims 38880 loops in 72h via `60 × 24 × 3 = 4320 loops/dia/region`, but 60min × 24h = 1440 loops/region/day, so 72h × 3 regions = 12960 total (not 38880); region codes IAD/LHR/BOM are IATA airport codes, **not the 4-char enum codes `wnam`/`weur`/`sam`** canonical in `data_model.md §2.1`. |

## P0 findings (catastrophic; block ship)

1. **Canary loop math wrong (WI-S09-007:43, 108)**. WI claims `60s × 60min × 24h × 3 days × 3 regions = 4320 × 3 × 3 = 38880` and "1 canary loop/min/region = 4320 loops/dia/region". Correct: 1 loop/min × 60min × 24h = **1440 loops/region/day**, × 3 days = **4320 loops/region/72h**, × 3 regions = **12960 loops total/72h**. The DoD ship-gate target "38880 loops" is inflated 3×; if QA verifies literally they will block on an unattainable target. Sprint contract §6 DoD inherits this error implicitly. **Fix:** rewrite as "1440 loops/region/day × 3 regions × 3 days = 12960 loops total"; update sprint contract §6 DoD ship-gate criterion. Alternative: reduce cron interval to 20s if 38880 was the intended sample size.

2. **NEW INVs placed in wrong registry section (all 7 WIs)**. WIs claim "INV-OBS-CARDINALITY-BUDGET (registry §3.13)" and "INV-OBS-AUDIT-CHAIN-INTEGRITY (registry §3.14)". `invariant_registry.md` actually places both in §3.12 (Sprint-driven invariants table, lines 164-165). §3.13 is **Key Management (KEY domain)**, §3.14 is **Auth domain (AUTH)**. This is exactly the Lote 10.8bis P1-13 lesson regression that the WIs claim to have absorbed. WI-S09-001 §1.1 even brags "verified canonical position via grep" — the grep must have failed. **Fix:** rewrite all WI references from §3.13/§3.14 to §3.12 (specifically the §3.12 table for Sprint-driven invariants).

3. **Cloudflare R2 storage class transitions are not a real product feature (WI-S09-002:355-360, 532-534)**. The Terraform IaC declares `transition { days = 30, storage_class = "InfrequentAccess" }` and `transition { days = 90, storage_class = "Archive" }`. Cloudflare R2 has a single storage tier; it does **not support storage-class lifecycle transitions** like AWS S3 (Standard → IA → Glacier). R2 lifecycle rules support only `Expiration` and `AbortIncompleteMultipartUpload`. Implementation will fail at Terraform apply or silently no-op. **Fix:** rewrite lifecycle as expiration-only at 400d; if cold-tier cost discipline is required, document migration to a different bucket via Worker cron (or out-of-scope until Cloudflare ships R2 storage classes).

4. **12 dashboards canonical list mismatches `observability_model.md §10` (WI-S09-005:39, 113)**. Canonical §10 list: DASH-GLOBAL-HEALTH, DASH-GLOBAL-PRODUCT, DASH-TENANT, DASH-CAS, DASH-AC, DASH-EXEC, DASH-GC, DASH-SUPPLY-CHAIN, DASH-SECURITY, DASH-PRIVACY, DASH-COST, DASH-SLO-CATALOG. WI-S09-005 list: DASH-GLOBAL-HEALTH, DASH-CAS, DASH-AC, DASH-AUTH, DASH-GC, DASH-BILLING, DASH-RATE-LIMIT, DASH-DEDUP, DASH-PRIVACY, DASH-CHAOS, DASH-COST, DASH-SLO. 6 dashboards disagree. Sprint contract §4 CAP-OBS-005 matches the WI list, but observability_model is the canonical Nível-3 source. **Fix:** ADR resolving canonical drift; update observability_model.md §10 OR realign WI to canonical names; either way reconcile sprint contract.

5. **Prometheus metric naming uses dots, not underscores (WI-S09-001:93-110, sprint contract §5.1 R-S09-1)**. Canonical naming in `observability_model.md §4.1` is `corelink_<pilar>_<nome>_<unit>[_suffix]` (snake_case with underscores). WI uses `corelink.cas.put.requests_total` (dots). Prometheus does not support dots in metric names (regex `[a-zA-Z_:][a-zA-Z0-9_:]*`); ingest into Mimir would silently drop or rewrite. The dot syntax is OpenTelemetry semantic conventions, **not** Prometheus. Affects 9 RED + 6 USE metric definitions in WI-S09-001 plus all alert recording rules in WI-S09-006 that copy this pattern. **Fix:** change all `corelink.X.Y.Z` to `corelink_X_Y_Z`.

6. **PagerDuty service count contradicts sprint contract (WI-S09-006:39, 124-129)**. Sprint contract §5.5 R-S09-14: "PagerDuty service per environment (staging, prod-us, prod-eu)" — **3 services**. WI declares 5 services (adds prod-sam, prod-iad). Sprint contract DoD §6 inherits the 3-service constraint. **Fix:** reconcile: either reduce to 3 (staging + 2 prod) per sprint contract OR amend sprint contract via change log to 5 with rationale.

7. **CloudEvents canonical specversion + type pattern drift (WI-S09-004:99, 167-168)**. Canonical `observability_model.md §7.1` declares `specversion: "1.0"` and event type pattern `dev.hugr.corelink.<op>.v1`. WI-S09-004 uses `specversion: "1.0.2"` and `io.corelink.<subject>.v1`. CloudEvents v1.0.2 is a **patch revision of v1.0** (no breaking changes; same wire format), so "1.0.2" is technically compliant — but emitting "1.0.2" while existing canonical events emit "1.0" creates two parallel chains for the same audit subjects. The type prefix change (`io.corelink.*` vs `dev.hugr.corelink.*`) is a real wire-format change that breaks SIEM consumer integration with prior events. **Fix:** keep `specversion: "1.0"` and `dev.hugr.corelink.*` prefix to stay consistent with existing audit events (S-06 GC sweep audits already emit canonical pattern).

8. **8 audit subjects vs sprint contract's 7 (WI-S09-004:75-93)**. Sprint contract §5.4 R-S09-10 lists 7 subjects (tenant, cas:put, cas:get, ac:lookup, gc:purge, auth:login, quota:exceeded). WI-S09-004 §6.2 enum has 8 (adds `abuse:detected`). Sprint contract DoD §6 line 134 says "8 subjects" — internal inconsistency within sprint contract. WI made a choice without amending the requirement. **Fix:** sprint contract §5.4 R-S09-10 should be amended to 8 subjects with justification (S-08 abuse-detected event needs auditing) OR WI dropped to 7 and abuse:detected handled elsewhere.

## P1 findings (high quality; address in bis)

1. **IPv6 redaction code/narrative/regex disagreement (WI-S09-002:89-99, 204, 514)**. Code: `format!("{:x}:{:x}:{:x}:{:x}::/64", segs[0..4])` produces `"2001:db8:0:0::/64"` for input `2001:db8::1`. Narrative line 89: "2001:db8::1 → 2001:db8::/64". Schema regex line 204: `^([0-9a-f]{1,4}:){4}::/64$` matches the **code output** (4 segment-colon groups before `::`) but **not the narrative claim** (which has only 2 segments before `::`). Three sources, three different formats. **Fix:** pick RFC 5952 canonical (e.g., apply zero-compression after redaction); update code, narrative, and regex consistently.

2. **Histogram cardinality math omits bucket multiplier (WI-S09-001:266-275)**. Each Prom histogram emits `_bucket{le=...}` × N buckets + `_count` + `_sum` ≈ N+2 series per label-tuple. Default 12 buckets → 14 series multiplier. WI claims `cas.put.duration_seconds: 5×30 = 150 séries`; actual `150 × 14 = 2100 series`. Total RED baseline ~3500 not 1700. Still under 100k global, so not a P0 — but cardinality_check.py validator must apply same multiplier or it's silently 14× off-budget. **Fix:** validator computes histogram bucket count from `buckets` array; WI math updated.

3. **Sprint contract §11.2 cardinality budget mismatch (WI-S09-001:91, 169-170)**. Sprint contract §5.1 R-S09-2 says "20k séries por métrica em produção, 100k global", and WI cites this as `observability_model.md §11.2`. Actual §11.2 lists **dimension-based limits** (`tenant_id: 100k provisioned`, `op: 30`, `error_code: 200`, etc.), not "20k per metric / 100k global". The 20k/100k numbers are S-09-introduced and never canonicalized in observability_model. **Fix:** amend `observability_model.md §11.2` to add the per-metric (20k) and global (100k) budget rows OR remove the citation and treat as S-09 contract delta.

4. **Cardinality validator narrative includes self-acknowledged math hole (WI-S09-003:478-486)**. Cost analysis paragraph contains "Wait, this is too high. Reconsidering: 1.1% × 100k req/sec × 10 KB = 11 MB/s wait that's 950 GB/dia... let me recompute." This is a draft note left in the spec; it never resolves cleanly. The final figure ($103k/yr) uses 10k req/sec — possibly correct for current scale, but the document reads as scratch work. **Fix:** rewrite §22 cost section coherently; remove "wait, this is too high" sentence.

5. **`_ms` suffix discipline cited but no actual suffix violation tested (all WIs)**. Every WI cites Lote 10.7bis P0-3 "column drift no `_ms` suffix" but most use `*_ms` and `*_seconds` interchangeably (e.g., WI-S09-007:69 uses `cas_put_latency_p99_ms` while WI-S09-001:103 uses `corelink.cas.put.duration_seconds`). The lesson was about **D1 column** drift (`deleted_at_ms` etc.); applying it to Prom histograms requires interpretation. Suffix `_seconds` is canonical Prom convention; `_ms` in canary metrics breaks that. **Fix:** standardize `_seconds` for Prom histograms; only use `_ms` in narrative prose, not metric names; clarify that Lote 10.7bis P0-3 was about D1 columns not Prom metric suffixes.

6. **Auto-quarantine recording rule cardinality unbounded (WI-S09-006:155, 168, 254)**. The flapping detection rule `count_over_time(ALERTS{alertstate="firing"}[7d]) > 3` produces one series per alertname × labels — every alert that ever fires creates a series. No cardinality bound declared. With ~56 alert rules × tenant_tier label × region label × ... this can drift to 10k+ series for a meta-rule. **Fix:** explicit recording-rule label set restriction (`without (tenant_tier, region)`); declare bound (~200 series) in narrative.

7. **`additionalProperties: false` semantics (WI-S09-002:153-215)**. Schema declares `additionalProperties: false` strictly. Forbidden raw fields like `email`, `ip_address`, `bearer_token` are simply absent from `properties` block — they are rejected by schema, but a developer reading the schema sees no explicit denylist. Schema 2020-12 does support `not` and `unevaluatedProperties` for explicit blocking. **Fix:** add `"not": {"required": ["email", "ip_address", "bearer_token", "blob_digest_full"]}` to make the forbidden set self-documenting (defense-in-depth on top of `additionalProperties: false`).

8. **Statistical rigor n=10k claim (WI-S09-002:248, 264)**. WI cites "n=10k via proptest random generators provides 95% CI < 0.04% leak rate (10× SOTA bar improvement vs P0-E lesson)". Calculation confirms: Clopper-Pearson upper bound for 0/10000 ≈ 0.0369% (< 0.04%) — **valid**. But the claim "10× SOTA bar improvement vs Lote 10.8bis P0-E n=50+50" is misleading: the original P0-E was about randomized A/B with two arms of 50 each (a Mann-Whitney rank test for indistinguishability), not a one-sided binomial. Different statistical procedure entirely. **Fix:** rephrase rationale; n=10k is correct rigor for binomial 0-leak CI but is not "10× P0-E".

9. **Tail buffer 10k spans memory math (WI-S09-003:132, 139)**. WI says "10k spans em memory budget; ~1 KB/span = 10 MB". OpenTelemetry SDK Rust `BatchSpanProcessor` default queue is 2048; CF Worker memory limit is 128 MB total. 10 MB is fine, but each span with attributes, events, and links is more like 2-4 KB on average. 10k × 4 KB = 40 MB = 31% of total Worker memory. Tight but feasible. **Fix:** declare conservative span size (~2-4 KB) or cap buffer at 5k.

10. **R2 region routing assumption broken (WI-S09-007:107)**. WI uses CF datacenter codes (IAD/LHR/BOM); these are CF colo codes for Workers `request.cf.colo`, **not** R2 bucket regions. R2 buckets are placed by region hint (`auto`, `enam`, `wnam`, `weur`, `eeur`, `apac`, `oc`). Canary using IATA codes for "regions" would require runtime translation that may not exist. **Fix:** align canary region codes to R2 region hints (`enam`, `weur`, `apac`) per `data_model.md §2.1` enum; document IATA→region mapping or drop IATA entirely.

11. **Cross-WI integration mechanism partially honored (Lote 10.8bis P1-NEW-2)**. WI-S09-005 §6.1.7 references "AdminCtx RBAC datasource permissions (Lote 10.8 P1-NEW-2 inheritance)" — but P1-NEW-2 in the brief is about "Cross-WI integration mechanism em scope code não só change log". WIs 002/003/004/005/006 cite each other in `Dependencies §18` but the actual scope/code-level integration (e.g., concrete `redact!` macro import path; `MetricsEmitter` trait import; `AuditEmitter::emit` propagation through the `?` operator from WI-S09-001 → WI-S09-004) is mostly described in narrative not declared in code stubs. **Fix:** §6.1 stubs should `use crate::corelink_metrics::MetricsEmitter` etc. so the inheritance is structurally evident.

12. **Logpush filter syntax (WI-S09-002:339)**. The filter `{"where":{"and":[{"key":"Outcome","operator":"!=","value":"unknown"}]}}` — Cloudflare Logpush filter requires the key path be the actual log dataset field name. For `workers_trace_events` dataset, this is `Outcome` (correct), but the operator `!=` is not in CF Logpush filter operators (canonical: `eq`, `neq`, `lt`, `gt`, `contains`, `in`, `not in`). **Fix:** use `"operator":"neq"` per CF Logpush spec.

## P2 findings (polish)

1. **Title-field length anti-pattern**. WI titles in front-matter §0 are paragraph-long descriptions (e.g., WI-S09-001 "title" is 200+ words). Reading in YAML view is unwieldy. Pattern repeats across all 7 WIs.

2. **Inconsistent INV section number citation**. Various sub-citations: §3.13/§3.14 (wrong), §3.X (placeholder left in some spots — WI-S09-002 line 775).

3. **`5 services` in narrative WI-S09-006 line 39 vs `5 PagerDuty services per sprint contract §5.5 R-S09-14` claim** — sprint contract has 3, not 5. Cross-reference broken.

4. **Sloth math notation inconsistency**. Recording rule uses `0.999` constant inline but SLI target table elsewhere uses 99.9% — would benefit from generated constants per slo_catalog.

5. **WI-S09-006 §6.1.1 alert YAML count**. WI says "~56 rules"; per Lote 10.8-tris P1-NEW-1 lesson, narrative count must match YAML count exactly. "~56" is approximate; CI hook checking exact count would fail or pass arbitrarily depending on interpretation. **Fix:** declare exact target (e.g., "56" or "60") and write that number verbatim in the verifier.

6. **Spurious narrative artifacts**. WI-S09-003 §22 "Wait, this is too high. Reconsidering..." reads as un-cleaned thinking. WI-S09-002 has many `Lote 10.8bis P0-E rigor adapted (n=50+50 → n=10k)` cross-citations but the original P0-E methodology isn't summarized in the WI for self-containment.

7. **Region 3-char regex (WI-S09-002:181)** `^[a-z]{3}$` — canonical region codes per `data_model.md §2.1` are 3-4 chars (`wnam`/`weur`/`sam`). Schema regex permits only 3.

8. **Mixed Portuguese / English in code-adjacent contexts**. Comments like `// Lote 10.8bis lesson: trace_id em exemplar NOT label` mix `em` (PT) with English in same comment. Stylistic but jarring.

## Cross-WI integration issues

1. **Metric naming convention coherence (P0)**. WI-S09-001 emits with dots; WI-S09-002, 003, 004, 005, 006, 007 all reference same metrics by dot-separated names. If WI-S09-001 fix (P0 #5) changes to underscores, all 6 dependent WIs must update simultaneously — cardinality budget tracking, dashboard panels, alert recording rules, exemplar config. This is a chain of ~50+ references.

2. **Fail-CLOSED vs fail-OPEN verification**. WI-S09-004 correctly uses fail-CLOSED; WI-S09-001/002/003 correctly use fail-OPEN. The boundary is most testable via WI-S09-006 alert config: which alerts fire when WI-S09-004 emit fails (SEV-1; transaction abort) vs when WI-S09-001/002/003 emit fails (SEV-3). WI-S09-006 §6.1 does not enumerate these alerts explicitly. **Mitigation:** add explicit alert mapping table in WI-S09-006 cross-referencing each fail-OPEN/fail-CLOSED service.

3. **5-tier canonical Plan tier consistency**. All 7 WIs use Free/Solo/Team/Business/Enterprise. WI-S09-001 §6.1 cardinality math uses 5×30 baseline for tier×region — consistent. Pass.

4. **trace_id in Exemplar discipline**. WI-S09-001 lint rejects trace_id label; WI-S09-003 emits via `observe_histogram(..., exemplar=Some(...))`; WI-S09-005 dashboard exemplar config; WI-S09-006 alert recording rules omit trace_id. End-to-end coherent. Pass.

5. **Audit chain integrity inheritance**. WI-S09-004 daily verifier emits `corelink.audit.chain_verify_runs_total{region, result}`; WI-S09-005 DASH-PRIVACY consumes this; WI-S09-006 alert SEV-1 on `result=break`; WI-S09-007 RB-AUDIT-CHAIN-001 inheritance. Coherent. Pass.

## Strengths

1. **Lote 10.6bis fail-CLOSED vs fail-OPEN canonical discipline cleanly absorbed.** WI-S09-004 explicitly identifies itself as the fail-CLOSED exemplar; WI-S09-001/002/003/007 correctly distinguish themselves as fail-OPEN. This is the cleanest absorption of any prior cycle's lesson.

2. **Lote 10.7bis P0-7 5-tier canonical** consistent throughout — never reverts to 3-tier.

3. **R5 P0-3 `worker::send_future` discipline** universally applied; no `tokio::spawn` slips through.

4. **Lote 10.8-tris P1-NEW-1 file-count discipline** explicitly cited and CI-verified in WI-S09-005 (12 dashboards) and WI-S09-006 (alert YAML count). The mechanism is correct even if the actual numbers (P0 finding 1, 4) need fixing.

5. **CTRL-PRIV-001 type-driven `redact!` macro** is genuine SOTA — compile-time enforcement of PII discipline beats runtime DLP scrub. The 4 built-in redactors (Email/IP/Bearer/BlobDigest) cover the high-volume vectors.

6. **DLP n=10k statistical methodology** valid (binomial 95% CI ≈ 0.037% for 0/10000); rigor improvement over n=10 baseline is real.

7. **Multi-burn-rate Sloth-style 14.4×/6× math** correct per Google SRE Workbook Ch 5; recording rules + alert rules structurally sound.

8. **W3C traceparent 55-char ASCII** verified correctly.

9. **BLAKE3 hash chain** consistency with CAS digests primary; right choice.

10. **Synthetic tenant SLI exclusion** (canary excluded via `tenant_id != CANARY00000` filter) inherits ManualOverride pattern coherently.

## Verdict

**REJECTED** for ship as-is. Score 7.6/10 falls below the 8.0 R4 baseline and significantly below S-06 (9.1) and S-07 (~8.5) post-bis benchmarks.

Pattern matches prior cycles: rigorous absorption of named lessons (10.6bis fail-closed, 10.7bis 5-tier, 10.8-tris file-count) but **regressions on first-principles correctness** — exactly what S-08 tris caught. Specifically:

- 8 P0 findings — too many to bis-fix incrementally; needs structural pass.
- 3 of the 8 P0s are **canonical-source disagreement** (INV §3.X, dashboards list, metric naming) that the WIs do not even flag as their decision; they cite canonical sources that disagree with what they declare.
- 1 P0 is a **factual platform error** (R2 storage classes don't exist) that suggests the Terraform was not validated.
- 1 P0 is **arithmetic** (38880 vs 12960 canary loops) where the math is wrong by 3×.

**Path to APPROVED CONDITIONALLY (target 8.5 post-bis):**

1. Fix all 8 P0s in a Lote 10.9bis pass (estimated ~6-8h diff).
2. Reconcile sprint contract §6 DoD with corrected canary loop count.
3. Decide canonical 12-dashboard list (ADR or amendment to observability_model.md §10).
4. Drop R2 storage class transitions; document expiration-only lifecycle.
5. Address top 5 P1s (IPv6, histogram cardinality, §11.2 budget canonicalization, auto-quarantine cardinality, R2 region routing).

After bis: re-run R4 audit; if score ≥ 8.5 and 0 P0, approve for sprint execution.

---

**End Agent R4 review of S-09 (Lote 10.9). 8 P0; 12 P1; 8 P2; 5 cross-WI integration issues. Aggregate 7.6/10. REJECTED pending bis cycle.**
