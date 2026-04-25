---
id: "WI-S03-002"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005", "FF-HR-009"]
parent: "S-03"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
tags: ["wi", "s03", "auth", "pat", "argon2id", "timing-safe", "high-risk"]
---

# WI-S03-002 — `corelink-pat` Crate (Argon2id OWASP 2024 + Timing-Safe Verify + Scope Bitset)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-03](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S03-002 |
| Título | `corelink-pat` — Argon2id (m=65536, t=3, p=4) + timing-safe verify + scope bitset + canonical PAT format |
| Sprint | S-03 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (token forge → cross-tenant), FF-HR-005 (cripto boundary), FF-HR-009 (5-layer defense Layer 1) |

## 1. Intent

Implementar `crates/corelink-pat/` — biblioteca de PAT (Personal Access Token) com:

1. **Geração canônica** PAT format `corelink_<env>_<base64url(32B random)>` (env ∈ `pat | ci | ro`).
2. **Hash Argon2id** com OWASP 2024 params (m=65536 KiB, t=3 iter, p=4 lanes) + salt 16B random per token; storage canônico PHC string.
3. **Timing-safe verify** via `subtle::ConstantTimeEq` em **dois níveis**: (a) prefix parsing constant-time; (b) hash comparison constant-time (via Argon2 lib API).
4. **Scope bitset** representação compacta de scopes (auth_model.md §3.1, 13 scopes) em `u64` (1 bit por scope; future-proof para até 64 scopes).
5. **Type-driven security**: newtypes opacos (`PatPlaintext`, `PatHash`, `PatId`) com construtor privado; impossível confundir plaintext com hash em chamada.

