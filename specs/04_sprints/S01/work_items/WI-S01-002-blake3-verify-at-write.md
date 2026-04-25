---
id: "WI-S01-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-01"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "DATA-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s01", "cas", "blake3", "integrity", "ctrl-cas-001", "tla-verified"]
---

# WI-S01-002 — BLAKE3 Hasher + Verify-at-Write (CTRL-CAS-001)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-002 |
| Título | BLAKE3 hasher + write-time verify |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-CAS-001 control implementation) |
| Tier | Todos (CAS hot path) |
| Fase produto | Fase 1 — Remote Cache |

## 1. Intent

Implementar **crate Rust `corelink-hash`** + integração inline no write path que verifica:
```rust
verify(body: &[u8], claimed_digest: &Digest) -> Result<(), HashMismatch>
```

Onde:
- `Digest` é `[u8; 32]` (256-bit BLAKE3 output) — newtype com type-driven invariants.
- Verify recompute hash do body e compara em **constant-time** (`subtle::ConstantTimeEq`).
- Mismatch → reject write com `COR_CAS_DIGEST_MISMATCH` (error_taxonomy) + audit emit + métrica `corelink.cas.put.hash_mismatch_total{tenant_tier}` increment.
- **SIMD-optimized**: BLAKE3 reference impl com `simd` feature; throughput ≥ 2 GB/s single-core target em CF Workers WASM runtime.

Foundation para INV-CAS-INTEGRITY (CRITICAL, TLA+) + INV-DIGEST-VERIFICATION (CRITICAL, TLA+).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

A integridade do CAS depende inteiramente de uma única regra: **`hash(body) == claimed_digest`** verificada **antes** do write em R2. Se este check falha ou é bypassado, o sistema aceita um body que **não corresponde** ao digest que o cliente alegou — quebrando `INV-CAS-INTEGRITY` (CRITICAL).

O ataque direto é **cache poisoning** (THR-T-001): atacante envia `PUT /v1/cas/<digest_X>` com body `Y` (onde `hash(Y) ≠ digest_X`); sistema persiste; próxima leitura de `digest_X` retorna `Y`; cliente confia digest_X mas recebe payload manipulado. Resultado: build outputs poisoned cross-cliente, supply chain attack via cache.

A mitigação é arquitetural via **type-driven security**:
1. **Newtype `Digest([u8; 32])` com private field** — só pode ser construído via `Digest::compute(body) -> Digest` (compute) ou `Digest::from_hex(s) -> Result<Digest>` (parse cliente).
2. **Newtype `VerifiedBody { body: Bytes, digest: Digest }`** — só construível via `VerifiedBody::new(body, claimed) -> Result<Self, HashMismatch>` que faz o verify.
3. **R2 write API só aceita `VerifiedBody`** — by construction, impossível escrever body sem verify prévio.
4. **Constant-time compare** via `subtle` crate — previne timing side-channel em adversarial scenarios (ex: atacante observa latência diff entre `digest[0..1]` correct vs `digest[0..2]` correct para learn digest).

Adversarial considerations:
- **BLAKE3 implementation correctness**: depende crate `blake3` upstream; cargo-audit + dupla-hash fallback (SHA-256 secondary verify) opcional para defense-in-depth.
- **WASM runtime perf**: Cloudflare Workers limitam CPU 50ms per request; BLAKE3 SIMD em WASM degrade ~30% vs native (still ≥ 1.4 GB/s); blob 5 MiB hash em ~3.5ms (well within budget).
- **Hash collision**: BLAKE3 256-bit = 2^128 birthday bound; práticamente impossível em escala humana (universe age × CoreLink lifetime).

**Risk justification para HIGH_RISK:**
- **FF-HR-005**: implementa CTRL-CAS-001 (write-time integrity verify) — controle de segurança canônico cuja falha = INV-CAS-INTEGRITY violation.
- **Blast radius**: cada write CAS depende deste WI; bug aqui afeta 100% dos blobs de 100% dos tenants.
- **Reversibility**: one-way-door — bug shipped permite poisoning; rollback não recupera blobs já poisoned (clientes podem ter consumido).
- **Type-driven invariant**: erro em compile-time (impossible to forget verify) >> runtime check (forgettable em refactor).

