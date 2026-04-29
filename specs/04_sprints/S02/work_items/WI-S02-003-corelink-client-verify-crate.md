---
id: "WI-S02-003"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
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
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "FAILURE-MODES"
tags: ["wi", "s02", "client-verify", "blake3", "ctrl-cas-002", "sdk-side", "ffi-ready"]
---

# WI-S02-003 — Crate `corelink-client-verify` (Default-On Bit-Rot + Cache Poisoning Detection)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-02](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S02-003 |
| Título | Crate corelink-client-verify (lib SDK side default-on) |
| Sprint | S-02 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (CTRL-CAS-002 implementation; default-on enforcement crítico) |
| Tier | Todos (SDK FFI consume em S-15) |
| Fase produto | Fase 1 — Remote Cache |

## 1. Intent

Crate Rust **`corelink-client-verify`** stand-alone (sem dependências de Worker internals) que SDK clients (Python pyO3, Go cgo, JS/TS WASM) consomem para verify integridade post-download:

```rust
pub struct ClientVerifier {
    config: VerifyConfig,
}

#[derive(Debug, Clone)]
pub struct VerifyConfig {
    /// Default true; opt-out requires explicit builder method (não direct field set).
    /// Cycle 9 SEAL fix: fields agora `pub(crate)` (private to crate) + builder pattern obrigatório
    /// para enforce default-on intent canonical (CTRL-CAS-002); evita silent opt-out via field assign.
    pub(crate) enabled: bool,
    pub(crate) warn_on_optout: bool,
}

impl VerifyConfig {
    /// Canonical builder: returns default config (`enabled: true`, `warn_on_optout: true`).
    pub fn new() -> Self { Self::default() }

    /// Explicit opt-out path; emits warning log + métrica counter (CI gate verifies opt-out warning emitted).
    pub fn disabled() -> Self { Self { enabled: false, warn_on_optout: true } }
}

impl Default for VerifyConfig {
    fn default() -> Self { Self { enabled: true, warn_on_optout: true } }
}

// === Rust-native API surface (idiomatic; for direct Rust consumers e.g. corelink-server, corelink-cli) ===
impl ClientVerifier {
    pub fn new(config: VerifyConfig) -> Self;

    /// Verify body matches expected digest. Default-on per VerifyConfig.
    pub fn verify(&self, body: &[u8], expected: &Digest) -> Result<(), VerifyError>;

    /// Stream-aware verify: incrementally hash chunks while downloading.
    pub fn verify_stream<R: AsyncRead + Unpin>(
        &self,
        reader: R,
        expected: &Digest,
    ) -> impl Stream<Item = Result<Bytes, VerifyError>>;
}

// === C-ABI FFI surface (separate module `ffi.rs`; consumed via cbindgen → Python pyO3 + Go cgo in S-15 FFI sprint) ===
// NOTE: JS/WASM consumers use a separate Rust → wasm-bindgen pipeline (NOT cbindgen + C ABI):
//   - WASM target `wasm32-unknown-unknown` com `#[wasm_bindgen]` attributes em separate module `wasm.rs` (feature-gated `wasm`).
//   - wasm-bindgen generates JS shim + .d.ts; consumes Rust-native types directly (Result<>, async/await, structs).
//   - No cbindgen header generated for WASM target; C ABI surface (ffi.rs) is consumed only by Python pyO3 + Go cgo.
// Public C-compatible signatures (no Rust-only types; explicit error codes; opaque handle pattern):
#[no_mangle]
pub extern "C" fn corelink_verifier_new(config_enabled: u8, config_warn_optout: u8) -> *mut ClientVerifier;
#[no_mangle]
pub extern "C" fn corelink_verifier_verify(
    handle: *const ClientVerifier,
    body_ptr: *const u8, body_len: usize,
    digest_hex_ptr: *const u8, digest_hex_len: usize,
    out_error_code: *mut i32,
) -> i32;  // 0 = OK; non-zero = error (mapped to VerifyError variants via error_code lookup table)
#[no_mangle]
pub extern "C" fn corelink_verifier_free(handle: *mut ClientVerifier);
// Stream variant deferred to S-15 FFI sprint (async cross-language complexity; sync verify suffices for SDK MVP).

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("Digest mismatch: expected {expected}, computed {computed}")]
    DigestMismatch { expected: String, computed: String },

    #[error("Verify disabled (opt-out); proceed at own risk")]
    VerifyDisabled,
}
```

Integra com `corelink-hash` crate (S-01 WI-S01-002) reusing `Digest` newtype + constant-time compare. Crate é **ABI-stable** porque vai ser consumido por FFI wrappers em S-15 (Python/Go/JS). Default-on garante CTRL-CAS-002 enforced by-construction.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Server-side write verify (CTRL-CAS-001 em S-01 WI-S01-002) protege contra cliente que envia body manipulado. Mas há cenários onde server alone não é suficiente:

1. **Bit rot em R2** (FM-051): R2 storage media decai over time; bit pode flip silently. Server reads "blob" mas é corrupted version. Sem client verify, cliente consome corrupted body como legítimo.
2. **Cache poisoning at transit** (THR-T-001 em S-02 surface): atacante MitM (improvável dado TLS 1.3, mas defense-in-depth): server returns blob correto mas atacante substitui em transit. Client verify catches.
3. **Server bug post-write** (FM-300 adjacent): refactor introduce silent corruption em R2 read path; client verify is last line of defense.

**Por que default-on:**
Opt-in verify = optional security; muitos clients disable for "performance" → silently shipped vulnerable. Default-on flip the burden: cliente que opt-out tem que explicitly fazer + emit warning log. CTRL-CAS-002 spec specifically mandates default-on.

**Why separate crate (não inline em SDK):**
- 3 SDKs (Python/Go/JS) reuse same Rust truth via FFI (WI-S15-004). Implementação única em Rust = single source of truth; impossible to forget verify em uma das 3 langs.
- Stand-alone (sem Worker internals deps) → small binary footprint para SDK distribution; embedded em FFI wrappers.
- ABI-stable signatures = SemVer commitment para SDK consumers.

**Stream-aware verify** é diferencial:
Naive: download full body → hash full body → compare (memory peak = full blob; problematic em SDK tight budget Python pyO3 / JS WASM). Stream-aware: hash incrementally durante download (BLAKE3 supports incremental update); memory peak ≤ 1 MiB chunk.

**Detection latency clarification (cycle 9 SEAL fix)**: BLAKE3 incremental hash detects corruption **at end-of-stream** (final hash compute); intermediate chunk integrity is not validated mid-download (no Merkle proof per chunk em current scope). Fail-fast em mid-download requires per-chunk Merkle proofs (deferred to S-05 multipart Merkle WI-S05-005); for S-02 single-blob streaming, detection é at-end. Memory benefit (incremental hash não buffer full body) holds independently of detection-latency tradeoff.

**Risk justification HIGH_RISK:**
- **FF-HR-005**: implementa CTRL-CAS-002 — security control canonical. Falha aqui = INV-CAS-INTEGRITY violated em client side.
- **Reversibility**: bug em verify shipped = cliente consome corrupted/poisoned body sem detect; data integrity claim falsa.
- **Customer trust**: "client verify default-on" é marketing claim; falha aqui = contract violation.

Por isso: 11 sign-offs canonical HIGH_RISK incl. Crypto SME (BLAKE3 review consistente com S-01 WI-S01-002), property test 10k iter mismatch detection, ABI-stable interface contract (FFI integration tests = S-15 forward), constant-time compare (subtle crate).

## 3. Customer Impact & Journey

**JTBD (dev integrating SDK):** "Eu uso CoreLink Python SDK; quero garantia matemática que o body que eu recebo de `client.get_blob(digest)` é exatamente o que foi escrito — sem ter que pensar em verify (default-on)."

**Customer-visible:**
- SDK API: `client.get_blob(digest)` retorna bytes; verify happens transparentemente.
- Mismatch → `DigestMismatchError` exception (Python) / `error.DigestMismatch` (Go) / `Promise.reject(DigestMismatchError)` (JS) — claras + actionable.
- Opt-out: `client.get_blob(digest, verify=False)` emit warning log "Verify opt-out; proceed at own risk".

## 4. Capability Mapping

- **CAP-CAS-005** (Client-side verify integrity SDK defaults on) — IMPLEMENTA primary.
- Trace: `security_model.md §6.1 CTRL-CAS-002` + `invariant_registry.md §3.2 INV-CAS-INTEGRITY`.

## 5. Tipo e Classificação

Library (FFI-ready); HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **Crate `corelink-client-verify`** em `crates/corelink-client-verify/`:
   - `Cargo.toml` standalone (deps: `corelink-hash` from S-01, `subtle`, `bytes`, `tokio` opt-in via feature `stream`).
   - **Two-layer API surface design** (resolves ABI/FFI contradiction):
     - `lib.rs` exposes Rust-native idiomatic API (`Result<(), VerifyError>`, `impl Stream<Item=...>`, `AsyncRead`) — for direct Rust consumers (corelink-server, corelink-cli).
     - `ffi.rs` (feature-gated `ffi`) exposes **C-ABI compatible** `extern "C"` functions com opaque handle pattern + explicit error codes (no `Result<>`, no generics, no `impl Trait`) — for cbindgen → S-15 FFI wrappers.
     - Build targets: default `rlib` (Rust-native consumers); `cdylib` apenas com `--features ffi` (C-ABI surface via `extern "C"` em `ffi.rs`).
   - cbindgen header generation gated em CI: `cbindgen --crate corelink-client-verify --output target/include/corelink_verify.h`; produces ONLY signatures from `ffi.rs` module (Rust-native types invisible).
2. **`ClientVerifier::verify` method**:
   - Sync compute BLAKE3(body) reusing `corelink_hash::Digest::compute`.
   - Constant-time compare via `subtle::ConstantTimeEq`.
   - Return `Result<(), VerifyError>`.
3. **`ClientVerifier::verify_stream` async method**:
   - Feature-gated `stream` (Tokio AsyncRead).
   - Incremental BLAKE3 update durante chunk read.
   - Stream output Bytes ao consumer; fail-fast em final mismatch.
4. **`VerifyConfig` builder pattern**:
   - `VerifyConfig::default()` = `enabled: true, warn_on_optout: true`.
   - `VerifyConfig::disabled()` = explicit opt-out (rare; emit warning).
5. **Warning log on opt-out**:
   - `tracing::warn!` ou stderr fallback: "client_verify=disabled; proceed at own risk".
   - Métrica counter: `corelink_client_verify_optout_total{lang}` (forward to telemetry S-15).
6. **Property tests**:
   - `prop_verify_match_always_ok` (10k iter).
   - `prop_verify_mismatch_always_err` (10k iter).
   - `prop_verify_constant_time` (1000 partial-match samples).
7. **Examples** em `examples/`:
   - `verify_simple.rs` — basic usage.
   - `verify_stream.rs` — async stream usage.
   - `disable_verify.rs` — opt-out warning.

### 6.2 Out-of-scope (deferred)

- **FFI wrappers** (Python/Go/JS): WI-S15-004 (S-15 sprint).
- **SDK auto-routing** size hint (GetBlob unary vs Read streaming): WI-S15-001/004.
- **Metric emission to Grafana**: forward to S-15 SDK telemetry; aqui apenas counter local.
- **C ABI binding generation tools**: cbindgen pre-config; usage em S-15.

## 7. Anti-Scope (expandido para HIGH_RISK)

- ❌ Crate dependa de Worker internals (deve ser stand-alone para FFI distribution).
- ❌ Hash algorithm pluggability (BLAKE3 only; alinhado com server S-01).
- ❌ Sync verify para large blobs (use stream para > 4 MiB; memory bounded).
- ❌ Verify cached em-memory (Worker é stateless; cliente pode cache mas não é responsabilidade desta crate).
- ❌ Custom error variant codes (use error_taxonomy COR_CAS_DIGEST_MISMATCH semantics).
- ❌ Dependência de `tokio` em default features (feature-gated `stream`).
- ❌ Public field em `ClientVerifier` ou `VerifyConfig` (private + builder pattern).
- ❌ Default-off variant ANY where (CTRL-CAS-002 mandates default-on).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Client-side verify default-on

  Scenario: Default config enables verify
    When ClientVerifier::new(VerifyConfig::default()) created
    Then verifier.config.enabled == true
    And verifier.config.warn_on_optout == true

  Scenario: Sync verify happy path
    Given body = "hello world" + expected = BLAKE3("hello world")
    When verifier.verify(body, &expected) called
    Then Result::Ok(()) returned

  Scenario: Sync verify mismatch detected
    Given body = "hello world" + expected = some_other_digest
    When verifier.verify(body, &expected)
    Then Result::Err(VerifyError::DigestMismatch { ... }) returned
    And error message includes both expected + computed hex

  Scenario: Stream verify fail-fast
    Given AsyncRead stream of 5 MiB body com tampering at byte 4 MiB
    When verifier.verify_stream(reader, &expected_digest) consumed
    Then mid-stream chunks pass through OK
    And final assertion at end-of-stream returns DigestMismatch
    And memory peak ≤ 1 MiB chunk + overhead ≤ 2 MiB total

  Scenario: Opt-out emit warning (builder pattern enforced; cycle 12 SEAL fix)
    Given VerifyConfig::disabled() (canonical builder; private fields no longer constructible directly)
    When verifier created
    Then tracing::warn! emitted: "client_verify=disabled; proceed at own risk"
    And counter corelink_client_verify_optout_total incremented

  Scenario: Property test — 10k random body+digest pairs
    Given 10k iter
    When matched pairs verified
    Then 100% Ok
    When mismatched pairs verified
    Then 100% DigestMismatch error

  Scenario: Property test — constant-time compare
    Given 1000 partial-match digests (different prefix lengths)
    When verify called for each
    Then variance em verify duration < 5% (criterion benchmark)
    And no statistical correlation prefix length vs duration

  Scenario: ABI stability for FFI (separate ffi.rs module)
    Given crate built as cdylib com `--features ffi`
    When examined via cbindgen com scope limited to ffi.rs module
    Then C-compatible signatures generated (opaque handle pattern + i32 error codes; no Result<>, no impl Trait, no generics)
    And no Rust-only types em FFI public API surface (lib.rs Rust-native types stay private to FFI consumer view)
    And Rust-native API (lib.rs com Result<>, impl Stream<>) continues to work for direct Rust consumers (corelink-server, corelink-cli)

  Scenario: Stand-alone build (no Worker deps)
    When cargo build -p corelink-client-verify --no-default-features
    Then build succeeds
    And dep tree contains only: corelink-hash, subtle, bytes, thiserror
```

