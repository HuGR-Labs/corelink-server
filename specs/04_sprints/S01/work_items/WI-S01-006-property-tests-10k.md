---
id: "WI-S01-006"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-01"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "INVARIANT-REGISTRY"
  - "SECURITY-MODEL"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "FAILURE-MODES"
tags: ["wi", "s01", "test", "property-test", "proptest", "tla-validation"]
---

# WI-S01-006 — Property Tests 10k iter (INV-TENANT-ISOLATION + INV-CAS-INTEGRITY + INV-CAS-IDEMPOTENCY)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-006 |
| Título | Property test suite 10k iter cobrindo CRITICAL invariants |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (proves tenant isolation runtime), FF-HR-005 (validates security controls em vivo) |

## 1. Intent

Suite proptest em `crates/corelink-worker/tests/prop_cas.rs` que valida 3 invariants CRITICAL via property-based testing 10k+ iterações:

```rust
proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn prop_tenant_isolation(
        tenants in vec(any::<TenantId>(), 2..50),
        ops in vec(any::<CasOp>(), 100..1000)
    ) {
        // Storage-layer test (R2Writer + R2Reader direct; not REAPI surface — REAPI read endpoint is S-02 anti-scope).
        // Run write ops + storage-layer reads via WI-S01-003 R2Reader integration helper;
        // assert no path collision (HMAC16 prefixes differ per tenant) and no cross-tenant byte access at R2 layer.
    }

    #[test]
    fn prop_cas_integrity(
        body in any::<Vec<u8>>().prop_filter(|b| b.len() <= 5_242_880),
        claimed_digest in any::<[u8; 32]>(),
    ) {
        let actual_digest = blake3::hash(&body);
        let result = process_put(claimed_digest, &body);
        if claimed_digest == actual_digest.as_bytes() {
            prop_assert!(result.is_ok());
        } else {
            prop_assert_eq!(result.err().unwrap(), Error::DigestMismatch);
        }
    }

    #[test]
    fn prop_cas_idempotency(body in any::<Vec<u8>>()) {
        let digest1 = blake3::hash(&body);
        let digest2 = blake3::hash(&body);
        prop_assert_eq!(digest1, digest2);
    }
}
```

Adicionalmente: **adversarial property tests** cobrindo:
- Path collision (tenant A path == tenant B path) → expect 0 cases em 100k pairs.
- HMAC truncation attack (try to construct valid prefix without TDK access).
- Refcount race conditions (concurrent increments + decrements).
- R2 If-None-Match bypass attempts.

## 2. Narrative (HIGH_RISK ≥ 300)

Property tests preenchem o gap entre TLA+ formal verification (small bounds, ~3-5 tenants × ~10 ops) e production scale (1000+ tenants × millions ops/dia). TLA+ prova invariants em modelo simplificado; property tests validam em código real com adversarial inputs.

Adversarial property generation strategy:
1. **Mutate digest bytes**: 32 random bytes vs body's BLAKE3 → expect 99.999%+ rejected.
2. **Swap tenant_ids**: 2 tenants A e B, ops randomly assigned tenants → assert no operation atribuída a A vê dado de B.
3. **Concurrent ops**: Tokio spawn 100 tasks com ops random → final state consistent (refcount sums match expected).
4. **Edge cases**: empty body (0 bytes), max body (5 MiB), digest com prefixos especiais (all zeros, all ones).

Property test failures são o **early warning system** antes de production deploy:
- TLA+ prova invariants em model.
- Property test prova invariants em código (small to medium scale).
- Pentest prova invariants contra atacante real (S-03 outbound dependency).
- Production deploy prova invariants em scale (S-20 GA gate).

Sem property tests robustos, regressions catastróficas (e.g., refactor que silently quebra path derivation) chegariam em production. Custo: CI runtime ~15-20s por suite execução; trade-off worthwhile.

**Risk justification HIGH_RISK:**
- **FF-HR-002**: property tests são primary defense para INV-TENANT-ISOLATION em código real.
- **FF-HR-005**: validate CTRL-CAS-001 + CTRL-AUTH-004 implementations.
- Falha aqui = catastrophic blast radius (regressões silently shipped).

## 3. Customer Impact & Journey

**JTBD:** "Como CoreLink team, preciso confiance que cada PR mantém invariants CRITICAL — não só passes em 5 hand-crafted tests mas em 10k+ randomly-generated adversarial inputs."

Indirect customer impact: trust + reliability sustained.

## 4. Capability Mapping

Validação cross-cutting — todos CAPs S-01 (CAP-CAS-001/002/003).

## 5. Tipo

