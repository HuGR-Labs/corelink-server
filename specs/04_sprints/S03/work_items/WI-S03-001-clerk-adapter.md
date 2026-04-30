---
id: "WI-S03-001"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-30"
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
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s03", "auth", "clerk", "jwks", "jwt", "high-risk"]
---

# WI-S03-001 — Clerk Adapter (JWKS Cache 24h + JWT Validate + Clock-Skew ±60s)

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-03](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S03-001 |
| Título | Clerk adapter — JWKS cache 24h + JWT validate (RS256) + clock-skew tolerance + claims extraction |
| Sprint | S-03 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (cross-tenant via JWT spoof), FF-HR-005 (JWT validation é boundary cripto), FF-HR-009 (auth controls boundary defense) |

## 1. Intent

Implementar `crates/corelink-clerk/` adapter que valida JWTs emitidos por Clerk SSO (signing key RS256 publicada em JWKS), extrai claims canônicos, e resolve `(user_id, org_id) → tenant_id` para hidratar `Principal::User` no request context.

```rust
pub struct ClerkAdapter {
    jwks_cache: KvJwksCache,           // CF KV; TTL 24h; key URL = clerk_instance_jwks_url
    clock_skew: Duration,              // ±60s tolerance per RFC 7519 §4.1.4
    issuer_allowlist: Vec<String>,     // exact match; multi-instance support (staging/prod)
    audience: String,                  // exact match
}

impl ClerkAdapter {
    /// Valida JWT, retorna ClerkPrincipal extraído. Erros mapeados em AuthError.
    pub async fn validate(&self, raw_jwt: &str) -> Result<ClerkPrincipal, AuthError>;

    /// Force JWKS refresh (rotation event hook).
    pub async fn refresh_jwks(&self) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct ClerkPrincipal {
    pub user_id: ClerkUserId,          // sub claim; opaque newtype
    pub org_id: Option<ClerkOrgId>,    // org_id custom claim; multi-tenant hint
    pub email: Email,                  // verified email
    pub clerk_role: ClerkRole,         // admin / member / guest
    pub session_id: ClerkSessionId,    // sid claim
    pub issued_at: SystemTime,
    pub expires_at: SystemTime,
}

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("JWT signature invalid")]
    SignatureInvalid,
    #[error("JWT expired (now > exp)")]
    Expired,
    #[error("JWT not yet valid (now < nbf)")]
    NotYetValid,
    #[error("issuer mismatch (got {got}, expected {expected})")]
    IssuerMismatch { got: String, expected: String },
    #[error("audience mismatch")]
    AudienceMismatch,
    #[error("JWKS fetch failed: {0}")]
    JwksFetchFailed(String),
    #[error("KID not in JWKS")]
    KidNotInJwks,
    #[error("malformed JWT: {0}")]
    Malformed(String),
}
```

JWKS cache: `KvJwksCache` em CF KV (key `clerk:jwks:<instance_hash>`), TTL 24h. Cold-start fetches Clerk JWKS endpoint; warm hits return cached. Rotation event triggers `refresh_jwks` (manual or signal).

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

JWT validation é boundary crítico: vulnerabilidades clássicas (alg=none confusion, key confusion RS↔HS, expired tokens accepted, audience spoof, missing kid, signature forgery via JWKS poisoning) já causaram CVEs em libs maduras (jsonwebtoken pre-9.0, node-jsonwebtoken 2015, rust-jwt 2017). Implementar adapter próprio sobre `jsonwebtoken` 9.x (Rust crate, audited) reduz superfície vs hand-roll, mas não elimina misconfiguração — therefore HIGH_RISK.

**Bugs catastróficos possíveis**:

1. **alg=none acceptance** (CVE-2015-9235 family): cliente envia JWT com `"alg":"none"` + sem signature; biblioteca lazy aceita. Mitigação: explicit allowlist `ALG ∈ {RS256}` no decoder config (`Validation::new(Algorithm::RS256)`); reject anything else.
2. **Key confusion RS↔HS** (CVE-2018-0114): cliente envia JWT alg=HS256 com signature usando public key como HMAC secret; lazy decoder aceita. Mitigação: never instantiate `DecodingKey::from_secret` em path RS; só `from_rsa_pem`.
3. **Expired token accepted**: clock-skew unbounded, ou validação não checa `exp`. Mitigação: clock-skew ±60s explícito; `Validation { leeway: 60, validate_exp: true, .. }`.
4. **Audience/issuer not validated**: lazy decoder skip claims check; cliente reusa JWT de outra instância Clerk (staging→prod). Mitigação: `aud` exact match + `iss` allowlist (staging|prod URL distintas).
5. **JWKS poisoning**: atacante intercepta JWKS HTTP fetch, substitui chaves; subsequente JWT signed por atacante valida. Mitigação: HTTPS only + TLS pinning (S-04); JWKS fetch via fetch API com CF cert validation; KV store é eventualmente consistente mas integridade é via TLS, não KV.
6. **Stale JWKS quando Clerk rotaciona**: JWKS cache 24h; Clerk rotates a cada 12h (typical); cliente apresenta JWT signed com new key; KID lookup miss → request rejected. Mitigação: lazy refresh em KID miss antes de rejeitar; é "1 extra fetch" cost mas evita rejection storm em rotação.
7. **Clock skew window abuse**: ±60s permite cliente com clock dessincronizado autenticar JWT já expirado por 50s. Mitigação: ±60s é industry standard (RFC 7519 §4.1.4); diminuir só introduz UX issues sem ganho de segurança real.
8. **org_id resolution race**: cliente em onboarding; user existe em Clerk, org criada, JWT emitido, mas tenant_id ainda não persistido em Neon. Mitigação: lazy provisioning — se `org_id` em JWT mas `tenant` não existe em D1/Neon, retorna `AuthError::TenantNotProvisioned` (HTTP 412 Precondition Required); cliente retry após onboarding finaliza.

**Atacante adversarial scenarios**:

- **JWKS swap attack**: atacante hospeda JWKS controlado; via DNS poisoning ou CDN misconfig, redireciona JWKS fetch. Mitigação: HTTPS strict + cert validation; Clerk JWKS URL hard-coded em config (não dynamic).
- **JWT replay**: atacante captura JWT válido, replays. JWT sem nonce; replay impossível mitigar puramente no JWT layer. Mitigação: TTL JWT short (Clerk default 60min); revocation propagation em S-03 Worker (KV blacklist verificada por `jti` claim — opt-in WI-S03-004).
- **Cross-tenant via org_id confusion**: usuário do Tenant A reutiliza JWT em endpoint Tenant B (path-based routing). Mitigação: este WI extrai org_id; downstream middleware (WI-S03-003) valida `request.tenant_path == principal.tenant_id`.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: signature bypass = full cross-tenant access bypassing tenant prefix.
- **FF-HR-005**: JWT validation é cripto boundary; `subtle::ConstantTimeEq` em signature compare.
- **FF-HR-009**: defense-in-depth layer 1 (PAT/JWT scope) do 5-layer defense (auth_model §8.1).
- **Reversibility**: signature bypass detectado tarde (audit gap) = breach com customer notification overhead; mitigação proativa via property tests + adversarial fuzzing.

11 sign-offs canonical incl. Architect (Crypto SME specialization for JWT validation) + AppSec (JWKS fetch boundary review).

## 3. Customer Impact & Journey

**Persona 1 — Dev fazendo `bazel build` first time**:
- Auth flow: dashboard SSO Clerk → JWT emitido → CLI generates PAT (separate flow S-03 WI-002) → CLI auth com PAT.
- JWT path direct: usuário acessa `dashboard.corelink.dev` → Clerk JS SDK → JWT em cookie → API call para `dashboard-api.corelink.dev/me` → ClerkAdapter valida → returns user info.
- Customer-visible error: `401 invalid_token` se signature/issuer/audience invalid; `412 tenant_not_provisioned` se org criado mas Neon não tem registro (race condition).

**Persona 2 — Admin gerenciando tokens**:
- Admin acessa `dashboard/tokens` → API call com JWT → ClerkAdapter valida + extrai role=admin → middleware (WI-S03-003) verifica scope `admin-tokens` → renderiza UI.
- Customer-visible: latência p99 validate ≤ 5ms (warm JWKS); cold start (KV miss): ≤ 200ms (single fetch + cache populate).

**Persona 3 — Auditor running compliance review**:
- Audit log filter `auth.token.*` events; cada validate emite trace span (`clerk.validate`) com hashed `sub` (não raw user_id em logs em CLAIM_LEVEL=PUBLIC mode); auditor pode rastrear flow sem PII leakage.

**SLA addendum**: clock skew ±60s public-documented; usuários com sistema dessincronizado por > 60s recebem error `not_yet_valid` ou `expired` — recommended NTP sync.

## 4. Capability Mapping

- **CAP-AUTH-001** (Clerk SSO integration) — IMPLEMENTA primary.
- **CAP-AUTH-005** (Tenant provisioning hook) — IMPLEMENTA partial (TenantNotProvisioned error path).
- Trace: `auth_model.md §1.1 (User identity)` + `§2.4 (JWT Clerk validation flow)` + `§8.1 (5-layer defense, layer 1)`.

## 5. Tipo

Auth adapter; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-clerk/` crate scaffold** (workspace member):
   - `lib.rs` re-exports `ClerkAdapter`, `ClerkPrincipal`, `AuthError`.
   - Cargo.toml depends on `jsonwebtoken = "9.3"` (audited; explicit ALG allowlist API), `serde`, `serde_json`, `subtle`, `worker = "0.4"` (CF Workers Rust runtime).
2. **`ClerkAdapter::new` constructor**:
   - Args: `jwks_url: Url`, `issuer_allowlist: Vec<String>`, `audience: String`, `kv_namespace: KvNamespace`.
   - Validates JWKS URL is HTTPS scheme.
3. **`ClerkAdapter::validate(raw_jwt: &str)`**:
   - Decode header (sem verify) → extract `kid`.
   - Lookup KV `clerk:jwks:<instance_hash>` → if miss or stale (TTL expired), fetch JWKS endpoint via `worker::Fetch`.
   - Find key by `kid`; if not found, refresh JWKS (rotation case) + retry; if still miss → `AuthError::KidNotInJwks`.
   - Decode + verify signature usando `Validation::new(Algorithm::RS256)` + `validate_exp: true` + `leeway: 60`.
   - Validate `iss ∈ issuer_allowlist` (exact match).
   - Validate `aud == audience` (exact match).
   - Extract claims into `ClerkPrincipal`.
   - Returns `Result<ClerkPrincipal, AuthError>`.
4. **`KvJwksCache`** (private):
   - `get(instance_hash) -> Option<Jwks>` — KV lookup com TTL check.
   - `set(instance_hash, jwks, ttl_secs)` — KV PUT.
   - `delete(instance_hash)` — explicit refresh trigger.
5. **`refresh_jwks` method**: explicit fetch + cache replace; emit metric `clerk.jwks.refresh_total`.
6. **Métricas** (canonical underscored Prometheus form per observability_model.md §4.1; label `plan` per §3.1; outcome enum per §3.1):
   - `corelink_auth_clerk_validate_total{outcome, plan}` (outcome ∈ ok|sig_invalid|expired|aud_mismatch|iss_mismatch|kid_miss|jwks_fetch_failed).
   - `corelink_auth_clerk_validate_duration_seconds_bucket{outcome}` (histogram p50/p95/p99; SI unit seconds).
   - `corelink_auth_clerk_jwks_cache_hits_total` (counter).
   - `corelink_auth_clerk_jwks_refresh_total{trigger}` (trigger ∈ scheduled|kid_miss|manual).
7. **Observability** — trace span `clerk.validate` com attributes `clerk.kid`, `clerk.iss`, `clerk.aud`, `result` (no `sub` raw, only hashed). Logs structured JSON.
8. **Property tests** (10k iter):
   - alg=none rejected (regression CVE-2015).
   - alg=HS256 with public key as secret rejected (key confusion).
   - Expired token rejected (now > exp + 60s leeway).
   - aud mismatch rejected.
   - iss not in allowlist rejected.
   - Malformed JWT rejected (no panic).
9. **Integration test E2E**:
   - Real Clerk staging instance.
   - End-to-end: dashboard signup → JWT emitido → CoreLink `/me` endpoint validates → 200 OK.

### 6.2 Out-of-scope (deferred)

- **PAT validation** (different credential type): WI-S03-002 (corelink-pat).
- **Tower middleware integration** (request injection): WI-S03-003.
- **Revocation via session blacklist** (`jti` claim check): WI-S03-004.
- **WebAuthn admin flows**: WI-S03-006.
- **Tenant provisioning automation**: S-13 admin plane.
- **Multi-issuer per tenant** (cliente brings own Clerk instance): S-14 enterprise.
- **JWT introspection endpoint** (RFC 7662): pós-GA.

## 7. Anti-Scope

- ❌ Custom JWT lib (use `jsonwebtoken` 9.x audited).
- ❌ HS256 support (only RS256; phishing-resistant).
- ❌ alg=none "for testing" feature flag (anti-pattern; never).
- ❌ Lax `exp` validation ("warn but allow").
- ❌ JWKS fetch via HTTP (HTTPS only).
- ❌ Clock-skew > 120s (window too wide).
- ❌ Stateful session cookie (Clerk handles; we are stateless validator).
- ❌ Rate limit em validate path (rate limit é S-08; este é cripto fast path).
- ❌ Multi-key signature verification (Clerk emits 1 key per JWT; KID resolves to single key).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: ClerkAdapter JWT validate

  Background:
    Given ClerkAdapter configured with
      issuer_allowlist = ["https://clerk.corelink.dev"]
      audience = "corelink-api"
      jwks cached with kid_v1 RS256 public key
      clock_skew = 60s

  Scenario: Valid JWT signed by current key
    Given JWT signed by kid_v1 with valid claims
      | iss | https://clerk.corelink.dev |
      | aud | corelink-api |
      | sub | user_2abc |
      | org_id | org_xyz |
      | exp | now + 3600s |
      | iat | now |
    When validate(jwt) called
    Then Result::Ok(ClerkPrincipal { user_id: "user_2abc", org_id: Some("org_xyz"), .. })
    And metric corelink_auth_clerk_validate_total{outcome="ok"} incremented

  Scenario: alg=none rejected (CVE-2015 regression)
    Given JWT with header alg=none and no signature
    When validate(jwt) called
    Then Result::Err(AuthError::SignatureInvalid)
    And metric corelink_auth_clerk_validate_total{outcome="sig_invalid"} incremented

  Scenario: Key confusion RS-to-HS rejected (CVE-2018 regression)
    Given JWT with header alg=HS256
    Given signature computed with RSA public PEM as HMAC secret
    When validate(jwt) called
    Then Result::Err(AuthError::SignatureInvalid)
    (Validation enforces ALG ∈ {RS256}; HS256 rejected at decoder)

  Scenario: Expired token rejected
    Given JWT exp = now - 65s (beyond 60s leeway)
    When validate(jwt) called
    Then Result::Err(AuthError::Expired)
    And metric clerk.validate_total{result="expired"} incremented

  Scenario: Within clock-skew leeway accepted
    Given JWT exp = now - 30s (within 60s leeway)
    When validate(jwt) called
    Then Result::Ok(_) returned

  Scenario: Audience mismatch rejected
    Given JWT aud = "other-api"
    When validate(jwt) called
    Then Result::Err(AuthError::AudienceMismatch)

  Scenario: Issuer not in allowlist rejected
    Given JWT iss = "https://clerk.attacker.com"
    When validate(jwt) called
    Then Result::Err(AuthError::IssuerMismatch { got: "https://clerk.attacker.com", expected: "https://clerk.corelink.dev" })

  Scenario: KID rotation triggers JWKS refresh
    Given JWKS cached with kid_v1 only
    When JWT signed by kid_v2 (Clerk rotated key) presented
    Then ClerkAdapter detects KID miss
    And invokes refresh_jwks() (lazy refresh)
    And re-fetches JWKS containing kid_v2
    And validates JWT successfully
    And metric clerk.jwks_refresh_total{trigger="kid_miss"} incremented

  Scenario: KID still missing after refresh rejected
    Given JWKS refreshed; kid_unknown still not present
    When validate(jwt) called
    Then Result::Err(AuthError::KidNotInJwks)
    And no infinite refresh loop (single retry only)

  Scenario: Malformed JWT no panic
    Given input "not.a.jwt.at.all"
    When validate(input) called
    Then Result::Err(AuthError::Malformed(_))
    And no panic / no unwrap
```

## 9. Design Decisions

### 9.1 Why `jsonwebtoken` 9.x (não hand-roll)

`jsonwebtoken` 9.0+ (2023+) tem:
- Explicit ALG allowlist API (`Validation::new(Algorithm::RS256)`) — não aceita alg=none mesmo se header diz.
- Audited via cargo-audit; última CVE 2018 (corrigida 2.0.0).
- WASM-compatible; usado em Cloudflare Workers ecosystem.
- Hand-roll é anti-pattern (security boundary; reuse audited).

Alternativa rejeitada: `jwt` crate (deprecated, no audit history); `josekit` (overkill, JWE features unused).

### 9.2 Why JWKS cache TTL 24h

- Clerk rota chaves a cada 12-24h (typical SaaS rotation cadence).
- 24h cache + lazy refresh em KID miss = best balance: rare cold-start cost + zero rejection storm em rotation.
- < 24h: fetch storm cada cycle.
- > 24h: stale risk se rotation fora do schedule.

### 9.3 Why clock-skew 60s

- RFC 7519 §4.1.4 não impõe número específico; industry standard 30-300s.
- 60s = covers AWS NTP drift + ECS clock skew typical.
- < 30s: false negatives em sistemas com NTP delay.
- > 120s: window abuse (replay tokens já expirados).

### 9.4 Why issuer allowlist (não single string)

- Multi-instance Clerk: staging vs prod distinct issuers.
- Allowlist permite operar com `["https://clerk.staging.corelink.dev", "https://clerk.corelink.dev"]` em test envs sem hardcode prod.
- Exact match (não prefix); evitar `https://clerk.corelink.dev.attacker.com` confusion.

### 9.5 Why lazy JWKS refresh em KID miss

- Eager: refresh a cada 24h scheduled → 50% requests durante rotation cycle reject (KID v1 evicted antes cliente atualizar).
- Lazy: detect KID miss → refresh → retry once → success → cache populated com v2.
- Cost: +1 fetch em raro miss; trade-off correto.

### 9.6 Why no `nbf` (Not Before) strict check

- `nbf` é opt-in em RFC 7519; Clerk sometimes omits.
- Clock skew already covers boundary; adding nbf check sem leeway breaks valid tokens.
- Decision: validate `nbf` se presente, com mesma leeway 60s.

### 9.7 Why no `jti` (replay) check em este WI

- `jti`-based revocation = stateful (KV blacklist lookup per request).
- Latency tax (~3ms KV lookup) em hot path.
- Defer para WI-S03-004 (Revocation DO + KV) com explicit opt-in (admin ops only? ou all? — decisão lá).
- Este WI é stateless validator; revocation é layered.

### 9.8 Why `ClerkPrincipal` newtype IDs (não String)

- Type-driven security: `ClerkUserId(String)` distinct de `ClerkOrgId(String)` distinct de `TenantId(Uuid)`.
- Compiler prevents accidental swap.
- Construtor privado: `ClerkUserId` só construído por `ClerkAdapter::validate` — caller cannot forge.

### 9.9 ADR potencial?

- Sim — **ADR-0024**: "Clerk JWKS cache strategy (lazy refresh em KID miss vs eager scheduled)". Trade-off documentado para reuse em S-04 (PAT signing key rotation pode reusar pattern).
- Whitelist em `validate_references.py` até materializar.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** Property test 10k iter (PR) + 100k iter (nightly) sobre fuzz JWT inputs → 0 panics, 0 false-accepts (EVT-002).
- [ ] **10.5.2** Adversarial test: 5 CVE regressions (alg=none, key confusion, exp bypass, aud spoof, iss spoof) — 100% rejected (EVT-040).
- [ ] **10.5.3** E2E test contra Clerk staging instance: signup → JWT → validate → 200 OK; latência p99 ≤ 5ms warm, ≤ 200ms cold (EVT-018).
- [ ] **10.5.4** JWKS rotation chaos test: rotate key em Clerk staging; verify lazy refresh + valid request post-rotation ≤ 1s (EVT-023).
- [ ] **10.5.5** SAST clean (cargo-audit + cargo-deny + clippy `-D warnings`) (EVT-002).
- [ ] **10.5.6** Memory bound: validate path ≤ 32 KiB stack; zero alloc em hot path (KV cache hit case) (EVT-021).
- [ ] **10.5.7** Cost regression gate: validate p99 ≤ 5ms sustained 72h staging (Lote 9.4 §14.10).
- [ ] **10.5.8** OWASP ASVS V3.5 (token validation) 100% checklist pass (EVT-002).

## 11. DoD

- [ ] Crate `corelink-clerk` compila (workspace + standalone).
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em CI.
- [ ] E2E test contra Clerk staging green.
- [ ] Métricas emitidas (5 listadas §6.1.6).
- [ ] Trace span `clerk.validate` em OTel pipeline.
- [ ] rustdoc + 3 examples (basic, multi-issuer, rotation handling).
- [ ] ADR-0024 escrito.
- [ ] Code review (Crypto SME + AppSec).
- [ ] PRR Architect sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

- **INV-AUTH-JWT-VALIDATE-RS256-ONLY** (CRITICAL): adapter rejects all JWTs not signed RS256; property test enforces.
- **INV-AUTH-CLOCK-SKEW-BOUND** (HIGH): leeway ≤ 60s em todos os paths; código-gated constant.
- **INV-AUTH-ISS-EXACT-MATCH** (CRITICAL): issuer comparado exact (não prefix); regression test.
- **INV-AUTH-KID-RESOLUTION** (HIGH): KID miss triggers refresh once; max 1 retry; no infinite loop.
- **INV-NO-PII-IN-LOGS** (HIGH): `sub` claim hashed em logs; raw user_id never emitted (privacy_model.md §3.2).

TLA+ alignment: `tenant_isolation.tla` Layer 1 (PAT/JWT scope) — este WI implementa a parte JWT; PAT é WI-S03-002.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| `corelink-clerk` crate | `crates/corelink-clerk/` | Rust workspace member |
| ClerkAdapter | `crates/corelink-clerk/src/adapter.rs` | Rust |
| ClerkPrincipal + newtypes | `crates/corelink-clerk/src/principal.rs` | Rust |
| AuthError enum | `crates/corelink-clerk/src/error.rs` | Rust |
| KvJwksCache | `crates/corelink-clerk/src/jwks_cache.rs` | Rust (private) |
| Property tests | `crates/corelink-clerk/tests/prop_validate.rs` | Rust |
| Adversarial regression tests | `crates/corelink-clerk/tests/adversarial.rs` | Rust |
| E2E integration | `tests/e2e_clerk.rs` | Rust |
| ADR-0024 | `specs/03_architecture/adrs/ADR-0024-clerk-jwks-cache.md` | Markdown |
| Examples | `crates/corelink-clerk/examples/` (basic.rs, multi_issuer.rs, rotation.rs) | Rust |

## 14. Quality Standards SOTA

- **14.5.1** Zero `unsafe`; zero `unwrap` em src/ (allow em tests).
- **14.5.2** rustdoc 100% public API + 3 examples.
- **14.5.3** Test coverage ≥ 90% (`cargo tarpaulin`).
- **14.5.4** Latência: validate p99 ≤ 5ms warm; cold ≤ 200ms.
- **14.5.5** SAST: `cargo-audit` + `cargo-deny` + `clippy -D warnings` clean.
- **14.5.6** Métricas RED (rate, errors, duration) + JWKS cache metrics.
- **14.5.7** Runbook RB-FM-160 (auth invalid storm) — referenciado WI-S03-008.
- **14.5.8** Breaking changes em `ClerkPrincipal` struct = bump major + migration note.
- **14.5.9** Memory: ≤ 32 KiB stack em validate path; KV cache não memory-resident em Worker.
- **14.5.10** Cost regression gate em CI (auth p99 ≤ 5ms; baseline benchmark sustained).

## 15. Chaos Experiments

1. **Clerk JWKS endpoint outage**: simulate 503 from Clerk; verify graceful degradation (return error from cached key se TTL fresh; fail validate se cache cold). Hypothesis: existing valid JWTs continue working durante outage até cache expira (24h). Procedure: block Clerk JWKS URL via wrangler dev fetch interceptor; run validate flow; verify cache-served validates pass; cold validate fails AuthError::JwksFetchFailed. Abort: outage > 60min in staging cancels chaos test (real customer impact).
2. **JWKS rotation storm**: rotate Clerk key 5x em 5min; verify lazy refresh handles rotation sem reject storm. Hypothesis: ≥ 99% requests pós-rotation pass (max 1 reject window per rotation). Procedure: trigger Clerk staging rotation script; load 1k req/s validate; measure reject rate. Abort: reject rate > 5% sustained.
3. **Clock drift simulation**: skew Worker clock ±90s vs Clerk; verify ±60s leeway boundary. Hypothesis: JWT issued with skewed clock at ±59s passes; ±61s fails. Procedure: mock SystemTime via test injection; iterate skew values. Abort: not applicable (synthetic).
4. **JWKS poisoning attempt** (red team): inject malicious JWKS in KV directly; verify validate ignores (TLS pinning + integrity not enforceable purely at KV layer; mitigation is HTTPS only). Hypothesis: poisoning KV directly bypasses HTTPS; **defense é TLS, KV não é trust boundary**. Procedure: red team simulates KV write (sandbox); validate against signed JWT by injected key; expected: passes (KV poisoning works). Conclusion: documented limitation; mitigation = wrangler config controle (KV write privilege restricted to deployer, not tenant code).

## 16. PRR (Production Readiness Review)

**PRR HIGH_RISK 11 sign-offs (S-03 ship gate é WI-S03-008; este WI passa por mini-PRR Architect review)**:

- [ ] All Gherkin scenarios green.
- [ ] Property tests + adversarial tests green.
- [ ] E2E Clerk staging green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards configurados.
- [ ] ADR-0024 published.
- [ ] Crypto SME review (JWT validation).
- [ ] AppSec review (JWKS fetch boundary + TLS).
- [ ] Architect approval (cache pattern reusable S-04).
- [ ] OWASP ASVS V3.5 100% pass.
- [ ] Pentest hooks documented (target em WI-S03-008 pentest engagement).

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Crate scaffold + Cargo.toml deps | 1h |
| ST-002 | `ClerkPrincipal` + newtypes (UserId, OrgId, SessionId) | 1.5h |
| ST-003 | `AuthError` enum + thiserror | 0.5h |
| ST-004 | `KvJwksCache` impl (get/set/delete) | 2h |
| ST-005 | `ClerkAdapter::new` constructor + validation | 1h |
| ST-006 | `ClerkAdapter::validate` core logic (decode header, lookup, validate) | 4h |
| ST-007 | Lazy JWKS refresh em KID miss | 1.5h |
| ST-008 | Métricas emit (5 metrics) + trace span | 2h |
| ST-009 | Property tests 10k iter | 3h |
| ST-010 | Adversarial regression tests (5 CVEs) | 2.5h |
| ST-011 | E2E integration test contra Clerk staging | 3h |
| ST-012 | Chaos test JWKS rotation storm | 2h |
| ST-013 | rustdoc + 3 examples | 2h |
| ST-014 | ADR-0024 redação | 1.5h |
| ST-015 | Crypto SME + AppSec review feedback iteration | 2h |
| ST-016 | PRR Architect review | 1h |

**Total Optimistic**: ~30.5h. **PERT** (O=27h, M=30.5h, P=48h): **~33h**.

## 18. Dependencies

### Hard blockers

- S-01 SEALED (workspace + KV bindings infra reused).
- Clerk staging instance provisioned (operations team task; not blocker for spec).

### Soft blockers

- WI-S03-005 (Neon schema) — não bloqueante para validate path; tenant resolution pode usar D1 stub durante S-03 dev iteration; transição para Neon antes de seal.

### Outbound

- WI-S03-003 (middleware) — consumes `ClerkAdapter::validate` output.
- WI-S03-004 (revocation) — overlay opcional (jti blacklist).
- WI-S03-007 (audit events) — consumes `ClerkPrincipal` para emit `auth.token.validated` event.
- WI-S03-008 (PRR ship gate) — gates S-03 close.

## 19. Effort PERT

O: 27h, M: 30.5h, P: 48h → PERT **33h**.

## 20. Time-boxing

**36h hard limit**. If exceeded → escalation: split em sub-WI (Clerk core vs JWKS rotation handler).

## 21. Observability

5 métricas listadas §6.1.6. Trace span `clerk.validate` com attributes:
- `clerk.kid` (string)
- `clerk.iss` (string)
- `clerk.aud` (string)
- `result` (enum)
- `auth.principal_hash` (sha256(sub) prefix 8 chars; PII-safe)

Logs structured JSON; nivel INFO em success, WARN em mismatch, ERROR em jwks_fetch_failed.

Dashboard widget DASH-AUTH:
- Validate rate (req/s).
- Error rate por result.
- p99 latency.
- JWKS cache hit ratio.
- JWKS refresh rate.

## 22. Cost Analysis

- Validate path warm (KV hit): 0 fetch + 1 KV read ($0.50/M) + 1-2 ms CPU. Em 10M req/dia: ~$5/dia KV reads + Worker CPU baseline.
- Validate path cold (KV miss): +1 fetch ($0.50/M Worker subrequest) + JWKS download (~5 KiB). Cold rate < 0.01% (rotation events).
- JWKS refresh (lazy on KID miss): bounded ~12/dia (Clerk typical rotation cadence).
- Total custo S-03 auth path ≤ $10/dia em 10M req/dia. Aceitável vs alternative D1-backed session lookup ($30+/dia).

## 23. API Contract

`ClerkAdapter` é internal Rust; no external HTTP API surface. Consumido por WI-S03-003 (Tower middleware). Public types stable post v1.0 (workspace constraint).

Erro mapping (downstream middleware responsability):
- `SignatureInvalid | KidNotInJwks | Malformed` → HTTP 401 `invalid_token`.
- `Expired | NotYetValid` → HTTP 401 `token_expired`.
- `IssuerMismatch | AudienceMismatch` → HTTP 401 `token_audience_invalid`.
- `JwksFetchFailed` → HTTP 503 `auth_unavailable`.

## 24. Post-mortem Hooks

- Bypass de validate (signature accepted maliciously) → CRITICAL post-mortem + breach notification consideration LGPD/GDPR.
- JWKS poisoning detected → CRITICAL.
- alg=none acceptance regression → SEV-0 (CVE territory).
- Clock-skew abuse exploit → SEV-1.
- JWKS cache stale > 48h sem refresh → SEV-2 (degradation, não breach).

5-Why mandatory em todos os SEV-0/SEV-1.

## 25. Rollback / Recovery

Hot rollback via Wrangler `wrangler deploy --version-id <prev>`. Cache flush:
- KV `clerk:jwks:*` → manual delete via wrangler CLI.
- RTO: ≤ 10 min.
- RPO: 0 (stateless validator; no data loss possível).

Fallback degradation: se ClerkAdapter completamente inoperante, S-03 WI-S03-003 middleware rejeita auth → graceful degradation (sistema offline, não unsafe state).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: JWT signature validation prevents impersonation (RS256 audited).
- **Tampering**: signature integrity guaranteed; alg allowlist prevents downgrade.
- **Repudiation**: audit event `auth.token.validated` (WI-S03-007) per validate call; cannot deny.
- **Info disclosure**: JWT contém PII (sub, email); logs hashed; no raw em traces.
- **DoS**: validate é CPU-light (~2ms); rate limit S-08 protege bulk.
- **Elevation of privilege**: org_id em JWT validated downstream (WI-S03-003); este WI extrai apenas.

**LINDDUN delta**:
- **Linkability**: `sub` (Clerk user_id) hashed em logs; análises temporais via session_id (Clerk-controlled rotation).
- **Identifiability**: `email` claim é PII; consumed downstream com PII redaction macros (S-09).
- **Non-repudiation**: token validation events imutables (S-09 chain integrity).
- **Detectability**: validate event surface não revela existência de outros usuários (timing constant via WI-S02-004 middleware).
- **Disclosure of information**: claims expostas mínimo necessário (sub, org, role); demais claims ignored.
- **Unawareness**: privacy notice em SaaS landing → cliente sabe Clerk processa identity (Clerk DPA).
- **Non-compliance**: LGPD/GDPR compliance via Clerk DPA + DSR support S-11.

## 27. Knowledge Transfer

- `crates/corelink-clerk/README.md` — overview + integration pattern para outros crates auth (S-04, S-13).
- ADR-0024 — JWKS cache strategy reusable.
- Doc `docs/internal/auth-flow.md` — sequence diagram Clerk→ClerkAdapter→middleware→handler.
- Workshop interno (1h) com Crypto SME + AppSec pós-merge.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | alg=none regression em jsonwebtoken upgrade | L | H | CRITICAL | M | LOW | Property test + cargo-audit + version pin |
| R-002 | JWKS rotation reject storm | M | M | HIGH | M | LOW | Lazy refresh em KID miss + chaos test |
| R-003 | JWKS endpoint outage | L | H | HIGH | L | LOW | 24h KV cache + graceful degradation |
| R-004 | Clock drift causa false negatives | M | M | LOW | L | LOW | ±60s leeway + customer NTP advisory |
| R-005 | KV poisoning (insider/misconfig) | L | L | CRITICAL | L | LOW | wrangler config restrict; HTTPS only fetch source-of-truth |
| R-006 | PII em logs (sub raw) | L | M | HIGH | L | LOW | Macro hash sub; static analysis lint |
| R-007 | jsonwebtoken supply chain | L | M | CRITICAL | L | LOW | cargo-audit weekly + cargo-deny denylist |
| R-008 | Latency p99 > 5ms warm | L | H | MEDIUM | L | LOW | Cost regression gate + bench |
| R-009 | Multi-issuer config drift staging→prod | M | L | MEDIUM | L | LOW | Issuer allowlist em config; deploy template review |
| R-010 | TenantNotProvisioned race em onboarding | M | M | LOW | L | LOW | 412 retry; idempotent provisioning hook S-13 |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect review struct shape + ADR-0024 outline.
2. **Crypto (D+2)**: Crypto SME review JWT validation logic; pair-program adversarial tests.
3. **Code (D+5)**: peer review (2 engineers).
4. **AppSec (D+6)**: AppSec review JWKS fetch boundary + TLS pinning posture.
5. **Adversarial (pre-merge D+7)**: red team session — alg=none, key confusion, JWKS poisoning.
6. **PRR (D+9)**: Architect sign-off + readiness review (gates inclusion em WI-S03-008 ship gate).

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; cache pattern + lazy refresh + Crypto SME specialization mandatory (JWT validation logic + alg allowlist + 5 CVE regressions)_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — JWT validation boundary review_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-03 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — JWKS fetch boundary + TLS posture_ | _pending_ | _pending_ |

> Crypto SME (JWT validation + alg allowlist + 5 CVE regressions) folds into Architect role specialization mandatory. Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect roles per framework §33.5.4.3 + ADR-0034 solo-tier waiver).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S03-001 (Lote 10.3). |

## 32. Anti-patterns evitados

- ❌ Hand-roll JWT (use `jsonwebtoken` 9.x audited).
- ❌ alg=none acceptance "for testing".
- ❌ HS256 fallback (RS256 only).
- ❌ Lax `exp` validation (enforced + leeway 60s explicit).
- ❌ JWKS over HTTP (HTTPS only).
- ❌ Single global JWKS cache (per-instance, KV namespace per env).
- ❌ Eager JWKS refresh em scheduled job (lazy on KID miss).
- ❌ PII raw em logs (`sub` hashed; `email` redacted via S-09 macros).
- ❌ Stateful session lookup em hot path (stateless validator; revocation overlay separado).
- ❌ Rate limit em validate (S-08 owns rate limiting; cripto fast path).

---

**Fim WI-S03-001.** Próximo: WI-S03-002 (corelink-pat crate Argon2id + timing-safe verify).