Por isso exige: 10–12 sign-offs, TLA+ (`cas_integrity.tla` cobre InvPoisoningRejected), property test 10k iter, fuzz adversarial 1h nightly, criterion benchmark, SAST clean.

## 3. Customer Impact & Journey

**JTBD:** "Como dev de CI, eu preciso garantia matemática que o body que eu vejo num cache hit é exatamente o que foi escrito — não um body manipulado por atacante OR corrupted em bit rot."

**Journey touch-points:**
- **Direct**: cada write CAS bate este check.
- **Customer-visible**: `COR_CAS_DIGEST_MISMATCH` (HTTP 409) com clear error message + next_action ("Recompute digest from body and retry; do not retry with same payload").
- **Indirect**: foundation para client-side verify (S-02 CTRL-CAS-002) — server reject + client verify = defense-in-depth.

## 4. Capability Mapping

- **CAP-CAS-001** (BLAKE3 content addressing) — IMPLEMENTA primary.
- **CAP-CAS-003** (integrity enforce at write-time) — IMPLEMENTA primary.
- Trace: `security_model.md §6.1 CTRL-CAS-001` + `invariant_registry.md §3.2 INV-CAS-INTEGRITY` + `auth_model.md §8.1 layer 4`.

## 5. Tipo e Classificação

