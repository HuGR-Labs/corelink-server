---
id: "WI-S02-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-02"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s02", "side-channel", "constant-time", "ctrl-iso-004", "mann-whitney", "appsec"]
---

# WI-S02-004 — Constant-Time 404/403 Middleware + Mann-Whitney U Adversarial Test

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-02](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S02-004 |
| Título | Constant-time 404/403 timing-padding middleware + statistical adversarial test |
| Sprint | S-02 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cross-tenant existence oracle = isolation breach indireta), FF-HR-005 (CTRL-ISO-004 implementation) |
| Tier | Todos (rate-limit + side-channel surface) |
| Fase produto | Fase 1 — Remote Cache |

## 1. Intent

Tower middleware em `crates/corelink-worker/src/middleware/timing_padding.rs` que envolve handlers (CAS read + AC read) e enforce **statistical indistinguishability** entre 404 (resource not found) e 403 (cross-tenant forbidden):

```rust
pub struct TimingPaddingLayer {
    target_p99_ms: f64,  // padding target window
    jitter_ms: f64,       // ±10% jitter
}

impl<S> Layer<S> for TimingPaddingLayer {
    type Service = TimingPaddingService<S>;
    fn layer(&self, inner: S) -> Self::Service { ... }
}

impl<S> Service<Request> for TimingPaddingService<S> {
    async fn call(&mut self, req: Request) -> Result<Response, Status> {
        let start = Instant::now();
        let resp = self.inner.call(req).await?;

        // If resp is 404 OR 403, pad to target_p99_ms ± jitter
        if matches!(resp.status(), 404 | 403) {
            let elapsed = start.elapsed();
            let target = self.compute_padded_target(elapsed);
            tokio::time::sleep_until(start + target).await;
        }
        Ok(resp)
    }
}
```

