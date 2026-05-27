---
id: "AUDIT-R4-OPUS-S09-PART1"
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
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
tags: ["audit", "review", "r4", "opus", "s09", "observability", "adversarial", "lote-10.9", "part1", "wi-001-to-004"]
---

# R4 (Opus) — S-09 Adversarial Review · Part 1 (WI-S09-001..004)

> **Reviewer persona:** R4 — Opus 4.7 1M-context architectural reviewer
> **Scope:** S-09 work items 1..4 (Worker Analytics Engine, Logpush/PII redaction,
> OTLP tracing, CloudEvents audit chain)
> **Companion:** `_review_R4_opus_part2.md` covers WIs 5..7
> **Cross-cut:** Sonnet-persona testability review in `_review_R5_sonnet_part{1,2}.md`
> **Framework citations:** Google SRE Workbook Ch 5 (multi-burn-rate), Ch 6
> (sampling), Ch 8 (alert discipline); W3C Trace Context Recommendation 2020;
> OpenMetrics 1.0 §exemplars; CloudEvents 1.0; RFC 8785 JCS; CTRL-PRIV-001;
> INV-OBS-CARDINALITY-BUDGET; INV-OBS-AUDIT-CHAIN-INTEGRITY.

---

## 0. Executive summary

**Aggregate SOTA score (WI-001..004): 8.6 / 10.** Above the 8.0 post-bis envelope
and within touching distance of the 9.0 SOTA bar the user has set for HIGH_RISK
shipping waves. The four "fundamentals" WIs (metrics, logs, traces, audit) are
each individually defensible: the cartesian closure over `MetricLabelTuple` in
WI-001 is the *single best* cardinality-discipline pattern I've reviewed in the
CoreLink corpus; the 5-pattern hand-rolled PII scanner + 100k Wilson-CI gate in
WI-002 is the right answer to CTRL-PRIV-001 falsifiability; the W3C ABNF strict
parser in WI-003 closes the cardinality-via-trace_id attack surface
*structurally*; the RFC 8785 JCS + BLAKE3 link-hash chain in WI-004 is correct
construction for SOC 2 CC7.2.

The residual gaps are NOT first-principles defects; they are *integration-edge*
and *provisioning-edge* findings — exactly what one expects when six trait
surfaces ship behind `trait-abstraction-defer` charter pattern. The headline
P0/P1 cluster is:

1. **P1 — INV reference drift in DASH-CAS / DASH-EXEC / DASH-SECURITY.** Three
   INV labels appear in dashboard panel descriptions that have no canonical
   `invariant_registry.md §3` row: `INV-CAS-DIGEST-INTEGRITY`,
   `INV-EXEC-IDEMPOTENT`, `INV-LGPD-AUTO-SUSPEND-FORBIDDEN`. Surfaced by
   `scripts/validate_inv_promotion.py` cross-check against WI-S09-005-12. This
   is a **registry-canonicality** defect — `INV-CAS-DIGEST-INTEGRITY` is almost
   certainly a drift alias for the canonical `INV-CAS-INTEGRITY` (registry
   §3.1, CRITICAL, TLA+ `cas_integrity.tla` GREEN), and the other two need
   §3.12 row additions. See §3 below for the concrete remediation matrix.
2. **P1 — `validator/` cardinality ledger pruning policy is unspecified.**
   WI-001 §6.1 `CardinalityValidator` ships a per-instance `Arc<Mutex<HashMap<
   RedMetricKind, HashSet<MetricLabelTuple>>>>` ledger but the WI does not
   specify when the ledger garbage-collects. Over a 72h sustained canary loop
   (12_960 loops/72h per WI-007) the ledger will grow monotonically against
   the `region × tier` cartesian; for a long-running CF Worker the same
   per-instance ledger accumulates over the Worker lifetime (24h+ on warm
   isolates). The 20k per-metric bound is a CEILING, not a GC threshold —
   without a sliding-window / TTL eviction policy on the ledger itself the
   *enforcement primitive* becomes the cardinality leak.