```rust
// crates/corelink-pat/src/lib.rs
pub struct PatPlaintext(String);  // private; construído só por mint()
pub struct PatHash(String);       // PHC string; safe to log
pub struct PatId(Uuid);           // DB primary key
pub struct PatScopes(u64);        // bitset

#[derive(Debug, Clone, Copy)]
pub enum PatEnv {
    Pat,    // user-facing PAT
    Ci,     // CI machine token
    Ro,     // read-only token
}

pub struct Pat {
    pub id: PatId,
    pub hash: PatHash,
    pub scopes: PatScopes,
    pub env: PatEnv,
    pub tenant_id: TenantId,
    pub principal_id: PrincipalId,
    pub issued_at: SystemTime,
    pub expires_at: Option<SystemTime>,
    pub last_used_at: Option<SystemTime>,
    pub revoked_at: Option<SystemTime>,
}

/// Mint nova PAT — retorna plaintext (1x; caller deve emitir e descartar)
/// + Pat (a persistir em D1/Neon).
pub fn mint(
    env: PatEnv,
    tenant_id: TenantId,
    principal_id: PrincipalId,
    scopes: PatScopes,
    ttl: Option<Duration>,
) -> Result<(PatPlaintext, Pat), PatError>;

/// Verify PAT plaintext contra hash armazenado. CONSTANT-TIME.
pub fn verify(plaintext: &str, stored: &PatHash) -> Result<(), PatError>;

/// Parse PAT format + extract env. CONSTANT-TIME (mesmo tempo para input válido vs malformado).
pub fn parse_env(plaintext: &str) -> Result<PatEnv, PatError>;
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

PAT hashing + verify é o backbone do auth não-SSO. Bugs conhecidos em libs que motivam HIGH_RISK + Crypto SME review:

1. **Underprovisioned Argon2 params** (CVE-class familiar): bcrypt cost=10 (2010 era), pbkdf2 100k iter (2018 era), Argon2 t=1 m=4096 — tudo **abaixo do recomendado OWASP 2024**. Atacante GPU offline crackearia 100M PATs/sec. Mitigação: m=65536 KiB (64 MiB RAM/hash), t=3 iter, p=4 lanes — calibrado para ~250ms em deploy hardware (Cloudflare Workers WASM runtime, x86_64 Apple M-series ref baseline).

2. **Plaintext storage** (real-world recurrence: rockyou, Adobe 2013, LinkedIn 2012): se PAT gravado em plaintext em DB ou logs, breach = direct credential leak. Mitigação: 100% hash storage; plaintext retornado **uma vez** em `mint()`; logs estritos via S-09 redact macro `redact_pat!`.

3. **Timing oracle em verify**: lazy implementation `if hash != computed { return InvalidPat }` faz early-return em primeiro byte mismatch. Atacante mede tempo: input com primeiro byte certo demora ~5ns mais; itera 256 valores por byte; recupera hash em ~256×32 = 8192 medições. **Catastrófico**. Mitigação: `subtle::ConstantTimeEq::ct_eq` para comparar hashes; Argon2 lib `verify_password` é constant-time by design (PHC compliant); reforçar com unit test Mann-Whitney U.

4. **Salt reuse** (rainbow table risk): se salt fixo, atacante pré-computa rainbow table 100M PATs comuns; mata defesa Argon2. Mitigação: salt 16B random per-token via OS CSPRNG (`getrandom` em Rust → `getentropy(2)` Linux / `RtlGenRandom` Windows / `arc4random_buf` BSD). Argon2 PHC string já incorpora salt; storage automatic.

5. **PAT format ambiguity**: `corelink_pat_xyz` vs `corelink_ci_xyz` parsing lazy permite atacante enviar prefix `pat` mas server scope-check assumir `ci` (default-deny correct? default-allow vulnerable?). Mitigação: parsing **strict** com explicit enum + erro em prefix desconhecido; default-deny no server.

6. **Scope spoofing**: cliente edita PAT estring local → muda prefix `ro_` para `ci_` esperando obter cache-w. Mitigação: server-side scope é SOT (D1/Neon `pat_scopes` column é fonte; PAT prefix é hint só); verify nunca inferir scope from string.

7. **Constant-time prefix parse**: trivial `if pat.starts_with("corelink_pat_")` é variável (parser bails early); atacante mede latência, infere prefix válido vs inválido. Mitigação: byte-fixed 13-char prefix lookup com `subtle::ConstantTimeEq` no tamanho fixo + scan completo independente do match.

8. **Argon2 lib downgrade**: se atacante consegue substituir lib argon2 0.5.x → 0.3.x (CVE-2022-24764 family — workaround existed pre-fix), upgrade automatic via dependabot pode ressuscitar bug. Mitigação: lock pin específico + cargo-deny `[bans]` blocking older versions + cargo-audit weekly.

**Atacante adversarial scenarios**:

- **Offline crack após DB breach**: atacante exfiltra `pat_hash` table; tenta GPU brute-force (RTX 4090: ~30 hashes/sec @ 64 MiB Argon2id; 30B PATs em 1 ano vs 2^256 keyspace = computacionalmente intratável). Cost analysis comprova mitigation Argon2 sufficient for foreseeable HW.
- **Online brute-force**: rate limit S-08 (per-PAT counter) cap 100 req/s; atacante max ~3M tries/dia; keyspace 2^256 → centenas de bilhões de anos.
- **Side-channel cripto** (ARM/x86 cache timing): `subtle` lib explicitly hardens against; nightly fuzz com `cachegrind` opcional pós-GA.
- **Insider threat (DB read)**: attacker obtém hash; ainda precisa quebrar Argon2; defense in depth.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: PAT forge = full tenant access; Layer 1 collapse cascateia 5-layer defense.
- **FF-HR-005**: Argon2 + constant-time verify são cripto boundaries.
- **FF-HR-009**: este WI implementa Layer 1 (PAT scope) do 5-layer defense (auth_model §8.1).
- **Reversibility**: weak hash em prod → exfil DB → offline crack → todos os PATs revogados forçadamente + customer notification + reputation hit. Pre-emptive defense via OWASP 2024 params + pentest validation (WI-S03-008).

11 sign-offs incl. Crypto SME (mandatory), AppSec, Security Lead.

## 3. Customer Impact & Journey

**Persona 1 — Dev gerando PAT pela primeira vez**:
- Dashboard → "Create token" → form (name, scopes, ttl) → POST `/api/tokens` → server `mint()` → returns PAT plaintext **1x** em response (banner "Copy now; you won't see this again").
- Dev copia para `~/.corelink/credentials` (mode 0600 mandatory; CLI helper sets).
- Customer-visible: latência mint ~300ms (Argon2 hash dominates); aceitável (mint é raro, 1 token/usuário).

**Persona 2 — CLI usando PAT em CI**:
- `corelink push` → CLI lê `~/.corelink/credentials` → envia `Authorization: Bearer corelink_pat_xyz`.
- Server: middleware (WI-S03-003) extrai → `verify(plaintext, stored_hash)` → 200 OK ou 401.
- Customer-visible: latência verify p99 ~250ms (Argon2 verify dominates) — **percebido**, mas é cripto correto.
- Cache mitigação: post-verify hot path usa session cache (KV 60s TTL) para evitar Argon2 em todo request — **separated em WI-S03-003** middleware.

**Persona 3 — Compliance auditor revisando token storage**:
- Audit query `SELECT id, principal_id, scopes, issued_at FROM api_tokens WHERE tenant = ? AND revoked_at IS NULL` — never `token_hash` em audit reports.
- LGPD/GDPR: PAT plaintext é "credential" categoria PII especial; never logged; never returned post-mint; full audit trail issued/used/revoked events (WI-S03-007).

**SLA addendum**: cripto path (Argon2 verify) p99 ~250ms by design; documentado em SLA Auth Path. Cache-hit path (post-verify) ≤ 5ms p99.

## 4. Capability Mapping

- **CAP-AUTH-002** (PAT lifecycle: emit/use/verify/revoke) — IMPLEMENTA primary (emit + verify).
- **CAP-AUTH-007** (Argon2id hashing OWASP) — IMPLEMENTA primary.
- **CAP-AUTH-003** (Scope enforcement) — IMPLEMENTA partial (scope bitset construct + serialize; enforcement em WI-S03-003).
- Trace: `auth_model.md §1.2/§1.3/§1.4 (PAT types)` + `§2 (Credentials format)` + `§5 (Rotation)` + `§8.1 Layer 1 (PAT scope)` + `key_management.md §3.13`.

## 5. Tipo

Cripto core crate; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-pat/` workspace member** com:
   - Cargo.toml deps: `argon2 = "0.5"` (audited 2024+; supports m=65536), `subtle = "2.5"`, `getrandom = "0.2"`, `base64 = "0.22"` (URL-safe), `serde`, `thiserror`, `time = "0.3"`.
   - `lib.rs` re-exports `mint`, `verify`, `parse_env`, types.

2. **PAT format**:
   - Canonical: `corelink_<env>_<base64url(32 random bytes)>` totaling ~57 chars.
   - `<env>` ∈ `pat | ci | ro` (3 chars); strict enum.
   - Suffix 32 bytes random (256-bit entropy) → base64url no padding (43 chars).
   - Total canonical length: 13 (`corelink_xxx_`) + 43 (b64) = 56 chars (predictable; constant-time validation).

3. **`mint()` function**:
   - Generate 32 random bytes via `getrandom` (panics if entropy unavailable; correct for cripto).
   - Generate 16 random salt bytes (separate `getrandom` call).
   - Format `plaintext = format!("corelink_{}_{}", env_str, base64url(random))`.
   - Compute `hash = argon2id::hash_encoded(plaintext.as_bytes(), &salt, &Params { m_cost: 65536, t_cost: 3, p_cost: 4, output_len: Some(32) })?`.
   - Returns `(PatPlaintext(plaintext), Pat { hash: PatHash(hash), .. })`.
   - Latência target: ~250-300ms (Argon2 cost-bound).

4. **`verify()` function**:
   - Input: `plaintext: &str`, `stored: &PatHash`.
   - Use `argon2::PasswordHasher::verify_password` (PHC-string-aware; constant-time by design).
   - Return `Ok(())` se match; `Err(PatError::InvalidPat)` se mismatch.
   - **CRITICAL**: same-time response em valid vs invalid (Mann-Whitney U test em § 14).