- **Tipo:** Foundation (CAS hot path crypto)
- **Lane:** HIGH_RISK
- **Lane forcing factors:** FF-HR-005

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-hash`** (workspace member em `crates/corelink-hash/`):
   - `Digest([u8; 32])` newtype com private field.
   - `Digest::compute(body: &[u8]) -> Digest` (BLAKE3 hash).
   - `Digest::from_hex(s: &str) -> Result<Digest, ParseError>` (cliente input parse).
   - `Digest::to_hex(&self) -> String` (display).
   - `Digest::verify_constant_time(&self, other: &Digest) -> bool` (subtle crate).
2. **`VerifiedBody` envelope**:
   - `VerifiedBody::new(body: Bytes, claimed: Digest) -> Result<Self, HashMismatch>` que faz compute + verify.
   - `VerifiedBody::body() -> &Bytes` + `VerifiedBody::digest() -> &Digest` accessors.
3. **R2 write integration** em `crates/corelink-worker/src/storage/r2.rs`:
   - `R2Writer::put(&self, ctx: TenantCtx, vb: VerifiedBody) -> Result<()>` — accepts only `VerifiedBody`.
4. **Métricas** (S-09 alignment forward):
   - `corelink.cas.put.hash_mismatch_total{tenant_tier, region}` (counter).
   - `corelink.cas.hash_compute_duration_seconds_bucket{blob_size_bucket}` (histogram).
5. **Audit emission** em mismatch:
   - CloudEvent `corelink.cas.poisoning_attempt` com `{tenant_id, claimed_digest, computed_digest, request_id}`.
6. **error_taxonomy mapping**: `COR_CAS_DIGEST_MISMATCH` (HTTP 409, retryable=never).

### 6.2 Out-of-scope (deferred)

- **Client-side verify** (CTRL-CAS-002): WI-S02-003 (S-02 sprint).
- **Dupla-hash SHA-256 fallback**: anti-scope GA; defense-in-depth opcional pós-GA Q1 se CVE em BLAKE3 emerge.
- **Streaming hash** for blobs > memory budget: WI-S05-002 (multipart Merkle); single-blob ≤ 5 MiB neste WI.
- **Hash algorithm pluggability**: BLAKE3-only at GA; SHA-256 backward-compat opcional pós-GA.

## 7. Anti-Scope (expandido para HIGH_RISK)

- ❌ Streaming hash em blobs > 5 MiB (WI-S05-002 multipart).
- ❌ Hash truncation (32 bytes full sempre; truncated digests anti-pattern).
- ❌ HMAC com tenant key na hash function (tenant isolation é via path prefix, não hash mixing).
- ❌ Custom hash combinator (use `blake3::hash()` directo; não roll own).
- ❌ Hash algorithm negotiation via header (BLAKE3 only at GA).
- ❌ `unsafe` em hot path (zero unsafe rule HIGH_RISK).
- ❌ Async hash compute (BLAKE3 é CPU-bound; sync compute em Tokio task se needed).
- ❌ Persist mismatch attempts (audit only; não poisoning attempts em DB queryable — privacy via PII em logs).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: BLAKE3 verify-at-write CAS

  Background:
    Given Tenant A authenticated
    And BLAKE3 hash of bytes "hello world" = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"

  Scenario: Successful write — digest matches body
    Given client sends PUT /v1/cas/d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24
    And body = "hello world"
    When server computes BLAKE3(body)
    Then computed digest equals claimed digest
    And R2 PutObject is called
    And response status is 201
    And metric corelink_cas_put_requests_total{result="ok"} incremented

  Scenario: Cache poisoning attempt — digest mismatch
    Given client sends PUT /v1/cas/d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24
    And body = "goodbye world"  # different content
    When server computes BLAKE3(body)
    Then computed digest != claimed digest
    And R2 PutObject is NOT called
    And response status is 409
    And response error_code is "COR_CAS_DIGEST_MISMATCH"
    And response message hints "Recompute digest from body and retry"
    And audit event "corelink.cas.poisoning_attempt" emitted
    And metric corelink_cas_put_hash_mismatch_total{tenant_tier=A} incremented

  Scenario: Constant-time verify (no timing oracle)
    Given client sends 1000 PUT requests with crafted partial-match digests
    Then verify time variance < 5% (criterion benchmark)
    And no statistical correlation between digest prefix length match and response latency

  Scenario: BLAKE3 throughput SOTA
    Given a 5 MiB blob in WASM runtime
    When BLAKE3 hash is computed
    Then duration_p99 ≤ 3.5ms (≥ 1.4 GB/s in WASM)

  Scenario: Property test — hash determinism
    Given any byte sequence b
    When BLAKE3(b) computed twice
    Then outputs are byte-identical
    And property holds for 10k random inputs (proptest)

  Scenario: Property test — collision resistance heuristic
    Given 10k pairs of distinct random byte sequences (b1, b2)
    When BLAKE3(b1) and BLAKE3(b2) computed
    Then 0 collisions observed
    (sanity check; theoretical prob = 10k^2 / 2^256 ≈ 0)

  Scenario: VerifiedBody envelope is unconstructible without verify
    Given Rust source code reviews
    When grep "VerifiedBody {" excludes "VerifiedBody::new"
    Then 0 matches outside the constructor
    (compile-time enforcement via private field)

  Scenario: WASM compile target works
    When compiled with target wasm32-unknown-unknown
    Then build succeeds
    And BLAKE3 SIMD intrinsics are detected at runtime
```

## 9. Design Decisions

### 9.1 Why BLAKE3 (vs SHA-256, SHA-3, BLAKE2b)

- **Throughput**: BLAKE3 SIMD ~6 GB/s native, ~2 GB/s WASM (vs SHA-256 ~500 MB/s, SHA-3 ~200 MB/s).
- **Tree mode**: BLAKE3 inherently parallelizable (Merkle tree de chunks 1024 bytes); aligned com S-05 multipart.
- **Output length flexible**: 256-bit default; pode estender para 512-bit pós-GA sem migration.
- **Provably secure**: based on Bao tree mode (Aumasson, NCC Group review).
- **Reference impl**: official `blake3` Rust crate; well-audited; no_std support para WASM.

### 9.2 Why constant-time compare (subtle crate)

Adversarial scenario: atacante consegue medir latency diff entre verify de digest com prefix correto vs incorreto. Se verify usa standard `==` byte-by-byte short-circuit, latência reflete prefix match length → leaks digest bits via timing. `subtle::ConstantTimeEq` força full-byte comparison sem branch.

Custo: ~5ns overhead per 32-byte compare; negligible vs hash compute time.