3. **P1 — Exemplar deep-link integrity claim is host-side-only.** WI-003 §1
   asserts `corelink-tracing::Exemplar` linkage to `corelink_analytics::
   RedMetricKind` per OpenMetrics 1.0 §exemplars + CAP-OBS-009. The
   `prop_exemplar_link_to_metric` 10k iter pins serialize round-trip — but
   "click in Grafana opens Tempo trace" requires (a) Mimir tenant config
   accepting OpenMetrics exemplars (Mimir supports as of 2.10 but tenant flag
   `exemplars: true` must be set), (b) Grafana datasource exemplar-link
   wired to Tempo, (c) Tempo tenant accepts OTLP. None of these three live
   surfaces is verified at SEAL — all three are `trait-abstraction-defer`
   shells. The DoD §4 row "Tracing: traces W3C-compliant + exemplars
   funcionando (click em Grafana abre Tempo)" is therefore marked ✅
   (host-side) but the *user-observable* claim is unverified end-to-end.
4. **P2 — Audit emit fail-CLOSED envelope ↔ daily-verifier interaction
   undocumented.** WI-004 ships `audit emit fail-CLOSED` (transaction abort
   on emit failure) AND a daily `ChainVerifier` that fails-CLOSED on first
   mismatch. The composition rule is unstated: if the daily verifier fires
   `chain_break_detected` SEV-0, does the verifier itself emit an audit event
   on the broken chain — and if so, fail-CLOSED would prevent the SEV-0
   notification from landing. This is a circular-fail-CLOSED hazard pattern.
   Need an explicit ADR-class carve-out: verifier audit events go to a
   *secondary* sink with weaker integrity guarantees, OR the verifier emits
   directly to PagerDuty/Slack bypassing the audit chain.

If the dev applies all 4 P1/P2 findings (Lote 10.9bis remediation), projected
score: **9.1 / 10** — within the SOTA bar. The remaining ~1 point gap is
forward-debt for the trait-abstraction-defer surfaces (real CF binding chain),
which is appropriately deferred to staging account provisioning per
spec contract §6 partial-bullet pattern.

## 1. Per-WI scores

| WI | Score | Strongest aspect | Headline weakness |
|---|---|---|---|
| **WI-S09-001** Worker Analytics + RED + cardinality validator | **9.0 / 10** | `MetricLabelTuple` enum-typed cartesian struct with NO `String` slot — forbidden labels NOT representable by construction; cumulative 60k+ iter prop suite | **P1 ledger pruning policy unspecified**; production `cardinality_check.py` static analyzer deferred |
| **WI-S09-002** Logpush + Loki + PII redaction | **8.8 / 10** | 5-pattern hand-rolled byte-level scanner + Luhn PAN + Brazilian CPF/CNPJ mod-11 + 100k synthetic Wilson 95% CI < 0.0037% leak rate | **P2 R2 lifecycle `single-expiration 400d`** (Lote 10.9-quaters NEW-P0-3) collapses to single tier — Loki 30d hot is query-tier only; effective storage retention is uniform; need explicit ADR carve-out for legal hold + chain-of-custody overlap with CTRL-AUDIT-001 7y window |
| **WI-S09-003** OTLP tracing + W3C + sampling + exemplars | **8.5 / 10** | W3C Trace Context Recommendation 2020 strict ABNF; `prop_traceparent_rejects_invalid` + `prop_zero_trace_id_rejected` 10k iter; deterministic head-sampler per W3C trace_id last-8-bytes-as-u64 ratio | **P1 exemplar deep-link** is host-side-only; tail-based 100% on errors is documented but no `trait-abstraction-defer` shell for tail decision (sampling §1 says "head-based deterministic" only) — claim ↔ shell mismatch |
| **WI-S09-004** CloudEvents audit + R2 hash chain + verifier | **8.4 / 10** | RFC 8785 JCS via serde_jcs canonical determinism + BLAKE3-256 link-hash Bitcoin-block-header pattern; ChainVerifier fail-CLOSED on first mismatch + canonical `corelink.audit_chain.chain_break_detected` SEV-0 emit | **P2 circular fail-CLOSED** envelope on verifier path (see §0 finding 4); per-region chain partition correctly prevents split-brain but cross-region SIEM fan-out via CF Queue (sprint §5.4 R-S09-11) ships as InMemoryQueue fake only |

**Aggregate (mean): 8.675 / 10.** Rounded headline: **8.7 / 10**.

## 2. P0 Findings (must-fix Lote 10.9bis)

