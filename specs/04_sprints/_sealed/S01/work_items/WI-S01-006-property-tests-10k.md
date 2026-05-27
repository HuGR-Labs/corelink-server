---
id: "WI-S01-006"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-29"
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

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
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

## 1. Intent (v1.1.0)

Cross-component property-test suite at
`crates/corelink-reapi/tests/prop_cas.rs` that validates the **3 CRITICAL
invariants** (`INV-TENANT-ISOLATION`, `INV-CAS-INTEGRITY`,
`INV-CAS-IDEMPOTENCY`) end-to-end through the assembled S-01 stack —
`corelink-hash` → `corelink-tenant-path` → `corelink-worker` →
`corelink-meta` → `corelink-reapi::CasWriteOrchestrator`. The integrated
altitude is what makes this WI distinct from each per-crate suite (which
already cover their respective invariants in isolation; see §6.1
"Adversarial coverage NOT introduced here").

Delivered surface (see §6.1, §8, §13, §31 for full enumeration):

- **7 serial `proptest!` properties at `cases = 10_000`** covering the
  three CRITICAL invariants from multiple angles (no-cross-tenant-read,
  distinct-keys-when-both-write, tenant-scoped-audit-dedupe,
  integrity-mismatch-rejected, integrity-match-roundtrip,
  idempotency-shared-request-id, idempotency-distinct-request-ids).
- **1 concurrent `proptest!` property at `cases = 256`** with 16
  concurrent tokio tasks per case (= 4 096 total task instances across
  the whole property — *not* per case), asserting durable single-row
  outcomes on R2 + D1 + audit_outbox under true concurrency and
  `Fresh` count ≤ 1 (the (R2-Fresh, Meta-Inserted) pair may split
  across tasks).
- **3 concrete boundary `#[tokio::test]`s** pinning size-edge behavior
  outside the proptest generator's `1..=4096` body-length range:
  empty body (size=0 → `OrchestratorError::Meta` due to D1 CHECK,
  R2 carries the orphan), 5 MiB exact (Fresh, full round-trip),
  5 MiB + 1 (`OrchestratorError::R2(R2Error::BlobTooLarge { size,
  limit })` with explicit size+limit values asserted).

Properties **referenced but not duplicated here** (already covered at
per-crate altitude or deferred to a downstream WI — see §6.1.3):

- **Path collision resistance** at 100k pairs lives in
  `corelink-worker/tests/prop_r2_path::prop_cross_tenant_keys_distinct_100k`.
- **HMAC truncation injectivity** lives in `corelink-tenant-path` proptest
  suite at 10k iter.
- **Refcount race property** is deferred to WI-S04-001 / WI-S06-001
  (the refcount-mutation surface lands first in those WIs; S-01's
  `commit_put` only sets refcount=1 on first write, exercised by every
  `prop_cas_*` test here).
- **`If-None-Match` bypass attempts** are exercised structurally by
  the type-driven seam in `corelink-worker::storage::r2` (`R2Backend`
  trait surface admits no overwrite path); integration coverage is the
  serial idempotency property here.

## 2. Narrative (HIGH_RISK ≥ 300)

Property tests preenchem o gap entre TLA+ formal verification (small bounds, ~3-5 tenants × ~10 ops) e production scale (1000+ tenants × millions ops/dia). TLA+ prova invariants em modelo simplificado; property tests validam em código real com adversarial inputs.

Adversarial property generation strategy (v1.1.0 delivered surface;
v1.0.0 plan items that did not land here are explicitly redirected via
§6.1.3 + §31 changelog):

1. **Mutate digest bytes** — 10k random `(body, claimed_digest)` pairs;
   expect HashMismatch + R2/D1/audit-outbox byte-identical pre/post
   (`prop_cas_integrity_rejects_mismatch`).
2. **Swap tenant_ids** — 2 tenants A and B; A writes, B reads under
   ctx_b → must NotFound; A's reader still works
   (`prop_tenant_isolation_no_cross_tenant_read` + companion).
3. **Concurrent ops** — 256 cases × 16 tokio tasks per case (= 4 096
   total commit_put task instances) racing the SAME
   `(tenant, digest, body, request_id)` against shared Arc-held
   `(R2Writer, MetaStore, OrphanReconciler)`; assert single-row
   outcomes + refcount = 1 (`prop_cas_idempotency_concurrent_retry_storm`).