5. **`parse_env()` function**:
   - Constant-time check de prefix `corelink_`.
   - Extract chars 9-11 (env tag); compare against `pat`/`ci`/`ro` via `subtle::ConstantTimeEq`.
   - Return `PatEnv` ou `Err(PatError::Malformed)`.
   - **CRITICAL**: same-time para todos os 3 valid envs + same-time para invalid.

6. **`PatScopes` bitset** (u64):
   - Constants `SCOPE_CACHE_R = 1 << 0`, `SCOPE_CACHE_W = 1 << 1`, `SCOPE_CACHE_FIND = 1 << 2`, `SCOPE_CACHE_DELETE = 1 << 3`, `SCOPE_ADMIN_TENANT_R = 1 << 4`, `SCOPE_ADMIN_TENANT_W = 1 << 5`, `SCOPE_ADMIN_TOKENS = 1 << 6`, `SCOPE_ADMIN_BILLING = 1 << 7`, `SCOPE_ADMIN_AUDIT = 1 << 8`, `SCOPE_ADMIN_USERS = 1 << 9`, `SCOPE_EXECUTE_ACTION = 1 << 10`, `SCOPE_REPORT_RESULT = 1 << 11`, `SCOPE_CACHE_RW = SCOPE_CACHE_R | SCOPE_CACHE_W` (alias).
   - Methods: `has(scope) -> bool`, `add`, `remove`, `union`, `intersection`.
   - Serde: serialize as `u64` (DB INTEGER) ou `Vec<String>` (API response readability) — feature flag.

7. **Error types**:
   ```rust
   #[derive(Debug, Error)]
   pub enum PatError {
       #[error("PAT format invalid (expected `corelink_<env>_<b64>`)")]
       Malformed,
       #[error("PAT hash verify failed")]
       InvalidPat,
       #[error("Argon2 hash error: {0}")]
       HashError(String),
       #[error("Entropy source unavailable")]
       EntropyUnavailable,
       #[error("concurrent verify capacity exhausted (Semaphore N=1; backpressure)")]
       BackpressureExhausted,
   }
   ```

7-bis. **Bounded concurrent verify via `Semaphore` (P0 fix Lote 10.3bis — Worker memory budget enforcement)**:
   - Argon2 verify cost = 64 MiB ephemeral RAM per call. CF Worker isolate budget = 128 MiB.
   - Max safe concurrent = ⌊128/64⌋ = **2** verifies; mas defensive default = **N=1** (serialize).
   - Implementação: `static VERIFY_SEMAPHORE: tokio::sync::Semaphore = Semaphore::new(N)` private em crate.
   - `pub async fn verify(plaintext, hash)` adquire permit antes de Argon2 compute; releases on drop.
   - **Backpressure**: se permit não disponível em ≤ 50ms (timeout via `try_acquire_owned` com tokio::time::timeout), retorna `Err(PatError::BackpressureExhausted)` → middleware (WI-S03-003) maps to HTTP 503 `service_busy` + Retry-After.
   - Configurable via env var `CORELINK_PAT_VERIFY_CONCURRENCY` (default 1; range [1, 2]); production deploy guard rejects > 2 (cripto correctness invariant).
   - **Decoupled de S-08 rate limit**: este Semaphore protege Worker memory; rate limit S-08 protege backend rate. Layered.

7-ter. **`dummy_verify_for_constant_time` public API (P0 cross-WI fix Lote 10.3bis — afford constant-time defense para WI-S03-003)**:
   - `pub async fn dummy_verify_for_constant_time(plaintext: &str) -> Result<(), PatError>`.
   - Executes Argon2 verify against fixed dummy hash com fixed salt + fixed dummy plaintext.
   - **Latência identical to real verify** (~250ms ± 20%); same Semaphore permit acquired.
   - Used by WI-S03-003 middleware em invalid-PAT path (parse_env failure OR token_hash não encontrado em D1) para evitar timing oracle.
   - WI-S03-003 calls `corelink_pat::dummy_verify_for_constant_time(input)` em cold-path branches; WI-S03-002 owns the implementation (encapsulação correta).

8. **Métricas (auth_model.md §3.13.7)**:
   - `corelink.auth.pat.mint_total{env}` (counter).
   - `corelink.auth.pat.verify_total{result}` (result ∈ ok|invalid|malformed).
   - `corelink.auth.pat.mint_duration_ms_bucket` (histogram p50/p95/p99).
   - `corelink.auth.pat.verify_duration_ms_bucket` (histogram).
   - `corelink.auth.pat.argon2_calibration_ms` (gauge — set em deploy startup).

9. **Calibration tooling**:
   - `cargo bench --bench argon2_calibrate` — output: tempo em current HW para m=65536/t=3/p=4. Used em deploy validation: se ≠ 200-350ms range, alert.
   - Auto-run em CI; results em artifact for review.

10. **Property tests** (10k iter PR; 100k nightly):
    - mint→verify roundtrip: always succeeds with same plaintext.
    - mint→tamper-1-byte→verify: always fails.
    - mint produces unique plaintext (32B = 2^256 keyspace; collision negligible).
    - parse_env strict: random bytes prefixed `corelink_xxx_` always Err except 3 valid envs.
    - Scope bitset roundtrip serde: u64 encode/decode lossless.

11. **Adversarial tests (Mann-Whitney U + power analysis 3-prong evidence-grade)**:
    - **N ≥ 10000 samples per arm**: valid PAT verify timing vs invalid PAT verify timing.
    - **Power calculation a priori**: `statrs` compute power 1−β ≥ 0.80 com effect size Cohen's d = 0.2 (small/practically-relevant).
    - **Mann-Whitney U test**: target p > 0.05 (fail to reject H0).
    - **Šidák 3-trial gate**: independent random seeds; ALL 3 trials p > 0.05 (combined α ≈ 0.000125 effective; flake mitigation).
    - **Bootstrap 95% CI on |Δmedian|**: target ≤ 5ms; CI must cross zero (zero-inclusive).
    - Combined verdict: high-power test failed to reject + small effect + CI cruzando 0 = **evidence-grade indistinguishability** claim (não cargo-cult).
    - Same 3-prong gate em parse_env (valid env vs invalid env strings; |Δmedian| ≤ 100µs).
    - **Methodology canon**: cf. WI-S03-003 §10.5.2 + WI-S02-004 §10.4.1; este WI segue mesmo padrão (gap remediation Lote 10.3bis P0).

