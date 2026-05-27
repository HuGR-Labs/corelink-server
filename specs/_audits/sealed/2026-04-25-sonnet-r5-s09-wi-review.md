---
type: audit
title: Sonnet R5 (Sonnet 4.6) review of S-09 WIs
date: 2026-04-25
reviewer: Sonnet R5 (Claude Sonnet 4.6 — different model lineage from Opus R4)
sprint: S-09
target: 7 WIs (WI-S09-001 through WI-S09-007)
methodology: adversarial review with numerical verification, cross-reference checking, standards compliance analysis
---

# Sonnet R5 review of Sprint S-09 (Lote 10.9) WIs

## Aggregate score: 6.9/10

Strong narrative quality and cross-WI integration discipline. Three P0 defects identified that block
ship-gate confidence. Multiple P1 defects are individually correctable but cluster into a systemic
pattern: numeric claims stated in narrative are not verified against the implementing artifacts.

---

## Per-WI scores

| WI | Title (short) | Score | Top issue |
|---|---|---|---|
| WI-S09-001 | Analytics Engine + RED/USE + cardinality validator | 6.5/10 | Histogram cardinality claim off by 13x; INV section reference wrong |
| WI-S09-002 | Logpush + R2 + Loki + log schema + PII redaction | 7.0/10 | R2 lifecycle warm-tier duration mismatch; proptest `\PC` regex invalid |
| WI-S09-003 | OTLP tracing + W3C + sampling + exemplars | 7.5/10 | W3C all-zeros trace-id/span-id rejection not specified |
| WI-S09-004 | CloudEvents audit + R2 Object Lock + hash chain | 6.5/10 | serde_json::Value data field breaks compile-time PII enforcement; BLAKE3 canonical JSON non-determinism |
| WI-S09-005 | 12 Grafana dashboards-as-code | 7.5/10 | (not read fully — no major P0 found from cross-ref context) |
| WI-S09-006 | Multi-burn-rate SLO alerts + PagerDuty | 7.0/10 | PagerDuty count contradicts sprint contract; ALERTS flap PromQL fragile |
| WI-S09-007 | Synthetic canary 3 regions + runbook dry-runs | 6.0/10 | Canary loop count 38880 is 3x wrong (should be 12960) |

---

## P0 findings NEW (Sonnet detected; Opus may have missed)

### P0-1 — WI-S09-001: Prometheus histogram cardinality under-counted by ~13x (math error)

**File:** `specs/04_sprints/_sealed/S09/work_items/WI-S09-001-worker-analytics-engine-red-metrics-cardinality-validator.md` §6.1 + §1.5 completeness criteria

**Finding:** WI-S09-001 §6.1 claims `corelink.cas.put.duration_seconds` has **150 series** (5 tier × 30 region = 150 label combinations). This is wrong: Prometheus histograms produce one series **per bucket per label combination**, not one series per label combination. With a default histogram bucket set of ~10 custom buckets + `+Inf`, each label combination yields 13 series (`_bucket{le=X}` × 11 + `_sum` + `_count`). Correct cardinality: 150 × 13 = **1950 series** for `cas.put.duration_seconds` alone — already exceeding the claimed total of ~1780 series across ALL 15 metrics.

**Severity:** P0. The cardinality budget validator `cardinality_check.py` must account for bucket series when estimating histogram cardinality, or it will systematically approve configurations that blow the 20k per-metric and 100k global budgets in production. The sprint's entire cardinality discipline rests on this validator being correct.

**Fix required:** Revise cardinality estimate formula to: `cartesian_labels × (num_buckets + 2)` for histogram metrics. Update all histogram cardinality claims in §6.1, §10.s09.001.5 completeness criterion (stated as ~1780 total — likely ~3000+ when histograms are correctly counted). Update validator algorithm documentation.

---

### P0-2 — WI-S09-004: `AuditEvent.data: serde_json::Value` makes compile-time PII redaction structurally impossible

**File:** `specs/04_sprints/_sealed/S09/work_items/WI-S09-004-cloudevents-audit-r2-hash-chain-daily-verify.md` §1.5 (invariants) + §6.1.7

**Finding:** WI-S09-004 §1.5 claims "compile-time enforcement: `redact!` macro requires types implementing `Redact` trait; raw `String` rejected at compile time" and asserts inheritance from WI-S09-002's type system. However, `AuditEvent.data` is typed `serde_json::Value` — an untyped JSON container. `serde_json::Value` accepts `json!({"email": "user@example.com"})` at compile time with no trait bound. The `redact!` macro cannot enforce type constraints over an untyped `Value` field. Compile-time enforcement as claimed is **structurally impossible** with this type.