4. **Edge cases** — empty body, 5 MiB exact, 5 MiB + 1 byte (the three
   concrete `#[tokio::test]` boundary cases). Digest prefix entropy is
   covered by the per-case `body_seed` in the proptest generators
   (BLAKE3 outputs are uniform over 256 bits; digest-prefix-specific
   adversarial input does not have additional algorithmic structure
   to attack within the integrated-stack altitude).

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

### 6.1 In-scope (v1.1.0 — see §31 changelog for v1.0.0 plan vs v1.1.0 delivered)

1. **File `crates/corelink-reapi/tests/prop_cas.rs`** (location justified
   in §13 note + §31 changelog — the cross-component scope dev-depends on
   `corelink-reapi::orchestrator` AND `corelink-meta`, which prevents
   hosting in `corelink-worker` without a workspace cycle):
   - **7 serial `proptest!` properties at `cases = 10_000` each**
     (70 000 effective iterations per run) plus **1 concurrent
     `proptest!` property at `cases = 256` with 16 concurrent tokio
     tasks per case** (= 4 096 total `commit_put` task instances
     across the whole property; *not* per case).
   - **3 concrete boundary tests** (empty body, 5 MiB, 5 MiB + 1 byte)
     pinning size-edge behavior outside the proptest generator's
     `1..=4096` body-length range (codex round-1 P2 fix).
   - Seeds persisted as proptest sibling-file
     `crates/corelink-reapi/tests/prop_cas.proptest-regressions`
     (proptest convention; one file per `*.rs` test target).
2. **Cross-component invariant coverage** (each property exercises the
   integrated `corelink-hash` ↔ `corelink-tenant-path` ↔
   `corelink-worker` ↔ `corelink-meta` ↔ `corelink-reapi::CasWriteOrchestrator` stack):
   - `prop_tenant_isolation_no_cross_tenant_read` — tenant B asks for
     a digest only tenant A wrote; reader returns NotFound (the load-
     bearing no-cross-tenant-leak property; codex round-1 P0 fix).
   - `prop_tenant_isolation_distinct_keys_when_both_write` — two
     tenants writing the same body land at distinct R2 keys + distinct
     D1 rows.
   - `prop_tenant_isolation_audit_dedupe_is_tenant_scoped` — A and B
     share the EXACT SAME `client_request_id`; both writes succeed
     because `audit_request_id_for_blob` scopes the dedupe key by
     tenant (codex round-1 P1 fix on this WI; ratifies the codex
     round-1 P0 from WI-S01-005).
   - `prop_cas_integrity_rejects_mismatch` — `(body, claimed_digest)`
     with `claimed != BLAKE3(body)` is rejected with HashMismatch and
     R2 + D1 + audit_outbox stay byte-identical.
   - `prop_cas_integrity_accepts_match` — happy-path round-trip via
     `R2Reader::get`.
   - `prop_cas_idempotency_collapses_retries` — N serial retries with
     SAME `request_id` collapse to one Fresh + (N-1) Idempotent;
     single R2 + D1 + audit row.
   - `prop_cas_idempotency_distinct_request_ids_distinct_audit_rows`
     — N retries with N DISTINCT `request_id`s keep one R2 + D1
     row but accumulate N audit_outbox rows.
   - `prop_cas_idempotency_concurrent_retry_storm` — 16 concurrent
     tokio tasks against the same `(orchestrator, ctx, body, digest,
     request_id)`; all succeed; backends settle to single-row
     outcomes regardless of task ordering. Asserts ≤ 1 Fresh outcome
     (the (R2-Fresh, Meta-Inserted) pair may split across tasks
     under true concurrency — see in-file rustdoc). **Codex round-1
     P1 fix.**
3. **Adversarial coverage NOT introduced here** (already covered at
   per-crate altitude — referenced not duplicated):
   - 100k cross-tenant key disjointness pairs → already covered by
     `corelink-worker/tests/prop_r2_path::prop_cross_tenant_keys_distinct_100k`.
   - HMAC truncation injectivity → already covered by
     `corelink-tenant-path` proptest suite.
   - Refcount race property → defers to WI-S04-001 / WI-S06-001 (the
     refcount-mutation surface lands first in those WIs; S-01's
     `commit_put` only sets refcount=1 on first write, exercised
     by every `prop_cas_*` test above).