12. **Adversarial regression tests** (CVE-class):
    - alg=none-equivalent: Argon2 hash must rejectado se PHC alg field changed.
    - Underprovisioned params test: explicit reject m_cost < 32768 em verify (defense em depth se DB compromised + atacante swap PHC).
    - Salt collision: same plaintext hashed twice produces distinct hashes.

13. **rustdoc + 4 examples**:
    - `examples/mint_and_verify.rs` — basic flow.
    - `examples/scope_check.rs` — bitset usage.
    - `examples/calibrate.rs` — params benchmark.
    - `examples/serde_roundtrip.rs` — DB serialize/deserialize.

### 6.2 Out-of-scope (deferred)

- **DB persistence** (`api_tokens` table): WI-S03-005 (Neon schema).
- **Tower middleware integration**: WI-S03-003.
- **Revocation propagation**: WI-S03-004.
- **Session cache (post-verify)**: WI-S03-003 middleware (KV 60s TTL).
- **WebAuthn**: WI-S03-006.
- **Audit emission**: WI-S03-007.
- **Argon2id post-quantum migration** (Argon3 / KMAC): pós-GA Q3+.
- **HSM-backed PAT signing** (cliente-verifiable): S-04 (AC digest signing) related but separate.

## 7. Anti-Scope

- ❌ bcrypt fallback (Argon2id only; OWASP 2024 deprecates bcrypt para new systems).
- ❌ pbkdf2 (insufficient resistance vs GPU).
- ❌ scrypt (Argon2id is successor).
- ❌ Custom Argon2 impl (use audited `argon2` crate 0.5+).
- ❌ Plaintext storage em ANY layer (DB, logs, traces, error messages).
- ❌ Variable-time prefix parsing (constant-time mandatory).
- ❌ Scope inference from PAT prefix string (DB é SOT).
- ❌ Single salt (per-token salt mandatory).
- ❌ Salt < 16 bytes.
- ❌ Argon2 t < 3 ou m < 65536 (OWASP 2024 floor).
- ❌ Lazy `==` em hash compare (use `subtle::ConstantTimeEq`).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: corelink-pat — mint, verify, parse_env

  Background:
    Given Argon2id params m=65536, t=3, p=4
    And entropy source available

  Scenario: mint produces canonical format
    When mint(PatEnv::Pat, tenant_X, principal_Y, scopes_rw, None) called
    Then PatPlaintext matches /^corelink_pat_[A-Za-z0-9_-]{43}$/
    And Pat.hash starts with "$argon2id$v=19$m=65536,t=3,p=4$"
    And Pat.scopes == SCOPE_CACHE_R | SCOPE_CACHE_W
    And Pat.expires_at == None
    And metric corelink.auth.pat.mint_total{env="pat"} incremented

  Scenario: mint produces unique plaintext (entropy)
    When mint called 10000 times com same args
    Then 10000 unique plaintexts (no collision)

  Scenario: verify roundtrip success
    Given (plaintext, pat) = mint(...)
    When verify(plaintext, &pat.hash) called
    Then Result::Ok(()) returned
    And metric corelink.auth.pat.verify_total{result="ok"} incremented

  Scenario: verify tamper detection
    Given (plaintext, pat) = mint(...)
    Given tampered = plaintext with last char modified
    When verify(tampered, &pat.hash) called
    Then Result::Err(PatError::InvalidPat) returned
    And metric corelink.auth.pat.verify_total{result="invalid"} incremented

  Scenario: verify constant-time (Mann-Whitney U + power analysis 3-prong)
    Given 10000 valid PAT verify timings collected per arm (N ≥ 10000)
    Given 10000 invalid PAT verify timings collected
    Given power 1−β ≥ 0.80 com effect size d = 0.2 calculado a priori
    When Mann-Whitney U test applied across 3 independent trials (Šidák correction)
    Then ALL 3 trials p > 0.05 (combined α ≈ 0.000125)
    And bootstrap 95% CI sobre |Δmedian| ≤ 5ms
    And CI cruzando 0 (zero-inclusive; effect indistinguishable from zero)
    (Combined evidence-grade indistinguishability: high-power test failed to reject + small effect + CI consistente com zero)

  Scenario: parse_env strict
    When parse_env("corelink_pat_xxxx") called
    Then Result::Ok(PatEnv::Pat)
    When parse_env("corelink_ci_xxxx") called
    Then Result::Ok(PatEnv::Ci)
    When parse_env("corelink_ro_xxxx") called
    Then Result::Ok(PatEnv::Ro)
    When parse_env("corelink_admin_xxxx") called
    Then Result::Err(PatError::Malformed)
    When parse_env("not-a-pat") called
    Then Result::Err(PatError::Malformed)

  Scenario: parse_env constant-time (Mann-Whitney U + power analysis 3-prong)
    Given 10000 valid env timings (mix pat/ci/ro) per arm
    Given 10000 invalid env timings (admin/exec/garbage)
    Given power 1−β ≥ 0.80 com d = 0.2
    When Mann-Whitney U applied 3 trials Šidák
    Then ALL 3 trials p > 0.05
    And bootstrap 95% CI sobre |Δmedian| ≤ 100µs
    And CI cruzando 0
    (Evidence-grade timing indistinguishability across env outcomes)

  Scenario: scope bitset operations
    Given scopes = SCOPE_CACHE_R | SCOPE_CACHE_W
    Then scopes.has(SCOPE_CACHE_R) == true
    Then scopes.has(SCOPE_CACHE_W) == true
    Then scopes.has(SCOPE_ADMIN_TOKENS) == false
    When scopes.add(SCOPE_CACHE_FIND)
    Then scopes.has(SCOPE_CACHE_FIND) == true

  Scenario: scope bitset serde u64 lossless
    Given scopes = SCOPE_CACHE_RW | SCOPE_ADMIN_AUDIT
    When serialize as u64 then deserialize
    Then equal to original

  Scenario: argon2 calibration in target range
    Given current deploy hardware (CF Worker WASM)
    When calibrate benchmark run
    Then mint duration p50 ∈ [200ms, 350ms]
    (alert if outside; deploy validation gate)

  Scenario: hashes from same plaintext differ (salt random)
    Given plaintext_X
    When mint twice (different salts)
    Then hash1 != hash2
    But verify(plaintext_X, hash1) == Ok
    And verify(plaintext_X, hash2) == Ok

  Scenario: malformed PHC string em verify
    When verify("corelink_pat_xxx", &PatHash("invalid_phc_string")) called
    Then Result::Err(PatError::HashError(_))
    And no panic

  Scenario: entropy unavailable
    Given getrandom returns Err (synthetic test injection)
    When mint called
    Then Result::Err(PatError::EntropyUnavailable)
    (cripto fail-safe; no degraded path)