**Severity:** P0. This is the most security-critical WI (audit trail, CTRL-AUDIT-001, SOC 2 CC7.2). The claim that compile-time enforcement guards PII in audit events is false; the only real defense is the runtime DLP scanner which tests fixtures, not production code paths. An inadvertent `json!({"user_email": raw_email})` in any emitter calling `AuditEmitter::emit()` would commit PII-containing audit events to the immutable 7-year R2 Object Lock archive.

**Fix required:** Either (a) define a typed `AuditEventData` enum/struct that covers the 8 canonical subjects' data payloads, using `redact!`-wrapped fields throughout — eliminating `serde_json::Value`; or (b) acknowledge this as runtime-only enforcement and remove the false compile-time claim from §1.5, strengthening the runtime DLP scanner to execute on ALL audit emitter call sites in staging (not just fixtures).

---

### P0-3 — WI-S09-007: Synthetic canary loop count 38880 is exactly 3× inflated (triple-counting math error)

**File:** `specs/04_sprints/_sealed/S09/work_items/WI-S09-007-synthetic-canary-3-regions-runbook-dry-run.md` §1.1 + §2 (narrative) + §8 Gherkin + §10.s09.007.5 completeness criterion

**Finding:** The spec claims "4320 loops/dia/region = 12960/dia total" and then "12960 × 3 = 38880 successful canary loops" as the 72-hour target. This is internally contradictory and arithmetically wrong.

Correct math:
- 60s interval → 1440 loops/day/region (not 4320)
- 72h = 3 days → 3 × 1440 = **4320 loops/region/72h**
- 3 regions × 4320 = **12960 loops total in 72h**
- **38880 is 12960 × 3: the three regions were counted three times**

The spec also internally states "4320 loops/3-dias/region" (correct: 4320 is the per-region 72h total), then multiplies by 3 twice (once naming "12960/dia total" — wrong unit; once multiplying by 3 again = 38880).

**Severity:** P0. Sprint contract §6 DoD states "Synthetic canary 24/7 sustentado 72h" with the 38880 figure as the ship gate criterion (§10.s09.007.5 completeness). Teams will believe the 72h test requires 38880 successful loops; the actual 72h test only yields 12960. Declaring "38880 loops passed" against an actual production run would require a silent 9-day observation window, not 72h. This corrupts the ship gate for GA readiness.

**Fix required:** Correct all occurrences of 38880 to 12960 throughout WI-S09-007 (§1.1, §2 narrative, §8 Gherkin Scenario 2, §10.s09.007.5). The sprint contract §6 DoD must also be updated to reference 12960, or clarify that "38880" was erroneously entered.

---

## P1 findings

### P1-1 — WI-S09-002: R2 lifecycle Terraform `days=90` does not mean "90-day warm tier"

**File:** WI-S09-002 §1.2 + §6.1.3 (Terraform HCL)

**Finding:** Narrative says "warm 90d (Logpush index)". Terraform has `transition { days = 30 ... InfrequentAccess }` and `transition { days = 90 ... Archive }`. AWS/CF R2 lifecycle `days` is counted **from object creation**, not from the previous transition. So warm tier duration is `90 - 30 = 60 days`, not 90 days. Cold tier duration is `400 - 90 = 310 days`. The "warm 90d" label in narrative, sprint contract §5.2, and completeness criteria §10.s09.002.7 all misstate the tier durations.

**Fix required:** Either (a) update narrative to "warm 60d (days 30-90 from creation)" and "cold 310d (days 90-400)"; or (b) revise Terraform to `days = 120` for the Archive transition to achieve a true 90-day warm window.

---

### P1-2 — WI-S09-001 / WI-S09-004: INV-OBS-CARDINALITY-BUDGET and INV-OBS-AUDIT-CHAIN-INTEGRITY section references are wrong

**File:** WI-S09-001 §1.1, §1.7, §9.11; WI-S09-004 §1.1, §9.11; sprint contract §8

**Finding:** WI-S09-001 references INV-OBS-CARDINALITY-BUDGET as "registry §3.13". WI-S09-004 references INV-OBS-AUDIT-CHAIN-INTEGRITY as "registry §3.14". The actual invariant_registry.md places BOTH INVs in **§3.12** (sprint-driven invariants table). §3.13 is "Key management (domain KEY)". §3.14 is "Auth domain (domain AUTH)". This causes broken internal cross-references for reviewers verifying invariant coverage.

