---
type: audit
title: Sonnet R5 (Sonnet 4.6) review of S-08 WIs
date: 2026-04-25
reviewer: Sonnet R5 (Claude Sonnet 4.6 — different model lineage from Opus R4)
sprint: S-08
target: 6 WIs (WI-S08-001 through WI-S08-006)
---

# Sonnet R5 review of Sprint S-08 (Lote 10.8) WIs

## Aggregate score: 6.9/10

Sprint S-08 is architecturally sophisticated and absorbs many prior-sprint lessons correctly. However, several numerical, logical, and API-correctness defects were found that Opus reviewers—focused on pattern-completeness—are likely to miss. The 4-camada defense-in-depth design is sound; the defects are concentrated in formula boundary cases, statistical methodology gaps, and one critical period-reset logic error.

---

## Per-WI scores

- **WI-S08-001** (DO RateLimiter token bucket): 7.5/10 — math sound; monotonic clamp correct; one P1 on retry_after formula edge case; one P2 on f64 drift concern framing.
- **WI-S08-002** (CF edge per-IP rules + CIDR blocklist): 7.0/10 — sound NAT-awareness; P1 on CF Ruleset `ratelimit{}` Terraform schema correctness; P2 on suggest_block trigger threshold exclusivity gap.
- **WI-S08-003** (Quota checker middleware atomic CAS + per-PAT): 5.5/10 — contains P0 on bandwidth period-reset function semantics, P0 on per-PAT cap mathematical inconsistency allowing aggregate bypass; P1 on overshoot calculation.
- **WI-S08-004** (Abuse detection heurística + LGPD humane): 6.0/10 — calibration methodology is statistically insufficient (P0); score formula has normalization inconsistency for the entropy feature (P1); the inline self-critique in §3 (Persona 3) revealing that typical abuse patterns score < 0.5 is alarming.
- **WI-S08-005** (RFC 9331 headers + global circuit breaker): 7.0/10 — multi-signal logic correct; P1 on `do_error_rate_5m` averaging method; P1 on HalfOpen `global_circuit_open` SLI classification ambiguity.
- **WI-S08-006** (DASH-RATE + PRR ship gate): 7.5/10 — PRR waiver path for mandatory roles needs scrutiny (P1); alert count claim inconsistency (P2); alert SEV-2 count in narrative body wrong (§6.1 §2 says "5" SEV-2 but lists 6).

---

## P0 findings NEW (Sonnet detected; Opus may have missed)

### P0-1: `tomorrow_at_utc_midnight()` does NOT compute next-1st-of-month (WI-S08-003)

**Location**: WI-S08-003 §1 invariant 8; §6.1 item 5; §14.s08.003 quality standard item 10 (chrono `tomorrow_at_utc_midnight()` noted as "cycle aligned").

**Finding**: The function `chrono::tomorrow_at_utc_midnight()` (referenced from Lote 10.5bis lesson) computes the **next calendar day at 00:00:00 UTC** — not the **next first-of-month at 00:00:00 UTC**. These are identical only on the last day of each month (when tomorrow IS the 1st). On any other day, `tomorrow_at_utc_midnight()` returns midnight of the next day, while the bandwidth quota reset is specified to occur on "1st UTC of month" (sprint contract §10.s08.5: "reset em 1º dia UTC do mês seguinte; não em sliding window").

The code in `check_bandwidth` uses `utc_period_for_ts(now_ms)` returning `"YYYY-MM"` to detect period transition, which is correct by design, but the `secs_until_next_utc_midnight_1st(now_ms)` function for the `Retry-After` value and the DO alarm scheduling via `tomorrow_at_utc_midnight()` in §14 are inconsistent:

- If the alarm is scheduled via `tomorrow_at_utc_midnight()`, it fires every night at midnight, causing the DO to check for period transition (YYYY-MM change). This is benign overhead but the function name is semantically wrong and creates a documentation/code mislead: the alarm will fire ~28-31 times before the actual reset occurs. More critically, if any future developer replaces the period-check logic with a direct `tomorrow_at_utc_midnight()`-derived reset, data will be wiped daily.

