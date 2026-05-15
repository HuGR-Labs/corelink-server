---
id: "AUDIT-R5-SONNET-S09-PART1"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "FRAMEWORK-00"
  - "OBSERVABILITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "PRIVACY-MODEL"
tags: ["audit", "review", "r5", "sonnet", "s09", "observability", "testability", "lote-10.9", "part1", "wi-001-to-004"]
---

# R5 (Sonnet) — S-09 Testability + Validator Coverage Review · Part 1 (WI-S09-001..004)

> **Reviewer persona:** R5 — Sonnet testability + structural-validator
> reviewer. Focus: property test density, structural validators, chaos-test
> framework, synthetic canary coverage, alert flapping detector calibration,
> falsifiability targets.
> **Scope:** S-09 WI-001..004
> **Companion:** `_review_R5_sonnet_part2.md` covers WIs 5..7
> **Cross-cut:** R4 Opus architectural review in `_review_R4_opus_part{1,2}.md`
> **Framework citations:** Framework §31 (testability), `00_framework.md §32`
> (proptest density gate), property-based testing canonical pattern (Hughes
> 2000 + QuickCheck).

---

## 0. Executive summary

**Testability aggregate (WI-001..004): 8.7 / 10.** Above SOTA-acceptance bar
of 8.0. The S-09 property-test corpus is dense and *load-bearing* — 81 +
77 + 77 + 83 = 318 tests across the 4 crates, with cumulative ~60k+ iter
across the property suites. The structural validators (`MetricLabelTuple`
enum-typed cartesian, `LogEventType` `#[non_exhaustive]` taxonomy, `SpanKind`
`#[non_exhaustive]`, `AuditEventKind` `#[non_exhaustive]`) make forbidden
states **not representable by construction** — exactly the discipline R5
audits for.

Two structural validators not in WI scope but flagged by the orchestrator
in this review's instructions are surfaced explicitly as P1 findings:

1. **P1 — INV drift triple in WI-005-12 dashboards.** Three INV labels in
   dashboard JSON have no canonical `invariant_registry.md §3` row:
   `INV-CAS-DIGEST-INTEGRITY` / `INV-EXEC-IDEMPOTENT` /
   `INV-LGPD-AUTO-SUSPEND-FORBIDDEN`. `scripts/validate_inv_promotion.py`
   flags this as drift. Either silenced (rubber-stamp) or unstated. **R5
   remediation:** rename `INV-CAS-DIGEST-INTEGRITY` → `INV-CAS-INTEGRITY`
   (alias drift; §3.1 row already exists; CRITICAL severity per registry);
   promote `INV-EXEC-IDEMPOTENT` to §3.12 as S-09 row (HIGH); promote
   `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` to §3.12 as S-09 row (HIGH; LGPD Art.
   20 + GDPR Art. 22 anti-automated-decision invariant). See §3 P1-S1.
2. **P1 — `pii_redaction_100k_synthetic_zero_leakage` Wilson 95% CI bound
   is not falsifiable at the right granularity.** The 100k synthetic gate
   reports "ZERO PII leakage; Wilson 95% CI upper-bound leak rate <
   0.0037%". The Wilson upper-bound is computed across 100k samples — but
   the 5 pattern categories (email / ip / token / pan / cpf_cnpj) each get
   20k samples. The per-category Wilson upper-bound is ~0.018% (4× looser).
   A leak rate of 0.01% in the CPF/CNPJ category specifically would NOT
   trigger the gate at the aggregate level (5× dilution) but IS a regression
   at the category level. Need per-category Wilson bound + per-category
   acceptance.

If the dev applies all P1 findings, projected score: **9.3 / 10.**

## 1. Per-WI testability scores