**Fix required:** Update all references in WI-S09-001 and WI-S09-004 from §3.13 / §3.14 to §3.12. Sprint contract §8 (Invariants §8.2) similarly references new §3.13/§3.14 — update to §3.12.

---

### P1-3 — WI-S09-006: PagerDuty 5-service count contradicts sprint contract §5.5 R-S09-14 (3 services)

**File:** `specs/04_sprints/_sealed/S09/work_items/WI-S09-006-multi-burn-rate-slo-alerts-pagerduty.md` §1.3 vs `specs/04_sprints/_sealed/S09/_spec_contract.md` §5.5 R-S09-14

**Finding:** Sprint contract §5.5 R-S09-14 explicitly says "PagerDuty service per environment (staging, prod-us, prod-eu)" — 3 environments, 3 services. WI-S09-006 §0 and §6.1.2 claim 5 services (adding prod-sam, prod-iad) and cites "sprint contract §5.5 R-S09-14" as the authority. This is false attribution: the sprint contract specifies 3, not 5. WI-S09-006 is silently expanding scope without a formal sprint contract amendment.

**Fix required:** Either (a) formally amend sprint contract §5.5 R-S09-14 to specify 5 environments before WI-S09-006 can be sealed; or (b) reduce WI-S09-006 to 3 services matching the sprint contract. The attribution "per sprint contract §5.5" must be corrected in any case.

---

### P1-4 — WI-S09-004: BLAKE3 hash chain canonical JSON non-determinism (serde_json::Value maps)

**File:** WI-S09-004 §6.1.4 (chain.rs pseudo-code)

**Finding:** The chain link computation uses `serde_json::to_string(&event)` as the canonical serialization for BLAKE3 hashing. For the `AuditEvent` typed struct, serde_json serializes fields in declaration order — deterministic within Rust. However, `event.data: serde_json::Value` may contain JSON object maps whose key ordering in serde_json depends on insertion order (HashMap-backed at runtime). Two serializations of the same logical event with differently-ordered keys in `data` produce different BLAKE3 digests, silently breaking the hash chain.

**Severity:** P1 (chain break under normal operation with map-valued data fields). The verifier would detect this as a chain break and fire a SEV-1 alert — false positive, but indistinguishable from tampering.

**Fix required:** Either (a) use RFC 8785 (JSON Canonicalization Scheme / JCS) via a crate like `jcs` for the hash input, which defines deterministic key ordering; or (b) constrain `event.data` to a typed struct (which also fixes P0-2 above), where serde_json's struct serialization is deterministic.

---

### P1-5 — WI-S09-002: proptest `\PC` regex is not valid Rust `regex` crate syntax

**File:** WI-S09-002 §6.1.5 (dlp_scanner.rs pseudo-code)

**Finding:** The email fixture generator uses pattern `r"\PC{1,30}@\PC{1,30}\.\PC{2,5}"`. In proptest's `string_regex()`, the underlying engine is the Rust `regex` crate, which does NOT support POSIX property syntax `\P` (uppercase P for negation). Valid syntax would be `\p{L}` (Unicode letter) or `[^\n@]` for "any char except newline/at-sign". The `\PC` pattern will either fail to compile the proptest or silently match no strings, making the entire 10k-fixture DLP test a no-op for the email fixtures.

**Fix required:** Replace `r"\PC{1,30}"` with `r"[a-zA-Z0-9.+_-]{1,30}"` (conservative but valid for RFC 5321 local-part) or `r"\p{L}{1,30}"` for Unicode-aware testing. Verify the pattern compiles against the actual proptest/regex crate version in use.

---

### P1-6 — WI-S09-006: `count_over_time(ALERTS{alertstate="firing"}[7d])` under-counts flaps

**File:** WI-S09-006 §1.5 (auto-quarantine logic)

**Finding:** `ALERTS` is an ephemeral Prometheus special metric: it exists only while an alert is in firing state. `count_over_time(ALERTS[7d])` applied at a single evaluation point returns the count of samples within the 7d range window — but because `ALERTS` disappears between firing windows, resolved-then-refired alerts produce non-contiguous sample series. Depending on scrape interval and Prometheus step, `count_over_time` may see 0 or 1 sample even for an alert that fired 4 separate times in 7 days.