Adicionalmente: **adversarial Mann-Whitney U test** em `tests/timing_indistinguishability.rs` que prova statistical p > 0.05 (null hypothesis: distributions são same — desejamos NOT reject):
- 10,000 amostras 404 (digest doesn't exist anywhere).
- 10,000 amostras 403 (digest exists em outro tenant).
- Run Mann-Whitney U test em latency distributions.
- Assert p-value > 0.05 (indistinguishable).
- Criterion benchmark: p99 diff < 5ms.

CTRL-ISO-004 enforcement layer + INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE (HIGH em registry §3.12 Lote 9.4 add).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Side-channel timing attacks são **invisible vulnerabilities**: handler responde corretamente em ambos 404 e 403, mas latência diff revela existence. Atacante:

1. Obtém PAT válido para Tenant B.
2. Probe "candidate" digests possivelmente pertencentes a Tenant A (e.g., guessed Docker image hashes, public model checkpoint digests).
3. Mede latency 404 (digest doesn't exist anywhere — fast path: KV negative cache hit OR D1 not found).
4. Mede latency 403 (digest exists em A — slower path: D1 found + AuthZ reject).
5. Statistical analysis: cluster fast vs slow → fast = doesn't exist; slow = exists em outro tenant.

Resultado: atacante **enumera** Tenant A's blob digests sem direct access — INV-TENANT-ISOLATION violation indireta + privacy leak (CTRL-ISO-005). Em build cache scenario, digest revelation pode permit reconstruction de proprietary ML models OR sensitive build artifacts.

Mitigação layered:
1. **Cross-tenant masked as 404** (WI-S02-001 + WI-S02-002): handler retorna mesmo status code. Mas latency ainda diff.
2. **This WI**: timing padding adicional. After handler resolve, middleware pads response time para fixed target (e.g., 200ms) com small jitter (±10%). Both 404 e 403 paths converge to same observable latency distribution.
3. **Constant-time per-byte ops** em digest compare (WI-S01-002) — nível mais baixo.
4. **Rate limit per-PAT** (S-08 forward): even com timing padding perfeito, 10k probes em sequence fica suspicious; rate limit caps enumeration speed.

**Statistical proof via Mann-Whitney U + power analysis (corrected methodology):**

T-test assume normal distributions; latency distributions são not normal (long-tailed). Mann-Whitney U é non-parametric, robust. Null hypothesis: "two distributions são same"; test outputs p-value.

**CRITICAL — methodology correta (não cargo-cult)**:

`p > 0.05` sozinho **NÃO prova distributions são same**; significa "fail to reject H0" — pode-se ser baixo poder estatístico (small sample, undetectable effect). Para defender side-channel ausente, **3-prong evidence-grade test**:

1. **Sample size + power**: N ≥ 10000 amostras por arm (cross-tenant existing vs cross-tenant non-existing). Power 1−β ≥ 0.80 com effect size d = 0.2 (small/practically-relevant) calculado a priori via `g*power`-equivalent (`statrs::distribution::statistical_power`).
2. **p-value gate**: Mann-Whitney U p > 0.05 (null retido).
3. **Confidence interval no |Δmedian|**: bootstrap CI 95% sobre median latency diff; target |Δmedian| ≤ 1ms com 95% CI cruzando 0 (zero-inclusive).

Combined: "high power test failed to reject; effect size point estimate < 1ms; CI consistent with zero" = evidence-grade indistinguishability claim.

**Multiple-test correction (CI flake mitigation)**:

Statistical tests em CI com p=0.05 falham 5% das runs por chance (1-em-20). Bonferroni/Šidák correction:
- Run **3 independent trials** (different random seeds; same handler state).
- Reject só se ALL 3 trials p < 0.05 (Šidák combined α ≈ 0.000125 effective; flake < 1-em-8000 runs).

**Why p > 0.05 target (not 0.01):**
- Strict (p > 0.01) = rare false positives mas requires more samples + tighter padding.
- p > 0.05 é industry-standard "indistinguishable" baseline (NIST SP 800-90B Annex C; CVE-2018-0114 mitigation guides).
- Power 1-β ≥ 0.80 + |Δmedian| ≤ 1ms é o **real gate** (não p sozinho).

**Criterion benchmark |Δmedian| < 1ms (não p99 diff < 5ms vago):**
Statistical test = property; benchmark = numerical bound on **median diff** (mais informativo que p99). < 1ms é practical threshold (CF Worker scheduler quanta ~1ms; padding granularity ≥ 5ms dominates jitter).

**Risk justification HIGH_RISK:**
- **FF-HR-002**: timing leak = cross-tenant existence oracle = isolation breach indireta.
- **FF-HR-005**: CTRL-ISO-004 (canonical control) implementação.
- **Reversibility**: information disclosure é one-way; atacante já learned existence.

10-12 sign-offs incl. AppSec (side-channel review) + Statistician advisor (Mann-Whitney methodology).

## 3. Customer Impact & Journey

**JTBD (security-conscious tenant):** "Como Enterprise tenant com proprietary ML models cached, eu preciso garantia que outros tenants (mesmo com PAT comprometido OR adversarial) não conseguem enumerar meus digests via timing analysis."

**Customer-visible:**
- Latency p99 cap: ~300ms cold / ~150ms warm (padded; minor regression vs unbounded handler).
- Trade-off documented em pricing/SLA: enterprise tier garante constant-time response.
- Métrica `corelink.cas.side_channel.timing_diff_ms` exposed em SLO dashboard.

## 4. Capability Mapping

- **CAP-CAS-008** (Side-channel-resistant 404/403) — IMPLEMENTA primary.
- Trace: `security_model.md §6.4 CTRL-ISO-004` + `invariant_registry.md §3.12 INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE`.

## 5. Tipo e Classificação

Middleware (cross-cutting); HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Tower `TimingPaddingLayer` middleware**:
   - Wrap CAS Read + GetBlob + FindMissingBlobs handlers.
   - Compute padded target em function of (status, elapsed): if status 404 OR 403, pad up to target_p99_ms.
   - Jitter ±10% via deterministic RNG seeded per request_id (não broken via correlation).
2. **`TimingPaddingConfig`**:
   - `target_p99_ms: 200` (default; tunable via DO config-singleton S-13 forward).
   - `jitter_pct: 10` (±10%).
   - `enabled: true` (default; opt-out via dev mode flag rare).
3. **Adversarial test em `tests/timing_indistinguishability.rs`**:
   - Setup: 10k requests 404 (random digests) + 10k requests 403 (digests existing em other tenant).
   - Capture latency em microseconds.
   - Mann-Whitney U test via `statrs` crate.
   - Assert p-value > 0.05.
   - Output report: distributions histogram + p-value.
4. **Criterion benchmark `benches/side_channel.rs`**:
   - Measure p50/p95/p99 latency 404 vs 403.
   - Assert p99 diff < 5ms.
5. **Métrica `corelink.cas.side_channel.timing_diff_ms`** (gauge):
   - Computed continuously from sliding window métricas.
   - Alert SEV-2 se p99 diff > 5ms sustained 5min.
6. **Documentation** em `docs/internal/side-channel-defense.md`:
   - Theory: timing attacks + Mann-Whitney rationale.
   - Implementation: middleware design + jitter strategy.
   - Operational: how to interpret métrica + alert response.

### 6.2 Out-of-scope (deferred)

- **Constant-time D1 query** (database-level timing): D1 SQLite engine tem inherent timing diffs; mitigated at middleware level only.
- **Cross-region timing** (latency varies by region): defer; per-region middleware config pós-S-14.
- **Adaptive padding** (dynamic target based on load): static target at GA; ML-based tuning pós-GA.
- **Timing attacks on auth path** (PAT validate timing): cobertos parcialmente em WI-S03-002 (Argon2id constant-time).

## 7. Anti-Scope

- ❌ Pad **all** responses (only 404/403; padding 200/201 OK responses adds latency tax sem security benefit).
- ❌ Random jitter sem seed (broken via correlation analysis attacker side).
- ❌ p < 0.05 target (would mean leak exists).
- ❌ T-test ou ANOVA (assume normal distributions; latency é not normal).
- ❌ Padding via spinlock (CPU waste; use tokio::time::sleep_until).
- ❌ Custom non-cryptographic RNG (use `rand::rngs::SmallRng` seeded properly).
- ❌ Adaptive ML padding (overkill at GA; static is fine).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Constant-time 404/403 middleware

  Background:
    Given Tenant A has digest D_X (existing)
    And Tenant B is authenticated
    And TimingPaddingConfig { target_p99_ms: 200, jitter_pct: 10 }

  Scenario: Padding applied to 404 response
    Given request resolves 404 in 50ms (fast path)
    When middleware processes
    Then total response time padded to 200ms ± 20ms (10% jitter)

  Scenario: Padding applied to 403 response
    Given request resolves 403 in 80ms (slower path)
    When middleware processes
    Then total response time padded to 200ms ± 20ms

  Scenario: 200 OK responses NOT padded
    Given request resolves 200 OK in 50ms
    When middleware processes
    Then total response time = 50ms (no padding)

  Scenario: Mann-Whitney U test — distributions indistinguishable
    Given 10k 404 samples + 10k 403 samples
    When Mann-Whitney U test computed
    Then p-value > 0.05 (null hypothesis NOT rejected)
    And distributions histogram visually overlap

  Scenario: Criterion benchmark p99 diff < 5ms
    Given criterion sustained measurement
    When p99(404) and p99(403) computed
    Then |p99(404) - p99(403)| < 5ms

  Scenario: Métrica side_channel timing_diff_ms emitted
    Given continuous response stream
    When sliding window computes
    Then métrica corelink_cas_side_channel_timing_diff_ms updated
    And SEV-2 alert se sustained > 5ms por 5min

  Scenario: Property test — adversarial enumeration attempt
    Given 1000 random digests probed (mix existing + non-existing cross-tenant)
    When Tenant B issues GET requests
    Then Mann-Whitney U on response latencies → p > 0.05
    And atacante cannot statistically distinguish 404 from 403

  Scenario: Configurability via DO singleton (S-13 forward)
    Given admin updates target_p99_ms to 250ms
    When config propagates ≤ 5s
    Then middleware uses new target em next request
```

## 9. Design Decisions

### 9.1 Why pad só 404/403 (não todos status)

Padding 200 OK adds latency tax sem security benefit — atacante já sabe blob existe (got the body). Padding apenas error responses focus mitigation on attack vector.

### 9.2 Why Mann-Whitney U (não t-test)

T-test assumes normal distributions. Latency distributions são long-tailed (network jitter, GC pauses, occasional slow queries). Mann-Whitney é non-parametric, robust to non-normality, uses rank-based comparison. Industry standard for timing attack analysis.

### 9.3 Why p > 0.05 (não p > 0.01 ou p > 0.5)

- p > 0.01: stricter; requires tighter padding + more samples; marginal benefit.
- p > 0.5: ideal but practically hard; achievable em controlled environment; document if achieved.
- p > 0.05: industry-standard "indistinguishable" baseline (NIST recommendations).
- Target: p > 0.05 mandatory; p > 0.5 achievement = bonus documented in PRR.

### 9.4 Why deterministic jitter (seeded per request_id)

Random jitter without seed = atacante can correlate adjacent requests → narrow padding window via averaging. Deterministic jitter per request_id (RNG seeded) provides per-request randomness sem correlation across requests.

### 9.5 Why static target (não adaptive)

Adaptive ML padding (e.g., target = current p99 + buffer) introduces feedback loop + non-determinism → harder to analyze + reason. Static target is provably analyzable. Adaptive defer pós-GA.

### 9.6 Tower middleware (não wrapper inline)

Tower é Tokio ecosystem standard; composable + reusable em outros handlers. Wrapping handler inline = code duplication; middleware = single place + apply globally.

### 9.7 ADR potencial?

Sim — ADR-0023 documentando "Constant-Time Defense via Timing Padding Middleware". Justification + alternatives (constant-time D1 query rejected) + Mann-Whitney rationale.

## 10. Completeness Criteria SOTA

- [ ] **10.4.1** Mann-Whitney U + power analysis 3-prong (EVT-002):
  - N ≥ 10000 samples per arm (cross-tenant existing vs non-existing).
  - Power 1−β ≥ 0.80 com effect d = 0.2 (calculated a priori).
  - Šidák correction 3-trial gate: ALL 3 independent trials p > 0.05 (combined α ≈ 0.000125).
  - Bootstrap 95% CI sobre |Δmedian| ≤ 1ms (CI cruzando 0 mandatory).
- [ ] **10.4.2** Criterion benchmark |Δmedian| < 1ms (não p99 diff < 5ms — median é evidence-grade) (EVT-002).
- [ ] **10.4.3** Métrica `corelink.cas.side_channel.timing_diff_ms` emitida continuously com aggregation method spec'd: 5min sliding window, p99 of |median(group_A) − median(group_B)|; alert SEV-2 se sustained > 2ms 30min (EVT-013).
- [ ] **10.4.4** Adversarial property test 1000 enumeration attempts → 0 statistical leak (EVT-002).
- [ ] **10.4.5** ADR-0023 ratificado (path concreto: `specs/02_governance/decisions/ADR-0023-constant-time-timing-padding.md`) (EVT-027).
- [ ] **10.4.6** Documentation `docs/internal/side-channel-defense.md` reviewed by AppSec + Crypto SME.
- [ ] **10.4.7** Cost regression gate (§14.10): padding adds bounded latency tax (~150ms expected; cost regression bench em CI).
- [ ] **10.4.8** Edge case `elapsed > target_p99_ms` handling: log SEV-3 anomaly + emit response sem additional padding (não shorten); rely on rate-limit + alert para detection. Spec'd em §15.5 chaos.
- [ ] **10.4.9** Padding granularity ≥ 5ms (CF Worker scheduler quanta ~1ms; ≥ 5ms dominates jitter); validated em chaos test §15.6.

## 11. DoD

- [ ] Tower middleware impl + integration em CAS handlers.
- [ ] Mann-Whitney U adversarial test green.
- [ ] Criterion benchmark green.
- [ ] Métrica + alert configured.
- [ ] ADR-0023 created + reviewed.
- [ ] Documentation completa.
- [ ] Code review + AppSec + Statistician advisor + Crypto SME (constant-time review).

## 12. Invariants

- **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** (HIGH — registry §3.12) — IMPLEMENTA primary.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+) — defesa indireta (timing leak = isolation breach).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Tower middleware | `crates/corelink-worker/src/middleware/timing_padding.rs` | Rust |
| Adversarial test | `crates/corelink-worker/tests/timing_indistinguishability.rs` | Rust |
| Criterion benchmark | `crates/corelink-worker/benches/side_channel.rs` | Rust |
| ADR-0023 | `specs/02_governance/decisions/ADR-0023-constant-time-timing-padding.md` | Markdown |
| Documentation | `docs/internal/side-channel-defense.md` | Markdown |

## 14. Quality Standards SOTA

- **14.4.1** Zero unsafe; zero unwrap.
- **14.4.2** rustdoc + 3 examples.
- **14.4.3** Test coverage ≥ 90%.
- **14.4.4** Padding overhead p99 ≤ 5ms variance (não absolute latency target).
- **14.4.5** SAST clean.
- **14.4.6** Métricas RED + side-channel gauge.
- **14.4.7** Runbook: nenhum (timing alert é informational; não SEV-1).
- **14.4.8** Breaking middleware API = bump major.
- **14.4.9** Memory bounded (middleware é stateless).
- **14.4.10** Cost regression gate.

## 15. Chaos Experiments

1. **Inject latency spike em handler** (1s artificial): verify middleware detect e padding aborted gracefully (não pad to 1s+).
2. **Mann-Whitney run em production sample**: validate p > 0.05 sustained 7d.
3. **Adversarial high-rate probe** (10k req/s): combine com S-08 rate limit; verify no leak via storm.

## 16. PRR

PRR + AppSec + Statistician advisor + Crypto SME.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Tower middleware scaffold + Layer/Service traits | 2h |
| ST-002 | Padding logic com seeded jitter | 3h |
| ST-003 | Integration em CAS handlers (Read + GetBlob + FindMissing) | 2h |
| ST-004 | Mann-Whitney U test framework via statrs crate | 4h |
| ST-005 | Adversarial test 10k×10k samples | 2.5h |
| ST-006 | Criterion benchmark p99 diff | 2h |
| ST-007 | Métrica computation sliding window + emit | 2.5h |
| ST-008 | Alert config (SEV-2 timing drift) | 1h |
| ST-009 | ADR-0023 authoring + Statistician review | 2.5h |
| ST-010 | Documentation `side-channel-defense.md` | 2h |
| ST-011 | rustdoc + examples | 1h |
| ST-012 | PRR + AppSec walkthrough | 2h |

**Total**: ~26.5h Optimistic; PERT ~32h.

## 18. Dependencies

### Hard blockers

- **WI-S02-001 SEALED** (Read handler; middleware envolves).
- **WI-S02-002 SEALED** (GetBlob + FindMissingBlobs; middleware envolves).

### Soft blockers

- WI-S02-005 (negative cache) — pode interagir com timing (negative cache hit é faster); padding compensa.

### Outbound

- S-04 AC handlers — reuse middleware.
- S-08 rate limit — complementary defense layer.

## 19. Effort PERT

O: 22h, M: 26.5h, P: 50h → PERT 30.7h.

## 20. Time-boxing

- 35h hard limit.
- Escalation ST-004 (Mann-Whitney) > 5h → Statistician advisor.

## 21. Observability

Métricas:
- `corelink.cas.side_channel.timing_diff_ms` (gauge, sliding window).
- `corelink.cas.timing_padding.elapsed_seconds_bucket{status}` (histogram).
- Alert: `timing_diff_ms{quantile=0.99} > 5` sustained 5min → SEV-2.

## 22. Cost Analysis

Padding latency = wasted user time. 200ms target × ~10% requests pad = ~$X TCO. Mitigation: padding budget é implicit em SLO-LAT-CAS-GET; não adds new cost.

## 23. API Contract

Tower middleware é internal Rust; no external API impact.
Métrica `corelink_cas_side_channel_timing_diff_ms` é public (SLO dashboard surface).

## 24. Post-mortem Hooks

- Mann-Whitney p-value < 0.05 sustained → CRITICAL post-mortem (timing leak detected).
- Métrica timing_diff_ms > 5ms sustained > 1h → 5-Why obrigatório.
- Adversarial enumeration detected em produção (anomaly metric) → SEV-2 + AppSec.
- ADR-0023 contradicted by new evidence → ADR superseded + post-mortem.

## 25. Rollback / Recovery

Hot rollback via WASM previous version; middleware disabled via DO config flag.

## 26. Security & Privacy

STRIDE:
- **Information disclosure**: THE primary threat — mitigated.
- **Detectability**: side-channel timing analysis covered.
LINDDUN:
- **Detectability**: timing attacks specifically addressed (NIST classification).
- **Linkability**: cross-tenant existence linkability prevented.

## 27. Knowledge Transfer

Tech talk: "Constant-Time Defense em Production: Mann-Whitney U Como Evidência" — 30 min + record.
Doc `side-channel-defense.md` + onboarding test 5 questions.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Mann-Whitney p-value drift em prod (e.g., DB load shifts) | M | M | HIGH | M | LOW | Continuous métrica + alert + adaptive padding pós-GA |
| R-002 | Padding window narrowed via correlation analysis | L | M | HIGH | L | LOW | Seeded jitter per request_id; AppSec review |
| R-003 | Performance regression (latency p99 inflado) | M | L | MEDIUM | L | LOW | SLO-LAT-CAS-GET budget includes padding tax |
| R-004 | False positive Mann-Whitney (test red mas leak não real) | L | L | LOW (CI noise) | L | LOW | Multiple sample sessions + 95% confidence interval |
| R-005 | Adaptive attacker (storm 1M probes) | M | M | HIGH | M | MEDIUM | S-08 rate limit + abuse score (forward dependency) |
| R-006 | Cost regression > 10% latency budget | M | L | MEDIUM | L | LOW | §14.10 |

## 29. Review Checkpoints

1. Design (D+0): AppSec + Statistician advisor.
2. Code (D+3): peer + AppSec + Crypto SME.
3. Adversarial (pre-merge): 10k×10k Mann-Whitney + criterion sustained 24h.
4. Pre-merge: ADR-0023 ratified.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — side-channel review_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD_ | _pending_ | _pending_ |
| 11 | Architect | _TBD_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — Mann-Whitney methodology + power analysis sign-off_ | _pending_ | _pending_ |
| 13 | Crypto SME | _mandatory; constant-time primitives + jitter design review_ | _pending_ | _pending_ |
| _advisory_ | Statistician advisor | _TBD; sourcing options: external consultant OR Crypto SME doubles role; non-fictional gate required_ | _advisory_ | _pending_ |

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.2.

## 32. Anti-patterns evitados

- ❌ Random jitter sem seed (correlation breakable).
- ❌ T-test em latency (assumes normal).
- ❌ p > 0.01 strict (impractical achievable).
- ❌ Padding all responses (latency tax sem benefit).
- ❌ Spinlock padding (CPU waste).
- ❌ Adaptive padding sem analysis (unanalyzable).
- ❌ Custom non-cryptographic RNG.
- ❌ Constant-time enforcement only at Rust-level (compiler optims break).

---

**Fim WI-S02-004.** Próximo: WI-S02-005 (negative cache KV adapter).