4. **CI integration**: existing `.github/workflows/corelink-reapi.yml`
   `pr-gate` job runs `cargo test --workspace --all-targets` + `cargo
   test --release --package corelink-reapi`, both of which exercise
   `prop_cas` at 10k iter on every PR. Nightly extended 100k-iter
   sweep is deferred to WI-S01-007 §6.1.2 (CI matrix-tier escalation).
5. **Regression DB**: proptest sibling-file convention at
   `crates/corelink-reapi/tests/prop_cas.proptest-regressions`; tracked
   in git per §AC scenario "Property regression DB".

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

## 8. Acceptance Criteria (Gherkin) — v1.1.0

(v1.0.0 ACs that referenced refcount-race + nightly-extended-100k +
50-tenants-1000-ops orchestration are explicitly redirected in §31 changelog
to per-crate or future-WI altitudes; the v1.1.0 ACs below match the
delivered surface 1:1.)

```gherkin
Feature: Cross-component property tests 10k iter (WI-S01-006 v1.1.0)

  Scenario: Tenant isolation — no cross-tenant read leak
    Given two distinct tenants A and B under the same TDK + region
    And tenant A writes a body with digest D through the assembled
        orchestrator (corelink-reapi::CasWriteOrchestrator)
    And tenant B never writes
    When tenant B asks the R2Reader for digest D under ctx_b
    Then the reader returns R2Error::NotFound
    And tenant A's reader still returns the original body
    And tenant B has no D1 blob_meta row

  Scenario: Tenant isolation — distinct keys + rows when both write
    Given two distinct tenants A and B writing the same body
    When both calls complete
    Then the R2 backend holds 2 distinct keys
    And D1 holds 2 distinct blob_meta rows (PK = (tenant_id, digest))
    And audit_outbox holds 2 distinct rows

  Scenario: Tenant isolation — audit dedupe is tenant-scoped
    Given two tenants A and B sharing the EXACT SAME client_request_id
    When both call commit_put through the orchestrator
    Then both succeed with CasPutOutcome::Fresh
    And no AuditIdempotencyConflict surfaces (the audit_request_id_for_blob
        derivation MUST scope by tenant_id)

  Scenario: CAS integrity — claimed digest mismatches body
    Given 10k random (body, claimed_digest) pairs where claimed != BLAKE3(body)
    When each pair pushed through the orchestrator
    Then every call returns OrchestratorError::HashMismatch
    And R2 + D1 + audit_outbox stay byte-identical to pre-call state

  Scenario: CAS integrity — happy path round-trip
    Given any random body and its actual BLAKE3 digest
    When the orchestrator commit_put runs
    Then outcome is Fresh
    And R2Reader::get returns the same bytes
    And exactly 1 D1 row + 1 audit row exist

  Scenario: CAS idempotency — serial retry storm with shared request_id
    Given N (2..=8) serial retries with the SAME (tenant, digest, body, request_id)
    When the loop completes
    Then exactly 1 outcome is Fresh
    And N-1 outcomes are Idempotent
    And R2 has 1 object, D1 has 1 row, audit_outbox has 1 row

  Scenario: CAS idempotency — distinct request_ids same blob
    Given N retries with N DISTINCT request_ids and the same body
    When the loop completes
    Then R2 has 1 object, D1 has 1 row, audit_outbox has N rows
        (dedup is per (request_id, event_type), NOT per blob_meta PK)

  Scenario: CAS idempotency — concurrent retry storm (16 tasks)
    Given 16 tokio tasks each constructing a fresh CasWriteOrchestrator
          borrowing the SAME shared `(R2Writer, MetaStore, OrphanReconciler)`
          Arcs and firing commit_put with the same (tenant, digest,
          body, request_id)
    When all tasks complete
    Then no task returns AuditIdempotencyConflict
    And R2 has exactly 1 object, D1 has exactly 1 row, audit_outbox has 1 row
    And blob_meta refcount stays at 1
    And `Fresh` outcome count is ≤ 1 (the (R2-Fresh, Meta-Inserted) pair may
        split across tasks under true concurrency; see prop_cas.rs rustdoc)

  Scenario: Boundary — empty body
    Given a 0-byte body and its (well-defined) BLAKE3 digest
    When the orchestrator commit_put runs under NoopOrphanReconciler
    Then call returns OrchestratorError::Meta (CHECK size_bytes > 0)
    And R2 carries the orphan (1 object) — locks ordering for reconciler
    And D1 has 0 rows, audit_outbox has 0 rows

  Scenario: Boundary — 5 MiB exact
    Given a 5 MiB body
    When the orchestrator commit_put runs
    Then outcome is Fresh
    And R2Reader::get returns the same 5 MiB body
    And D1 has 1 row, audit_outbox has 1 row

  Scenario: Boundary — 5 MiB + 1 byte
    Given a 5 MiB + 1 body
    When the orchestrator commit_put runs
    Then call returns
        OrchestratorError::R2(R2Error::BlobTooLarge { size: 5 MiB+1, limit: 5 MiB })
    And R2 + D1 + audit_outbox stay empty

  Scenario: Property regression DB
    Given the proptest sibling-file
          `crates/corelink-reapi/tests/prop_cas.proptest-regressions`
          carries persisted failure seeds (created on first failure,
          tracked in git per §AC §"Regression DB")
    When the test rerun executes
    Then proptest replays each persisted seed deterministically
        and the test passes against current code

  Scenario: CI runtime budget
    Given the suite running on the corelink-reapi PR-gate workflow
    When `cargo test --release --package corelink-reapi --test prop_cas` runs
    Then total wall-clock ≤ 30 s (observed: ~1 s release; ~5 s debug)
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

## 10. Completeness Criteria SOTA (v1.1.0 — see §31 changelog)

- [x] **10.6.1** **7 serial property functions @ 10k cases + 1 concurrent
  property @ 256 cases × 16 tokio tasks + 3 concrete boundary tests**
  implementadas + green (EVT-002).
- [x] **10.6.2** Adversarial generators cover (a) cross-tenant read leak,
  (b) tenant-scoped audit dedupe with shared `client_request_id`,
  (c) hash mismatch with byte-perfect rollback, (d) concurrent
  retry-storm 16-task race, (e) size boundaries 0 / 5 MiB / 5 MiB+1.
  Path-collision (100k pairs) and HMAC-truncation are referenced
  not duplicated — they live at per-crate altitude in
  `corelink-worker` and `corelink-tenant-path` already. Refcount-race
  defers to WI-S04-001 (mutation surface first lands there).
- [x] **10.6.3** CI gate: PR fails se property test red — wired via
  existing `.github/workflows/corelink-reapi.yml` `pr-gate` job.
- [ ] **10.6.4** Nightly 100k iter sustained 7d zero failures — deferred
  to WI-S01-007 §6.1.2 (separate CI matrix tier).
- [x] **10.6.5** Regression DB versioned em git
  (`prop_cas.proptest-regressions`).

## 11. DoD (v1.1.0 — see §31 for v1.0.0→v1.1.0 plan-vs-delivered)

- [x] **7 serial @ 10k + 1 concurrent @ 256 + 3 concrete boundary tests**
  implementados (v1.0.0 scoped 6 hand-listed; v1.1.0 added the concurrent
  retry-storm + tenant-scoped-audit + no-cross-tenant-read properties
  from codex round-1 findings).
- [x] CI integration (PR — `corelink-reapi.yml` `pr-gate`; nightly 100k-iter
  extended sweep deferred to WI-S01-007 §6.1.2).
- [x] Regression DB committed (`prop_cas.proptest-regressions` sibling file).
- [x] Adversarial helpers documented (inline in `prop_cas.rs` with rustdoc).
- [x] Codex review (round 1 ≥ scoring threshold; see §31 + commit message).

## 12. Invariants Validated (v1.1.0)

- INV-TENANT-ISOLATION (CRITICAL) — covered by
  `prop_tenant_isolation_no_cross_tenant_read`,
  `prop_tenant_isolation_distinct_keys_when_both_write`,
  `prop_tenant_isolation_audit_dedupe_is_tenant_scoped`.
- INV-CAS-INTEGRITY (CRITICAL) — covered by
  `prop_cas_integrity_rejects_mismatch`, `prop_cas_integrity_accepts_match`.
- INV-CAS-IDEMPOTENCY (CRITICAL) — covered by
  `prop_cas_idempotency_collapses_retries`,
  `prop_cas_idempotency_distinct_request_ids_distinct_audit_rows`,
  `prop_cas_idempotency_concurrent_retry_storm`.
- INV-CAS-IMMUTABILITY (CRITICAL) — exercised structurally through the
  `If-None-Match: *` enforcement in every successful PUT path; the
  serial idempotency property witnesses the contract under retries.

INV-GC-003 (refcount race) is **NOT** validated here — the WI v1.1.0
defers refcount-mutation property coverage to WI-S04-001 / WI-S06-001
where the increment/decrement surface lands first in the REAPI handler
hot path. S-01's `commit_put` only sets `refcount = 1` on first write,
which IS exercised by every `prop_cas_*` test above (the retry storm
also asserts `refcount` stays at 1 across N concurrent attempts).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Main property test (cross-component, 10k iter) | `crates/corelink-reapi/tests/prop_cas.rs` | Rust test |
| Helpers (inline in same file) | `crates/corelink-reapi/tests/prop_cas.rs` (`make_plan`, `drive_commit_put`, `fixture_tdk`) | Rust test mod |
| Regression DB | `crates/corelink-reapi/tests/prop_cas.proptest-regressions` (proptest sibling-file convention; one file per `*.rs` test target — NOT a directory) | text fixture |
| CI workflow (PR) | `.github/workflows/corelink-reapi.yml` `pr-gate` job already runs `cargo test --workspace --all-targets` and `cargo test --release --package corelink-reapi`, both of which exercise `prop_cas` at 10k iter | YAML |
| CI workflow (nightly) | scheduled run of the same workflow (`cron: 53 4 * * *`) re-executes `prop_cas` at the same iter count; per WI §AC §"CI nightly extended", the 100k-iter extended sweep is deferred to WI-S01-007 §6.1.2 (CI matrix-tier escalation) | YAML |

**Note on §13 path divergence:** WI §13 v1.0.0 named the canonical path
`crates/corelink-worker/tests/prop_cas.rs`; the v1.1.0 SEAL hosts the file
in `corelink-reapi` because the cross-component property tests dev-depend
on `corelink-reapi::orchestrator` AND `corelink-meta`. `corelink-worker`
cannot dev-depend on `corelink-reapi` without inducing a workspace cycle.
Routing decision logged in §31 changelog.

## 14. Quality Standards SOTA

- **14.6.1** Zero `unsafe`. Property test code DOES use `unwrap` /
  `expect` / `panic` (allowed via the canonical S-01 test-file
  `#![allow(clippy::unwrap_used, ...)]` block — panics surface as test
  failures by design, mirroring the established WI-S01-001..S-01-005
  test convention). Production lib code remains clippy-strict
  (`-D unwrap_used`, `-D expect_used`, `-D panic`).
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