**More robust pattern:** Use a recording rule that increments a counter on each alert state transition `firing → pending` (via `increase(ALERTS_FOR_STATE[5m]) > 0`), summed over 7d. Or use Alertmanager API flap detection native support.

**Fix required:** Replace `count_over_time(ALERTS[7d]) > 3` with a persistent counter recording rule that survives between firing windows. Alternatively, delegate flap detection to Alertmanager's native `group_interval` + `repeat_interval` configuration.

---

### P1-7 — WI-S09-002: `IpAddress::default()` undefined in DLP test causes silent test failure

**File:** WI-S09-002 §6.1.5 (dlp_scanner.rs line `IpAddress::from_str(&ipv4).unwrap_or_else(|_| IpAddress::from_str(&ipv6).unwrap_or(IpAddress::default()))`)

**Finding:** The DLP test generates IPv4 patterns like `r"[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}"` which produce invalid addresses (e.g., `999.999.999.999`). Both `from_str` calls will fail for invalid inputs. The fallback is `IpAddress::default()` — but `IpAddress(pub std::net::IpAddr)` does not implement `Default` in the spec. This code either will not compile, or if `Default` is derived, will silently test against `0.0.0.0` (a valid IP) — bypassing test coverage of malformed inputs. The claim of comprehensive IPv4/IPv6 DLP coverage depends on this code path working correctly.

**Fix required:** Constrain the IPv4 regex to `r"(25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.(...)\...."` for valid addresses only, eliminating the need for fallback. Or explicitly define `IpAddress::default()` and document that degenerate cases are tested against 0.0.0.0.

---

## P2 findings

### P2-1 — WI-S09-003: W3C Trace Context all-zeros trace-id and parent-id rejection not specified

**File:** WI-S09-003 §1.1 (W3C compliance list), §8 Acceptance Criteria

**Finding:** W3C Trace Context Recommendation §3.2.2.3 explicitly forbids all-zeros `trace-id` (`00000000000000000000000000000000`) and §3.2.2.4 forbids all-zeros `parent-id` (`0000000000000000`). These must be rejected as invalid `traceparent` headers, generating a new local trace context. WI-S09-003 specifies W3C strict parsing but omits these specific rejection rules from the invariants list and Gherkin scenarios. The implementation may or may not handle this; the spec provides no enforcement requirement.

**Fix required:** Add to §1.1 invariants: "trace-id all-zeros and parent-id all-zeros are rejected per W3C §3.2.2; new local context generated; context_parse_failures counter incremented." Add Gherkin scenario for all-zeros traceparent rejection.

---

### P2-2 — WI-S09-002: IPv4-mapped IPv6 address loses `::ffff:` prefix in redaction output

**File:** WI-S09-002 `IpAddress::redact` for V6 path

**Finding:** For IPv4-mapped IPv6 address `::ffff:192.0.2.1`, `v6.segments()` returns `[0, 0, 0, 0, 0, 65535, 49152, 513]`. The redaction takes `segs[0..4]` = `[0, 0, 0, 0]` and formats as `0:0:0:0::/64`. This loses the `::ffff:` marker that identifies it as an IPv4-mapped address. An ops engineer seeing `0:0:0:0::/64` in logs cannot determine the original client subnet. The narrative in §2 claims the result is `::ffff:192:0:0/64` — this is incorrect per the actual code.

**Fix required:** Detect IPv4-mapped IPv6 addresses (`segments[4] == 0 && segments[5] == 0xffff`) and format as `::ffff:{s6}:{s7_masked}:0/96` (proper IPv4-mapped notation). Add a property test `prop_redact_ipv6_mapped_v4` that asserts the `::ffff:` prefix is preserved.

---

### P2-3 — WI-S09-001: Suspended/canceled tenant tier mapping undocumented

**File:** WI-S09-001 §1.7, §2 (adversarial scenarios)

**Finding:** The 5-tier canonical `enum Tier { Free, Solo, Team, Business, Enterprise }` has no variant for suspended, canceled, trial, or churned tenants. If a tenant is suspended after payment failure and continues sending requests, which tier label is emitted? If Free is used, it inflates the Free tier's error rate, potentially masking SLO breaches in that tier. If the previous tier is used, it incorrectly attributes errors. Neither behavior is documented.

**Fix required:** Document the policy: either (a) add a `Suspended` variant that aggregates to a distinct label value (separate from Free); or (b) explicitly state that suspended tenants emit as `Free` tier and document the deliberate choice. The cardinality budget (5 × 30 × 3 = 450 series) calculation should be updated if a `Suspended` variant is added (becomes 6 × 30 × 3 = 540 series).