**None.** No catastrophic-if-deployed defects in WIs 001..004. The closest
candidate (P1-1 INV drift triple) is a registry-hygiene defect — it does not
break observability emit at runtime, but it breaks the
`validate_inv_promotion.py` audit chain. Promoted to P1 below.

## 3. P1 Findings

### P1-1 — INV reference drift triple in WI-S09-005 dashboard JSON [REGISTRY-HYGIENE, LOAD-BEARING for audit chain]

**Severity:** P1 — `validate_inv_promotion.py` flags 3 INV labels with no
canonical registry §3 row. This breaks the registry ↔ dashboard ↔ alert
traceability claim that PRR-S09 §10 makes ("CI gate
`validate_inv_promotion.py` validates the WI-declared INVs match registry; CI
green per quality gates"). Either the validator is silenced (rubber-stamp
risk) or the labels are unstated invariants masquerading as canonical INVs.

**Defects (3 of them; surfaced via grep over `dashboards/grafana/*.json`):**

1. **`INV-CAS-DIGEST-INTEGRITY`** appears in `DASH-CAS.json:307,310` as panel
   title + description. The canonical registry entry (§3.1) is
   **`INV-CAS-INTEGRITY`** (CRITICAL; TLA+ `cas_integrity.tla` GREEN;
   description "Blob body hash matches path digest"). This is *almost
   certainly an alias drift* — the panel describes "SHA-256 digest mismatch
   on read = corruption signal; > 0 = SEV-0 PagerDuty page Architect +
   Security", which is exactly the runtime expression of INV-CAS-INTEGRITY.