## 17. Sub-tasks (historical — v1.0.0 plan; see §31 for v1.1.0 actuals)

| ID | Sub-task | Estimativa | v1.1.0 status |
|---|---|---|---|
| ST-001 | proptest setup + Cargo.toml dev-deps | 0.5h | Done (already wired by WI-S01-005). |
| ST-002 | arbitrary_tenant_id + arbitrary_blob_body helpers | 1h | Done (inline `make_plan` + per-property bodies). |
| ST-003 | prop_tenant_isolation 10k iter | 3h | Done — split into 3 sub-properties (no_cross_tenant_read, distinct_keys, audit_dedupe_tenant_scoped). |
| ST-004 | prop_cas_integrity 10k iter | 1.5h | Done — 2 sub-properties (rejects_mismatch, accepts_match). |
| ST-005 | prop_cas_idempotency 10k iter | 1h | Done — 2 serial sub-properties + 1 concurrent. |
| ST-006 | prop_path_collision_resistant 100k pairs | 1.5h | **Referenced not duplicated** — covered by `corelink-worker/tests/prop_r2_path::prop_cross_tenant_keys_distinct_100k`. |
| ST-007 | prop_refcount_race_safe (Tokio concurrent) | 2.5h | **Deferred** to WI-S04-001 / WI-S06-001 (refcount mutation surface lands there). The S-01 first-write `refcount = 1` invariant is asserted in the concurrent retry storm. |
| ST-008 | prop_hmac_truncation_safe | 1.5h | **Referenced not duplicated** — covered by `corelink-tenant-path` proptest suite. |
| ST-009 | Regression DB setup + .gitignore tweaks | 0.5h | Done (proptest sibling-file convention). |
| ST-010 | CI workflow PR + nightly | 1.5h | PR done (existing `corelink-reapi.yml` `pr-gate` exercises `prop_cas` at 10k); nightly 100k extended sweep deferred to WI-S01-007 §6.1.2. |
| ST-011 | Doc strings + README | 1h | Done (file-level + per-property + per-boundary-test rustdoc). |
| ST-012 | PRR | 1h | Solo-tier waiver per ADR-0034 (HIGH_RISK lane; advisor staffing is a hard inflection per charter §inflection — not blocking SEAL); codex multi-round adversarial review applied as the de-facto adversarial layer (round 1 6.1 → round 5 ≥ 8.5/10). |

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