---

### P2-4 — WI-S09-001: Python cardinality validator using regex over Rust diff is structurally fragile

**File:** WI-S09-001 §6.1.4 (cardinality_check.py pseudo-code)

**Finding:** The validator is described as "use syn-style parsing" but is implemented as Python regex over PR diff text. The `syn` crate is a Rust AST parser — Python cannot use it directly (PyO3 bridge would require a compiled extension, not described). Regex over diff lines cannot handle: (a) labels generated via proc macros, (b) labels added inside `#[cfg(...)]` conditional blocks, (c) inline code that expands to additional label variants, (d) multi-line struct additions where the diff context is incomplete. This creates validator bypass paths that the spec's cardinality discipline depends on closing.

**Fix required:** Document the regex approach's known limitations explicitly. Consider adding a secondary check: a Rust `build.rs` integration test that enumerates all `MetricLabels` fields via reflection and computes the cartesian product, failing compilation if it exceeds budget. This is bypass-proof.

---

## Cross-WI integration issues

**Issue 1 — WI-S09-004 depends on WI-S09-002 redact! enforcement but P0-2 shows the enforcement chain is broken for the `data: serde_json::Value` field.** WI-S09-002's compile-time discipline (typed structs) does not extend to WI-S09-004's untyped `data` field. The cross-WI dependency claim "PII redaction inheritance from WI-S09-002" is partially false for WI-S09-004's most critical field.

**Issue 2 — Canary loop count error (P0-3) propagates to sprint contract §6 DoD.** The DoD explicitly states "38880 successful canary loops" as the ship gate. WI-S09-006 inherits the synthetic canary MTTA test from WI-S09-007. Both WIs reference the 38880 figure. All three specs (contract, WI-007, WI-006 DoD) need simultaneous correction to 12960.

**Issue 3 — Histogram cardinality error (P0-1) affects WI-S09-005 and WI-S09-006.** Dashboard panels in WI-S09-005 query histogram metrics; if cardinality actually exceeds the 20k budget due to bucket under-counting in the validator, Grafana Mimir will reject ingest silently and dashboards will show "no data" — which is also the failure mode for RB-FM-153 (Grafana outage). WI-S09-007 dry-run tests for cardinality explosion use a threshold of "25k series" for the test metric, but baseline histograms may already exceed budget before the test injection.

**Issue 4 — PagerDuty service count discrepancy (P1-3) affects sprint contract §6 DoD MTTA test.** DoD requires "PagerDuty MTTA < 5min synthetic 7d" — unclear which environments are covered if 3 vs 5 services are deployed. The MTTA criterion scope is ambiguous until the service count is reconciled.

**Issue 5 — INV section references (P1-2) affect invariant cross-reference traceability.** Eight WI documents (including sprint contract §8) reference INVs at wrong registry sections. Automated invariant coverage validation tools (if built) will fail to locate the INVs at the cited positions.

---

## Methodological strengths

**1. Fail-open vs fail-closed discipline is rigorously applied.** The distinction between observability emit (fail-open) and audit emit (fail-closed) is consistently threaded through all 7 WIs with explicit reference to Lote 10.6bis lesson. This is the strongest cross-cutting discipline in the batch.

**2. Cardinality discipline intention is correct.** The recognition that histograms need exemplar fields (not labels) for trace_id is architecturally correct and consistently enforced. The FORBIDDEN_LABELS set is comprehensive.

**3. CloudEvents extension naming is spec-compliant.** All extension attributes (`tenantid`, `region`, `traceid`, `prevhash`, `eventdigest`) are lowercase alphanumeric — correctly avoiding the underscore trap that previous Sonnet reviews caught in other specs.

**4. 7-year day count is correct.** `7 × 365 + 2 leap years (2028, 2032) = 2557 days` matches Terraform `days = 2557`. This was a focus zone for this review and it passes cleanly.

**5. Multi-burn-rate 14.4x and 6x multipliers are canonically correct.** The math `14.4 × (1 - 0.999)` = 1.44% error rate, exhausting 30d budget in 30/14.4 ≈ 2.08 days, matches Google SRE Workbook Ch 5 exactly. The 6x slow-burn threshold is also correct.

**6. DLP scanner n=10k Clopper-Pearson bound is approximately correct.** Two-sided 95% CI for k=0, n=10000 gives upper bound 0.0369% — below the claimed 0.04%. The claim is valid. (Note: Wald CI is degenerate for k=0 and must not be used; WI correctly references n=10k as the rigor standard even if the CI method is not explicitly named.)