### 9.3 Why type-driven enforcement (vs runtime check)

Runtime: dev pode esquecer verify em refactor; reviewer pode missar; CI test pode não cobrir specific path. Compile-time: impossible to construct `VerifiedBody` without going through `::new()` constructor (private field). Mesmo principle aplicado em S-01 WI-S01-001 `TenantPrefix`.

### 9.4 Single-blob ≤ 5 MiB scope decision

Multipart (blobs > 5 MiB) vai em WI-S05-002 com Merkle tree decomposition. Single-blob simpler — full body em memory + single hash compute. Trade-off: Worker memory budget 128 MiB → 5 MiB body + 5 MiB R2 staging buffer + overhead = ~20 MiB peak; well within budget.

### 9.5 Why 256-bit (não 512-bit ou 128-bit)

- 128-bit: insufficient collision resistance (2^64 birthday).
- 256-bit: 2^128 collision; aligned com industry standard (SHA-256, BLAKE2s, Bitcoin).
- 512-bit: overkill; doubles hash size em DB + R2 path → cost; defer pós-GA se needed.

### 9.6 ADR potencial?

Não identificada decisão arquitetural disruptiva nova requerendo ADR. Patterns reused do ecosystem (BLAKE3 via crate, subtle via crate, type-newtype pattern já estabelecido em WI-S01-001).

## 10. Completeness Criteria SOTA (HIGH_RISK = TODAS aplicáveis)

- [ ] **10.2.1** TLA+ `cas_integrity.tla` (InvPoisoningRejected) verde sustained CI (EVT-022).
- [ ] **10.2.2** Property test 10k iter hash determinism + heuristic collision (EVT-002).
- [ ] **10.2.3** Constant-time variance < 5% em 1000 partial-match attempts (criterion EVT-002).
- [ ] **10.2.4** WASM throughput ≥ 1.4 GB/s single-core (criterion benchmark).
- [ ] **10.2.5** Type-enforcement audit: grep VerifiedBody out-of-constructor = 0 matches.
- [ ] **10.2.6** Audit emission cobertura: 100% mismatch attempts → `corelink.cas.poisoning_attempt` event.
- [ ] **10.2.7** error_taxonomy `COR_CAS_DIGEST_MISMATCH` rendered em SDK errors (S-15 forward).
- [ ] **10.2.8** Cost regression gate (Lote 9.4 §14.10): hash compute hot path benchmark; PR > 10% regression bloqueia.

## 11. Definition of Done

- [ ] Crate `corelink-hash` published em workspace + zero clippy warnings.
- [ ] `Digest` + `VerifiedBody` newtype com private field reviewed.
- [ ] R2Writer integration: only accepts `VerifiedBody`; compile-time check.
- [ ] BLAKE3 SIMD feature enabled em WASM target.
- [ ] Métricas emitidas em handler.
- [ ] Audit emission em mismatch.
- [ ] error_taxonomy entry mapped + tested.
- [ ] Property test 10k iter green.
- [ ] Criterion benchmark commited com baseline.
- [ ] Code review por 2 peers + Crypto SME (BLAKE3 review) + Security lead.
- [ ] Documentação rustdoc completa + 3 examples.
- [ ] PRR sign-offs 10-12 roles documented.

## 12. Invariants

### Implementadas/enforced

- **INV-CAS-INTEGRITY** (CRITICAL, TLA+): `cas_integrity.tla` InvPoisoningRejected — write-time hash check rejeita body que não corresponde a claimed digest.
- **INV-DIGEST-VERIFICATION** (CRITICAL, TLA+): hash mismatch sempre rejected; nunca persiste body com digest incorreto.

### Preservadas (não introduzidas mas honradas)

- **INV-CAS-IDEMPOTENCY** (CRITICAL): same body → same digest → same R2 key (BLAKE3 determinístico).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Hash crate | `crates/corelink-hash/src/lib.rs` | Rust source |
| `Digest` newtype + impls | `crates/corelink-hash/src/digest.rs` | Rust source |
| `VerifiedBody` envelope | `crates/corelink-hash/src/verified_body.rs` | Rust source |
| R2Writer integration | `crates/corelink-worker/src/storage/r2.rs` | Rust source |
| Property test | `crates/corelink-hash/tests/prop_blake3.rs` | Rust test |
| Criterion benchmark | `crates/corelink-hash/benches/blake3_bench.rs` | Rust benchmark |
| Adversarial fuzz target | `crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs` | cargo-fuzz target |