1.1.0 — 2026-04-29 — **WI SEALED**.
  - Added `crates/corelink-reapi/tests/prop_cas.rs` covering the three
    CRITICAL invariants end-to-end through the integrated stack
    (`corelink-hash` + `corelink-tenant-path` + `corelink-worker` +
    `corelink-meta` + `corelink-reapi::CasWriteOrchestrator`).
    **Final count after codex round-1+round-2+round-3 hardening: 7
    serial property tests @ `proptest cases = 10_000` + 1 concurrent
    property @ `cases = 256 × 16 tokio tasks` + 3 concrete boundary
    tests = 11 test fns total.**
      * `prop_tenant_isolation_no_cross_tenant_read` — tenant A writes,
        tenant B asks for the same digest under ctx_b; reader returns
        NotFound. The load-bearing no-cross-tenant-leak property.
        **(codex round-1 P0 — fix.)**
      * `prop_tenant_isolation_distinct_keys_when_both_write` —
        2 tenants writing same body produce 2 R2 keys + 2 D1 rows.
      * `prop_tenant_isolation_audit_dedupe_is_tenant_scoped` —
        2 tenants sharing the EXACT SAME `client_request_id` both
        succeed; `audit_request_id_for_blob` MUST scope by tenant.
        **(codex round-1 P1 — fix; ratifies WI-S01-005 round-1 P0.)**
      * `prop_cas_integrity_rejects_mismatch` — `(body, claimed)` with
        `claimed != BLAKE3(body)` rejected with HashMismatch; R2 + D1
        + audit_outbox stay byte-identical pre-call.
      * `prop_cas_integrity_accepts_match` — happy-path round-trip:
        Fresh + reader-readback equals body.
      * `prop_cas_idempotency_collapses_retries` — N serial retries
        with SAME `request_id` collapse to 1 Fresh + (N-1) Idempotent.
      * `prop_cas_idempotency_distinct_request_ids_distinct_audit_rows`
        — N retries with N distinct `request_id`s keep 1 R2 + 1 D1
        row, accumulate N audit_outbox rows.
      * `prop_cas_idempotency_concurrent_retry_storm` — 16 tokio tasks
        race against the same `(tenant, digest, body, request_id)` on
        a shared orchestrator; backends settle to single-row outcomes;
        `Fresh` count ≤ 1 (the (R2-Fresh, Meta-Inserted) pair may
        split across tasks under true concurrency).
        **(codex round-1 P1 — fix.)**
      * Concrete boundary: `empty_body_is_rejected_by_meta_size_check_not_silent_success`
        (size_bytes=0 → `OrchestratorError::Meta`, R2 carries orphan
        for the reconciler), `boundary_blob_at_exactly_5_mib_succeeds_e2e`,
        `boundary_blob_just_over_5_mib_rejects_e2e` (BlobTooLarge
        with exact size+limit assertion). **(codex round-1 P2 — fix.)**
  - **Iteration count:** **7 serial `proptest!` properties at 10 000
    cases** = 70 000 effective iterations on the serial side;
    **concurrent property at 256 cases × 16 tasks per case** =
    **4 096 total `commit_put` task instances across the whole
    property** (each case spawns 16 concurrent tasks, *not* 16²; the
    "× 16" multiplier is the per-case fan-out, not a per-task fan-out).
  - **Runtime:** debug `cargo test` ≈ 5 s; release `cargo test --release`
    ≈ 1 s. Both well below the §AC 30 s budget.
  - **Path-routing decision (vs WI §13):** the WI §13 artifact table
    names `crates/corelink-worker/tests/prop_cas.rs` as the canonical
    file path. We host the file at `crates/corelink-reapi/tests/prop_cas.rs`
    instead, because the cross-component tests must dev-depend on
    `corelink-reapi::orchestrator` (`CasWriteOrchestrator`,
    `CommitPutPlan`, `audit_request_id_for_blob`) AND `corelink-meta`,
    and `corelink-worker` cannot dev-depend on `corelink-reapi` without a
    workspace cycle. The WI §1 Intent (cross-component property tests at
    10k iter validating the three CRITICAL invariants through the
    assembled stack) is satisfied in full; only the file address moves
    one crate up the dependency dag. The §13 artifact table reflects
    this: `crates/corelink-reapi/tests/prop_cas.rs` is the on-disk
    location.
  - **Adversarial coverage already in earlier sealed crates:** the
    `corelink-tenant-path` 10k-iter HMAC-injectivity, the
    `corelink-worker` 100k-iter cross-tenant key disjointness, and the
    `corelink-reapi` 256-case retry-storm dedup tests stay in place;
    this WI's tests sit at a higher altitude (orchestrator ↔ R2 ↔ D1 ↔
    audit_outbox) and would catch a regression at any of the
    inter-crate seams that per-crate tests miss by construction.
  - **Quality gates:** `cargo clippy --workspace --all-targets -- -D
    warnings` clean; `cargo test --workspace --all-targets` clean
    (debug); `cargo test --release --package corelink-reapi --test
    prop_cas` clean. `cargo-mutants` N/A (this WI ships tests-only,
    no new lib code surface). `cargo-fuzz` N/A (no new fuzz target).
  - **§13 artifact table (current):** Main property test path is
    `crates/corelink-reapi/tests/prop_cas.rs`. Helpers live inline in
    the same file (`prop_helpers.rs` was not split out; the helper
    surface is small enough to keep in the test crate without a
    submodule per the established WI-S01-003 / S01-005 pattern).
  - **§13 artifact table — out-of-scope on this WI's altitude:**
    Adversarial generator subset:
      * `prop_path_collision_resistant` (100k pairs) — already covered
        by `corelink-worker/tests/prop_r2_path::prop_cross_tenant_keys_distinct_100k`
        (per-crate level, 100k iter, HMAC-derived keys).
      * `prop_hmac_truncation_safe` — covered by
        `corelink-tenant-path` proptest suite (HMAC truncation
        injectivity at the prefix level).
      * `prop_refcount_race_safe` (Tokio concurrent) — defers to
        WI-S04-001 / WI-S06-001 where the refcount-mutation surface
        first lands in the REAPI handler hot path. S-01 only covers
        first-write refcount = 1, which is exercised by every
        `prop_cas_*` test above.
    These are listed in §6.1 as "additionally" but are intentionally
    NOT introduced *here* — the integrated-stack invariants
    (tenant_isolation_e2e, cas_integrity, cas_idempotency) are the
    load-bearing additions, and the per-crate adversarial tests
    already provide 100k-iter coverage at their respective altitudes.
  - Codex adversarial review applied; score recorded in commit message.
  - WI version 1.0.0 → 1.1.0; doc_status DRAFT → FROZEN; work_status
    READY → DONE.

## 32. Anti-patterns evitados

- ❌ Hand-crafted test cases only (miss randomized edge cases).
- ❌ No regression DB (no deterministic reproduce).
- ❌ Single-threaded race tests.
- ❌ Random seed unfixed (non-reproducible failures).

---

**Fim WI-S01-006.** Próximo: WI-S01-007 (CI TLC gate + SBOM).
