---
type: audit
title: Sonnet R5 tris (round-2) review of S-09 WIs post-Lote 10.9bis
date: 2026-04-25
reviewer: Sonnet R5 (Claude Sonnet 4.6 — different model lineage; round 2 / tris)
sprint: S-09
target: 7 WIs post-bis cycle (commits 6fe9605, 0f49b13, 8579fee, e908c37)
methodology: adversarial verification of each claimed bis fix; fresh-defect probing; cross-WI coherence; math verification; Rust type-system boundary analysis; residual-reference scan
---

# Sonnet R5 tris review S-09 (Lote 10.9bis cycle validation)

## Aggregate score: 8.0/10 (vs round-1 baseline 6.9; pre-bis aggregate 7.25)

The bis cycle resolved the three structural P0s that blocked round-1 (histogram math, serde_json::Value, canary count). However the bis fixes themselves introduced six new defects, three of which are P0 severity. The net effect: score rises from 6.9 → 8.0, still below the ≥ 8.5 target. A second ter (quater) cycle covering the newly-introduced P0s is required before ship gate.

---

## Per-WI scores

| WI | Title (short) | R1 score | Tris score | Delta | Top remaining issue |
|---|---|---|---|---|---|
| WI-S09-001 | Analytics Engine + RED/USE + cardinality validator | 6.5 | 7.5 | +1.0 | Completeness criterion 10.s09.001.5 still says "~1780 séries" despite body saying 5400; internal contradiction introduced by bis |
| WI-S09-002 | Logpush + R2 + Loki + log schema + PII redaction | 7.0 | 8.0 | +1.0 | Narrative still describes 4-tier lifecycle (hot/warm/cold/purge) contradicting single-expiration Terraform (P0-C partial fix) |
| WI-S09-003 | OTLP tracing + W3C + sampling + exemplars | 7.5 | 8.0 | +0.5 | Unfixed P2 items (all-zeros trace-id rejection, cost §22 scratch note) — no new P0 introduced |
| WI-S09-004 | CloudEvents audit + R2 Object Lock + hash chain | 6.5 | 8.0 | +1.5 | P0-G comment typo: "corrected from '1.0'" (should be "1.0.2"); residual §3.14 in §4 Capability Mapping; AuditEventData serde PII-enforcement gap (see P0-NEW-2) |
| WI-S09-005 | 12 Grafana dashboards-as-code | 7.5 | 7.0 | -0.5 | P0-D introduced a split-brain: §0 header and §6.1 In-scope still enumerate OLD 12 dashboards (DASH-AUTH/BILLING/RATE-LIMIT/DEDUP/CHAOS/SLO); §1.1 corrects to new canonical list. Two co-existing incompatible dashboard lists in same document (NEW P0) |
| WI-S09-006 | Multi-burn-rate alerts + PagerDuty | 7.0 | 8.0 | +1.0 | ST-002 sub-task still says "PagerDuty 5 services Terraform" despite narrative claiming 3; Terraform code block shows 5 resources (lines 124-127) |
| WI-S09-007 | Synthetic canary 3 regions + runbook dry-runs | 6.0 | 7.5 | +1.5 | IATA codes (IAD/LHR/BOM) incompletely replaced: remain in adversarial scenarios, Gherkin (lines 393, 408-411), §18 Dependencies, change log v1.0.0 historical. These are not historical references — Gherkin and §18 are operative specs |

---

## Bis fix verification (each P0 + P1)

### P0-A: Canary loop count 38880 → 12960

**PASS (partial).** The main body of WI-S09-007 correctly shows 12960 throughout: §1.1, narrative §2, Personas §3, completeness criterion 10.s09.007.5, cost analysis §22 ("12960 PUT + 12960 GET"). Sprint contract §6 DoD corrected.

**RESIDUAL FAIL — Gherkin and §18 not fully corrected.** Gherkin Scenario 1 line 393 reads `"Given canary deployed em IAD + LHR + BOM"` — still IATA codes not region hints (partial fix scope issue, but this Gherkin is also operative spec for canary execution). Scenario 3 adversarial (line 408-411) uses `R2 IAD`, `IAD`, `LHR + BOM`. §18 Dependencies line 569: `"Hard infra: 3 CF regions (IAD/LHR/BOM) provisioned"`. These are operative, not historical. The change log calls these as fixed but the edits missed 5 occurrences in Gherkin + §18.

**Math verification**: 60s interval × 60 min/h × 24h × 3 days × 3 regions = 1440 × 3 × 3 = 12960. Correct.

---

### P0-B: INV §3.13/§3.14 → §3.12

**PASS (partial).** WI-S09-001 §1.1, §9.11, §12 all say §3.12. WI-S09-007 §1.1 says §3.12. WI-S09-006 §12 says §3.12.