| WI | Score | Property test density | Structural validator | Falsifiability target |
|---|---|---|---|---|
| **WI-S09-001** Worker Analytics + RED + cardinality validator | **9.1 / 10** | 81 tests; `prop_cardinality_budget_enforced` 10k iter PR / 100k iter nightly; `prop_label_cartesian_closure` (implicit via type-check) | `MetricLabelTuple` enum-typed cartesian (no `String` slot); `RedMetricKind` `#[non_exhaustive]` 9-RED + 6-USE; `FORBIDDEN_LABEL_NAMES` const for CI lint | INV-OBS-CARDINALITY-BUDGET HIGH — falsifiability target ≥ 100k iter without violation |
| **WI-S09-002** Logpush + Loki + PII redaction | **9.0 / 10** | 77 tests; `prop_pii_redaction_no_leakage` 10k iter PR + `pii_redaction_100k_synthetic_zero_leakage` deterministic seeded ChaCha20Rng 100k zero-leak gate; Luhn-validated PAN; Brazilian CPF/CNPJ mod-11 | `LogEventType` `#[non_exhaustive]` 4-event taxonomy; `PiiPatternKind` `#[non_exhaustive]` 5-canonical taxonomy; 6 canonical placeholders | CTRL-PRIV-001 — Wilson 95% CI upper-bound leak rate < 0.0037% |
| **WI-S09-003** OTLP tracing + W3C + sampling + exemplars | **8.5 / 10** | 77 tests; `prop_traceparent_rejects_invalid` 10k iter; `prop_zero_trace_id_rejected` 10k iter; `prop_sampler_rate_proportional` deterministic seeded ChaCha20Rng sweep; `prop_exemplar_link_to_metric` 10k iter | `SpanKind` `#[non_exhaustive]` 6-canonical; W3C Trace Context Recommendation 2020 strict ABNF compliance; 16-byte trace_id + 8-byte span_id + 1-byte flags shape | W3C ABNF reject discipline + zero-trace_id rejection + sampler determinism |
| **WI-S09-004** CloudEvents audit + R2 hash chain + verifier | **8.2 / 10** | 83 tests; `prop_jcs_canonicalization_deterministic` 10k iter; `prop_chain_break_detected_on_tamper` 10k iter; `prop_chain_verify_passes_on_unmodified` 10k iter | `AuditEventKind` `#[non_exhaustive]` 8-canonical CNCF subjects; `specversion: "1.0"` hard-pin; BLAKE3-256 link-hash | INV-OBS-AUDIT-CHAIN-INTEGRITY HIGH — RFC 8785 JCS canonical determinism + chain-break detection |

**Aggregate (mean): 8.70 / 10.**

## 2. P0 findings (testability-blocking)

**None.** The 318-test corpus + 60k+ iter property suite is dense enough to
satisfy framework §32 proptest density gate. No falsifiability target is
missing at the must-fix level.

## 3. P1 findings

### P1-S1 — INV reference drift triple in WI-S09-005 dashboard JSON [REGISTRY-CANONICALITY]

**Severity:** P1 — `scripts/validate_inv_promotion.py` flagged drift; either
silenced or unstated. R5 audits the **validator chain**; this is squarely
in R5 scope.

**Defects (surfaced via grep over `dashboards/grafana/*.json`):**

| Drifted INV label | Where | Canonical registry row | Remediation |
|---|---|---|---|
| `INV-CAS-DIGEST-INTEGRITY` | `DASH-CAS.json:307` (panel title), `DASH-CAS.json:310` (description) | **`INV-CAS-INTEGRITY`** (§3.1; CRITICAL; TLA+ `cas_integrity.tla` GREEN) | **Rename in dashboard JSON.** Alias drift — the canonical row exists; the dashboard panel is the regression. Add a `validate_dashboards.py` lint rejecting any `INV-*` substring not matching a registered §3 row ID. |
| `INV-EXEC-IDEMPOTENT` | `DASH-EXEC.json:288` (panel title), `DASH-EXEC.json:291` (description) | **NONE** — no canonical row | **Promote to §3.12 (S-09 row).** Severity HIGH; enforce via S-17 execution race detector. The dashboard panel description ("Same action_digest executed concurrently > 1 = race; > 0 sustained = SEV-2") is the runtime expression; the registry must catch up. |
| `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` | `DASH-SECURITY.json:219` (panel title), `DASH-TENANT.json:305` (description) | **NONE** — no canonical row | **Promote to §3.12 (S-09 row).** Severity HIGH; LGPD Art. 20 + GDPR Art. 22 anti-automated-decision invariant; enforce via S-11 / S-13 abuse heuristic gating with mandatory human-review carve-out. |

**Falsifiability target gap:** the `validate_inv_promotion.py` script must be
green to promote WI-S09-005-12 from REVIEWING → DONE. PRR-S09 §10 claims
green. The artifact says drift. Resolve before next SEAL ceremony or restate
the script's expected output.

**R5 remediation proposal (concrete):**

