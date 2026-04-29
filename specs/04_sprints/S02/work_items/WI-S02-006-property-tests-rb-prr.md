---
id: "WI-S02-006"
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
  - "INVARIANT-REGISTRY"
  - "SECURITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s02", "test", "property-test-100k", "rb-fm-253", "bit-rot", "prr"]
---

# WI-S02-006 — Property Test 100k Tenant Isolation (Round-Robin Strategy) + RB-FM-253 Dry-Run + Bit-Rot Test + PRR

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-02](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S02-006 |
| Título | Property tests 100k iter + bit-rot integration test + RB-FM-253 dry-run + PRR HIGH_RISK |
| Sprint | S-02 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (proves runtime tenant isolation read path), FF-HR-005 (validates security controls + chaos resilience) |

## 1. Intent

Suite de validação final da S-02 que materializa evidence para PRR HIGH_RISK sign-off:

1. **Property test 100k iter** em `crates/corelink-worker/tests/prop_cas_read.rs`:
   - `prop_tenant_isolation_read_path` (100k iter; round-robin strategy — vide §2 narrative; ≥ 40 reps per (tenant_i, tenant_j) pair).
   - `prop_get_blob_unary_isolation` (10k iter).
   - `prop_find_missing_no_existence_oracle` (10k iter; inclui response-size leak guard).
   - `prop_negative_cache_correctness` (10k iter race conditions; exercita put_miss vs invalidate_on_write eventual-consistency convergence invariant per WI-S02-005 §9.11; **NÃO** monotonic version stamp — KV não oferece atomic CAS; oracle: 0 incorrect hot reads + convergence within TTL bound).
   - `prop_constant_time_response` (10k iter Mann-Whitney + power analysis; vide WI-S02-004 §10.4.1; reused gate).
2. **Bit rot integration test** em `tests/integration_bit_rot.rs`:
   - Setup: write blob via S-01 path, body persisted em R2.
   - Inject corruption: directly modify R2 object out-of-band (via wrangler R2 admin API).
   - Read via S-02 GET handler with WI-S02-003 client verify default-on.
   - Assert: 100% bit rot scenarios caught (HashMismatch error returned).
3. **RB-FM-253 (cross-tenant read) dry-run** em staging:
   - Execute runbook step-by-step com Security lead + SRE.
   - Output: EVT-017 evidence captured + runbook updates if drift detected.
4. **PRR doc** em `specs/04_sprints/S02/PRR-S02.md`:
   - 11 sign-offs documented.
   - Adversarial review summary (pentest internal).
   - Promotion gate decision.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

WI-S02-001..005 implementam features. **Este WI prova que features funcionam sob adversarial scrutiny + production-equivalent load**. É o "ship gate" do S-02 sprint.

Property tests 100k iter cover state space muito maior que TLA+ small bounds (~3 tenants × 10 ops). Adversarial property:
- 50 tenants × 1000 random read ops com mixed digests (próprios + cross-tenant + non-existent).
- Assert: 100% das reads cross-tenant retornam 404 uniform per ADR-0028 (CrossTenantMasked variant) — NUNCA 403 (status-code differential = enumeration oracle); NUNCA blob alheio.
- Assert: 100% das reads tombstoned retornam 404 (não retornam body do blob soft-deleted).
- 0 false positive (legítimo read retorna blob errado) em 100k iter = 99.999%+ confidence.

**Round-Robin shrinker strategy (não cargo-cult; explicit semantics):**

Random sampling sobre 50 tenants pode-se miss subtle (tenant_i, tenant_j) cross-pair interactions — birthday-bound says 50² = 2500 distinct pairs; em 100k iter random, alguns pairs underexplored. Round-robin garante coverage:

- **Strategy explicit**: per iteration `i`, `tenant_actor = i % 50`; per op `j`, `tenant_target = (i // 50 + j) % 50` para cross-tenant attempts. Garante cada (i, j) pair com `i != j` é tested ≥ 40× em 100k iter (100k / 2500 = 40 reps minimum). Pair coverage matrix verified em test setup.
- **Shrinker**: ao falha encontrada, proptest shrinker rotaciona deterministicamente — primeiro reduz ops por iter, depois reduz tenants ativos (mantendo round-robin invariant), depois reduz state action space. Output: minimal counter-example com structure preserved.
- **vs pure random** (rejected baseline): random 100k iter cobre ~2400/2500 pairs em expectation com 100 missing; round-robin garante 100% coverage with 40× redundancy.
- **Implementation**: custom `proptest::Strategy` via `derive(Arbitrary)` override; deterministic via `proptest::test_runner::Config { rng_algorithm: RngAlgorithm::ChaCha, .. }`.

Without round-robin, a (tenant_42, tenant_17) cross-pair bug might escape 100k random iter; with round-robin, deterministic 40 reps per pair guarantees catch. Property = "for all (i, j) with i != j, ≥ 40 cross-attempt iterations exercised".

`prop_find_missing_no_existence_oracle`:
- Tenant B requests FindMissingBlobs([1000 random digests]).
- Some digests pertencem a Tenant A.
- Assert: response shows them as "missing" (não distinguishable de "really missing").

`prop_negative_cache_correctness` (aligned com WI-S02-005 §9.11 eventual-consistency model):
- Mixed sequence: write digest_X → invalidate cache (KV.delete) → read digest_X within same region/replica → assert 200 OK (cache hit cleared; fall-through D1+R2).
- Mixed sequence: read non-existent → cache populated (NotFound TTL 300s) → re-read within TTL → cache hit returns 404.
- Race conditions: concurrent write + read same digest; assert eventual convergence — **stale 404 transient acceptable up to TTL bound (≤ 300s)**; assert (a) NO incorrect 200 OK (returning blob from wrong tenant OR stale body); (b) post-TTL-expiry OR explicit invalidation propagation, next read converges to correct semantic; (c) Bazel cliente retry pattern resolves remaining staleness. **NÃO** invariant of "no stale cached negative ever" — that contradicts CF KV eventual consistency reality per WI-005 §9.11.

**Bit rot integration test:**
- Real-world FM-051 (R2 silent corruption) é low probability mas catastrophic; chaos test simulates.
- Inject corruption directly em R2 via `wrangler r2 object put --force corrupted-bytes`.
- GET via S-02 handler with client verify (WI-S02-003).
- Verify: HashMismatch error returned to cliente; 0 corrupted bodies returned.

**RB-FM-253 dry-run:**
Runbook walkthrough simulado em staging:
1. Inject simulated cross-tenant attempt (Tenant B PAT + Tenant A digest).
2. Verify alert fired (`corelink_cas_cross_tenant_attempt > 0` SEV-1).
3. Verify oncall page received.
4. Verify forensic via R2 audit log (S-09 forward) + customer notification preparation.
5. Document findings in EVT-017.

**PRR HIGH_RISK 11 sign-offs canonical** (per framework §33.5.4.3 + ADR-0034):
- Owner + Final Approver + Architect + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance + Privacy + AppSec advisor = 11.
- Crypto SME (BLAKE3 + Mann-Whitney methodology) folds into Architect role; Adversarial reviewer folds into AppSec; peer reviewers contribuem em PR sem sign-off canonical separado.
- Each role validates specific aspect (Architect: design + Crypto SME specialization; Security: STRIDE delta; AppSec: side-channel + pentest; etc.).
- PRR doc captures sign-off matrix + adversarial review + risk acceptance for residuals.

**Risk justification HIGH_RISK:**
- **FF-HR-002**: property tests são primary defense for INV-TENANT-ISOLATION at runtime.
- **FF-HR-005**: validates CTRL-CAS-002 + CTRL-ISO-002 + CTRL-ISO-004 simultaneously.
- Falha aqui = ship sprint sem evidence; first production incident = all credibility lost.

## 3. Customer Impact & Journey

**JTBD (CoreLink team + customer trust):** "Como customer enterprise, eu vejo PRR doc com 11 sign-offs canonical (HIGH_RISK matrix) + 100k property test green + bit rot test passed + RB-FM-253 dry-run executed = confidence to commit DPA."

Indirect: foundation para enterprise customer engagement (S-19 onboarding pre-engagement).

## 4. Capability Mapping

Cross-cutting validação — todos CAPs S-02 (CAP-CAS-004..009).

## 5. Tipo

Test infrastructure + PRR; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Property test suite expandida** em `crates/corelink-worker/tests/prop_cas_read.rs`:
   - `prop_tenant_isolation_read_path` 100k iter.
   - `prop_get_blob_unary_isolation` 10k iter.
   - `prop_find_missing_no_existence_oracle` 10k iter.
   - `prop_negative_cache_correctness` 10k iter race conditions.
2. **Bit rot integration test** em `tests/integration_bit_rot.rs`:
   - 10 scenarios (corrupt single byte, corrupt range, corrupt at end, swap with another blob, truncate, etc.).
   - All scenarios → 100% HashMismatch error rate.
3. **RB-FM-253 dry-run script** em `scripts/rb_fm_253_dry_run.sh`:
   - Automated steps em staging.
   - Captures EVT-017 evidence.
   - Reports drift se runbook steps vs reality differ.
4. **PRR doc** `specs/04_sprints/S02/PRR-S02.md`:
   - Sign-off matrix (11 roles canonical per framework §33.5.4.3 + ADR-0034; Crypto SME folds into Architect; Adversarial folds into AppSec).
   - Risk register summary (residual after mitigations).
   - Promotion gate criteria checklist.
   - Adversarial review summary (link to pentest report).
5. **Adversarial review** (internal pentest):
   - Cross-tenant attempt scenarios.
   - Side-channel timing analysis (Mann-Whitney supplementary run).
   - Bit rot detection rate.
   - Probe storm enumeration cost.
   - Findings em `specs/_audits/2026-XX-XX-pentest-s02.md`.
6. **Sprint review preparation**:
   - All DoD items checked.
   - All sub-tasks SEALED.
   - All EVT-XXX evidences linked.

### 6.2 Out-of-scope

- External pentest (S-20 GA gate).
- TLA+ specs (já existentes; reused).
- Chaos engineering (S-17 sprint).
- Customer-facing PRR signing (only internal team at S-02).

## 7. Anti-Scope

- ❌ Property tests para write path (S-01 WI-S01-006).
- ❌ AC-related tests (S-04).
- ❌ Multipart tests (S-05).
- ❌ Production runbook execution (dry-run staging only).
- ❌ External vendor pentest (S-20).
- ❌ Customer-facing PRR sign-off (internal only at S-02).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: S-02 ship gate validation

  Scenario: prop_tenant_isolation_read_path 100k iter
    Given 50 tenants × 1000 random read ops mix
    When property test runs 100k cases
    Then 0 cross-tenant reads returned wrong tenant's blob
    And test runtime ≤ **5min em CI typical, ≤ 10min upper bound** (cycle 9 SEAL: previous 90s claim was optimistic — 100k iter × per-iter ~5ms tenant_path derive + R2/D1 mock + AuthZ check = ~500s realistic; CI runner CPU varies; upper bound 600s prevents flake while keeping fast feedback). Soft target ≤ 5min para fast PR feedback; hard fail apenas se > 10min.

  Scenario: prop_get_blob_unary_isolation
    Given 10k random (tenant, digest) pairs com mixed ownership
    When GetBlob unary called
    Then cross-tenant attempts return 404 (não 403, masked)
    And own-tenant valid digests return 200 com body byte-identical

  Scenario: prop_find_missing_no_existence_oracle
    Given 10k batch queries
    When Tenant A queries FindMissingBlobs com digests mixed (own + cross-tenant + truly missing)
    Then cross-tenant digests reported as "missing"
    And response indistinguishable from truly missing
    And Mann-Whitney U on response sizes/timings p > 0.05

  Scenario: prop_negative_cache_correctness (eventual-consistency aligned)
    Given 10k race scenarios (write + read concurrent)
    When property runs
    Then 0 incorrect hot reads (no 200 OK returning wrong content; no cross-tenant cache poisoning)
    And eventual convergence dentro de TTL bound (≤ 300s) OR explicit invalidation propagation
    And transient stale 404 acceptable per WI-S02-005 §9.11 (KV não atomic CAS; CF docs propagation "60s OR MORE")
    And cliente retry pattern resolves staleness within bounded window

  Scenario: Bit rot detection 100%
    Given 10 bit-rot scenarios injected directly em R2
    When client GETs via S-02 with verify default-on
    Then 100% return HashMismatch error
    And 0 corrupted bodies returned to cliente

  Scenario: RB-FM-253 dry-run
    Given runbook RB-FM-253 step-by-step em staging
    When automated script executes simulated cross-tenant attempt
    Then alert fires within 30s
    And oncall paged via PagerDuty (synthetic)
    And forensic capture preparation verified
    And runbook drift detected if any steps diverge from reality
    And EVT-017 evidence captured

  Scenario: PRR sign-offs
    Given S-02 implementation complete + tests green + adversarial review done
    When PRR review session
    Then 11 sign-offs canonical collected (Owner + Final Approver já incluídos no count per framework §33.5.4.3)
    And residual risks accepted formally
    And promotion gate criteria checked all green
    And PRR-S02.md committed em main branch

  Scenario: All EVT linked
    Given S-02 sprint contract DoD items
    When checked
    Then 100% items have EVT-XXX evidence linked + verifiable
```

## 9. Design Decisions

### 9.1 Why 100k iter (não 10k)

S-02 é HIGH_RISK; cross-tenant catastrophe. Higher confidence statistical bound: 100k > 10k = 10× more confidence. Runtime ~5min typical em CI runner (per cycle 9 SEAL realistic estimate; previous "~90s" claim was optimistic — 100k iter × ~5ms per-iter = ~500s realistic; CI runner CPU varies; soft target ≤ 5min para fast PR feedback; hard fail apenas se > 10min upper bound).

### 9.2 Why bit rot direct injection (não simulated em test mock)

Real R2 corruption escenario; only honest test uses real R2 admin API to corrupt object. Tests via mock would miss real R2 SDK quirks.

### 9.3 Why RB-FM-253 automated script (não manual run)

Manual = drift risk (runbook outdated; reviewer skips step). Automated = reproducible + drift-detectable + EVT-017 captured cleanly.

### 9.4 Why pentest internal (não external em S-02)

External pentest é S-20 GA gate. S-02 internal pentest is reasonable confidence + cheaper + more iterative.

### 9.5 Why 11 sign-offs canonical (per framework §33.5.4.3 HIGH_RISK matrix + ADR-0034 solo-tier)

Per S-02 sprint contract §14, advisory roles (Crypto SME) são strongly recommended for crypto-touching surface. Treating as mandatory ensures scrutiny.

### 9.6 ADR potencial?

Não. Standard practice; aligned com framework HIGH_RISK matrix.

## 10. Completeness Criteria SOTA

- [ ] **10.6.1** Property test 100k iter green sustained 7d nightly (EVT-002).
- [ ] **10.6.2** Bit rot 10 scenarios → 100% caught (EVT-002).
- [ ] **10.6.3** RB-FM-253 dry-run executed staging + EVT-017 captured (EVT-017).
- [ ] **10.6.4** PRR-S02.md com 11 sign-offs canonical documented (EVT-031).
- [ ] **10.6.5** Adversarial review report + findings em audit doc (EVT-040 internal).
- [ ] **10.6.6** Promotion gate checklist 100% green.
- [ ] **10.6.7** All S-02 DoD items have EVT linked + verifiable.

## 11. DoD

- [ ] Property test suite 100k iter green em CI.
- [ ] Bit rot integration test 10 scenarios.
- [ ] RB-FM-253 automated dry-run script.
- [ ] EVT-017 captured em staging dry-run.
- [ ] PRR-S02.md com 11 sign-offs canonical.
- [ ] Adversarial review report.
- [ ] Sprint review presentation.

## 12. Invariants Validated

- INV-TENANT-ISOLATION (CRITICAL).
- INV-CAS-INTEGRITY (CRITICAL).
- INV-CAS-IMMUTABILITY (CRITICAL).
- INV-CAS-IDEMPOTENCY (CRITICAL).
- INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE (HIGH).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Expanded property test | `crates/corelink-worker/tests/prop_cas_read.rs` | Rust test |
| Bit rot integration test | `crates/corelink-worker/tests/integration_bit_rot.rs` | Rust test |
| RB-FM-253 dry-run script | `scripts/rb_fm_253_dry_run.sh` | Bash |
| PRR doc | `specs/04_sprints/S02/PRR-S02.md` | Markdown |
| Adversarial review report | `specs/_audits/2026-XX-XX-pentest-s02.md` | Markdown |

## 14. Quality Standards SOTA

- **14.6.1** Test reliability ≥ 99% (no flakes em 7d nightly).
- **14.6.2** Documentação: each test has docstring explaining invariant.
- **14.6.3** Coverage: combined unit + integration + property = ≥ 95%.
- **14.6.4** CI runtime ≤ 5min soft target / ≤ 10min hard upper bound for 100k property test (cycle 9+10 SEAL: realistic estimate; 100k × ~5ms per-iter ≈ ~500s).
- **14.6.5** SAST clean.
- **14.6.6** Métricas: CI test duration tracked.
- **14.6.7** Runbook: nenhum novo (RB-FM-253 reused; this WI executes).
- **14.6.8** PRR-S02.md is canonical decision record; semver-locked.
- **14.6.9** Memory bounded em property tests.
- **14.6.10** Cost regression: CI minutes budget tracked.

## 15. Chaos Experiments

Already covered em RB-FM-253 dry-run + bit rot test.

## 16. PRR

This WI **is the PRR**. Output: `PRR-S02.md` com 11 sign-offs canonical.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | prop_tenant_isolation_read_path 100k iter | 3h |
| ST-002 | prop_get_blob_unary_isolation 10k | 1.5h |
| ST-003 | prop_find_missing_no_existence_oracle 10k | 2h |
| ST-004 | prop_negative_cache_correctness 10k race | 2.5h |
| ST-005 | Bit rot 10 scenarios + R2 admin API integration | 3h |
| ST-006 | RB-FM-253 automated dry-run script | 3h |
| ST-007 | Staging deployment for dry-run | 1.5h |
| ST-008 | Adversarial review (pentest internal) | 4h |
| ST-009 | Audit doc authoring | 1.5h |
| ST-010 | PRR-S02.md authoring + 11 sign-offs canonical collection | 3h |
| ST-011 | Sprint review presentation prep | 1.5h |
| ST-012 | All EVT linked verification | 1h |

**Total**: ~27.5h Optimistic; PERT ~32h.

## 18. Dependencies

### Hard blockers

- WI-S02-001..005 SEALED (all features implemented; cannot test partial).
- Staging environment available (S-01 + S-02 deployed).
- RB-FM-253 runbook exists (já existe desde Lote 6).

### Outbound

- S-02 SEAL → S-03 começa (S-03 requires S-02 SEALED).

## 19. Effort PERT

O: 24h, M: 27.5h, P: 50h → PERT 32h.

## 20. Time-boxing

35h hard limit; escalation pentest > 6h → AppSec.

## 21. Observability

CI métricas + Property test runtime + dry-run drift detector.

## 22. Cost Analysis

CI cost: ~$0 (GitHub Actions free). Pentest internal: ~16h engineer time = ~$2k cost-equivalent.

## 23. API Contract

PRR-S02.md is canonical; semver-locked.

## 24. Post-mortem Hooks

- Property test red sustained > 1 week → CRITICAL post-mortem.
- Bit rot test fails detection → CRITICAL.
- PRR review reveals gap → post-mortem + remediation sprint.
- Pentest CRITICAL finding → block S-02 SEAL.

## 25. Rollback / Recovery

Test failures → fix forward (não rollback); engineer responds.

## 26. Security & Privacy

Adversarial review captures STRIDE/LINDDUN delta evidence.

## 27. Knowledge Transfer

PRR-S02.md serves as KT artifact; future S-XX use as template.
Tech talk: "S-02 Ship Gate: 100k Property Tests + Adversarial Review" — 30 min.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Property test flake em CI | M | M | LOW | M | LOW | Regression DB persistente; deterministic seeds |
| R-002 | Bit rot test não cobre todos vectors | M | M | MEDIUM | M | LOW | 10 scenarios cobertura broad; iterar baseado em prod observations |
| R-003 | RB-FM-253 dry-run drift detected late | L | M | MEDIUM | L | LOW | Automated script runs nightly; drift = SEV-3 alert |
| R-004 | Pentest CRITICAL finding bloqueia SEAL | M | M | HIGH (delay sprint) | M | LOW | Pentest early em sprint; remediation buffer |
| R-005 | PRR sign-off staffing missing (Tier 1 reviewers) | M | L | HIGH | M | MEDIUM | Reviewer staffing strategy (Lote 9.5b doc); waiver path se needed |

## 29. Review Checkpoints

1. Design (D+0): QA + Security + Architect approve test strategy.
2. Code (D+5): peer + Security.
3. Adversarial run (D+10): pentest internal.
4. PRR review session (D+13): 11 sign-offs canonical.

## 30. Sign-off (HIGH_RISK 11 roles canonical)

Per S-02 sprint contract §14: 11 sign-offs canonical incluindo Crypto SME (specialized reviewer dentro dos 11).

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.2.

## 32. Anti-patterns evitados

- ❌ Property test < 10k iter (insufficient confidence HIGH_RISK).
- ❌ Mocked R2 em bit rot test (não detect real corruption vectors).
- ❌ Manual runbook run (drift risk).
- ❌ External pentest em S-02 (defer S-20).
- ❌ Customer PRR sign-off (internal only).
- ❌ Skip Crypto SME advisory (crypto-touching surface).
- ❌ PRR sem evidence trail (every DoD item must have EVT linked).

---

**Fim WI-S02-006.** **Sprint S-02 fully specified — 6/6 WIs SOTA.** Próximo Lote 10.3: S-03 (8 WIs HIGH_RISK auth real).