- The `Retry-After` header for `BandwidthOver` must compute "seconds until next 1st of month UTC", NOT "seconds until tomorrow midnight". These values differ by up to 30 days. A customer who exhausted bandwidth on the 2nd of the month would receive `Retry-After: 86400` (1 day) instead of `Retry-After: ~2592000` (30 days). This is a direct customer-facing correctness bug.

**Required fix**: Replace `tomorrow_at_utc_midnight()` references in this context with a correct `next_month_first_utc_midnight(now: DateTime<Utc>) -> DateTime<Utc>` implementation using `chrono`'s `NaiveDate::from_ymd_opt(y, m+1, 1)` (handling December → January year increment). The DO alarm should be scheduled to fire at `next_month_first_utc_midnight()`, not nightly. The `secs_until_next_utc_midnight_1st` function referenced in check_bandwidth code must be verified to implement this correctly (no such function is defined in the WI; the name implies it computes "next first of UTC" but could easily be implemented as "tomorrow midnight").

**Severity**: P0 — customer-facing Retry-After miscalculation of up to 30× magnitude; potential silent bandwidth quota reset every midnight if developer interprets the lesson incorrectly during implementation.

---

### P0-2: Per-PAT cap mathematical amplification allows aggregate tenant rate bypass (WI-S08-003)

**Location**: WI-S08-003 §1 invariant 9; §6.1 item 6.

**Finding**: The per-PAT cap formula is:
```
pat_cap = tenant_refill_rate × 10
```

For team tier: `tenant_refill = 200 RPS` → `pat_cap = 2000 RPS`.

The PAT burst is:
```
pat_burst = pat_cap × 5 = 10000 tokens
```

The WI states: "Single PAT cannot exhaust > 10% of tenant rate; rate budget shared across multiple PATs (typical scenarios: 5–20 PATs per tenant)."

This reasoning is **mathematically inverted**. If a single PAT has a cap of `10× tenant_refill`, then ONE PAT is allowed to consume **10× the tenant's entire sustained rate**. With 5 PATs at team tier: `5 × 2000 = 10000 RPS aggregate from 5 PATs` vs the tenant's DO RateLimiter cap of `200 RPS sustained`. The per-PAT camada 3 does NOT constrain aggregate PAT consumption relative to tenant capacity — it constrains individual PAT abuse (misuse detection), but allows the aggregate to wildly exceed tenant rate.

The WI author appears to intend that per-PAT cap is a misuse DETECTOR (`cap = 10× tenant` means a single PAT exceeding 10× refill signals misuse), not a rate enforcer. But this creates a critical gap: multiple legitimate PATs (5–20 per tenant) each consuming at the tenant's full refill rate (200 RPS) would never individually trigger the misuse threshold (each is at 200 RPS, well below 2000 RPS cap), yet aggregate to 5× the tenant's DO rate limit. The camada 1 DO RateLimiter (WI-S08-001) handles this correctly per-tenant, but the per-PAT layer does not aggregate across PATs.

**Correctness issue**: If the intent is "per-PAT cap = 10× tenant refill as misuse signal", that is not a rate ENFORCEMENT cap — it is a soft detection threshold. But the WI frames it as "camada 3 of 4 PAT-RATE-LIMIT-001" enforcement. Either:
(a) The cap should be `tenant_refill / expected_num_PATs` (e.g., 200 / 5 = 40 RPS per PAT), so PATs collectively cannot exceed tenant rate — true enforcement; OR
(b) The current 10× cap is purely a misuse detector (not enforcer), and camada 1 is the sole rate enforcer — in which case the 4-camada narrative for CAP-RATE-003 is misleading.

The formula as specified allows a tenant with 10 PATs to legitimately request 10 × 10 × 200 = 20,000 RPS aggregate (each PAT at 2000 RPS cap) while camada 1 limits the tenant to 200 RPS. Camada 3 is not adding enforcement — it is adding noise. This is fine architecturally IF clearly stated as a misuse detector, but the current spec claims it is an enforcement camada.

**Severity**: P0 — The 4-camada defense-in-depth claim is partially false for CAP-RATE-003. Compliance/Privacy sign-off would approve a "per-PAT enforcement layer" that doesn't enforce per-tenant aggregate rate.

---

### P0-3: Calibration methodology statistically insufficient for HIGH_RISK SOTA bar (WI-S08-004)