**RESIDUAL FAIL — WI-S09-004 §4 Capability Mapping.** Line 297: `"invariant_registry.md INV-AUDIT-APPEND-ONLY (S-06 inherited) + INV-OBS-AUDIT-CHAIN-INTEGRITY (NEW §3.14)"` — still references §3.14. This is in the Capability Mapping section (§4), not the change log. The bis fix updated §12 Invariants Validated (§3.12 correct) but missed §4. Both sections are operative.

**ALSO FAIL — Sprint contract §8 invariants section.** The sprint contract §8 body text lists the new INVs without section number anchors (just "HIGH — novo"), so not a broken reference there. But the sprint contract §3 inherits_from block lists `"INVARIANT-REGISTRY"` without section number — not a defect.

---

### P0-C: R2 storage class transitions REMOVED from WI-S09-002

**PASS (Terraform).** The Terraform HCL block (§6.1.3) correctly has single `expiration { days = 400 }` with no transition blocks. "InfrequentAccess" and "Archive" appear only in a comment describing the future roadmap — explicitly marked as "Future."

**PARTIAL FAIL — Narrative + lifecycle enumeration still describes 4 tiers.** WI-S09-002 §1.1 #2 still enumerates `"Hot 30d / Warm 90d / Cold 400d / Purge > 400d"` as four tiers. WI-S09-002 title/header (line 28) still reads `"lifecycle hot 30d Loki query / warm 90d Logpush index / cold 400d R2 Glacier-equiv / purge > 400d"`. §10.s09.002.7 completeness criterion says `"R2 lifecycle 4-tier configured"`. Section §2 narrative says `"archived 400d via R2 Glacier-equivalent storage class."` The Loki integration paragraph (§6.1.4) says `"Retention policy: 30d hot tier em Loki; 90d warm tier via Logpush index; cold 400d direct R2 Glacier-equivalent query."` The chaos suite item 9 says `"R2 Glacier retrieval (cold tier): 90d+ archive."` 

This is a significant narrative coherence failure: the Terraform correctly implements single-expiration, but the entire conceptual model in narrative, invariants list, completeness criteria, and chaos suite still describes a 4-tier lifecycle that does not exist in CF R2. The customer journey Persona 3 also references "R2 archive 90d-400d cold tier." The narrative P1-1 from R4 (warm-tier duration mismatch) has been patched by removing the Terraform transitions, but the surrounding narrative was not updated to match reality. This is a bis-introduced incoherence: the fix removed the mechanism without removing all the prose describing that mechanism.

---

### P0-D: 12 dashboards canonical reconcile with observability_model.md §10

**PASS (§1.1 corrected list).** WI-S09-005 §1.1 correctly enumerates the observability_model.md §10 canonical 12: GLOBAL-HEALTH, GLOBAL-PRODUCT, TENANT, CAS, AC, EXEC, GC, SUPPLY-CHAIN, SECURITY, PRIVACY, COST, SLO-CATALOG. Sprint contract §4 CAP-OBS-005 amended.

**CRITICAL FAIL — §0 header and §6.1 In-scope still contain the OLD list.** The document title/header (line 27) lists `"DASH-GLOBAL-HEALTH + DASH-CAS + DASH-AC + DASH-AUTH + DASH-GC + DASH-BILLING + DASH-RATE-LIMIT + DASH-DEDUP + DASH-PRIVACY + DASH-CHAOS + DASH-COST + DASH-SLO"` — old list, 12 names, but they include AUTH/BILLING/RATE-LIMIT/DEDUP/CHAOS which the §1.1 correction explicitly replaces with GLOBAL-PRODUCT/TENANT/EXEC/SUPPLY-CHAIN/SECURITY/SLO-CATALOG.

The §6.1 In-scope section (lines 221-233) lists all 12 JSON file paths for implementation: `dash-auth.json`, `dash-billing.json`, `dash-rate-limit.json`, `dash-dedup.json`, `dash-chaos.json`, `dash-slo.json` — the OLD names. The Terraform code block (lines 97-107) implements all 12 with OLD names: `dash_auth`, `dash_billing`, `dash_rate_limit`, `dash_dedup`, `dash_chaos`, `dash_slo`.

The §17 Sub-tasks (lines 490-494) enumerate: `DASH-AUTH + DASH-GC`, `DASH-BILLING + DASH-DEDUP + DASH-RATE-LIMIT`. These are the implementation artifacts. A developer reading the spec to implement would follow §6.1 and produce 6 wrong dashboards. The §1.1 correction note explains that AUTH→SECURITY etc., but the actual file specs in §6.1 were never updated.

This is the most severe new P0 introduced by bis: the correction was applied only to one section while the implementation section, the Terraform code, and the sub-tasks still specify the old list. This creates a split-brain between the canonical declaration (§1.1) and the implementation specification (§6.1), which is arguably worse than the original P0-D problem — now the document is internally inconsistent rather than just misaligned with observability_model.md.

---

### P0-E: Prometheus metrics dots → underscores