```

## 9. Design Decisions

### 9.1 Why Argon2id (não bcrypt/scrypt/pbkdf2)

- OWASP Password Storage Cheat Sheet 2024 recommends Argon2id as primary (Argon2 won PHC 2015).
- Argon2id = hybrid Argon2i (side-channel resistant) + Argon2d (GPU-resistant) → best defense em ambos contexts.
- bcrypt: max cost 31 (~17 sec); insufficient memory hardness (4 KiB fixed); deprecated for new systems.
- scrypt: alternative, mas Argon2 superseded; mais auditing surface em Argon2id 2024.
- pbkdf2: insufficient GPU resistance.

### 9.2 Why params m=65536, t=3, p=4 (OWASP 2024 floor)

- **m=65536 KiB = 64 MiB RAM/hash**: GPU/ASIC memory-bound (RTX 4090 24 GB → 384 concurrent hashes max).
- **t=3 iterations**: balance speed vs cost.
- **p=4 parallelism**: 4 lanes; CF Workers x86_64 com 4 vCPU benefits.
- **Output 32 bytes**: matches AES-256 keyspace; future-proof.
- Calibration target ~250ms em ref HW (Apple M-series WASM ≈ x86 baseline; CF Workers similar).

### 9.3 Why salt 16 bytes (não 8)

- 8 bytes = 2^64 keyspace; rainbow tables computacionalmente plausíveis a longo prazo.
- 16 bytes = 2^128 keyspace; intratável.
- OWASP: ≥ 16 bytes recommended.

### 9.4 Why PHC string format (não custom)

- PHC (Password Hashing Competition) standardized format `$argon2id$v=19$m=...,t=...,p=...$<salt>$<hash>`.
- Self-describing: params embedded → permite future cost increase sem migration breaking.
- Argon2 lib parser audited; less custom code.

### 9.5 Why `subtle::ConstantTimeEq` em adição ao Argon2 lib

- `argon2::PasswordHasher::verify_password` é constant-time **internalmente** mas API retorna `Result` que pode-se variar tempo no error path (allocação + format string em error case).
- `subtle::ct_eq` no PHC string content compare adiciona segunda layer; defense em depth.
- Mann-Whitney U test verifies overall path, não só Argon2 internal.

### 9.6 Why `getrandom` (não OsRng)

- `getrandom` é mais portable: Linux `getrandom(2)`, macOS `getentropy`, Windows `BCryptGenRandom`, WASM via JS `crypto.getRandomValues`.
- WASM Cloudflare Workers: usa Web Crypto via `getrandom` feature `js`.
- `OsRng` em `rand` crate é wrapper sobre `getrandom`; usar direto reduz dep surface.

### 9.7 Why scope bitset u64 (não Vec<String>)

- u64 = 8 bytes; trivial em DB INTEGER column.
- Bitwise check (`scopes & SCOPE_X != 0`) é O(1) zero alloc.
- Vec<String> em hot path = heap alloc + string compare per scope check.
- Future: até 64 scopes (current: 13 + reserved); migrate to u128 if needed (binary compat strategy doc TBD em ADR).
- API surface (response) pode optar Vec<String> human-readable via feature flag.

### 9.8 Why PatPlaintext newtype com private field

- Compile-time prevention de PAT plaintext leakage:
  - `PatPlaintext` não impl `Display` ou `Debug` (debug redacts: `PatPlaintext(***)`).
  - Não impl `Serialize` (não pode escapar para JSON response).
  - Único getter `into_string()` → caller tem que **conscientemente** opt-in para retornar plaintext (apenas em mint() response path).
- `PatHash` derive Debug/Display/Serialize (PHC string é safe to log).

### 9.9 Why CSPRNG fail-safe (panic se getrandom unavailable)

- Cripto sem entropy = catastrophic; melhor falhar loud do que generate weak token.
- Server abort em entropy missing é OK (Cloudflare Workers cold-start path; if persistent, deploy issue).

### 9.10 ADR potencial?

- **ADR-0025**: "Argon2id params calibration target (250ms ±20%) e validation gate em deploy". Trade-off documentado: mais cost = mais defense + mais latency. Calibration sustained em deploy validation.
- **ADR-0026**: "Scope bitset u64 layout + future migration path para u128 se exceder 64 scopes". Lock binary layout; reserved bits.
- Whitelist em validate_references.py.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** Property tests 10k iter (PR) + 100k iter (nightly) → 0 panics, 0 false-accepts (EVT-002).
- [ ] **10.5.2** Mann-Whitney U + power analysis 3-prong em verify timing (EVT-002):
  - N ≥ 10000 samples per arm (valid PAT verify vs invalid PAT verify).
  - Power 1−β ≥ 0.80 com effect size d = 0.2 (calculated a priori via `statrs::distribution::statistical_power`).
  - Šidák correction 3-trial gate: ALL 3 independent trials p > 0.05 (combined α ≈ 0.000125; flake < 1-em-8000 runs).
  - Bootstrap 95% CI sobre |Δmedian| ≤ 5ms (CI cruzando 0 mandatory).
  - **NOTE methodology correta**: `p > 0.05` sozinho NÃO prova distributions são same; apenas "fail to reject H0". Evidence-grade requires high-power test failing to reject + small effect size + CI consistent with zero. Cf. WI-S03-003 §10.5.2 padrão canônico.
- [ ] **10.5.3** Mann-Whitney U + power analysis 3-prong em parse_env timing (EVT-002):
  - Mesma metodologia 3-prong (N ≥ 10000, power ≥ 0.80, Šidák 3-trial, |Δmedian| ≤ 100µs 95% CI).
  - Compara 3 valid envs (pat/ci/ro) vs invalid envs.
- [ ] **10.5.4** Adversarial regression suite (5+ classes): rainbow table, salt reuse, downgrade, timing oracle, salt < 16B — todos rejected (EVT-040).
- [ ] **10.5.5** Argon2 calibration p50 ∈ [200ms, 350ms] em deploy hardware (EVT-021).
- [ ] **10.5.6** Cargo-audit + cargo-deny clean (no CVE em deps) (EVT-002).
- [ ] **10.5.7** Cargo-fuzz target em verify (1h corpus build); 0 crashes (EVT-008).
- [ ] **10.5.8** Memory: hash uses 64 MiB ephemeral (Argon2 m_cost); Worker memory budget validated (EVT-021).
- [ ] **10.5.9** No plaintext em logs/traces/errors — static analysis (clippy custom lint OR grep gate em CI) (EVT-002).
- [ ] **10.5.10** OWASP ASVS V2.4 (cripto storage) + V2.10 (auth secrets) 100% pass (EVT-002).

## 11. DoD

- [ ] Crate compila standalone + workspace.
- [ ] All Gherkin scenarios green.
- [ ] Property tests 10k green em CI.
- [ ] Mann-Whitney U tests green (p > 0.05 em dois paths).
- [ ] Calibration bench em range em CI.
- [ ] cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] cargo-fuzz target 1h corpus done; 0 crashes.
- [ ] rustdoc 100% public API + 4 examples.
- [ ] Métricas + dashboard.
- [ ] ADR-0025 + ADR-0026 published.
- [ ] Crypto SME + AppSec + Security Lead reviews.
- [ ] PRR Architect sign-off.

## 12. Invariants Validated

- **INV-AUTH-PAT-HASH-ARGON2ID-2024** (CRITICAL): all PAT hashes use Argon2id m≥65536/t≥3/p≥4; verify rejects underprovisioned. Static check em deploy gate.
- **INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED** (CRITICAL): static analysis lint impede `PatPlaintext::into_string()` chamadas fora de `mint()` return path. CI gate.
- **INV-AUTH-PAT-VERIFY-CONSTANT-TIME** (CRITICAL): Mann-Whitney U test sustained em CI nightly.
- **INV-AUTH-PAT-SALT-PER-TOKEN** (HIGH): mint() cada salt independente via getrandom.
- **INV-AUTH-PAT-SCOPE-DB-IS-SOT** (HIGH): scope nunca inferido from PAT string; sempre query DB.

TLA+ alignment: `tenant_isolation.tla` Layer 1 (PAT scope) — este WI implementa o backbone cripto consumido por middleware (WI-S03-003).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| `corelink-pat` crate | `crates/corelink-pat/` | Rust workspace member |
| Pat struct + types | `crates/corelink-pat/src/pat.rs` | Rust |
| PatScopes bitset | `crates/corelink-pat/src/scopes.rs` | Rust |
| mint/verify/parse_env | `crates/corelink-pat/src/lib.rs` | Rust |
| PatError enum | `crates/corelink-pat/src/error.rs` | Rust |
| Property tests | `crates/corelink-pat/tests/prop_pat.rs` | Rust |
| Mann-Whitney tests | `crates/corelink-pat/tests/timing_mw.rs` | Rust |
| Adversarial regression | `crates/corelink-pat/tests/adversarial.rs` | Rust |
| Calibration bench | `crates/corelink-pat/benches/argon2_calibrate.rs` | Rust criterion |
| Fuzz target | `crates/corelink-pat/fuzz/fuzz_targets/verify.rs` | cargo-fuzz |
| ADR-0025 | `specs/02_governance/decisions/ADR-0025-argon2-calibration.md` | Markdown |
| ADR-0026 | `specs/02_governance/decisions/ADR-0026-scope-bitset-layout.md` | Markdown |
| Examples (4) | `crates/corelink-pat/examples/` | Rust |

## 14. Quality Standards SOTA

- **14.5.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.5.2** rustdoc 100% public API + 4 examples + threat model section em README.
- **14.5.3** Test coverage ≥ 95% (cripto crate; higher bar).
- **14.5.4** mint p99 ≤ 350ms; verify p99 ≤ 350ms; parse_env p99 ≤ 100µs.
- **14.5.5** SAST: cargo-audit + cargo-deny + clippy + cargo-fuzz 1h.
- **14.5.6** Métricas RED + Argon2 calibration gauge.
- **14.5.7** Runbook: nenhum (failures são auth flow rejections; not ops).
- **14.5.8** Breaking changes em PHC format = bump major + DB migration plan.
- **14.5.9** Memory bounded: 64 MiB Argon2 ephemeral per hash; ≤ 1 KiB stack.
- **14.5.10** Cost regression gate: latency p99 limits (Lote 9.4 §14.10).

## 15. Chaos Experiments

1. **Argon2 lib downgrade simulation**: pin lib to 0.4.x (older) em test scenario; verify cargo-deny rejects build OR property test detects underprovisioned params. Hypothesis: defense em depth catches lib downgrade. Procedure: edit Cargo.toml to old version; run CI gate; expect FAIL. Abort: not applicable (pre-merge gate).

2. **Entropy starvation**: synthetic injection `getrandom` Err; verify mint returns `EntropyUnavailable` cleanly + metric emit. Hypothesis: cripto fails loud not silent. Procedure: feature flag + mock; assertions. Abort: synthetic.

3. **Mann-Whitney drift**: simulate timing skew (introduce branch with var timing); verify Mann-Whitney p-value detects (< 0.05; H0 rejected). Hypothesis: test catches regression. Procedure: malicious patch in fork; CI fails. Abort: synthetic.

4. **Calibration HW divergence**: deploy em em distinct HW (CF Workers UK vs US vs JP); verify calibration ∈ range across regions. Hypothesis: WASM runtime consistent o suficiente. Procedure: deploy canary; observe calibration metric per region. Abort: outlier > 500ms triggers WI-S03 review (params rebaseline).

5. **Memory budget chaos**: run 16 concurrent mints (all 64 MiB each = 1 GiB peak); verify Worker memory limit honored (CF Workers 128 MiB default → ≤ 2 concurrent mints; backpressure expected). Hypothesis: Argon2 memory cost can DoS Worker if unbounded. Mitigação documentada: rate limit + queue mint requests (S-13 admin plane).

## 16. PRR

PRR HIGH_RISK 11 sign-offs gated em WI-S03-008 ship gate. Este WI mini-PRR Crypto SME + AppSec + Architect.

- [ ] All Gherkin green.
- [ ] Property + Mann-Whitney + adversarial green.
- [ ] Calibration in range em deploy.
- [ ] cargo-audit/deny/fuzz clean.
- [ ] ADR-0025 + ADR-0026 published.
- [ ] Crypto SME pair-program review log.
- [ ] AppSec threat model review log.
- [ ] No plaintext leakage CI gate.
- [ ] OWASP ASVS V2.4 + V2.10 100%.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml deps | 1h |
| ST-002 | Newtypes (PatPlaintext, PatHash, PatId, PatScopes) | 2h |
| ST-003 | PatEnv enum + parse_env constant-time | 2.5h |
| ST-004 | mint() implementation | 2h |
| ST-005 | verify() implementation + subtle wrapper | 2h |
| ST-006 | PatError + thiserror | 1h |
| ST-007 | PatScopes bitset (constants + methods + serde) | 2h |
| ST-008 | Métricas emit (5 metrics) | 1.5h |
| ST-009 | Property tests 10k iter | 3h |
| ST-010 | Mann-Whitney U test impl + 10k samples each | 4h |
| ST-011 | Adversarial regression tests (5 classes) | 3h |
| ST-012 | Calibration bench criterion | 2h |
| ST-013 | cargo-fuzz target em verify (1h corpus build) | 2h |
| ST-014 | rustdoc + 4 examples + threat model README | 4h |
| ST-015 | ADR-0025 + ADR-0026 redação | 3h |
| ST-016 | Crypto SME pair review (mandatory) | 4h |
| ST-017 | AppSec threat model review feedback iteration | 2h |
| ST-018 | PRR Architect review | 1.5h |

**Total Optimistic**: ~42.5h. **PERT** (O=38h, M=42.5h, P=64h): **~46h**.

## 18. Dependencies

### Hard blockers

- S-01 SEALED (workspace).
- Crypto SME availability (mandatory pair review).

### Soft blockers

- WI-S03-005 (Neon schema) — não bloqueante para crate; integration via WI-S03-003.

### Outbound

- WI-S03-003 (middleware) — consumes mint() em token creation API + verify() em request path.
- WI-S03-004 (revocation) — consumes PatId.
- WI-S03-005 (schema) — persists Pat struct.
- WI-S03-007 (audit) — emits `auth.token.issued/used/revoked`.
- WI-S03-008 (ship gate).

## 19. Effort PERT

O: 38h, M: 42.5h, P: 64h → PERT **46h**.

## 20. Time-boxing

**52h hard limit**. Se exceder → escalation: split em sub-WI (Argon2 core vs scope bitset).

## 21. Observability

5 métricas listadas §6.1.8. Trace span `pat.verify` com attributes:
- `result` (ok|invalid|malformed|hash_error)
- `pat.env` (pat|ci|ro)
- `pat.id_hash` (sha256(pat_id) prefix 8 chars; PII-safe)
- `argon2.duration_ms`

Logs structured JSON; INFO em mint/verify success; WARN em mismatch; ERROR em hash_error.

Dashboard widget DASH-AUTH:
- Mint rate (req/s).
- Verify rate.
- Verify success ratio.
- p99 latency mint/verify.
- Argon2 calibration gauge per deploy.
- Mann-Whitney U p-value (nightly job).

## 22. Cost Analysis

- Mint: ~250ms CPU (Argon2 dominate) + 1 DB INSERT (deferred WI-S03-005). Em 100 mints/dia: 25 sec CPU/dia ≈ negligível.
- Verify: ~250ms CPU per verify (Argon2). Em 10M req/dia × 0.5% verify (cache miss; rest hit session cache em WI-S03-003 middleware): 50k verifies/dia × 250ms = 12.5k sec CPU = 3.5h CPU/dia em workload total.
- Hot-path mitigation: WI-S03-003 caches verify result em KV (TTL 60s); reduce verify rate ≥ 99%.
- Storage: 1 row per PAT em api_tokens (WI-S03-005); ~200 bytes/row.

## 23. API Contract

Crate é internal Rust; HTTP API exposed por WI-S03-003 middleware:

- `POST /api/v1/tokens` (admin op) → returns `{ id, plaintext, expires_at, scopes }`.
- `DELETE /api/v1/tokens/:id` (admin op) → marks `revoked_at` (WI-S03-004 propagates).
- `GET /api/v1/tokens` (admin op) → list (no plaintext, no hash).

Erro mapping (downstream middleware):
- `Malformed` → HTTP 401 `invalid_token_format`.
- `InvalidPat` → HTTP 401 `invalid_token` (constant-time; same response body as not-found).
- `HashError` → HTTP 500 `internal_error` (DB corruption).
- `EntropyUnavailable` → HTTP 503 `service_unavailable`.

## 24. Post-mortem Hooks

- Plaintext leakage em log/trace → CRITICAL post-mortem + immediate revoke all + customer notification (LGPD/GDPR breach if PII).
- Argon2 verify timing oracle detected (Mann-Whitney p < 0.05) → SEV-1 + 5-Why + emergency patch.
- Underprovisioned hash em prod (params drift) → SEV-1 + force re-mint all PATs.
- Scope spoofing exploit (server inferred from PAT prefix) → CRITICAL + breach review.
- DB exfil → activate runbook RB-FM-160 (auth invalid storm) + force PAT rotation.

5-Why mandatory em todos SEV-0/SEV-1.

## 25. Rollback / Recovery

Hot rollback via Wrangler. Per-token revoke via WI-S03-004 (emergency mass-revoke).

- RTO: ≤ 10 min (deploy rollback).
- RPO: 0 (stateless verify; storage state em D1/Neon owned by WI-S03-005).

Se Argon2 lib bug discovered: emergency rotation script — re-mint all active PATs + force user-facing notification "rotate your token" (UI banner + email via S-09 events).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Argon2 + constant-time verify resist offline crack + timing oracle.
- **Tampering**: PHC string self-validating; corrupted DB row → HashError.
- **Repudiation**: WI-S03-007 audit events `auth.token.issued/used/revoked`.
- **Info disclosure**: PatPlaintext newtype; no Debug/Display; no Serialize; cannot leak via accidental log.
- **DoS**: 64 MiB Argon2 cost = bound concurrent mints (memory budget); rate limit S-08.
- **Elevation of privilege**: scope DB-is-SOT (não PAT prefix).

**LINDDUN delta**:
- **Linkability**: PAT plaintext is opaque; principal_id em DB pseudonymized (UUID).
- **Identifiability**: PAT plaintext nunca em logs; hash safe.
- **Non-repudiation**: audit events S-09 chain.
- **Detectability**: timing constant prevents PAT enumeration via timing.
- **Disclosure**: redact_pat! macro S-09 em error paths.
- **Unawareness**: dashboard "Copy now you won't see again" UX guidance.
- **Non-compliance**: LGPD Art. 46 (technical/admin measures) + GDPR Art. 32 satisfied via Argon2 + audit.

## 27. Knowledge Transfer

- `crates/corelink-pat/README.md` — threat model + integration pattern.
- ADR-0025 + ADR-0026 — design rationale.
- Doc `docs/internal/pat-lifecycle.md` — sequence: mint→store→use→verify→revoke.
- Workshop interno (2h) com Crypto SME + AppSec + Engineering team pós-merge — adversarial walkthrough.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Argon2 lib CVE upgrade injete weakness | L | M | CRITICAL | L | LOW | cargo-audit weekly + version pin + property test |
| R-002 | Calibration drift (HW upgrade reduz cost) | M | H | MEDIUM | M | LOW | Calibration metric + alert; rebaseline annual |
| R-003 | Plaintext leak em log via accidental Display | L | M | CRITICAL | L | LOW | Newtype no Debug/Display; CI grep gate |
| R-004 | Timing oracle regression (constant-time bug) | L | L | HIGH | L | LOW | Mann-Whitney CI nightly; alert p < 0.05 |
| R-005 | Salt collision (entropy bug) | L | L | HIGH | L | LOW | getrandom CSPRNG + property test 100k unique |
| R-006 | DB exfil → offline crack | L | M | HIGH | L | LOW | Argon2id 64 MiB cost; intratável em foreseeable HW |
| R-007 | Latency p99 > 350ms breaks SLA | M | H | MEDIUM | M | LOW | Cost regression gate; calibration alert |
| R-008 | Scope spoofing via PAT prefix manipulation | L | H | HIGH | L | LOW | DB-is-SOT enforcement; never infer from string |
| R-009 | Memory DoS (concurrent mints exhaust 128 MiB) | M | M | MEDIUM | M | LOW | Mint rate limit + queue (S-13); calibration test |
| R-010 | Lib supply chain (typosquatting argon2 → argon2-evil) | L | L | CRITICAL | L | LOW | cargo-deny denylist + lockfile pin |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME review struct shape; ADR-0025/0026 outline.
2. **Crypto pair-program (D+2..D+5)**: Crypto SME pair-program mint+verify+parse_env; threat model walkthrough; adversarial test design.
3. **Code (D+6)**: peer review (2 engineers).
4. **AppSec (D+7)**: AppSec review threat model + cargo-fuzz results.
5. **Adversarial (pre-merge D+8)**: red team session — offline crack feasibility, timing oracle, downgrade.
6. **Mann-Whitney baseline (D+8)**: establish p-value baseline em CI nightly.
7. **PRR (D+10)**: Architect sign-off + readiness review.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — Argon2 params + threat model_ | _pending_ | _pending_ |
| 5 | Engineer (peer 1) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (peer 2) | _TBD_ | _pending_ | _pending_ |
| 7 | QA | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy | _TBD; PAT plaintext storage policy review_ | _pending_ | _pending_ |
| 11 | Architect | _TBD; calibration target + bitset layout reuse review_ | _pending_ | _pending_ |
| 12 | AppSec | _TBD; emphatic — threat model + cargo-fuzz results_ | _pending_ | _pending_ |
| 13 | Crypto SME | _**mandatory pair-program** — Argon2 OWASP 2024 params + Mann-Whitney constant-time + adversarial regression suite_ | _pending_ | _pending_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S03-002 (Lote 10.3). |

## 32. Anti-patterns evitados

- ❌ bcrypt/scrypt/pbkdf2 (Argon2id é OWASP 2024 primary).
- ❌ Custom Argon2 impl (audited crate only).
- ❌ Hard-coded salt (per-token random).
- ❌ Salt < 16 bytes.
- ❌ Plaintext storage em DB/log/trace/error.
- ❌ Variable-time `==` em hash compare.
- ❌ Variable-time prefix parsing.
- ❌ Scope inference from PAT string (DB é SOT).
- ❌ Lazy entropy (panic se unavailable).
- ❌ Static Argon2 params sem calibration validation.
- ❌ Single salt for tenant.
- ❌ PAT in URL path/query (header only; logs sanitize).

---

**Fim WI-S03-002.** Próximo: WI-S03-003 (Tower middleware + TenantCtx injection + 5-layer defense propagation).