**Location**: WI-S08-004 §1 invariant 6; §6.1 item 12; sprint contract §6 DoD "Abuse score calibration" criterion.

**Finding**: The calibration target is "10 synthetic workloads (5 benign + 5 abusive); 0 FP + ≥ 80% TP."

Statistical analysis:
- **5 benign workloads with 0 FP**: Under a binomial model, the 95% CI upper bound on FP rate from observing 0 FP in 5 trials is `1 - (0.05)^(1/5) ≈ 45%`. This means the sample is consistent with a true FP rate up to 45%. Claiming "calibrated for HIGH_RISK lane" from 5 observations is statistically meaningless.
- **5 abusive workloads with ≥ 80% TP** (i.e., ≥ 4/5): This is a binomial with n=5, k≥4. At true TP=80%, P(k≥4) = 0.737. At true TP=60%, P(k≥4) = 0.337. The test cannot distinguish 60% TP from 80% TP at any meaningful confidence level.
- **No confidence intervals specified**: Pass/fail on n=5 + n=5 without CI is not a scientific calibration — it is manual spot-checking.
- **No out-of-distribution workload**: all 5 abusive are extreme cases (cpu farming, scraping, spam, exec flood, multi-vector). Real abuse is subtle and partial. No intermediate cases are tested.
- **Self-critique visible in §3 Persona 3**: The WI author notes inline: "Actually rebuilding: cpu_ratio=0.95 + entropy=1.5 + exec=5×baseline → score = 0.27+0+0.125+0.008 = 0.40; still Noop." This is catastrophic for a compute-farming scenario — a tenant running cpu_ratio=0.95 with entropy=1.5 (few unique actions) and 5× exec baseline scores 0.40, BELOW the 0.5 SilentDowngrade threshold. If this is a realistic adversarial pattern, the calibration would FAIL the ≥ 80% TP target (4 of 5 extreme abusive must hit ≥ 0.5; the compute farming pattern doesn't).

**Required fix**: Minimum n=50 benign + 50 abusive for 95% CI FP rate < 10% and TP rate within ±10%. Additionally, the weight calibration for the staging workloads should be specified as a prerequisite (weights must be optimized before claiming calibration). The self-contradicting example in §3 must be resolved: either the weights need revision to ensure compute-farming patterns score ≥ 0.5, or the calibration target must be revised to reflect achievable thresholds.

**Severity**: P0 — The sprint contract's calibration DoD criterion is not falsifiable with n=5 samples, and the inline example demonstrates a likely calibration failure. Compliance/Data Scientist sign-off on this methodology is a rubber-stamp.

---

## P1 findings

### P1-1: `retry_after_secs` computation in `check_and_consume` uses `.ceil() as u64` on partial-refill (WI-S08-001)

**Location**: WI-S08-001 §6.1 item 3, `check_and_consume` failure branch.

```rust
let needed = amount - refilled_tokens;
let retry_after_secs = (needed / self.plan_refill_rate_per_sec).ceil() as u64;
```

When `refill_rate_per_sec = 0.0` (canceled tenant, R-009 risk), division by zero produces `f64::INFINITY`, and `.ceil() as u64` on `f64::INFINITY` is **undefined behavior in Rust** — it produces `u64::MAX` (implementation-defined; the Rust reference notes that float-to-int cast with out-of-range value is saturating since Rust 1.45, yielding `u64::MAX` = 18446744073709551615). This would result in a `Retry-After: 18446744073709551615` header being emitted, which HTTP clients will misinterpret.

The WI correctly handles plan_refill_rate=0 in the narrative (§2 "Adversarial Scenarios" R-009: "refill_rate=0; all checks Exhausted; correct fail-closed behavior") but the code does not guard against division by zero.

**Fix**: Add explicit guard: `if self.plan_refill_rate_per_sec == 0.0 { return RateLimitResult { allowed: false, retry_after_seconds: Some(u64::MAX), ... }; }` BEFORE the division, or use `f64::INFINITY.min(u64::MAX as f64) as u64` with explicit `u64::MAX` semantic in the header.

### P1-2: `do_error_rate_5m` averaging in circuit breaker signal computation is mathematically suspect (WI-S08-005)

**Location**: WI-S08-005 §6.1 item 6, `evaluate_signals`:

```rust
let do_err_rate = window_5min.iter()
    .map(|o| o.do_error_rate_5m)
    .sum::<f64>() / total;
```

Each `observation.do_error_rate_5m` is already a 5-minute rolling error rate. Averaging a set of 5-minute rolling rates over a 5-minute window produces a value that double-counts observations — it is a weighted average of overlapping windows. If observations arrive every second and each carries a `do_error_rate_5m`, then summing and dividing by total observation count is essentially computing a moving average of a moving average. This does not produce a "5-minute error rate" — it produces a smoothed lag-biased estimate.

More critically: if observations arrive at high frequency (100 per minute), the sum will be dominated by the early window observations (300 observations × ~0.3 error_rate each), and a transient spike in the last 60 seconds will be diluted by historical stable values. This biases the circuit breaker toward LATE detection.

**Fix**: Use the most recent `do_error_rate_5m` observation rather than averaging all observations' `do_error_rate_5m` fields. Signal C should be: `let do_err_rate = window_5min.last().map(|o| o.do_error_rate_5m).unwrap_or(0.0);` — or if multiple recent observations are desired, average only the last N=5 observations, not the full 5-minute window.

### P1-3: `global_circuit_open` 429 SLI classification creates metric-gaming incentive (WI-S08-005)

**Location**: WI-S08-005 §1 invariant 3 + `RateLimitType::counts_against_sli()`.

`global_circuit_open` is classified as `counts_against_sli = true` (bug nosso). This is correct when the circuit trips due to real system failure. However, if the circuit is tripped via `ManualOverride` (planned maintenance, chaos drill, load shedding), the resulting 429 responses will be counted as SLI failures (bug nosso) even though they are INTENTIONAL.

During an RB-FM-250 DDoS dry-run (WI-S08-006 chaos scenario 9: "Manual override open admin drill"), ALL requests will be 429 `global_circuit_open`, all counting as SLI failures. The error budget will be consumed by a planned drill, not a real failure. The sprint contract does not specify a "planned maintenance window" exclusion for SLI computation.

**Fix**: Add `ManualOverride` to TripReason; have `counts_against_sli()` return false when `TripReason::ManualOverride`. This requires the Tower middleware to inspect the TripReason, not just the CircuitState. Alternatively, specify that planned drill windows are excluded from SLO computation (maintenance window annotation).

### P1-4: `secs_until_next_utc_midnight_1st` function undefined but critical (WI-S08-003)

**Location**: WI-S08-003 §6.1 item 5, `check_bandwidth` code.

The function `secs_until_next_utc_midnight_1st(now_ms)` is called to populate `seconds_until_reset` in `BandwidthCheckResult` and `QuotaError::BandwidthOver.reset_secs`. This function is never defined in the WI or any referenced document. Given P0-1 above (confusion between `tomorrow_at_utc_midnight()` and next-month-1st), the implementation risk for this function is high. No test covers "called on the 2nd of the month" → expected result "~29 days in seconds" vs "~1 day".

**Fix**: This function must be explicitly defined, tested, and included in the D1 `bandwidth_period_transition` property test with boundary cases: (a) called on 1st at 00:00:01 UTC → ~2.59M seconds; (b) called on 31st at 23:59:59 → ~1 second; (c) called on Feb 28 non-leap → ~1 second; (d) called on Feb 28 leap year → correctly computes to March 1.

### P1-5: CF Ruleset Terraform schema uses non-canonical `ratelimit{}` block syntax (WI-S08-002)

**Location**: WI-S08-002 §1 Terraform code block.

The Terraform resource `cloudflare_ruleset` with `ratelimit{}` block is referenced as:
```hcl
ratelimit {
  characteristics       = ["ip.src"]
  period                = 60
  requests_per_period   = 600
  mitigation_timeout    = 60
  counting_expression   = ""
}
```

The current Cloudflare Terraform provider (`cloudflare/cloudflare` v4.x+) uses `action_parameters` with embedded `overrides` rather than a top-level `ratelimit` block for the `http_ratelimit` phase. The `ratelimit {}` nested block schema shown in the WI matches the **legacy** CF provider v3 syntax. As of provider v4, the correct block is `action_parameters { ratelimit { ... } }` nested inside the rule block.

If the Terraform is applied with provider v4+ (which is standard in 2026), the configuration will fail schema validation or be silently ignored. The CF Ruleset Engine enforcement will NOT be applied, meaning camada 2 is absent at deployment with no CI-visible failure.

**Fix**: Verify provider version in `terraform.lock.hcl`; update schema to provider v4 canonical form. Add `terraform validate` + `terraform plan` output as DoD criteria for WI-S08-002.

### P1-6: Overshoot calculation in `check_and_reserve` is off-by-one (WI-S08-003)

**Location**: WI-S08-003 §6.1 item 3, `check_and_reserve` failure branch:

```rust
let overshoot = total_committed.saturating_sub(self.max_storage_bytes - 1);
```

When `total_committed == max_storage_bytes` (exact boundary, which the strict-< predicate correctly rejects), the overshoot is:
```
overshoot = max_storage_bytes - (max_storage_bytes - 1) = 1
```

This is correct — overshoot of 1 byte at exact boundary.

When `total_committed == max_storage_bytes + 5`, overshoot = `max_storage_bytes + 5 - (max_storage_bytes - 1) = 6`. This is 1 too many. The formula `total_committed - (max_storage_bytes - 1)` = `(total_committed - max_storage_bytes) + 1`. True overshoot is `total_committed - max_storage_bytes`, not `+1`.

The formula is systematically overreporting overshoot by 1 byte. In `QuotaError::StorageOver { overshoot }`, the logged overshoot will always be 1 byte higher than reality. This is a low-severity correctness issue but will cause systematic confusion in logs and runbooks.

**Fix**: `let overshoot = total_committed.saturating_sub(self.max_storage_bytes);` — the strict-< predicate already ensures `total_committed >= max_storage_bytes` in this branch, so saturating_sub won't underflow.

---

## P2 findings

### P2-1: f64 precision drift concern is understated (WI-S08-001)

The WI claims "precision ≥ 9 decimal digits sufficient for 10000 RPS × 1ms granularity." For 10000 RPS enterprise tier:
- Token count range: [0, 50000] (burst capacity)
- Refill delta per ms: 10000 / 1000 = 10.0 tokens
- f64 mantissa bits: 52 → 15-17 significant decimal digits

At 50000 tokens, the precision is `50000 / 2^52 ≈ 1.1e-11` — well within requirement. However, the concern is not precision at a given moment but **accumulated error over time**. If `last_refill_at_ms` is stored as i64 and the delta computation produces `(now_ms - last_refill_at_ms) as f64 / 1000.0`, for `last_refill_at_ms` around unix epoch ms (1.7 trillion), the difference `(1700000000000 - 1700000000001) as f64` = `-1.0 f64` exactly. No loss. This analysis is correct.

However: the WI stores `tokens: f64` in DO durable storage. DO storage serializes via JSON. `serde_json` serializes f64 with full precision, but **JSON has no native f64 semantics**—some JSON parsers lose precision for large f64 values. If `tokens = 49999.999999999996` is serialized as `"50000.0"` by a JSON rounding, the burst cap check `min(burst_capacity, ...)` would be exceeded by epsilon. This is theoretical but real in JavaScript JSON parsers that CF Workers' DO runtime may use internally.

**Risk**: Low but real for high burst enterprise tenants. Recommend serializing `tokens` as an integer (microtoken units: `tokens_microtokens: u64 = tokens * 1_000_000 as u64`) for DO storage, converting to f64 only for arithmetic.

### P2-2: Alert count inconsistency in WI-S08-006

**Location**: WI-S08-006 §2 Narrative: "SEV-2 (5): abuse admin review + PAT misuse + manual override + blocklist drift + appeals queue overflow + storage 100%."

Counting the list: 6 items, not 5. The §6.1 item 2 also says "SEV-2 (5)" but the alert YAML defines 6 SEV-2 rules (abuse-admin-review, pat-misuse, manual-override, blocklist-drift, appeals-queue-overflow, storage-100pct). The Gherkin §8 scenario "8 alert rules canonical fire correctly" says "4 SEV-1 + 5 SEV-2 + 5 SEV-3 (total 14 unique conditions; 8 distinct rules)". 4 + 6 + 5 = 15, or 4 + 5 + 5 = 14 SEV rules vs the 8 distinct rules claim. The math doesn't reconcile: there are at least 15 distinct conditions described.

**Impact**: DoD criterion "8 alert rules canonical" may not match actual implementation. Compliance review sign-off on "8 alert rules" may miss the 9th-15th conditions.

### P2-3: Weight bias toward Bazel CPU workloads (WI-S08-004)

The initial weights `cpu=0.30, egress=0.25, entropy=0.25, exec=0.20` are calibrated on the stated benign workload set which includes "typical Bazel build 200 RPS team plan" and "parallel CI at 1000 RPS business." These are CPU-heavy, low-egress, high-entropy patterns. ML model training (benign scenario 3: "egress 10 GB/min") would score `egress_norm=1.0` → contribution = 0.25×1.0 = 0.25 toward abuse detection, with no countervailing weight reduction. ML-heavy customers would be biased toward higher scores.

The WI acknowledges this risk in Persona 2 ("Heavy ML training NOT false-positive") and shows score = 0.25 (safe). But the baseline `egress_baseline_for_tier(team) ` is never specified numerically. If team-tier egress baseline = 100 MB/min, then 10 GB/min = 100× baseline → `egress_norm = min(1.0, (100-1)/10) = 0.99`, contributing 0.25×0.99 = 0.25 to the score. Across teams running intensive builds, this creates a constant 0.25 abuse score floor from egress alone — pushing tenants to SilentDowngrade at lower composite abuse.

**Risk**: Egress baseline values must be defined and validated in the `egress_baseline_for_tier` function. The WI references the function but never specifies return values. This is a completeness gap that will cause calibration failure.

### P2-4: `suggest_block` trigger condition uses `ratio_4xx == 1.0` (exact float equality) (WI-S08-002)

**Location**: WI-S08-002 §6.1 item 6: "Trigger suggest_block emit if: `rps > 10000 AND ratio_4xx == 1.0 AND duration ≥ 5min`."

`ratio_4xx == 1.0` is an exact floating-point comparison. If 9999/10000 requests are 4xx (99.99%), `ratio_4xx = 0.9999` and the trigger does NOT fire. Using exact equality on a computed float ratio is fragile. A single successful health-check or keep-alive response within the 5-minute window would suppress the suggest_block even during an active DDoS.

**Fix**: Change to `ratio_4xx >= 0.99` (99% threshold), consistent with the narrative description "100% 4xx" — which in practice means "near-100%".

---

## Cross-WI integration issues

### CI-1: Bandwidth `Retry-After` inconsistency between WI-S08-003 and WI-S08-005

WI-S08-003's `QuotaError::BandwidthOver.reset_secs` feeds into WI-S08-005's Retry-After for `over_quota (bandwidth)`:
> "over_quota (bandwidth): seconds-until-next-month-1st-UTC (chrono crate Lote 10.5bis)"

If P0-1 (wrong function) exists in WI-S08-003's `secs_until_next_utc_midnight_1st`, WI-S08-005 will propagate the wrong Retry-After to the customer. The two WIs must share a single canonical `next_month_first_utc_midnight` utility function, not duplicate the computation.

### CI-2: Silent downgrade (WI-S08-004) applies rate_cap × 0.5, but interacts with WI-S08-001 token bucket in undefined way

WI-S08-004's SilentDowngrade50pct1h tier "applies rate cap × 0.5 for 1h." The mechanism is "Tower middleware response gradient apply" in `crates/corelink-worker/src/middleware/abuse_response.rs`. But WI-S08-001's token bucket reads `plan_refill_rate_per_sec` from DO state. The downgrade must either:
(a) Modify the DO token bucket state (but WI-S08-001 owns the DO; WI-S08-004 cannot write to it without cross-WI coupling); OR
(b) Apply at Tower middleware layer BEFORE calling WI-S08-001 (by halving the `amount` parameter, not the cap); OR
(c) Use a separate downgrade flag in the Quota DO.

None of the 6 WIs define who owns the downgrade state or how it is propagated to WI-S08-001's `check_and_consume`. The cross-WI integration test in WI-S08-006 ("Abuse silent-downgrade reduces rate cap by 50%") requires this interaction to be specified. It is currently unspecified.

### CI-3: `global_circuit_open` 429 from WI-S08-005 — what Retry-After does WI-S08-003 quota middleware return?

WI-S08-003's Tower middleware `quota_layer` runs BEFORE WI-S08-005's circuit breaker wrapper (logically — quota check is inner; circuit check is outer). When the circuit is open, WI-S08-005's middleware short-circuits ALL requests with 429. This means WI-S08-003's `check_storage_and_reserve` DOES NOT run during circuit-open, leaving active reservations orphaned without a companion `release_storage_reservation`. The DO alarm sweep will eventually release them (TTL), but during a circuit-open event lasting > 60s, orphaned reservations inflate `bytes_used + active_reserved` and could cause spurious over-quota rejections when the circuit closes and requests resume.

---

## Methodological strengths

1. **CF Workers Rust runtime API discipline**: All 6 WIs explicitly state `worker::send_future()` canonical; `tokio::spawn` explicitly anti-scoped. Consistent absorption of Lote 10.7bis R5 P0-3 lesson.

2. **DO actor race-correctness**: The per-tenant DO model correctly eliminates CAS race conditions for storage quota and rate limiting. The strict-< predicate in WI-S08-003 is the correct boundary predicate (per S-06 INV-GC-004 lesson absorption).

3. **Humane LGPD Art. 20 response**: Three-endpoint transparency pattern (score inspection + appeal + audit) is genuinely compliant with the spirit of LGPD Art. 20. `AutoSuspendForbidden` at the type level is a defensible enforcement mechanism.

4. **Multi-signal circuit breaker**: Requiring ≥ 2 signals for trip is correct mitigation for single-metric false-positives. The threshold values (5xx > 0.5, p99 > 5×SLO, DO error > 0.3) are operationally reasonable.

5. **5-tier canonical absorption**: All 6 WIs consistently reference the 5-tier canonical from Lote 10.7bis P0-7. Cross-WI consistency is maintained for tier vocabulary.

6. **Audit fail-closed pattern**: Consistent across all 6 WIs. Lote 10.6bis lesson well-absorbed.

7. **CHECK constraints inline per Lote 10.5bis**: All D1 migrations verified to use `CHECK` constraints inline in `CREATE TABLE`, not `ALTER TABLE ADD CONSTRAINT`. Correctly absorbs D1/SQLite limitation.

---

## Methodological gaps (Sonnet lens)

### Gap 1: No property test for `next_month_first_utc_midnight` function correctness
The Lote 10.5bis lesson warns about chrono pitfalls. WI-S08-003's `prop_bandwidth_period_transition` tests "simulate 1st-UTC-midnight crossing at random ms offsets; assert deterministic reset" — but the crossing detection uses string comparison `"YYYY-MM"`. The test does not verify that the DO alarm's scheduled wake-up time is correctly `next_month_first_utc_midnight`, only that period transitions are detected. The alarm scheduling correctness is untested.

### Gap 2: No adversarial weight-gaming detection beyond threshold observation
The WI-S08-004 §2 ("Adversarial game-theoretic: adversary just-below-threshold attack vector") acknowledges the gaming risk. The stated mitigation ("rotating weights via admin S-13") is deferred S-13 and not present in S-08. For S-08, an adversary who learns the published weights (which are DISCLOSED in the transparency endpoint `AbuseScoreBreakdown.feature_weights`) can trivially compute the exact threshold combination and stay below 0.5. No randomization or obfuscation is available in the S-08 scope. This is not a new finding (the WI itself notes it), but the lack of a concrete S-08 mitigation beyond "it's tunable later" is a gap.

### Gap 3: HalfOpen sampling determinism via `observation_id % 10` is not uniform under adversarial conditions
WI-S08-005 §6.1 item 8:
```rust
(observation_id % 10) < (sample_rate * 10.0) as u64
```
If `observation_id` is sequential (as implied by its name), this gives exactly 1-in-10 deterministic sampling. However, if requests are batched (multiple requests arrive in the same DO actor call or have the same timestamp-derived ID), the modulo could be biased toward certain values. More importantly, the WI does not specify what `observation_id` is — a sequential counter, a hash of request ID, or timestamp-derived. If it is sequential per-DO and the DO resets on cold start, the first 10 requests after a cold start would all be in the same deterministic slots, potentially allowing an adversary to predict which requests will be sampled and craft only those requests to appear healthy.

### Gap 4: `abort_response_actions.expires_at` CHECK constraint forbids NULL, but `suspend_candidate` tier has no time-bound
WI-S08-004's `abuse_response_actions` table has:
```sql
expires_at INTEGER NOT NULL,
CHECK (expires_at > applied_at)
```
For `SilentDowngrade50pct1h`, `expires_at = applied_at + 3600000`. But for `AdminReviewTriggerSev2` and `SuspendCandidateHumanReviewOnly`, there is no natural expiry — these are human-review-required states without a time-bound. The `NOT NULL` constraint forces some value, but what? The WI doesn't specify. An arbitrary large value (e.g., applied_at + 365 days) would pass the CHECK but is semantically incorrect for "pending human review." Either:
(a) `expires_at` should be NULLABLE for admin-review states; or
(b) A sentinel value convention must be documented.

### Gap 5: PRR sign-off list inconsistency — Compliance/Privacy marked "mandatory" but ADR-0034 shows Option A waivable
WI-S08-001 §30: "Compliance — mandatory — LGPD Art. 20 (decisões automatizadas) advisory" but the sign-off table lists it as pending with no waiver noted. ADR-0034's decision matrix shows Compliance is Option A waivable (Architect compensation). However, LGPD Art. 20 and GDPR Art. 22 compliance review for automated decision-making (abuse detection, rate limiting blocking) is arguably a mandatory role that should NOT be Option A waivable for WI-S08-004, given the explicit "mandatory emphatic" designation.

The ADR-0034 decision matrix has "Compliance — Option A allowed: yes; Option B: n/a; Option C: yes; Non-waivable: no." But for WI-S08-004 with LGPD Art. 20 significance, marking Compliance as "not non-waivable" creates a path where the LGPD review never happens: Architect signs off on a rate-limiting + abuse-detection system that makes automated decisions without a Compliance officer reviewing it. This may be legally insufficient under LGPD Art. 20(1) which requires the data controller to ensure review by a competent person.

---

## Verdict

**APPROVED CONDITIONALLY** — with the following mandatory pre-SEAL conditions:

**Must fix before implementation begins (P0):**
1. **P0-1**: Replace all `tomorrow_at_utc_midnight()` references in bandwidth period reset and Retry-After computation with a correctly implemented `next_month_first_utc_midnight()` function. Add explicit property test coverage for boundary months (February, December→January).
2. **P0-2**: Clarify CAP-RATE-003 (per-PAT camada 3) as misuse DETECTOR, not rate enforcer. Remove "enforcement camada" framing OR change formula to `pat_cap = tenant_refill / expected_PATs_count` for true enforcement. Update sprint contract §5 R-S08-3 accordingly.
3. **P0-3**: Increase calibration sample size to minimum n=50 benign + 50 abusive workloads OR add confidence interval requirement to DoD criterion. Resolve the inline Persona 3 self-contradiction showing compute-farming at score=0.40 (below 0.5 threshold) — either adjust weights or document that this scenario is intentionally "noop" (acceptable).

**Must fix before PRR sign-off (P1):**
4. Guard division-by-zero in `retry_after_secs` for `refill_rate=0` (P1-1).
5. Fix `do_error_rate_5m` averaging methodology in circuit breaker (P1-2).
6. Add `ManualOverride` TripReason exclusion from `counts_against_sli()` (P1-3).
7. Define and test `secs_until_next_utc_midnight_1st` explicitly (P1-4).
8. Verify CF Terraform provider version and update `ratelimit{}` schema to provider v4+ (P1-5).
9. Fix overshoot formula off-by-one (P1-6).

**Cross-WI integration must be resolved:**
10. CI-1: Share single canonical `next_month_first_utc_midnight` utility between WI-S08-003 and WI-S08-005.
11. CI-2: Define the mechanism by which WI-S08-004 SilentDowngrade communicates rate reduction to WI-S08-001 token bucket. This is currently architecturally unspecified.
12. CI-3: Document orphaned reservation behavior during circuit-open events.