2. **`INV-EXEC-IDEMPOTENT`** appears in `DASH-EXEC.json:288,291` as panel
   title + description ("Same action_digest executed concurrently > 1 = race;
   > 0 sustained = SEV-2"). No canonical registry row exists for this label.
   The panel describes an EXEC-001/EXEC-002 racing invariant from S-17
   forward-looking scope — it has not been promoted into §3.
3. **`INV-LGPD-AUTO-SUSPEND-FORBIDDEN`** appears in `DASH-SECURITY.json:219`
   and `DASH-TENANT.json:305`. No canonical registry row. The label aligns
   with LGPD Art. 20 + GDPR Art. 22 (automated-decision-making prohibition
   without human review); this is a S-11 / S-13 privacy invariant scope.

**Concrete remediation matrix:**

| Drift label | Action | Where | Why |
|---|---|---|---|
| `INV-CAS-DIGEST-INTEGRITY` | **Rename to `INV-CAS-INTEGRITY`** in DASH-CAS.json:307,310 | Edit `dashboards/grafana/DASH-CAS.json` panel `title` + `description` | Alias drift — canonical §3.1 row already exists; rename closes the gap without registry edit. Add a `validate_dashboards.py` lint that rejects any `INV-*` substring not matching `invariant_registry.md §3` row IDs. |
| `INV-EXEC-IDEMPOTENT` | **Promote to §3.12** (S-09 row addition under WI-S09-005-12) | Add row to `invariant_registry.md §3.12 sprint-driven invariants table` | The dashboard surface is shipping the canary; the registry must catch up. HIGH severity; enforce via S-17 execution race detector at the worker scheduler boundary. |
| `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` | **Promote to §3.12** (S-09 row addition; OR forward-whitelist to S-11/S-13 with explicit canary-only stub row) | Add row to `invariant_registry.md §3.12` with `enforce: pending` + cross-reference S-11 WI-S11-XXX | Compliance load-bearing — LGPD Art. 20 GDPR Art. 22 anti-rubber-stamp gate; canary panel without registry row violates the validator audit chain. HIGH severity. |

**Why I escalate this to P1 and not P2:** the `validate_inv_promotion.py`
**must** be green to promote WI-S09-005-12 from REVIEWING → DONE. Either the
validator is on (in which case PRR-S09 §10 is *false* and the SEAL is
contested), or it is off (rubber-stamp risk — the entire audit chain
ratification ceremony hinges on this script being green). PRR-S09 currently
claims green; the artifact says drift. Resolve before next SEAL ceremony.

### P1-2 — `CardinalityValidator` ledger pruning policy unspecified [LOAD-BEARING for enforcement primitive]

**Severity:** P1 — the enforcement primitive's *own* state is a cardinality
leak under sustained load.

**Defect:** WI-S09-001 §6.1 `CardinalityValidator` is documented as a
per-instance `Arc<Mutex<HashMap<RedMetricKind, HashSet<MetricLabelTuple>>>>`
ledger. The 20k per-metric / 100k global bound is enforced at emit time. The
WI does not specify:

1. **When entries are evicted.** Over 72h sustained canary (12_960 loops/72h
   per WI-007), the same `(metric_kind, MetricLabelTuple)` will be inserted
   12_960 times — the `HashSet` deduplicates, fine — but if a tenant churns
   regions across rebalancing, the `(metric_kind, tenant_tier=Pro,
   region=Apac)` entry from yesterday is still in the ledger today even
   though no traffic is hitting that bucket. The ledger grows monotonically.
2. **What happens when the bound IS hit at emit time.** §6.1 says
   "`CardinalityValidator` enforcing INV-OBS-CARDINALITY-BUDGET HIGH at the
   emit boundary" — but the *behavior* on hit is undocumented. Drop the
   emit? Sample? Emit a `corelink.analytics.budget_exceeded` event and
   continue? The fail-OPEN vs fail-CLOSED contract here is load-bearing for
   the SEV-2 alert in DoD §4 ("`corelink_metrics_cardinality_budget_violation_total{scope,
   metric}` MUST = 0").
3. **Whether the ledger is per-Worker-isolate or per-region.** CF Workers
   isolates have independent memory; a per-instance `Arc<Mutex<...>>` ledger
   means each warm isolate has its OWN 20k budget. If a region warms 100
   isolates, the effective budget is 100 × 20k = 2M series. This is either
   intentional (in which case the global 100k bound is wildly understated)
   or unintentional (in which case the enforcement primitive is broken in
   practice).

**Concrete remediation:**

- Add §6.1 ledger lifecycle: TTL-based eviction (e.g. 1h TTL on every
  `(metric_kind, MetricLabelTuple)` last-seen timestamp; sliding-window
  cardinality budget).
- Add §6.1 fail-mode: emit `corelink.analytics.budget_exceeded` audit event;
  DROP the over-budget emit (fail-OPEN to observability, fail-CLOSED to
  business path — the cost-bomb is the worse failure mode); increment
  `corelink_metrics_cardinality_budget_violation_total`.
- Add §6.1 scope clarification: per-isolate vs per-region; if per-isolate,
  document the multiplier explicitly OR pivot to a region-shared DO state
  ledger (with the appropriate trait-abstraction-defer shell).

### P1-3 — Exemplar deep-link integrity is host-side-only [INTEGRATION-EDGE]

**Severity:** P1 — DoD §4 row reads ✅ (host-side) but the user-observable
claim ("click em Grafana abre Tempo") is unverified end-to-end.

**Defect:** WI-S09-003 ships exemplar linkage via OpenMetrics 1.0 §exemplars
+ `prop_exemplar_link_to_metric` 10k iter pinning serialize round-trip. The
production wiring requires *three* live surfaces:

1. **Mimir tenant accepts exemplars.** Mimir 2.10+ supports OpenMetrics
   exemplars but the tenant config flag `exemplars: true` must be set in
   `runtime.yaml`; default is OFF. No Terraform IaC ships at SEAL.
2. **Grafana datasource exemplar-link wired to Tempo.** Each
   PrometheusDataSource needs `exemplarTraceIdDestinations` configured
   pointing to the Tempo datasource UID. The 12 dashboards JSON ship without
   this field set (verified via grep over `dashboards/grafana/*.json` for
   `exemplarTrace` — zero hits).
3. **Tempo tenant accepts OTLP `/v1/traces`.** No Tempo tenant config
   ships at SEAL.

None of these is *wrong* to defer — they are appropriate
`trait-abstraction-defer` charter items. The defect is the *DoD framing*:
the row should read ⚠️ DEFERRED (live wiring is S-20) rather than ✅
(host-side), because the property test ONLY pins the serialize layer, not
the end-to-end deep-link.

**Concrete remediation:**

- Restate DoD §4 row as ⚠️ DEFERRED for the user-observable claim; keep ✅
  for the serialize layer (separate the two).
- Add an S-20 GA gate item: end-to-end smoke test "Mimir accepts exemplar,
  Grafana datasource has `exemplarTraceIdDestinations`, click opens Tempo
  span" — block S-20 GA on this smoke test.

## 4. P2 Findings

### P2-1 — R2 lifecycle single-expiration 400d for logs ↔ audit 7y overlap

**Severity:** P2 — WI-S09-002 §1 (Lote 10.9-quaters NEW-P0-3 corrected) ships
R2 single-tier 400d expiration for logs. WI-S09-004 ships Object Lock
Governance Mode 7y for audit. The two retention windows are intentionally
distinct (logs are operational, audit is compliance). The gap: in some
compliance scenarios (LGPD Art. 19 portability request; SOC 2 CC7.2 evidence
preservation), the LOG body contains evidence that mirrors the AUDIT body.
The 400d log retention may purge evidence the 7y audit chain *needs to
reference* for inclusion proofs.

**Concrete remediation:**

- Add ADR-class carve-out in WI-S09-002 §10: log-row purging policy must
  check `audit_chain_reference_count(log_event_id)` before purge — if any
  audit event references this log row's `request_id`, hold for the 7y
  audit window.
- OR: explicitly separate `log_event` (purgeable at 400d) from
  `audit_event_log_excerpt` (held 7y) at the schema layer.

### P2-2 — Circular fail-CLOSED hazard on verifier path

**Severity:** P2 — WI-S09-004 §1 ships `audit emit fail-CLOSED` AND a daily
`ChainVerifier` that emits `corelink.audit_chain.chain_break_detected` on
break. If the verifier's own emit goes through the same fail-CLOSED envelope,
a broken chain causes the SEV-0 notification to ALSO fail-CLOSED — the SEV-0
emit silently swallows.

**Concrete remediation:**

- WI-004 §6: verifier-path emits go to a **secondary** sink (Slack webhook +
  PagerDuty direct dispatch, NOT the audit chain itself). Or: verifier emits
  use a fail-OPEN envelope explicitly, with a separate
  `corelink_audit_chain_verifier_emit_failures_total` counter that SEV-1
  alerts on > 0.
- Add a property test
  `prop_verifier_emit_path_bypasses_chain_fail_closed` 10k iter pinning the
  carve-out.

### P2-3 — Tail-based sampling claim ↔ shell mismatch

**Severity:** P2 — WI-S09-003 §0 claims "head-based sampling 1% default +
tail-based sampling 100% para spans com `error=true` OR
`latency_p99_breach=true`". The §1 `sampler` module ships **head-based
deterministic** only (per W3C trace_id last-8-bytes-as-u64 ratio). Tail-
based requires an OTel Collector with `tail_sampling` processor — which is
not in the InMemory shell.

**Concrete remediation:**

- Either: restate §0 to ship head-based at SEAL and forward tail-based to
  S-17 chaos sprint (where it is naturally exercised).
- Or: ship a `TailSampler` trait surface with `AlwaysSampleErrorTailSampler`
  + `InMemoryTailSampler` fakes, mirroring the `OtlpExporter` defer pattern.

## 5. P3 Findings

### P3-1 — Lote 10.9-quaters NEW-P0-3 correction (R2 single-tier) buries the rationale

The "single-expiration 400d / Loki 30d hot is query-tier NOT storage class"
correction is the right call but is documented only in the `Título` field of
WI-S09-002. A reader of `_spec_contract.md §5.2` may not realize that
Logpush → R2 is no longer multi-tier. Suggest pulling the correction into a
top-level §1 NOTE block.

### P3-2 — `audit_chain.specversion` "1.0" hard-pin without forward-compat path

WI-S09-004 hard-pins `specversion: "1.0"` on every CloudEvent. CNCF
CloudEvents 1.0 is stable as of 2019, but CloudEvents 1.1 has been in
working-draft for some time. A hard-pin without a forward-compat path means
the moment 1.1 emits, downstream subscribers (SIEM fan-out) may parse
incorrectly. Suggest adding `accepts_specversion: ["1.0"]` allowlist on the
SIEM fan-out side, and a renegotiation hook on the emitter.

### P3-3 — PII redaction pattern precedence is implicit

WI-S09-002 ships 5 redaction patterns (email / ip / token / pan / cpf_cnpj).
Some inputs match multiple patterns (e.g. a CPF embedded in a PAN-shaped
prefix). The match precedence is implicit in the scanner order. Suggest
documenting the canonical order Token → IP → CPF/CNPJ → PAN → Email in §6 +
adding `prop_redaction_match_precedence_canonical` 10k iter.

## 6. Cross-WI consistency claims verified

| Claim | Status | Notes |
|---|---|---|
| Audit-emit BEFORE state mutation fail-CLOSED envelope | **PASS** | Confirmed in WI-001/002/003 (fail-OPEN observability emit) and WI-004 (fail-CLOSED audit emit) per Lote 10.6bis canonical pattern. |
| `MetricLabelTuple` enum-typed cartesian (no `String` slot) | **PASS** | WI-001 ships the structural lint; cross-validated by `FORBIDDEN_LABEL_NAMES` const for CI lint secondary defense. |
| RFC 8785 JCS canonical determinism via `serde_jcs` 0.2 | **PASS** | WI-004 pins via `prop_jcs_canonicalization_deterministic` 10k iter. |
| W3C Trace Context Recommendation 2020 strict ABNF | **PASS** | WI-003 pins via `prop_traceparent_rejects_invalid` + `prop_zero_trace_id_rejected`. |
| 5-pattern hand-rolled byte-level scanner (no regex crate) | **PASS** | WI-002 §1 explicit anti-scope INV-AVAIL-DOS canary; Luhn + mod-11 validated. |
| AdminCtx-RBAC drill-down ACL on dashboards | **DEFER to part2** | Tracked in WI-S09-005 review (Part 2 §3 P1-A). |
| Trait-abstraction-defer charter pattern applied to every prod surface | **PASS** | 9 trait surfaces with InMemory fakes; production binding deferred to staging-account provisioning. |
| INV §3.12 row promotion claims match registry | **FAIL** | Three drifted INV labels in dashboards (see P1-1 above). |

## 7. Recommendation

**CONDITIONAL APPROVE** for WIs 001..004 → STAGING-STABLE *contingent* on:

- **Block S-20 GA** until P1-1 (INV drift triple) is fully remediated;
- **Block S-20 GA** until P1-3 (exemplar deep-link end-to-end smoke) is
  green;
- P1-2 (ledger pruning policy) may be **deferred to Lote 10.9bis remediation
  wave** with explicit time-bound: ≤ 4 weeks post-SEAL or before any tenant
  beyond Free tier onboards;
- P2-2 (circular fail-CLOSED on verifier) **must land** in Lote 10.9bis
  with the property test.

Per Lote 10.9 review charter: this report is an AUDIT artifact (`type:
audit`, `doc_status: REVIEW`) and does NOT modify WI-S09-001..004 doc_status
(which remain FROZEN per the original SEAL). The remediation lands as a
separate Lote 10.9bis WI authored against this report.

---

**End R4 Opus Part 1 v1.0.0.**

---

## Closure footnote (Lote 10.9bis wave 17 — 2026-05-15)

**P1-1 INV reference drift triple — CLOSED.** Per the §3 remediation matrix:

1. `INV-CAS-DIGEST-INTEGRITY` → renamed to canonical `INV-CAS-INTEGRITY` (registry §3.2; CRITICAL; TLA+ `cas_integrity.tla` GREEN) in `dashboards/grafana/DASH-CAS.json` panel 10 title + description AND in `specs/04_sprints/_sealed/S09/work_items/WI-S09-005-12-grafana-dashboards-as-code.md` SEAL row.
2. `INV-EXEC-IDEMPOTENT` → promoted to `invariant_registry.md §3.12` (S-09 row; HIGH severity; INSERT ON CONFLICT (tenant_id, exec_id) idempotency; S-17 worker scheduler race detector PLANNED).
3. `INV-LGPD-AUTO-SUSPEND-FORBIDDEN` → promoted to `invariant_registry.md §3.12` (S-09 row; HIGH severity; LGPD Art. 20 + GDPR Art. 22 anti-automated-decision; requires `human_reviewer_id` in audit emit + dual-approval via INV-ADMIN-DUAL-APPROVAL).

**Validators verde:** `validate_inv_promotion.py` drift 3 → 0; `validate_specs.py`, `validate_references.py`, `validate_canonical_consistency.py`, `validate_dashboards.py` all exit 0.