```yaml
# Lote 10.9bis remediation WI:
remediation:
  - file: dashboards/grafana/DASH-CAS.json
    action: rename
    from: INV-CAS-DIGEST-INTEGRITY
    to: INV-CAS-INTEGRITY
  - file: specs/03_architecture/invariant_registry.md
    action: promote
    new_inv: INV-EXEC-IDEMPOTENT
    section: §3.12
    severity: HIGH
    enforce: pending (S-17 forward)
  - file: specs/03_architecture/invariant_registry.md
    action: promote
    new_inv: INV-LGPD-AUTO-SUSPEND-FORBIDDEN
    section: §3.12
    severity: HIGH
    enforce: S-11 / S-13 abuse heuristic gating
  - file: scripts/validate_dashboards.py
    action: extend
    add_lint: reject any INV-* substring not in invariant_registry.md §3 row IDs
```

### P1-S2 — `pii_redaction_100k_synthetic_zero_leakage` per-category Wilson bound missing

**Severity:** P1 — the aggregate Wilson 95% CI upper-bound across 100k
samples dilutes per-category bounds 5×. A regression in one category (e.g.
CPF/CNPJ scanner mod-11 logic) could pass the aggregate gate but break the
per-category guarantee.

**Defect:** WI-S09-002 §6.1 `pii_redaction_100k_synthetic_zero_leakage`
splits 5 categories × 20k samples = 100k. Wilson upper-bound is reported
at the aggregate level (< 0.0037%). At the per-category level, the bound
is ~5× wider (~0.018%). A category-specific regression of 0.005% leak rate
(1 leak in 20k samples per category) would:
- PASS the aggregate gate (1 leak in 100k = 0.001%, well below 0.0037%)
- FAIL the per-category guarantee (1 leak in 20k = 0.005%, above the
  aggregate threshold)

This is the **dilution-by-aggregation** anti-pattern in statistical-gate
design.

**Concrete remediation:**

1. Refactor the gate to compute Wilson 95% CI per-category AND aggregate.
2. Per-category acceptance: zero leaks per category over 20k samples (or
   per-category Wilson upper-bound < 0.005% per CTRL-PRIV-001 strict
   interpretation).
3. Add `prop_pii_redaction_per_category_zero_leakage` 10k iter per category
   = 50k cumulative iter (separate from the 100k aggregate gate).

### P1-S3 — Property tests pin Rust-code intent, but YAML / JSON artifacts have no equivalent gate

**Severity:** P1 — load-bearing for "validator chain green" claim.

**Defect:** S-09 ships several YAML / JSON artifacts:
- `dashboards/grafana/*.json` (12 files)
- `dashboards/alerts/dash-slo-multi-burn.yml` (35 rules)
- `specs/_schemas/log_event.schema.json`

The Rust property tests pin the *Rust code* discipline. The YAML/JSON
artifacts are validated by `validate_dashboards.py` (structural) and (per
PRR-S09) `promtool test rules` (deferred). The structural validators check
*parsability + count + tags*, NOT *semantic discipline* (e.g. tenant
exclusion clause in SLI recording rules — see Part 2 P1-D).

**Concrete remediation (cross-references Part 2 P1-D):**

1. Extend `validate_dashboards.py` with a semantic-lint pass: panel titles
   must reference registered INV/SLI/SLO/dashboard-canonical IDs.
2. Add `validate_alert_rules.py` (new file) with the canary-tenant
   exclusion clause check + multi-burn-rate window canonical (5min/30min/
   1h/6h/24h/3d) shape check.
3. Add `validate_log_schema.py` (new file) with the forbidden-payload-field
   list check (no `email`, no `ip` without redaction, no `bearer_token`,
   no `blob_digest` > 16 chars).

### P1-S4 — Chaos test framework documented but not executable at SEAL

**Severity:** P1 — the 12 chaos scenarios catalogued in
`specs/_audits/2026-05-03-adversarial-s09.md` are a *narrative* artifact,
not an *executable* test suite.

**Defect:** PRR-S09 §6 cites "chaos suite 12 scenarios catalogued in
`specs/_audits/2026-05-03-adversarial-s09.md`". The 12 scenarios are
documented (catastrophic cardinality bomb, PII leak via raw bearer token,
hash chain break via canonical-bytes mutation, etc.) — each scenario has
an "Outcome" section claiming "structurally impossible" or pointing to a
property test. The chaos test framework that *executes* these scenarios end-
to-end (e.g. inject a malformed `MetricLabelTuple` via `unsafe` cast and
verify panic? or inject a tampered `prev_hash` and verify SEV-0 fires?) is
not in the WI scope.