Test infrastructure; HIGH_RISK; FF-HR-002 + FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-worker/tests/prop_cas.rs`**:
   - 6 property test functions (3 main invariants + 3 adversarial).
   - 10k iter padrão; 100k em CI nightly.
   - Seeds persistidos em `tests/proptest-regressions/` (deterministic reproduce).
2. **Adversarial test functions**:
   - `prop_tenant_isolation_adversarial` — 50 tenants × random ops.
   - `prop_path_collision_resistant` — 100k pairs (tenant_id_a, tenant_id_b) → 0 collisions.
   - `prop_refcount_race_safe` — concurrent 100 increments + 100 decrements.
   - `prop_hmac_truncation_safe` — try construct valid path without TDK; expect impossible.
3. **Helper module** `prop_helpers.rs` com:
   - `arbitrary_tenant_id()` — UUID v7 generator (canonical per data_model.md §3 tenant_id schema).
   - `arbitrary_blob_body(max_size)` — Vec<u8> with bounded size.
   - `arbitrary_cas_op()` — enum {Put, GetStorageLayer} com payload (Get exercises R2Reader directly; REAPI Read endpoint is S-02 anti-scope; Delete is S-06 GC scope).
4. **CI integration**: `cargo test --test prop_cas --release` em PR; nightly extended (100k iter).
5. **Regression DB**: `tests/proptest-regressions/` versioned em git.

### 6.2 Out-of-scope

- TLA+ specs (já existentes; reused but não modificados aqui).
- Fuzz testing (cargo-fuzz; em WI-S01-007 CI).
- Pentest (S-03 outbound).
- Performance/load test (criterion separate; em respective WIs).

## 7. Anti-Scope

- ❌ Tests para read path (S-02).
- ❌ Tests para AC (S-04).
- ❌ Tests para multipart (S-05).
- ❌ Test fixtures real customer data (sempre hypothetical/random).
- ❌ Manual hand-crafted edge cases (cobertos via integration tests; property tests são para randomized).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Property tests 10k iter

  Scenario: Tenant isolation property (storage-layer)
    Given 50 tenants e 1000 random CAS ops distributed (Put + storage-layer GetStorageLayer)
    When suite runs (via WI-S01-003 R2Writer/R2Reader; REAPI Read endpoint NOT exercised — S-02 scope)
    Then 0 cross-tenant byte access in 10k iterations (HMAC16 path injectivity holds)
    And test runtime ≤ 30s em CI

  Scenario: CAS integrity property
    Given 10k random body+digest pairs
    When each pair processed
    Then mismatched pairs → DigestMismatch error
    And matched pairs → Ok
    And 0 false positives/negatives

  Scenario: CAS idempotency property
    Given any byte sequence
    When BLAKE3 hashed twice
    Then outputs byte-identical (10k iter)

  Scenario: Path collision resistance
    Given 100k random (tenant_id_a, tenant_id_b) pairs com a != b
    When TenantPath::derive called for both
    Then 0 collisions observed

  Scenario: Refcount race safety (D1 single-row atomic)
    Given 100 concurrent BlobMetaStore.increment_refcount + 100 concurrent decrement_refcount (S-01 scope per WI-S01-004 §6.1.4)
    When all complete
    Then final refcount == initial (no drift; D1 single-statement atomic UPDATE RETURNING)

  Scenario: Property regression DB
    Given seed for known-failure case persisted
    When test runs
    Then deterministic reproduce of failure
    And fix verifies same seed passes

  Scenario: CI nightly extended
    Given proptest config 100k cases
    When nightly suite runs
    Then 0 failures
    And runtime ≤ 5 min
```

## 9. Design Decisions

### 9.1 Why proptest (vs quickcheck)

proptest é Rust-native, more idiomatic, better shrinking, supports persistent regression DB. quickcheck older + less ergonomic.

### 9.2 Why 10k iter (default)

- 10k cobre 99.9%+ statistical confidence em property holds.
- CI runtime ≤ 30s aceitável.
- Nightly 100k para deeper exploration.

### 9.3 Why persistent regression DB

Property failures revelam edge cases. Sem regression DB, fix verification é manual ("did I fix THE failing case or just A failing case?"). Persistent DB → deterministic reproduce.

### 9.4 Why concurrent property tests via Tokio

Race conditions só detectables com concurrent execution. Single-threaded property tests miss races. Tokio spawn N tasks + assert final state consistent.

### 9.5 ADR potencial?

Não. Standard testing practice.

## 10. Completeness Criteria SOTA

- [ ] **10.6.1** 6 property test functions implementadas + green em 10k iter (EVT-002).
- [ ] **10.6.2** Adversarial generators cover path collision + HMAC truncation + refcount race.
- [ ] **10.6.3** CI gate: PR fails se property test red.
- [ ] **10.6.4** Nightly 100k iter sustained 7d zero failures (EVT-002).
- [ ] **10.6.5** Regression DB versioned em git.

## 11. DoD

- [ ] 6 property tests implementados.
- [ ] CI integration (PR + nightly).
- [ ] Regression DB committed.
- [ ] Adversarial helpers documented.
- [ ] Code review.

## 12. Invariants Validated