## 14. Quality Standards SOTA (HIGH_RISK = TODAS inclui 14.9 + 14.10)

- **14.2.1** Zero unsafe; zero unwrap em lib code.
- **14.2.2** Documentação rustdoc + 3 examples por public function.
- **14.2.3** Test coverage ≥ 95% (cargo-tarpaulin) — foundation crate.
- **14.2.4** Perf BLAKE3 SIMD ≥ 1.4 GB/s WASM (benchmark sustained).
- **14.2.5** SAST clean: clippy + semgrep + cargo-audit zero findings HIGH/CRITICAL.
- **14.2.6** Métricas RED emitidas per request.
- **14.2.7** Runbook: nenhum runbook novo (mismatches são feature behavior; cobertos por RB-FM-254 cache poisoning).
- **14.2.8** Breaking changes = bump major; algoritmo BLAKE3 lock at v1.x.x.
- **14.2.9** Memory bounded: peak per request ≤ 20 MiB (5 MiB body + 5 MiB staging + overhead).
- **14.2.10** Cost regression gate (§14.10): hash compute per-MiB cost benchmark; PR > 10% regression bloqueia.

## 15. Chaos Experiments (HIGH_RISK = obrigatório)

1. **Inject crafted bodies em test environment**: 10k bodies com claimed_digest deliberately wrong; expect 100% rejection.
2. **BLAKE3 crate version regression**: patch version downgrade; verify CI catches via cargo.lock check.
3. **WASM SIMD intrinsic absence**: simulate missing SIMD; expect throughput degradation but functionality intact.
4. **Memory pressure**: concurrent 25 writes 5 MiB cada (= 125 MiB load); verify Worker doesn't OOM.

## 16. Production Readiness Review (HIGH_RISK = obrigatório)

PRR doc em `specs/04_sprints/S01/PRR-WI-S01-002.md` (a criar pré-merge). Sign-offs 10-12 roles incluindo Crypto SME (BLAKE3 review).

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml + workspace integration | 1h |
| ST-002 | `Digest` newtype + compute/parse/display impls | 2h |
| ST-003 | `VerifiedBody` envelope + private field discipline | 1.5h |
| ST-004 | Constant-time compare via subtle crate | 1h |
| ST-005 | R2Writer integration: API accepts only VerifiedBody | 2h |
| ST-006 | Métricas emit + error_taxonomy mapping | 1h |
| ST-007 | Audit emission CloudEvent on mismatch | 1.5h |
| ST-008 | Property test 10k iter (proptest) | 2h |
| ST-009 | Criterion benchmark + WASM SIMD verify | 2h |
| ST-010 | cargo-fuzz target digest parse | 1.5h |
| ST-011 | Adversarial test 1000 partial-match (constant-time validation) | 1.5h |
| ST-012 | rustdoc + 3 examples per public function | 1h |
| ST-013 | PRR doc + Crypto SME walkthrough | 1.5h |

**Total**: ~19.5h Optimistic; ~24h PERT-weighted (PERT (O+4M+P)/6 com M=19.5h, P=36h).

## 18. Dependencies

### Hard blockers

- **WI-S01-001 SEALED** — `corelink-tenant-path` crate (TenantCtx integration).
- **R2 SDK Rust** crate available.

### Soft blockers

- BLAKE3 crate `blake3` upstream stability — version pin recommended.

### Outbound

- WI-S01-003 (R2 adapter) — uses `VerifiedBody` envelope.
- WI-S01-005 (REAPI handler) — invokes `VerifiedBody::new` em request handling.
- WI-S02-003 (client verify, S-02) — reuses `Digest::compute` + `verify_constant_time`.