**Concrete remediation:**

1. Add a `crates/corelink-chaos/` crate (or extend an existing one) with
   the 12 chaos scenarios as runnable tests.
2. Each chaos scenario should have:
   - A "happy path" baseline test (existing behavior).
   - An "adversarial path" injection that flips one input and asserts the
     defense fires (SEV-0 emit, INV violation count incremented, etc.).
3. Wire to `nightly.yml::chaos-suite` matrix; 10k iter PR / 100k iter
   nightly.

## 4. P2 findings

### P2-S1 — `prop_sampler_rate_proportional` deterministic seeded ChaCha20Rng

WI-S09-003 §6.1 ships `prop_sampler_rate_proportional` using deterministic
seeded ChaCha20Rng. The seed value is hardcoded; if the seed produces a
pathologically unbalanced sample, the test passes deterministically. Suggest
running the test across **multiple seeds** (e.g. 16 canonical seeds) and
asserting the rate is proportional within ±0.5% per seed.

### P2-S2 — `prop_chain_break_detected_on_tamper` coverage of tampering location

`prop_chain_break_detected_on_tamper` 10k iter pins detection at the
*verifier* boundary. The property tests tamper with a single event in the
middle of the chain. The verifier walks from genesis to most-recent (per
WI-004 §1). Suggest extending the test to tamper at:
- The genesis event (first link)
- The most-recent event (last link)
- Two adjacent events (cascading tamper)
- A random position with chain length 1, 2, 100, 10000

Each tampering location exercises a different code path in the walker.

### P2-S3 — `prop_traceparent_rejects_invalid` allowed-character class

WI-S09-003 §6.1 `prop_traceparent_rejects_invalid` ships strict ABNF
compliance per W3C Trace Context Recommendation 2020. The ABNF in the W3C
spec includes specific allowed characters; suggest adding category-specific
rejection tests:
- Reject non-hex characters (`G`-`Z`, `g`-`z`)
- Reject incorrect length (15 or 17 bytes for trace_id; 7 or 9 for span_id)
- Reject `00` flags + non-zero-randomness combination (edge case in v2 of
  the spec)

### P2-S4 — `prop_jcs_canonicalization_deterministic` adversarial input space

`prop_jcs_canonicalization_deterministic` 10k iter pins serde_jcs 0.2
canonical determinism. The adversarial input space is implicitly the
property-test default (proptest std generators). Suggest expanding:
- Unicode normalization edge cases (NFC vs NFD vs NFKC)
- Number serialization edge cases (0.0 vs -0.0; NaN; ±Infinity; very
  large / very small doubles)
- Key ordering edge cases (empty keys; keys differing only in case;
  keys with embedded whitespace)

These are the canonical RFC 8785 §3 failure modes.

## 5. P3 findings

### P3-S1 — Property test naming convention drift

The S-09 corpus uses `prop_*` prefix consistently. A few legacy tests in
the test fixtures use `test_*`. Suggest a `cargo-spellcheck`-style lint:
all tests in `tests/property/` directories must use the `prop_*` prefix;
all `test_*` tests in property directories must be renamed or moved.

### P3-S2 — `#[non_exhaustive]` discipline cross-validation

The 4 WIs ship `#[non_exhaustive]` on enums (RedMetricKind, LogEventType,
PiiPatternKind, SpanKind, AuditEventKind). Suggest a CI lint that scans
for `#[non_exhaustive]` on every canonical-taxonomy enum + verifies the
match arms in dependent crates use `_ => unreachable!()` or
`_ => return Err(...)` (NOT panicking silently).

### P3-S3 — `prop_pii_redaction_no_leakage` shrinking minimization

proptest shrinking on the PII redaction property test will reduce a
failing input to a minimal counterexample. The current test does not
report the shrunk counterexample with sufficient context (which pattern?
which byte offset? what was the redaction output?). Suggest adding a
proptest config with `verbose: true` + a custom shrink-output formatter
that emits the (pattern, byte_offset, redaction_output) tuple.

## 6. Cross-WI structural validator coverage