- INV-TENANT-ISOLATION (CRITICAL).
- INV-CAS-INTEGRITY (CRITICAL).
- INV-CAS-IDEMPOTENCY (CRITICAL).
- INV-CAS-IMMUTABILITY (CRITICAL).
- INV-GC-003 (HIGH; refcount race).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Main property test | `crates/corelink-worker/tests/prop_cas.rs` | Rust test |
| Helpers | `crates/corelink-worker/tests/prop_helpers.rs` | Rust test mod |
| Regression DB | `tests/proptest-regressions/` | text fixture |
| CI workflow (PR) | embedded em `.github/workflows/cas_foundation.yml` (rust-test job per WI-S01-007 §6.1.1) | YAML |
| CI workflow (nightly) | embedded em `.github/workflows/nightly.yml` (proptest-extended job per WI-S01-007 §6.1.2) | YAML |

## 14. Quality Standards SOTA

- **14.6.1** Zero unsafe; zero unwrap em test code.
- **14.6.2** Documentação: cada property test tem `///` doc string explaining what holds.
- **14.6.3** Coverage: property tests + integration tests = ≥ 95% line coverage.
- **14.6.4** Runtime: PR ≤ 30s; nightly ≤ 5 min.
- **14.6.5** SAST clean.
- **14.6.6** Métricas: CI emit duration_seconds.
- **14.6.7** Runbook: nenhum.
- **14.6.8** Breaking changes em property test signatures = test refactor.
- **14.6.9** Memory bounded.
- **14.6.10** Cost regression: CI runtime budget tracked.

## 15. Chaos Experiments

N/A — testing infrastructure não tem chaos.

## 16. PRR

PRR + QA lead + Architect.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | proptest setup + Cargo.toml dev-deps | 0.5h |
| ST-002 | arbitrary_tenant_id + arbitrary_blob_body helpers | 1h |
| ST-003 | prop_tenant_isolation 10k iter | 3h |
| ST-004 | prop_cas_integrity 10k iter | 1.5h |
| ST-005 | prop_cas_idempotency 10k iter | 1h |
| ST-006 | prop_path_collision_resistant 100k pairs | 1.5h |
| ST-007 | prop_refcount_race_safe (Tokio concurrent) | 2.5h |
| ST-008 | prop_hmac_truncation_safe | 1.5h |
| ST-009 | Regression DB setup + .gitignore tweaks | 0.5h |
| ST-010 | CI workflow PR + nightly | 1.5h |
| ST-011 | Doc strings + README | 1h |
| ST-012 | PRR | 1h |

**Total**: ~17h Optimistic; PERT ~21h.

## 18. Dependencies

- WI-S01-001/002/003/004/005 SEALED (todos os crates produced; property tests testam end-to-end).

### Outbound

- S-02 (read path) reuses helpers via `pub use` em test mod.

## 19. Effort PERT

O: 14h, M: 17h, P: 28h → PERT 18.7h.

## 20. Time-boxing

20h hard limit.

## 21. Observability

CI métricas: test duration, pass rate.

## 22. Cost Analysis

CI cost: GitHub Actions free tier; runtime 30s/PR + 5min nightly = ~$0 marginal.

## 23. API Contract

Test signatures internal; semver não-applicable.

## 24. Post-mortem Hooks

- Property test red em PR sustained > 1 week → post-mortem (regression mascarada).
- Nightly 100k red → CRITICAL post-mortem.

## 25. Rollback / Recovery

N/A.

## 26. Security & Privacy

Property test gerados são all hypothetical/random; nunca real customer data.

## 27. Knowledge Transfer

Tech talk: "Property-Based Testing em Rust: Beyond Unit Tests" — 30 min.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Property test flaky (random seed-dependent failures) | M | L | LOW | L | LOW | Regression DB persists failing seeds; deterministic reproduce |
| R-002 | False positive (test fails but invariant holds) | L | M | MEDIUM | L | LOW | Code review property assertion logic; manual verify failing case |
| R-003 | False negative (test passes but invariant violated em prod) | L | M | HIGH | L | LOW | TLA+ + property + pentest = 3 layers; prod monitoring |
| R-004 | CI runtime budget exceeded | M | L | LOW | L | LOW | Iter count tunable; nightly extended |
| R-005 | Adversarial generator insufficient (misses edge case) | M | L | MEDIUM | L | LOW | Code review generators; iterate based on production observations |

## 29. Review Checkpoints

1. Design (D+0): QA + Architect.
2. Code (D+2): peer.
3. Pre-merge: CI green.

## 30. Sign-off

10 roles; QA lead emphatic.

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.1.

## 32. Anti-patterns evitados

- ❌ Hand-crafted test cases only (miss randomized edge cases).
- ❌ No regression DB (no deterministic reproduce).
- ❌ Single-threaded race tests.
- ❌ Random seed unfixed (non-reproducible failures).

---

**Fim WI-S01-006.** Próximo: WI-S01-007 (CI TLC gate + SBOM).