## 19. Effort Estimate (PERT, HIGH_RISK)

- **Optimistic (O)**: 16h
- **Most-likely (M)**: 19.5h
- **Pessimistic (P)**: 36h
- **PERT** = (16 + 78 + 36) / 6 = **21.7h**

## 20. Time-boxing & Escalation

- **Time-box**: 25h hard limit (PERT + 15%).
- **Escalation**: ST-005 (R2Writer integration) > 4h → Architect intervention.
- **Mid-WI review**: D+1 (T+50% time-box).

## 21. Observability Plan

Métricas:
- `corelink.cas.put.hash_mismatch_total{tenant_tier, region}` (counter; SLI quality).
- `corelink.cas.hash_compute_duration_seconds_bucket{blob_size_bucket}` (histogram p50/p95/p99).

Alerts:
- `cas_put_hash_mismatch_total > 0` em janela 5min → SEV-2 alert (potential poisoning attack).

Audit events:
- `corelink.cas.poisoning_attempt` per mismatch → S-09 audit chain.

## 22. Cost Analysis (HIGH_RISK = obrigatório + TCO 12m)

- **Per-write CPU cost**: ~3.5ms BLAKE3 compute em 5 MiB blob WASM = ~0.0001 vCPU-ms per byte.
- **TCO 12m** (1k tenants, 10M writes/dia avg, avg blob 1 MiB): ~10M × 1MiB × 12m × 30d = 3.6 PB writes; ~$0 (BLAKE3 é compute-bound; já contabilizado em Worker CPU TCO).

## 23. API / Contract Impact

- `Digest` é semver-locked at v1.x.x (256-bit BLAKE3 only).
- `VerifiedBody` é internal Rust API; não expõe externalmente.
- error_taxonomy `COR_CAS_DIGEST_MISMATCH` é public API — semver commitment.

## 24. Post-mortem Hooks (HIGH_RISK = obrigatório)

Triggers:
- Cache poisoning detectado em produção (any) → CRITICAL post-mortem + Security lead + breach notification consideration.
- Hash mismatch rate > 0.1% sustained 1h → 5-Why obrigatório (atacante storm? bug em client? bit flip em transit?).
- BLAKE3 crate CVE descoberto → immediate post-mortem + dupla-hash fallback consideration.
- WASM throughput regression > 30% → post-mortem + SIMD intrinsic review.

## 25. Rollback / Recovery

- **Hot rollback**: deploy previous Worker WASM binary; takes ≤ 5 min via S-13 progressive rollout.
- **Recovery**: zero data corruption (write-rejected paths não persistem); rollback é safe.
- **Persisted poisoning** (se bug shipped): forensic via R2 hash recompute + audit log; quarantine affected blobs; customer notification.

## 26. Security & Privacy (HIGH_RISK = STRIDE + LINDDUN completos)

### STRIDE delta

- **Spoofing**: digest claimed by client validated against compute(body); spoofing detected.
- **Tampering**: write-time check rejeita tampered body; INV-CAS-INTEGRITY enforced.
- **Repudiation**: audit emission per mismatch; chain integrity S-09.
- **Information disclosure**: constant-time verify previne timing oracle leaks.
- **Denial of service**: hash compute é O(n) em blob size; bounded by Worker CPU 50ms; no DoS vector.
- **Elevation of privilege**: mismatch returns 409, não 5xx; zero privilege escalation surface.

### LINDDUN delta

- **Linkability**: digest é content-derived; não identifies tenant.
- **Identifiability**: blob digest é PII se blob contém PII (cliente responsibility); digest itself é não-identifying.
- **Non-repudiation**: audit chain.
- **Detectability**: constant-time previne side-channel.
- **Disclosure**: covered acima.
- **Unawareness**: mismatch error é clear + actionable.
- **Non-compliance**: GDPR Art. 32 (security measures) — coberto.

## 27. Knowledge Transfer (HIGH_RISK = obrigatório)