| Validator | Layer | Covers | Gap |
|---|---|---|---|
| `MetricLabelTuple` enum-typed cartesian | Rust type | INV-OBS-CARDINALITY-BUDGET via forbidden-label closure | Per-isolate ledger growth (Part 1 P1-2 R4) |
| `RedMetricKind` `#[non_exhaustive]` | Rust type | 9-RED + 6-USE canonical metric kinds | Forward-compat path for new metric kinds — `_ => unreachable!()` discipline at consumer side |
| `LogEventType` `#[non_exhaustive]` | Rust type | 4-event canonical log event taxonomy | Pattern-specific Wilson bound (P1-S2) |
| `PiiPatternKind` `#[non_exhaustive]` | Rust type | 5-pattern PII taxonomy | Match-precedence canonical order documented (R4 Part 1 P3-3) |
| `SpanKind` `#[non_exhaustive]` | Rust type | 6-canonical SpanKind | Tail-sampling decision shell (R4 Part 1 P2-3) |
| `AuditEventKind` `#[non_exhaustive]` | Rust type | 8-canonical CNCF subjects | `specversion: "1.0"` forward-compat path (R4 Part 1 P3-2) |
| `validate_dashboards.py` | YAML/JSON structural | 12-count + panel ≥ 8 + tags + lastUpdated | INV reference drift (P1-S1); recency lint (R4 Part 2 P2-C); AdminCtx-RBAC ACL (R4 Part 2 P1-A) |
| `validate_inv_promotion.py` | Cross-WI | WI-declared INV ↔ registry §3 row alignment | **FAIL** — 3 drifted INV labels (P1-S1) |
| `pii_redaction_100k_synthetic_zero_leakage` | Statistical gate | CTRL-PRIV-001 enforcement | Per-category Wilson bound (P1-S2) |
| Chaos suite | Narrative + property tests | 12 scenarios catalogued in audit | Not executable at SEAL (P1-S4) |

## 7. Recommendation

**CONDITIONAL APPROVE for testability** for WIs 001..004 → STAGING-STABLE
*contingent* on:

- **P1-S1** (INV drift triple) MUST land in Lote 10.9bis remediation wave
  before any S-09 → S-10 unblocking — the validator chain claim must be
  green;
- **P1-S2** (per-category Wilson bound) MUST land before S-20 GA (CTRL-PRIV-
  001 is a forcing factor; the dilution-by-aggregation pattern undermines
  the falsifiability target);
- P1-S3 (YAML/JSON semantic lint) + P1-S4 (chaos suite executable) MAY land
  in subsequent waves with explicit time-bound (≤ 4 weeks for P1-S3; ≤ 8
  weeks for P1-S4).

Per Lote 10.9 review charter: this report is an AUDIT artifact (`type:
audit`, `doc_status: REVIEW`) and does NOT modify WI-S09-001..004 doc_status
(which remain FROZEN per the original SEAL). Remediation lands as a separate
Lote 10.9bis WI.

**Calibrated praise:** the 318-test corpus + structural-validator discipline
in WIs 001..004 is the **strongest testability foundation** I've reviewed
in the S-09 corpus to date. The `MetricLabelTuple` "forbidden states not
representable by construction" pattern is the gold standard for cardinality
discipline. The Wilson 95% CI gate is the right falsifiability target for
CTRL-PRIV-001 — the only critique is the dilution-by-aggregation (P1-S2),
which is fixable in a small Lote.

---

**End R5 Sonnet Part 1 v1.0.0.**

---

## Closure footnote (Lote 10.9bis wave 17 — 2026-05-15)

**P1-S1 INV drift triple — CLOSED.** Remediation matrix applied (reciprocal of R4-P1-1):

- `INV-CAS-DIGEST-INTEGRITY` renamed to canonical `INV-CAS-INTEGRITY` (alias drift fixed in `dashboards/grafana/DASH-CAS.json` panel 10 + WI-S09-005-12 SEAL row).
- `INV-EXEC-IDEMPOTENT` promoted to `invariant_registry.md §3.12` (S-09 row, HIGH; runtime enforcement S-17 PLANNED).
- `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` promoted to `invariant_registry.md §3.12` (S-09 row, HIGH; LGPD Art. 20 + GDPR Art. 22).

`validate_inv_promotion.py` drift 3 → 0; `validate_specs.py`, `validate_references.py`, `validate_canonical_consistency.py`, `validate_dashboards.py` all exit 0.