## 9. Design Decisions

### 9.1 Why default-on (não opt-in)

Opt-in security é optional security; muitos cliente disable for "performance" → silent vulnerability. Default-on inverts burden: opt-out requires explicit + warning. CTRL-CAS-002 spec mandates default-on. Industry parallel: TLS é default-on em modern HTTP libs (não opt-in).

### 9.2 Why standalone crate (não inline em SDK)

3 SDK languages × 1 verify implementação = drift risk. Single Rust truth + FFI wrappers = guarantee mesma logic em 3 langs. Crate standalone também = small binary footprint para distribution.

### 9.3 Why stream verify (não só sync)

Memory budget tight em SDK side (Python pyO3 ~50 MiB OK; JS WASM ~30 MiB OK). 5 MiB blob sync verify needs ~10 MiB peak (body + hash state). Stream incremental update keeps peak ≤ 2 MiB. Fail-fast em mismatch = cancel download mid-stream.

### 9.4 Why subtle crate (não custom constant-time)

`subtle` é battle-tested (used em ring, rustls, ed25519-dalek). Custom constant-time é hard to get right; compiler optimizations can break naive impl. `subtle` provides `ConstantTimeEq` trait que compile-checked.

### 9.5 Why Tokio feature-gated (não default)

Python pyO3 wrappers podem usar sync API (Python's own asyncio handles concurrency). Go cgo prefer sync. JS WASM uses Promise; doesn't need Tokio. Default sync API + opt-in stream feature = smaller default footprint.

### 9.6 Why error_taxonomy alignment (COR_CAS_DIGEST_MISMATCH)

Crate emits Rust-typed `VerifyError`; FFI wrappers map to language-idiomatic exceptions. Mapping: `DigestMismatch` → COR_CAS_DIGEST_MISMATCH semantics. Single source of truth em error_taxonomy.md (Lote 9.4).

### 9.7 ADR potencial?

Não. Patterns reused do S-01 WI-S01-002 + standard FFI design.

## 10. Completeness Criteria SOTA

- [ ] **10.3.1** Property test 10k pairs → 100% accuracy detection (EVT-002).
- [ ] **10.3.2** Constant-time variance < 5% (criterion EVT-002).
- [ ] **10.3.3** Stream verify memory peak ≤ 2 MiB para 5 MiB blob (EVT-002).
- [ ] **10.3.4** Stand-alone build verified (no Worker deps).
- [ ] **10.3.5** ABI stability documented + cbindgen verified.
- [ ] **10.3.6** Default-on enforcement: Rust source review zero default-off paths.
- [ ] **10.3.7** Cost regression gate (§14.10): hash compute hot path benchmark.

## 11. DoD

- [ ] Crate `corelink-client-verify` published em workspace.
- [ ] Sync + async stream APIs.
- [ ] Property tests 10k iter green.
- [ ] Examples documented + executable.
- [ ] cbindgen header generated.
- [ ] Code review + Crypto SME (BLAKE3 + subtle review).
- [ ] PRR sign-offs.

## 12. Invariants

- INV-CAS-INTEGRITY (CRITICAL, TLA+): client-side enforcement layer (defense-in-depth com S-01 server-side).
- INV-CAS-IDEMPOTENCY (CRITICAL): same body always produces same digest; verify deterministic.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Crate root | `crates/corelink-client-verify/Cargo.toml` + `src/lib.rs` | Rust |
| ClientVerifier impl | `crates/corelink-client-verify/src/verifier.rs` | Rust |
| Stream impl | `crates/corelink-client-verify/src/stream.rs` (feature-gated) | Rust |
| VerifyError | `crates/corelink-client-verify/src/error.rs` | Rust |
| Property test | `crates/corelink-client-verify/tests/prop_verify.rs` | Rust |
| Criterion benchmark | `crates/corelink-client-verify/benches/verify.rs` | Rust |
| Examples | `crates/corelink-client-verify/examples/*.rs` | Rust |
| cbindgen config | `crates/corelink-client-verify/cbindgen.toml` | TOML |
| Generated C header | `crates/corelink-client-verify/include/corelink_client_verify.h` | C header |

## 14. Quality Standards SOTA

- **14.3.1** Zero unsafe; zero unwrap em lib code.
- **14.3.2** Documentação rustdoc + 3 examples por public function.
- **14.3.3** Test coverage ≥ 95%.
- **14.3.4** Verify p99 ≤ 3.5ms para 5 MiB blob (criterion sustained).
- **14.3.5** SAST clean.
- **14.3.6** Métricas: counter local; emit forward em S-15.
- **14.3.7** Runbook: nenhum (mismatch é feature behavior).
- **14.3.8** SemVer commitment ABI-stable v1.x.x.
- **14.3.9** Memory bounded sync ≤ 10 MiB / stream ≤ 2 MiB.
- **14.3.10** Cost regression gate.

## 15. Chaos Experiments

1. **Inject mid-stream tampering**: simulate adversarial MitM; verify catches at end.
2. **BLAKE3 crate version regression**: same as S-01 WI-S01-002 chaos.
3. **Constant-time compiler optimization break**: verify subtle crate generates expected machine code (objdump check).
4. **Stand-alone build em air-gapped environment**: verify zero unexpected deps.

## 16. PRR

PRR + Crypto SME + AppSec.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml + workspace integration | 1.5h |
| ST-002 | ClientVerifier + VerifyConfig builder | 2h |
| ST-003 | Sync verify reusing corelink-hash | 1.5h |
| ST-004 | Async stream verify (feature-gated) | 3h |
| ST-005 | VerifyError + thiserror integration | 1h |
| ST-006 | Warning emit + counter metric | 1.5h |
| ST-007 | cbindgen config + C header generation | 1.5h |
| ST-008 | Property test 10k iter | 2h |
| ST-009 | Criterion benchmark sync + stream | 1.5h |
| ST-010 | 3 examples + rustdoc | 1.5h |
| ST-011 | Stand-alone build verification | 0.5h |
| ST-012 | PRR doc + Crypto SME walkthrough | 1.5h |

**Total**: ~19h Optimistic; PERT ~22.5h.

## 18. Dependencies

### Hard blockers

- **WI-S01-002 SEALED** (`corelink-hash` crate; reuses Digest + compute).

### Outbound

- **WI-S15-004** (S-15 SDK FFI wrappers) — consume crate via cgo / pyO3 / wasm-bindgen.
- **WI-S02-001** (read path) — Worker internal pode também usar crate para integrity check em forward streaming.

## 19. Effort PERT

O: 16h, M: 19h, P: 32h → PERT 21h.

## 20. Time-boxing

24h hard limit; escalation ST-004 (stream) > 4h → Architect.

## 21. Observability

Counter local em crate; emit forward para Grafana via S-15 SDK telemetry layer.

## 22. Cost Analysis

Pure compute (client-side); zero infra cost. Distribution: cdylib ~500 KB compiled; pyO3/cgo/WASM bundles ~1-2 MB additional (reasonable for SDK).

## 23. API Contract

`ClientVerifier`, `VerifyConfig`, `VerifyError`, `Digest` (re-export from corelink-hash) são SemVer-locked v1.x.x. Breaking change requires major bump.

## 24. Post-mortem Hooks

- Client verify silently bypassed em produção (dep upgrade flipped default) → CRITICAL post-mortem + dep audit.
- Constant-time variance > 5% sustained → 5-Why obrigatório.
- Stream verify memory leak → post-mortem (FM-403).
- BLAKE3 CVE upstream → immediate patch + dep update + retroactive customer notification consideration.

## 25. Rollback / Recovery

SDK distribution: rollback via SDK previous version; cliente atualiza Cargo.toml/pip/npm.
Client-side bug shipped: customer notification + corrective release; recommend re-download affected blobs (re-fetch + verify).

## 26. Security & Privacy

STRIDE:
- **Tampering**: client verify catches MitM tampering em transit.
- **Information disclosure**: constant-time prevents timing oracle.
LINDDUN:
- **Detectability**: bit rot or tampering detectable via mismatch error.
- **Non-repudiation**: cliente has cryptographic proof body matches digest.

## 27. Knowledge Transfer

Tech talk: "Client-Side Verify: Last Line of Defense em CAS Integrity" — 30 min.
Doc `docs/internal/client-verify-pattern.md` — explica default-on rationale + FFI design.

## 28. Risk Register

| ID | R | P | D | I | E | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Default-off slipped via dep upgrade | L | L | CRITICAL | L | LOW | CI test verifies VerifyConfig::default().enabled == true |
| R-002 | Stream verify memory leak | L | M | MEDIUM | L | LOW | valgrind/MSAN em CI; bounded chunk |
| R-003 | Constant-time variance via compiler optim | L | M | HIGH | L | LOW | subtle crate + black_box em benchmark |
| R-004 | ABI breaking change em FFI | M | L | HIGH | L | LOW | SemVer discipline + cbindgen header diff CI |
| R-005 | BLAKE3 crate CVE | L | L | CRITICAL | L | LOW | cargo-audit + dupla-hash fallback opcional |
| R-006 | Stand-alone build broken (Worker dep slipped) | L | L | LOW | L | LOW | CI build crate isolated em separate job |

## 29. Review Checkpoints

1. Design (D+0): Crypto SME approves API + default-on.
2. Code review (D+2): peer + Crypto SME.
3. ABI review (D+3): FFI surface review com S-15 owner forward (planning).
4. Pre-merge: property tests + criterion + cbindgen verify.

## 30. Sign-off (HIGH_RISK 11 canonical; framework §33.5.4.3 + ADR-0034)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; mandatory — Crypto SME specialization for client-side BLAKE3 verify constant-time + threat model attestation chain_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-02 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — client-side verify threat model + FFI ABI surface review_ | _pending_ | _pending_ |

> Crypto SME (mandatory for BLAKE3 + threat model) folds into Architect role specialization. S-15 future owner FFI review é informational input antes do sprint S-15, não sign-off canonical aqui. Peer reviewers contribuem em PR review sem sign-off canonical separado.

## 31. Change Log

1.0.0 — 2026-04-25 — Lote 10.2.

## 32. Anti-patterns evitados

- ❌ Default-off (opt-in security é optional security).
- ❌ Custom constant-time (use subtle).
- ❌ Sync verify > 4 MiB (memory bloat).
- ❌ Crate depends on Worker internals (FFI distribution friction).
- ❌ ABI-instable types em public API (Rust-only types break FFI).
- ❌ Public fields em ClientVerifier (builder + private).
- ❌ Silent opt-out (warning + metric).

---

**Fim WI-S02-003.** Próximo: WI-S02-004 (constant-time 404 MissReason parity middleware per ADR-0028 3-arm).