**PASS (WI-S09-001).** All CanonicalMetric enum variants (lines 93-125) use `corelink_X_Y_Z` underscore notation. Validator uses underscore names.

**FAIL (sprint contract §5.1 R-S09-1).** The sprint contract still lists all 9 RED metrics with dot notation: `corelink.cas.put.requests_total`, `corelink.ac.lookup.requests_total`, etc. (lines 82-90). The sprint contract was claimed as amended in P0-E, but the requirement text was not changed. This creates a contract-vs-implementation divergence: the sprint contract's Requirements section R-S09-1 specifies dot-notation metrics that are invalid Prometheus metric names, while WI-S09-001 correctly implements underscores.

---

### P0-F: PagerDuty 5 → 3 services

**PASS (narrative + completeness criteria).** WI-S09-006 §0 title, §1.1 #3, §2 narrative, §6.1.2, §9.4, 10.s09.006.10 all correctly say 3 services (staging, prod-us, prod-eu).

**FAIL (Terraform code block + ST-002 sub-task).** The Terraform code block (§6.1 lines 116-127) still instantiates 5 resources: `corelink_staging`, `corelink_prod_us`, `corelink_prod_eu`, `corelink_prod_sam`, `corelink_prod_iad`. The comment on line 129 says `"# 3 services per sprint contract §5.5 R-S09-14 canonical (Lote 10.9bis P0-F corrected)"` but the code above it has 5 resources. Sub-task ST-002 (line 506) reads `"PagerDuty 5 services Terraform + escalation policies"`. Any developer implementing from this spec will create 5 PagerDuty services based on the code and sub-tasks, not 3.

---

### P0-G: CloudEvents specversion "1.0.2" → "1.0"; io.corelink → dev.hugr.corelink