- Tech talk: "BLAKE3 + Type-Driven Integrity em Rust+WASM" (≤ 30 min) — record + arquivar.
- Onboarding test: novo engenheiro lê WI-S01-002 + crate (1h) → answers 5 questions sobre poisoning prevention → > 80% correto.
- Doc: `docs/internal/cas-write-integrity.md` (criar pre-merge) — explica type-driven security pattern + reusable em outros WIs.

## 28. Risk Register (HIGH_RISK = inclui detectability + exposure + residual)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|---|
| R-WI-001 | BLAKE3 crate CVE descoberto | L | L | CRITICAL | L | LOW | cargo-audit nightly + dupla-hash fallback opcional |
| R-WI-002 | WASM SIMD intrinsics não detected runtime | M | M | MEDIUM (perf degrade 30%) | M | LOW | Runtime feature detect + criterion benchmark fallback path |
| R-WI-003 | Type-newtype contornado via unsafe | L | L | CRITICAL | L | LOW | Zero unsafe rule + clippy::missing_safety_doc + code review checklist |
| R-WI-004 | Constant-time leak via compiler optim | L | M | HIGH | L | LOW | subtle crate é battle-tested + black_box em benchmark |
| R-WI-005 | Hash collision (theoretical) | L | L | CRITICAL | L | LOW | 256-bit BLAKE3 = 2^128 birthday; impossible em escala humana |
| R-WI-006 | Memory exhaustion concurrent writes | M | M | MEDIUM (FM-403 adjacente) | M | LOW | 5 MiB single-blob hard limit + Worker CPU 50ms guard |
| R-WI-007 | Audit emission falha durante mismatch storm | M | M | HIGH (compliance gap) | M | LOW | Audit emit é fail-closed; mismatch + audit fail = reject + log critical |
| R-WI-008 | error_taxonomy COR_CAS_DIGEST_MISMATCH semantics drift | L | L | LOW | L | LOW | Single source error_taxonomy.md; CI gate verifies SDK alignment |
| R-WI-009 | Cost regression > 10% baseline | M | L | MEDIUM | L | LOW | §14.10 cost regression gate; criterion CI |

## 29. Review Checkpoints (HIGH_RISK = code + design + pre-merge + adversarial)

1. **Design review** (D+0): Architect + Security lead + Crypto SME approve type-driven approach.
2. **Code review intermédio** (D+1): peer 1 review ST-001..006.
3. **Code review final** (D+3): peer 2 + Crypto SME + Security lead approve full.
4. **Adversarial review** (pre-merge): 10k poisoning attempts + 1000 partial-match timing analysis.
5. **Pre-merge gate**: PRR sign-offs documented + criterion benchmark green + cargo-audit clean.

## 30. Sign-off (HIGH_RISK = 10–12 roles)

| Role | Name | Signed | Date |
|---|---|---|---|
| Owner | Gustavo Schneiter | | |
| Final Approver | Gustavo Schneiter | | |
| SRE Lead | TBD | | |
| Security Lead | TBD | | |
| Engineer (S-01 lead) | TBD | | |
| QA Lead | TBD | | |
| Product | TBD | | |
| Compliance Officer | TBD | | |
| Architect | TBD | | |
| AppSec advisor | TBD | | |
| Crypto SME (BLAKE3 + subtle review) | TBD | | |
| Peer reviewer 1 | TBD | | |
| Peer reviewer 2 | TBD | | |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S01-002 — BLAKE3 + verify-at-write (Lote 10.1 — full WI spec series). |

## 32. Apêndice: Anti-patterns evitados

- ❌ Verify post-write em background job (window of vulnerability; rejected).
- ❌ Trust client-computed digest (precisamente o que CTRL-CAS-001 previne).
- ❌ Truncated digest (collision-probable; rejected).
- ❌ Hash em loop sem SIMD (perf regression; benchmark gate enforces).
- ❌ String-based digest representation em hot path (parse cost; use `[u8; 32]`).
- ❌ Standard `==` byte compare (timing oracle; subtle crate enforced).
- ❌ Public field em `Digest`/`VerifiedBody` (type-driven invariant breach; private field rule).

---

**Fim WI-S01-002.** Próximo: WI-S01-003 (R2 adapter single-blob).