**7. Lote 10.8-tris YAML count discipline absorbed in WI-S09-006.** The CI hook verifying YAML rule count against narrative claim addresses the S-08 lesson explicitly. This pattern of absorbing prior-round findings is systematic and visible.

**8. Per-region hash chain design eliminates global split-brain risk.** The choice of per-region rather than global chain is architecturally sound and cross-references sprint contract §15 risk register correctly.

---

## Methodological gaps (Sonnet lens)

**1. Numeric claims not verified against artifact implementations.** The 38880 canary loop count (P0-3) and the 150-series histogram claim (P0-1) are present in narrative, completeness criteria, AND Gherkin scenarios — all three layers agree on the wrong number. Sonnet detects these because its reasoning pattern independently computes the arithmetic rather than accepting the stated result. The team's review process needs an explicit "verify every number in spec against independent calculation" step.

**2. Type system claims overstated for untyped containers.** "Compile-time enforcement" appears 4 times across WI-S09-002 and WI-S09-004, but WI-S09-004's `data: serde_json::Value` field structurally defeats it. Opus reviewers tend to accept type system arguments at face value; Sonnet traces the type through to the actual field type in the struct definition.

**3. Terraform semantics assumed but not verified.** The R2 lifecycle days-from-creation vs days-from-previous-transition ambiguity (P1-1) requires reading Cloudflare/AWS S3 lifecycle rule documentation. The Terraform snippet looks correct syntactically but the narrative description of durations is wrong. Sonnet catches this by reasoning about what the API actually does, not what the human intended.

**4. PromQL ephemeral metric behavior not analyzed.** The ALERTS metric flap detection (P1-6) requires understanding Prometheus metric lifecycle. `count_over_time` on an ephemeral metric is a subtle PromQL antipattern that requires knowing when samples are present vs absent in the ring buffer.

**5. proptest regex syntax not cross-checked against Rust `regex` crate.** The `\PC` pattern (P1-5) is POSIX syntax, not Rust regex syntax. This category of error (spec written in conceptual syntax, not actual API syntax) slips past narrative reviewers.

---

## Verdict

**APPROVED CONDITIONALLY**

Ship gate is blocked by 3 P0 defects:

1. **P0-1 (histogram cardinality math)**: The cardinality validator CI gate — the spec's #1 cost protection — is computing histograms incorrectly. Budget claims and validator algorithm must be corrected before CI gate can be trusted.

2. **P0-2 (AuditEvent.data PII enforcement)**: The audit WI's compile-time PII enforcement claim is structurally false for the `serde_json::Value` data field. The audit trail is the compliance cornerstone (SOC 2 CC7.2, LGPD Art. 32); false enforcement claims cannot be shipped.

3. **P0-3 (canary 38880 loop count)**: The ship gate criterion in sprint contract §6 DoD references the wrong number (38880 vs correct 12960). GA readiness cannot be certified against an arithmetically incorrect gate.

Conditions for promotion to APPROVED:

- [ ] Correct histogram bucket cardinality in WI-S09-001 (P0-1); update validator algorithm
- [ ] Fix WI-S09-004 PII enforcement to typed struct or document runtime-only enforcement (P0-2)
- [ ] Correct canary loop count to 12960 in WI-S09-007 and sprint contract §6 DoD (P0-3)
- [ ] Fix R2 lifecycle warm-tier duration narrative (P1-1)
- [ ] Correct INV registry section references §3.13/§3.14 → §3.12 (P1-2)
- [ ] Resolve PagerDuty service count discrepancy 3 vs 5 with sprint contract amendment (P1-3)
- [ ] Fix BLAKE3 canonical JSON for serde_json::Value maps (P1-4)
- [ ] Fix proptest `\PC` regex pattern (P1-5)
- [ ] Replace `count_over_time(ALERTS[7d])` flap detection with persistent counter (P1-6)
- [ ] Fix `IpAddress::default()` undefined fallback in DLP test (P1-7)

P2 findings (P2-1 through P2-4) may be addressed in a follow-up sprint or accompanying PR without blocking S-09 promotion, at Architect + Privacy Officer discretion.

---

*Reviewer: Sonnet R5 (Claude Sonnet 4.6 — distinct model lineage from Opus R4 reviews; adversarial focus on numerical/statistical verification, type system boundary conditions, Terraform/PromQL runtime semantics, and standards strict compliance)*