**PASS (event_type field, §1.1 #5, Gherkin).** `event_type` comment shows `"dev.hugr.corelink.<subject>.v1"`. Gherkin Scenario 9 says `"dev.hugr.corelink.dsr.request.v1"`. No `io.corelink` found in active spec definitions.

**PARTIAL FAIL — spec_version comment self-contradicts.** Line 146: `pub spec_version: &'static str, // "1.0" (Lote 10.9bis P0-G corrected from "1.0"; observability_model.md §7.1 canonical)`. The comment says "corrected from '1.0'" but the previous value was "1.0.2" — the comment has a typo that makes the correction appear as a no-op ("1.0 corrected from 1.0"). The actual value `"1.0"` is correct, but the historical note in the comment is wrong. Minor but misleading for audit trail.

**ALSO NOTE — Sprint contract §17 References.** Line 277: `"CloudEvents Specification v1.0.2 (CNCF)"` cited as a reference. This is fine as a citation (the spec is v1.0.2), but it visually contradicts the P0-G fix. Not a defect per se — citing the spec version is distinct from the wire-format version claimed. No issue.

---

### P0-H: Sprint contract §5.4 R-S09-10 amended to 8 subjects (abuse:detected added)

**PASS.** Sprint contract §5.4 R-S09-10 explicitly lists 8 subjects with abuse:detected. DoD §6 says "8 subjects". WI-S09-004 AuditSubject enum has 8 variants. All three sources consistent.

---

### P0-I: Histogram cardinality validator bucket multiplier

**PASS (algorithm).** `estimate_cardinality` function correctly takes `metric_kind` parameter and applies `HISTOGRAM_BUCKET_COUNT + 2` multiplier for histograms. Code at lines 304-311 is correctly implemented.

**PARTIAL FAIL — completeness criterion 10.s09.001.5 not updated.** Line 527: `"10.s09.001.5: cardinality_check.py green em 9 RED + 6 USE = 15 métricas baseline; total ~1780 séries"`. The §6.1.2 body correctly claims ~5400 séries post-correction, but the completeness criterion (the ship gate) still says ~1780. This is the criterion engineers will check off — if they see "~1780" in the DoD criterion and "~5400" in the body, they cannot both be correct. Whoever signs off on this criterion against actual measured cardinality will have a discrepancy. This is a bis-introduced internal contradiction: the body was updated but the DoD criterion was not.

---

### P0-J: Typed AuditEventData enum replaces serde_json::Value

**PASS (structural).** AuditEventData enum exists with 8 typed variants. BlobDigest, BearerToken, IpAddress fields declared. Change log documents the replacement.

**NEW DEFECT INTRODUCED BY P0-J FIX (see P0-NEW-2 below).** The typed enum uses `BlobDigest`, `BearerToken`, `IpAddress` as field types. These types implement `Redact` (with `Output = String`), but `AuditEventData` derives `serde::Serialize`. The serde `#[derive(Serialize)]` on these wrapper types will serialize the inner raw value, not the redacted output, unless the types themselves implement `Serialize` to emit the redacted form. WI-S09-002 defines `BlobDigest(pub String)` with `impl Redact for BlobDigest { fn redact(self) -> String { ... truncated ... } }` — but this is a manual method, not a `Serialize` implementation. Without `impl Serialize for BlobDigest` that calls `self.redact()`, `#[derive(Serialize)]` on `AuditEventData` will serialize `BlobDigest("full_64_char_hash")` as `{"digest_truncated": "full_64_char_hash"}` — exposing the full digest in the audit log, which is written to the 7-year R2 Object Lock archive. The type system boundary at the redact! macro was not extended to serde::Serialize for the wrapper types.

---

### P1 fixes verification

**R4 P1-10 (region codes): PARTIAL PASS.** Narrative, §1.1, §2, §9.1 correctly use enam/weur/apac. However Gherkin (lines 393, 408-411), §18 Dependencies (line 569), and change log v1.0.0 historical still use IAD/LHR/BOM. The Gherkin and §18 are operative specifications, not historical context. Change log v1.0.0 is historical — acceptable. Gherkin and §18 are failures.

**R4 P1-12 (Logpush filter operator `!=` → `neq`): PASS.** Terraform HCL line 339 reads `"operator":"neq"`.

**R5 P1-5 (proptest regex `\PC` → valid Rust regex): PASS.** Line 384 shows `r"[a-zA-Z0-9.+_-]{1,30}@[a-zA-Z0-9-]{1,30}\.[a-z]{2,5}"` — valid Rust regex crate syntax.

**R5 P1-6 (ALERTS → ALERTS_FOR_STATE): PASS.** §1.1 #5 and §6.1.3 both show `sum_over_time(increase(ALERTS_FOR_STATE{alertstate="firing"}[5m])[7d:5m]) > 3`. Correctly uses persistent counter.

**R5 P1-7 (IpAddress::default() eliminated): PASS.** DLP test fixture now uses valid IPv4 regex `r"(25[0-5]|2[0-4][0-9]|1[0-9][0-9]|[1-9]?[0-9])(\.(...))`; `IpAddress::from_str` called with `.expect("proptest IPv4 regex now produces valid addresses")`.

---

## NEW P0 findings (defects introduced by bis OR missed by round 1)

### NEW-P0-1 — WI-S09-005: Split-brain dashboard list (OLD vs NEW) in same document

**File:** `specs/04_sprints/S09/work_items/WI-S09-005-12-grafana-dashboards-as-code.md`

**Finding:** P0-D correction updated §1.1 Invariants (the declaration section) to the canonical 12 dashboards (GLOBAL-HEALTH, GLOBAL-PRODUCT, TENANT, CAS, AC, EXEC, GC, SUPPLY-CHAIN, SECURITY, PRIVACY, COST, SLO-CATALOG). However it failed to update:
- Document title/header (#1): lists OLD 12 (AUTH/BILLING/RATE-LIMIT/DEDUP/CHAOS/SLO).
- §0 Identificação Título field: lists OLD 12 with AUTH/BILLING/RATE-LIMIT/DEDUP/CHAOS/SLO.
- §6.1 In-scope enumeration (the **implementation spec**): lists `dash-auth.json`, `dash-billing.json`, `dash-rate-limit.json`, `dash-dedup.json`, `dash-chaos.json`, `dash-slo.json` — 6 OLD names.
- Terraform code block in §6.1: implements OLD 12 with OLD resource names.
- §17 Sub-tasks: references `DASH-AUTH + DASH-GC`, `DASH-BILLING + DASH-DEDUP + DASH-RATE-LIMIT`.
- §18 Dependencies: `"S-07 SEALED (DASH-DEDUP); S-08 SEALED (DASH-RATE-LIMIT...)"`.

The document is now internally inconsistent. A developer implementing from §6.1 will build 6 wrong dashboards. The ship gate criterion 10.s09.005.1 is `"All 10 Gherkin scenarios green"` — but the Gherkin scenarios reference panels by the name implying DASH-GLOBAL-HEALTH and DASH-CAS (which are in both lists), making Gherkin partially valid but unverifiable against the new names (GLOBAL-PRODUCT, TENANT, EXEC, SUPPLY-CHAIN, SECURITY, SLO-CATALOG are never described in §6.1).

**Severity:** P0. The bis fix created a document that simultaneously specifies two mutually-exclusive sets of 12 dashboards. Implementation is ambiguous; ship gate is unverifiable.

**Fix required:** Update §0 header, §6.1 In-scope file list, Terraform code, §17 Sub-tasks, §18 Dependencies to use the NEW canonical 12 names. Add implementation detail for GLOBAL-PRODUCT, TENANT, EXEC, SUPPLY-CHAIN, SECURITY, SLO-CATALOG (6 dashboards with no panel specs anywhere in the document).

---

### NEW-P0-2 — WI-S09-004: AuditEventData typed enum does not enforce redaction at serde boundary

**File:** `specs/04_sprints/S09/work_items/WI-S09-004-cloudevents-audit-r2-hash-chain-daily-verify.md`

**Finding:** P0-J replaced `serde_json::Value` with typed `AuditEventData` enum. The enum uses wrapper types `BlobDigest`, `BearerToken`, `IpAddress` as field types. However:

1. `AuditEventData` derives `serde::Serialize` (line 99: `#[derive(serde::Serialize)]`).
2. `BlobDigest(pub String)` is defined in WI-S09-002 as a newtype with `impl Redact`. There is NO `impl Serialize for BlobDigest` specified anywhere that calls `self.redact()`.
3. `#[derive(Serialize)]` on a newtype struct `BlobDigest(pub String)` produces `{"digest_truncated": "full-64-char-blake3-hash"}` — the raw full digest, not the 16-char truncation.
4. The same applies to `BearerToken` and `IpAddress` — the raw values (full token, full IP) will be serialized into the 7-year immutable audit log archive.

The spec's claim in §7 (line 225): `"chain digest BLAKE3(serde_json::to_string(&audit_event)) deterministic via struct field ordering"` relies on `serde_json::to_string(&event)` producing correct output. If BlobDigest serializes the raw string (not truncated), then (a) PII is leaked into immutable audit storage, and (b) the canonical hash is computed over raw rather than redacted data — which also means the BLAKE3 chain digest changes if redaction is later fixed, breaking chain verification.

The compile-time enforcement claim is partially correct in that the type system prevents raw `String` fields from being used where `BlobDigest` is expected, but it does NOT prevent the raw digest value from being serialized. The Redact trait's `fn redact(self) -> String` is only called when explicitly invoked; it is not a `Serialize` impl.

**Severity:** P0. This is a security architecture gap in the compliance-critical WI. Audit events written to the 7-year Object Lock archive may contain full blob digests, full bearer token suffixes, and full IP addresses rather than their redacted forms. The entire premise of P0-J — "compile-time PII enforcement" — is only half-implemented.

**Fix required:** For each wrapper type (`BlobDigest`, `BearerToken`, `IpAddress`), implement `serde::Serialize` explicitly to call `self.clone().redact()` and serialize the redacted output. Or wrap fields in `Redacted<T>` newtype that implements `Serialize` via `T::redact()`. The spec must show this impl explicitly, since the type appears in an immutable archive.

---

### NEW-P0-3 — WI-S09-002: 4-tier lifecycle narrative contradicts single-expiration Terraform (P0-C partial fix)

**File:** `specs/04_sprints/S09/work_items/WI-S09-002-logpush-r2-loki-log-schema-pii-redaction.md`

**Finding:** P0-C correctly removed the Terraform storage class transition rules. However the narrative model describing a "4-tier lifecycle" (hot 30d / warm 90d / cold 400d / purge) was not updated to match the corrected single-expiration reality. The following locations still describe the non-existent tiers:

- Title/header (line 28): `"lifecycle hot 30d Loki query / warm 90d Logpush index / cold 400d R2 Glacier-equiv / purge > 400d"`.
- §1.1 #2 Logpush+R2 lifecycle invariant: enumerates all 4 tiers as distinct behaviors.
- §2 narrative: `"archived 400d via R2 Glacier-equivalent storage class (cost ~$0.004/GB/mo cold vs $0.015/GB/mo hot)"` — the cold vs hot cost differential no longer applies (single tier at $0.015/GB-mo throughout).
- §6.1.4 Loki integration: `"cold 400d direct R2 Glacier-equivalent query (slower but available)"` — no such query tier exists.
- §6.1.7 chaos suite item 9: `"R2 Glacier retrieval (cold tier): 90d+ archive; LogQL query slower (~30s)"`.
- Completeness criterion 10.s09.002.7: `"R2 lifecycle 4-tier configured per region"` — the criterion is now false; only 1-tier expiration exists.
- Quality standard 14.s09.002.12: `"4-tier R2 lifecycle hot/warm/cold/purge"`.
- Persona 3 customer journey: `"queries R2 archive 90d-400d cold tier"`.

The document now specifies a Terraform implementation (single expiration at 400d) that contradicts its own invariants, narrative, completeness criteria, and quality standards. The 4-tier model is operationally meaningful — "warm 90d" implies LogQL queries are feasible up to 90d, which is actually true via Loki (not R2 transitions). The underlying conceptual model (Loki for hot, Logpush index for warm, R2 direct for cold) may remain valid as a query-tier model without storage class transitions. But the spec never distinguishes these two concepts; it conflates query tiers with R2 storage class tiers, and now that the storage class fix has been applied, the distinction is unspecified.

**Severity:** P0. The completeness criterion 10.s09.002.7 says `"R2 lifecycle 4-tier configured"`. This criterion will FAIL against the actual Terraform (single-expiration). Ship gate cannot be satisfied unless criterion is updated. Engineers implementing the lifecycle will see conflicting instructions.

**Fix required:** Rewrite §1.1 #2 and all narrative lifecycle descriptions to describe the actual implementation: single R2 expiration at 400d + Loki retention 30d hot + Logpush index warm (query-only tiers, not storage classes). Update completeness criterion 10.s09.002.7 to `"R2 lifecycle single-expiration 400d configured; Loki 30d hot retention configured"`. Remove all Glacier-equivalent language.

---

## NEW P1 findings

### NEW-P1-1 — WI-S09-006: Terraform code block and ST-002 sub-task still specify 5 PagerDuty services

**File:** WI-S09-006 §6.1, §17

**Finding:** P0-F narrative fix is comprehensive (all prose says 3 services), but the operative code and sub-task were not updated:
- Terraform resource block (lines 124-127) declares `corelink_prod_sam` and `corelink_prod_iad` resources alongside the 3 correct ones.
- Sub-task ST-002 (line 506): `"PagerDuty 5 services Terraform + escalation policies"` — wrong count in implementation task.

These are execution artifacts. The Terraform will provision 5 services in prod despite the narrative saying 3. The sprint costs and oncall routing will differ from the spec.

**Fix required:** Remove `corelink_prod_sam` and `corelink_prod_iad` Terraform resources. Update ST-002 to `"PagerDuty 3 services Terraform"`.

---

### NEW-P1-2 — WI-S09-007: IATA codes remain in Gherkin (operative scenarios) and §18 Dependencies

**File:** WI-S09-007 §8 (Gherkin), §18 Dependencies

**Finding:** P1-10 fix replaced IATA codes in narrative and §1.1, but missed:
- Gherkin Scenario 1: `"Given canary deployed em IAD + LHR + BOM"` (line 393).
- Gherkin Scenario 3 adversarial: `"Given R2 IAD experiencing transient outage 5min"`, `"When canary IAD fails 3 consecutive loops"`, `"Then cross-region canary (LHR + BOM) continues"` (lines 408-411).
- Adversarial scenario bullet (§2): `"R2 down em IAD only"`.
- Persona 2 (§3): `"canary IAD failed 3 consecutive"`.
- Chaos suite item 1 (§6.1.11): `"R2 IAD down"`.
- §18 Dependencies: `"3 CF regions (IAD/LHR/BOM) provisioned"`.

Gherkin scenarios are executable acceptance criteria. A QA engineer implementing these scenarios would configure for IAD/LHR/BOM datacenter codes, not enam/weur/apac R2 region hints. §18 Dependencies drives infra provisioning requests.

**Fix required:** Replace all remaining IAD/LHR/BOM occurrences with enam/weur/apac in §2 adversarial, §3 Persona 2, §6.1.11 chaos item 1, §8 Gherkin scenarios 1 and 3, §18 Dependencies.

---

### NEW-P1-3 — WI-S09-004: Residual §3.14 reference in §4 Capability Mapping

**File:** WI-S09-004 §4 (line 297)

**Finding:** P0-B fixed §12 Invariants Validated and §9.11 Design Decisions to say §3.12. However §4 Capability Mapping still says `"INV-OBS-AUDIT-CHAIN-INTEGRITY (NEW §3.14)"`. This is the only remaining stale reference, but it is in the Capability Mapping section which is used by compliance reviewers to trace invariant coverage.

**Fix required:** Change `(NEW §3.14)` to `(NEW §3.12)` in §4.

---

### NEW-P1-4 — WI-S09-001: Completeness criterion 10.s09.001.5 internal contradiction (~1780 vs ~5400)

**File:** WI-S09-001 §10 (line 527)

**Finding:** §6.1.2 body correctly states `"Total RED (Lote 10.9bis P0-I corrected): ~5400 séries"`. Completeness criterion 10.s09.001.5 still says `"total ~1780 séries (well under 100k global budget)"`. The change log v1.1.0 lists this as fixed, but the criterion was not updated. Ship gate criterion references wrong number. Any CI-automated criterion check will compare against 1780, failing if the actual validated cardinality is ~5400.

**Fix required:** Update 10.s09.001.5 to read `"total ~5400 séries (well under 100k global budget)"`.

---

### NEW-P1-5 — WI-S09-004: spec_version comment self-contradicts (corrected from "1.0")

**File:** WI-S09-004 §6.1 AuditEvent struct (line 146)

**Finding:** `pub spec_version: &'static str, // "1.0" (Lote 10.9bis P0-G corrected from "1.0"; observability_model.md §7.1 canonical)`. The comment says "corrected from '1.0'" but the previous incorrect value was "1.0.2". The correction appears as "1.0 corrected from 1.0" — a no-op change. Minor, but the audit trail for this field is incorrect, which matters for compliance review.

**Fix required:** Change comment to `// "1.0" (Lote 10.9bis P0-G corrected from "1.0.2"; observability_model.md §7.1 canonical)`.

---

### NEW-P1-6 — WI-S09-002: Narrative warm-tier duration claim internally inconsistent with Terraform single-expiration

**File:** WI-S09-002 narrative §1.1 #2, §2, §6.1.4 (see also P0-NEW-3 above — this is the P1 aspect of what is declared as P0-NEW-3; listed separately for granularity)

**Note:** This is the P1 dimension of NEW-P0-3 above. The underlying issue is the same; fix is the same. Listed to ensure both the completeness-criterion failure (P0) and the narrative incoherence (P1 dimension) are explicitly tracked.

---

## Residual P2 findings (unchanged from round 1; not fixed in bis)

### P2-R1 — WI-S09-003: W3C all-zeros trace-id rejection not specified

Per R5 round-1 P2-1. W3C Trace Context §3.2.2.3 forbids all-zeros trace-id. The spec still does not specify this rejection rule in invariants or Gherkin. No fix attempted in bis.

---

### P2-R2 — WI-S09-002: IPv4-mapped IPv6 address redaction loses ::ffff: prefix

Per R5 round-1 P2-2. `IpAddress::redact` for `::ffff:192.0.2.1` produces `0:0:0:0::/64` rather than `::ffff:192:0:0/64`. Not fixed in bis. `prop_redact_ipv6_mapped_v4` property test declared but not tested against this assertion.

---

### P2-R3 — WI-S09-001: Suspended/cancelled tenant tier mapping undocumented

Per R5 round-1 P2-3. `enum Tier { Free, Solo, Team, Business, Enterprise }` has no Suspended variant. Not addressed.

---

### P2-R4 — R4 P1-1 (unfixed): IPv6 redaction code/narrative/regex disagreement

Per R4 P1-1. The IpAddress::redact for IPv6 produces `2001:db8:0:0::/64` (4 hex groups + `::`) while:
- The narrative claims `2001:db8::/64` (2 groups + `::`) in the adversarial scenario.
- The schema regex `^([0-9a-f]{1,4}:){4}::/64$` matches the CODE output (4 groups), not the NARRATIVE claim.

Round-1 R4 identified this; not fixed. Three sources remain inconsistent.

---

### P2-R5 — R4 P1-3 (unfixed): Sprint contract §11.2 cardinality budget not canonicalized

Sprint contract §5.1 R-S09-2 cites `observability_model.md §11.2` for the 20k/100k budget, but observability_model.md §11.2 reportedly contains dimension-based limits (per R4 P1-3), not per-metric and global budgets. The cross-reference is still unverified in spec text. Not addressed.

---

### P2-R6 — R4 P1-4 (unfixed): WI-S09-003 §22 cost analysis contains scratch-work text

The "Wait, this is too high. Reconsidering..." passage was mentioned in R4 P1-4. WI-S09-003 was not included in the bis fix passes for §22. Cost analysis is draft-quality.

---

### P2-R7 — R4 P1-9 (unfixed): Tail buffer 10k spans memory math conservative estimate

Per R4 P1-9. 10k spans × 4 KB average = 40 MB of CF Worker 128 MB limit (31%). Spec still says "~1 KB/span = 10 MB". The conservative 4 KB estimate should be declared. Not addressed.

---

### P2-R8 — R4 P1-5 (unfixed): _ms vs _seconds suffix discipline incomplete

`corelink_audit_chain_verify_duration_ms` (WI-S09-004 §6.1.10) uses `_ms` suffix for a Prometheus histogram. Per Prom naming convention, histogram durations should use `_seconds`. `_ms` suffix is acceptable for operational counters/gauges but creates inconsistency with `corelink_cas_put_duration_seconds`. Not addressed.

---

### P2-R9 — R4 P1-11 (unfixed): Cross-WI integration mechanism in scope code not declared

WI-S09-005/006 reference `corelink_metrics::*` types but no `use` statements in §6.1 stubs establish the actual import path. The integration is narrative-only. Not addressed.

---

## Bis fix coherence summary

| Fix | Verification result |
|---|---|
| P0-A canary 12960 | PASS (main body); FAIL (Gherkin + §18 IATA codes remain) |
| P0-B INV §3.12 | PASS (most WIs); FAIL (WI-S09-004 §4 still §3.14) |
| P0-C R2 transitions removed | PASS (Terraform); FAIL (narrative, invariants, completeness criterion still describe 4-tier) |
| P0-D 12 dashboards reconcile | PASS (§1.1 declaration); CRITICAL FAIL (§0/§6.1/Terraform still OLD list) |
| P0-E dots → underscores | PASS (WI-S09-001 code); FAIL (sprint contract §5.1 R-S09-1 still dot notation) |
| P0-F PagerDuty 3 services | PASS (narrative/criteria); FAIL (Terraform code + ST-002 still 5) |
| P0-G CloudEvents 1.0 + dev.hugr | PASS (types); PARTIAL FAIL (comment typo) |
| P0-H 8 subjects | PASS (consistent across all 3 sources) |
| P0-I histogram bucket multiplier | PASS (algorithm); FAIL (10.s09.001.5 criterion says 1780 not 5400) |
| P0-J typed AuditEventData | PASS (structural type); FAIL (serde::Serialize does not call Redact; raw values leak to immutable archive) |
| R4 P1-10 region codes | PASS (narrative); FAIL (Gherkin + §18 still IATA) |
| R4 P1-12 Logpush neq | PASS |
| R5 P1-5 proptest regex | PASS |
| R5 P1-6 ALERTS_FOR_STATE | PASS |
| R5 P1-7 IpAddress::default() | PASS |

**Fully clean passes: 4 of 15 (P0-H, P1-12, P1-5, P1-6, P1-7 = 5)**
**Fully clean fails (NEW defects introduced): P0-D, P0-J are the most severe**
**Partial passes with residual defects: 9 of 15**

---

## Verdict

**REJECTED (second ter / quater cycle required)**

The aggregate score rises from 6.9 → 8.0, which is progress but does not reach the ≥ 8.5 target established in the brief (S-08 precedent 8.1 from 7.45 post-tris; S-09 should reach ≥ 8.5).

**Three new P0s block ship gate:**

1. **NEW-P0-1 (WI-S09-005 split-brain dashboard list)**: The implementation specification (§6.1, Terraform, sub-tasks) still specifies the OLD 12 dashboards while the declaration (§1.1) specifies the NEW canonical 12. A developer implementing from §6.1 will build the wrong dashboards. Six canonical dashboards (GLOBAL-PRODUCT, TENANT, EXEC, SUPPLY-CHAIN, SECURITY, SLO-CATALOG) have no implementation specs.

2. **NEW-P0-2 (WI-S09-004 AuditEventData serde gap)**: Typed enum fields using BlobDigest/BearerToken/IpAddress will serialize their raw (non-redacted) values to the 7-year immutable R2 audit archive because `serde::Serialize` is derived (not custom-implemented to call `Redact::redact()`). This is a security regression introduced by the P0-J fix.

3. **NEW-P0-3 (WI-S09-002 lifecycle narrative vs Terraform incoherence)**: Completeness criterion 10.s09.002.7 says `"R2 lifecycle 4-tier configured"` but actual Terraform is single-expiration. Ship gate criterion will fail or mislead. Narrative describes Glacier-tier behavior that does not exist.

**Conditions for promotion to APPROVED CONDITIONALLY:**

- [ ] NEW-P0-1: Update WI-S09-005 §0, §6.1, Terraform, §17, §18 to use canonical 12 dashboard names; add implementation details for GLOBAL-PRODUCT, TENANT, EXEC, SUPPLY-CHAIN, SECURITY, SLO-CATALOG.
- [ ] NEW-P0-2: Implement `serde::Serialize` for BlobDigest, BearerToken, IpAddress wrapper types to emit redacted output; or introduce `Redacted<T>` wrapper. Spec must show explicit impl.
- [ ] NEW-P0-3: Rewrite WI-S09-002 lifecycle narrative, invariants, completeness criteria to match single-expiration reality; clarify Loki query tiers are not R2 storage tiers.
- [ ] NEW-P1-1: Fix WI-S09-006 Terraform code block and ST-002 to declare 3 PagerDuty services.
- [ ] NEW-P1-2: Fix WI-S09-007 Gherkin scenarios 1+3, §2 adversarial, §3 Persona 2, §6.1.11 chaos item 1, §18 Dependencies to use enam/weur/apac.
- [ ] NEW-P1-3: Fix WI-S09-004 §4 residual §3.14 reference.
- [ ] NEW-P1-4: Fix WI-S09-001 completeness criterion 10.s09.001.5 from ~1780 to ~5400 séries.
- [ ] NEW-P1-5: Fix WI-S09-004 spec_version comment ("corrected from '1.0.2'" not "corrected from '1.0'").
- [ ] P0-E residual: Fix sprint contract §5.1 R-S09-1 metric names from dot to underscore notation.

P2 residuals (P2-R1 through P2-R9) may be deferred to a follow-up sprint at Architect discretion.

---

## Score trajectory

| Cycle | Score | P0 count | Status |
|---|---|---|---|
| Round 1 (R4 + R5 aggregate) | 7.25 | 10 P0 | REJECTED |
| Post-bis target | ≥ 8.5 | 0 P0 | — |
| Tris (this review) | 8.0 | 3 NEW P0 | REJECTED — quater cycle required |

---

*Reviewer: Sonnet R5 (Claude Sonnet 4.6 — distinct model lineage from Opus R4; round 2 / tris adversarial focus on bis-fix validation, cross-section coherence, Rust serde boundary conditions, completeness criterion cross-checking, and residual reference scanning)*
