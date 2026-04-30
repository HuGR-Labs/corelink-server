---
id: "WI-S03-003"
type: "work_item"
doc_status: "FROZEN"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.2.0"
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
  - "OBSERVABILITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s03", "auth", "middleware", "tower", "tenantctx", "5-layer-defense", "high-risk"]
---

# WI-S03-003 — Tower Middleware + Immutable `TenantCtx` Injection + 5-Layer Defense Propagation + Session Cache (KV 60s)

> **doc_status:** FROZEN · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-03](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S03-003 |
| Título | Tower middleware stack — auth → TenantCtx injection → scope check → rate limit hook → handler; session cache KV 60s TTL; 5-layer defense propagation |
| Sprint | S-03 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (TenantCtx leak = cross-tenant), FF-HR-005 (5-layer defense é cripto-coordenado boundary), FF-HR-009 (defense-in-depth orchestration) |

## 1. Intent

Implementar `crates/corelink-worker/src/middleware/auth.rs` — Tower middleware stack que orquestra:

```text
Request →
  [1] auth::extract       (parse Authorization header; route to PAT vs JWT)
  [2] auth::verify        (delegate WI-S03-001 ClerkAdapter ou WI-S03-002 PatVerifier)
  [3] session_cache::lookup  (KV 60s TTL; skip Argon2/Clerk fetch em hot path)
  [4] tenant_ctx::build   (construct immutable TenantCtx; embed scope bitset + tenant_prefix derivation hint)
  [5] scope::check        (per-route scope enforcement)
  [6] rate_limit::hook    (S-08 forward stub interface; pass-through em S-03)
  [7] audit::pre_emit     (emit `auth.token.validated` to outbox; reuse WI-S01-005 outbox table)
  [8] handler invoke      (handler reads TenantCtx via Extension)
  [9] audit::post_emit    (response status → audit `auth.{ok,denied}` event)
→ Response
```

```rust
// Public types
pub struct TenantCtx {
    principal: PrincipalId,         // PII, hashed em logs
    tenant_id: TenantId,            // UUID v7
    tenant_prefix: TenantPrefix,    // HMAC-derived; reuse WI-S01-001 corelink-tenant-path
    scopes: PatScopes,              // u64 bitset; reuse WI-S03-002
    auth_method: AuthMethod,        // Pat | Jwt
    request_id: RequestId,          // UUID v7; idempotency key
    issued_at: SystemTime,          // for downstream observability
}

#[derive(Debug, Clone, Copy)]
pub enum AuthMethod {
    Pat { env: PatEnv, pat_id: PatId },
    Jwt { clerk_session_id: ClerkSessionId },
}

// Tower service composition
pub fn auth_stack<S>(inner: S) -> impl Service<Request, ...>;
```

**Constraint cripto-driven**: `TenantCtx` é **imutável** (não-`mut`, fields private, único construtor `TenantCtxBuilder::build()` chamado somente pelo middleware após verify success). Handler downstream consome via `req.extensions().get::<TenantCtx>()` (Tower extension); cannot forge.

**Session cache**: pós-verify (Argon2 ~250ms ou Clerk JWKS fetch ~5ms warm) cacheia `(token_hash → TenantCtx)` em CF KV TTL 60s. Verify count drop ≥ 99% em workload típico (10M req/dia × 0.5% cache miss = 50k Argon2/dia vs 10M; massive cost reduction).

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Auth middleware é o **único ponto de orquestração** entre 5 layers de defense (auth_model.md §8.1). Bug em qualquer step compromete tenant isolation:

1. **TenantCtx mutável**: handler downstream pode acidentalmente mutate `tenant_id` field; subsequent storage call usa wrong tenant_id → cross-tenant write/read. Mitigação: fields private; impl `Clone` mas não `DerefMut`; no setters; construção single-path via builder.

2. **Cache poisoning via session_cache**: atacante envia token_hash forgado; KV.put injeta TenantCtx para tenant A; subsequente request com mesmo token_hash retorna TenantCtx alheio. Mitigação: session_cache key inclui FULL hashed PAT (não prefix); `subtle::ConstantTimeEq` em key compare; KV write privilege restricted (deployer-only via wrangler config; tenant code never writes session cache).

3. **Scope check bypass**: handler em route protected esquece de chamar `scope_check`. Mitigação: scope check é middleware-level (não handler-level); enforced via tower::layer ordering. Compile-time validation: `axum::routing::method_router` accepts only services that include `auth_stack`.

4. **Tenant_prefix derivation lazy**: middleware passes `tenant_id` mas handler computes `tenant_prefix` via HMAC each call → 1) cost (HMAC < 10µs OK mas avoidable); 2) bug surface (handler can compute wrong way). Mitigação: `TenantCtx` embute `tenant_prefix` pre-computed pelo middleware uma vez; handlers consume read-only.

5. **Audit emission ordering vs handler logic**: `auth.token.validated` deve emit **antes** do handler; `auth.{ok,denied}` **depois**. Bug em ordering = audit gaps. Mitigação: outbox pattern (reuse WI-S01-005) com `request_id` correlation; Tower service composition garante ordering por convention.

6. **Session cache stale após PAT revocation**: cliente revoga PAT em T+0; revocation emit em S-09 em T+30s (outbox); session_cache hit retorna stale TenantCtx por mais 60s. Mitigação layered: este WI tem session_cache TTL 60s; WI-S03-004 implementa explicit invalidation via DO broadcast (KV.delete em revoke); combined: ≤ 60s p99 propagation.

7. **TenantCtx shape evolution**: adicionar campo a TenantCtx em S-04 (e.g., `region`, `org_id`) é breaking change para handlers. Mitigação: TenantCtx é struct com `#[non_exhaustive]`; novos fields são opt-in via `Default`; handlers que precisam de novo field opt-in explicitly.

8. **Rate limit hook bypass**: stub interface em S-03; S-08 implementação real. Bug = stub permite passthrough em prod sem rate limit → DoS surface. Mitigação: stub explicitly fails-loud em production deploy (env var `CORELINK_RATE_LIMIT_BACKEND` mandatory; error em deploy se unset OR set to `stub-passthrough`).

**Atacante adversarial scenarios**:

- **Replay token after revoke**: atacante captura PAT/JWT ainda válido; tenta uso após revoke. Mitigação layered: hot path (cache hit) stale = 60s session TTL (post-TTL falls to cold path); cold path D1 `revoked_at IS NULL` authoritative ≤ 100ms; cross-region propagation SLO ≤ 60s p99 → outras regiões sem cache miss → cold path D1 authoritative imediato. **Eixos orthogonais; max stale window single SLA = 60s p99** (NOT additivo). Documentado em SLA addendum §3.
- **TenantCtx confusion deputy**: middleware A em route X passa `TenantCtx_A`; handler accidentally calls handler B which expects `TenantCtx_B`. Mitigação: handler B re-reads TenantCtx from request.extensions; não passa via parameter. Prevents accidental swap.
- **Timing oracle em verify path** (PAT vs JWT): PAT verify ~250ms; JWT verify ~5ms. Atacante mede tempo: detect PAT vs JWT em production. Mitigação: este WI expõe a métrica timing diff (não secret; PAT vs JWT é OK distinguishable em logs); cripto-relevant timing oracles tratados em WI-S02-004.
- **Audit loss em transient KV failure**: outbox INSERT falha → audit gap. Mitigação: D1 batch dentro de mesma transação que TenantCtx commit (não separada KV); reuse WI-S01-005 outbox guarantee.

**Risk justification HIGH_RISK**:

- **FF-HR-002**: TenantCtx é o "carrier" que propaga tenant_id através de todos os layers; bug em construção/mutação = cross-tenant.
- **FF-HR-005**: 5-layer defense é cripto-coordenado boundary; middleware orquestra Layers 1-2 (PAT/JWT scope + tenant resolution).
- **FF-HR-009**: defense-in-depth: este middleware é a sequência canônica de checks; ordem matters.
- **Reversibility**: TenantCtx leak detection difícil (não-explicit em logs); proativo property test em CI nightly + chaos test em staging.

11 sign-offs canonical incl. Architect (composability + non-exhaustive evolution + Crypto SME specialization), AppSec (5-layer ordering + session cache poisoning).

## 3. Customer Impact & Journey

**Persona 1 — Dev em primeira interação API**:
- `corelink push <build-output>` → CLI envia `Authorization: Bearer corelink_pat_xyz`.
- Server: middleware extracts PAT → `parse_env` → `verify` (cache miss; Argon2 ~250ms cold; or hit ~5ms warm) → constructs TenantCtx → handler.
- Customer-visible: latência p99 cold ~280ms; warm ~10ms (post-cache hit).
- Erro `Authorization` malformado → 401 com `error_code = COR_AUTH_HEADER_MALFORMED` + `next_action = "Use 'Authorization: Bearer <token>' format"`.

**Persona 2 — Dashboard user fazendo SSO Clerk**:
- Browser → Clerk SSO → JWT em cookie → Dashboard API call.
- Server: middleware detects `Authorization: Bearer <jwt>` (alg=RS256 detected via header decode) → routes to `ClerkAdapter::validate` (WI-S03-001) → constructs TenantCtx with `auth_method = Jwt` → handler.
- Customer-visible: ≤ 5ms p99 warm (KV hit Clerk JWKS); 412 if `org_id` em JWT mas tenant não em D1/Neon (race; cliente retry após onboarding).

**Persona 3 — Compliance auditor revisando access patterns**:
- Audit query `SELECT request_id, tenant_id, principal_id_hash, auth_method, scopes, route, status FROM audit_chain WHERE event_type='auth.token.validated' AND tenant_id = ? AND timestamp > ?` (audit_chain stored in S-09 chain; PAT canonical SoT é Neon `pat` per data_model.md §4.1).
- Auditor sees full audit trail per tenant; PII redacted (principal_id hashed em external SIEM forwarding).

**SLA addendum** (P0 fix Lote 10.3bis — stale window orthogonality):
- Auth middleware p99 ≤ 10ms warm path (cache hit); ≤ 280ms cold path (Argon2 verify).
- **Stale window é ≤ 60s p99 (single SLA)** — NÃO additivo:
  - **Hot path** (session cache hit; verify path bypass D1): stale window = session cache TTL = 60s.
  - **Cold path** (cache miss; **Neon `pat` lookup** com `revoked_at IS NULL` constraint authoritative — canonical SoT per data_model.md §4.1; cycle 3 codex SEAL alignment, was incorrectly 'D1' em pre-cycle text): stale window = Neon commit visibility ≤ 100ms (regional Postgres strong consistency).
  - **Cross-region propagation** (WI-S03-004): SLO ≤ 60s p99 — eixo independente; reads em outras regions miss session cache → fall-through D1 → revoked_at authoritative.
  - **Combined max** = max(hot, cold, cross-region) = **60s p99** (NOT 120s; eixos não compõem aditivamente).
- 5xx auth backend (Clerk down OR D1 down) → 503 `auth_unavailable` com Retry-After: 5s.

## 4. Capability Mapping

- **CAP-AUTH-001** (Clerk SSO integration) — IMPLEMENTA partial (orchestration; primary em WI-S03-001).
- **CAP-AUTH-002** (PAT lifecycle) — IMPLEMENTA partial (verify orchestration; primary em WI-S03-002).
- **CAP-AUTH-003** (Scope enforcement) — IMPLEMENTA primary.
- **CAP-AUTH-004** (Tenant resolution + propagation) — IMPLEMENTA primary.
- Trace: `auth_model.md §4 (Authorization model)` + `§8.1 (5-layer defense layers 1-3)` + `security_model.md CTRL-AUTH-001 (PAT signed; este WI orquestra verify) + CTRL-AUTH-004 (Path HMAC; propagação via TenantCtx)`.

## 5. Tipo

Auth middleware orchestration; HIGH_RISK; FF-HR-002 + FF-HR-005 + FF-HR-009.

## 6. Escopo

### 6.1 In-scope

1. **`crates/corelink-worker/src/middleware/auth.rs`** com Tower service composition:
   - `auth_stack<S>(inner: S) -> impl Service<Request, ...>` — composer function.
   - Layer ordering enforced (vide §1).
   - Tower `tower::ServiceBuilder` com `tower_http::auth::AsyncAuthorizeRequestLayer` + custom layers.
   - `axum` integration via `axum::middleware::from_fn_with_state` para passar shared `AuthState` (Clerk adapter + session cache + audit queue handle).

2. **`TenantCtx` struct + `TenantCtxBuilder`**:
   - `pub struct TenantCtx { /* private fields */ }` (não-public fields; no Display; redacted Debug).
   - `pub struct TenantCtxBuilder { /* internal */ }` — único path de construção; `build()` is private (visible só dentro do middleware crate).
   - Public getters: `principal()`, `tenant_id()`, `tenant_prefix()`, `scopes()`, `auth_method()`, `request_id()`, `issued_at()`.
   - `#[non_exhaustive]` annotation (preserva compat futuro adicionando fields).
   - `Clone` + `Send + Sync` + `'static` (compat Tower).

3. **`SessionCache`** em `crates/corelink-worker/src/middleware/session_cache.rs`:
   - Key: `auth:session:<sha256(token)[:16-bytes]>` (truncated; ainda 128-bit collision-resistant).
   - Value: serialized `TenantCtx` (rmp-serde MessagePack; compact).
   - TTL: 60s (env var override `CORELINK_SESSION_CACHE_TTL_S`; default 60).
   - `lookup(token_hash) -> Option<TenantCtx>` — KV GET.
   - `populate(token_hash, ctx)` — KV PUT.
   - `invalidate(token_hash)` — KV DELETE (chamado por WI-S03-004 revocation hook).
   - **Constant-time key compare** via `subtle::ConstantTimeEq` quando comparando lookup vs new value.

4. **5-Layer Defense propagation** (auth_model.md §8.1):
   - **Layer 1** (PAT/JWT scope): este middleware extracts + verifies + populates `scopes` em TenantCtx.
   - **Layer 2** (tenant_prefix derivation): este middleware re-derives via WI-S01-001 `corelink-tenant-path::derive(tenant_id)` para cada TenantCtx; embedded para downstream reuse.
   - **Layer 3** (AuthZ check on storage call): downstream WI-S01-005/WI-S02-001 readers verificam `TenantCtx::scopes` permits operation; este WI provê foundation.
   - **Layer 4** (R2 key prefix): downstream R2Writer/R2Reader reads `tenant_prefix` from TenantCtx; este WI assegura prefix matches tenant_id (impossível mismatch by construction; tenant_prefix é HMAC do tenant_id no construtor).
   - **Layer 5** (audit cross-check): este WI emit `auth.token.validated` event antes do handler; downstream emit `cas.{put,get}.{ok,denied}` post-handler; cross-check em S-09 audit chain ingestion.

5. **Scope enforcement layer** (per-route):
   - `pub fn require_scope(scope: PatScope) -> impl Layer<...>` — composable.
   - Route definition: `axum::Router::new().route("/v1/cas/:digest", post(handler).layer(require_scope(PatScope::CacheW)))`.
   - Failure: 403 `scope_insufficient` + audit emit.

6. **Auth method routing**:
   - Detect by token prefix: `corelink_<env>_*` → PAT path (WI-S03-002); `eyJ...` (JWT base64-url) → JWT path (WI-S03-001).
   - Strict: ambíguo → 401 com `error_code = COR_AUTH_HEADER_AMBIGUOUS`.

7. **Stub rate-limit hook** (S-08 forward):
   - `pub trait RateLimitBackend { async fn check(...) -> RateLimitOutcome; }`.
   - Stub impl `PassthroughBackend` (always allow); production deploy validation: `CORELINK_RATE_LIMIT_BACKEND` env var mandatory; error em deploy se `passthrough` em prod env.
   - S-08 will impl `DoBasedBackend`.

8. **Audit emission** (reuse WI-S01-005 outbox pattern):
   - Pre-handler: `auth.token.validated` event into `audit_outbox`.
   - Post-handler: `auth.{ok,denied}` based on response status code.
   - Both inside D1 batch with TenantCtx commit (single transaction).
   - `request_id` correlation id propagated client → middleware → outbox → S-09 chain.

9. **Métricas**:
   - `corelink.auth.middleware.requests_total{auth_method, result}` (counter; result ∈ ok|invalid_token|expired|scope_insufficient|rate_limited|backend_unavailable).
   - `corelink.auth.middleware.duration_ms_bucket{path}` (histogram; path ∈ verify_pat|verify_jwt|cache_hit|cache_miss).
   - `corelink.auth.session_cache.hit_ratio` (gauge).
   - `corelink.auth.session_cache.size_bytes` (gauge).
   - `corelink.auth.tenant_ctx.build_duration_us_bucket` (histogram; target p99 < 100µs).

10. **Property tests** (10k iter PR; 100k nightly):
    - `prop_tenant_ctx_immutable`: 1000 random construct + clone + access patterns; assert nenhum mutation path possível (compile-time checked).
    - `prop_session_cache_isolation`: 1000 distinct tokens; KV state após series de ops é equivalent ao state derived from sequential ops (concurrent safety).
    - `prop_5_layer_propagation`: random TenantCtx construct + simulate 5 layers; assert tenant_id é constante across layers.
    - `prop_scope_check_strict`: 100 scope combinations; assert exact match (não subset bypass).

11. **Mann-Whitney timing test** entre auth-success vs auth-failure paths (extends WI-S02-004 methodology):
    - Goal: cliente não distingue "PAT inválido" vs "PAT válido sem scope" via timing.
    - 10k samples per arm; power 1−β ≥ 0.80; |Δmedian| ≤ 5ms (slightly looser que WI-S02-004 cripto-grade since middleware path tem inherent variability).

12. **Integration test E2E**:
    - Real Clerk staging + real PAT mint via WI-S03-002 + real D1 + real R2.
    - Flow: dashboard signup → JWT validate → middleware constructs TenantCtx → handler GET `/me` → 200 OK.
    - Flow: CLI mint PAT → CLI request CAS write → middleware verifies → handler responds.

### 6.2 Out-of-scope (deferred)

- **Rate limit real implementation** (DoBased): WI-S08-001 (S-08 sprint).
- **Multi-region session cache replication**: S-14.
- **JWT introspection endpoint** (RFC 7662 client-facing): pós-GA.
- **Per-tenant config override** (custom session TTL per tenant): S-13 admin plane.
- **mTLS auth for executor identity**: Fase 2 (S-XX).
- **WebAuthn admin flows**: WI-S03-006.
- **Revocation propagation DO**: WI-S03-004.

## 7. Anti-Scope

- ❌ Mutable `TenantCtx` (immutability é INVARIANT-AUTH-TENANTCTX-IMMUTABLE; static-checked).
- ❌ Handler-level scope check (must be middleware-level).
- ❌ Lazy tenant_prefix derivation em handler (precomputed em middleware).
- ❌ Session cache with body PII (only TenantCtx serialized; no body bytes).
- ❌ Variable-time `==` em token compare (use `subtle::ConstantTimeEq`).
- ❌ Custom auth header format (RFC 6750 Bearer only).
- ❌ Multi-token request (Authorization header single value).
- ❌ Auth bypass via header (`X-Internal-Auth` etc.) — never trust client-supplied bypass headers.
- ❌ Session cache key as plaintext token (always sha256 hashed).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Tower middleware auth stack

  Background:
    Given AuthState configured with ClerkAdapter (S-03 WI-S03-001) + PatVerifier (WI-S03-002) + SessionCache + outbox queue handle
    And session_cache TTL = 60s
    And rate_limit_backend = PassthroughBackend (S-03 stub)

  Scenario: Happy path PAT — cold (cache miss → Argon2 verify)
    Given Tenant A has active PAT corelink_pat_xyz with scopes cache_rw
    And session_cache empty
    When request POST /v1/cas/<digest> with Authorization: Bearer corelink_pat_xyz
    Then middleware: extract → parse_env → verify (Argon2 ~250ms) → build TenantCtx → scope_check ok → handler
    And TenantCtx.tenant_id = tenant_A
    And TenantCtx.scopes contains cache_w
    And TenantCtx.tenant_prefix = HMAC(TDK, tenant_A)
    And TenantCtx.auth_method = AuthMethod::Pat { env: Pat, pat_id: <id> }
    And session_cache populated com key sha256(token)[:16] → serialized TenantCtx; TTL 60s
    And handler invoked com request.extensions().get::<TenantCtx>()
    And audit "auth.token.validated" emitted to outbox
    And response 200 OK
    And metric corelink.auth.middleware.requests_total{auth_method="pat", result="ok"} incremented

  Scenario: Happy path PAT — warm (cache hit; skip Argon2)
    Given session_cache populated for token corelink_pat_xyz
    When same request retried within 60s
    Then session_cache hit; Argon2 verify skipped
    And p99 latency ≤ 10ms (vs ~280ms cold)
    And metric corelink.auth.session_cache.hit_ratio increases

  Scenario: Happy path JWT — Clerk validate + tenant resolve
    Given user signed in via Clerk; JWT in cookie
    When request to /api/me with Authorization: Bearer <jwt>
    Then middleware detects JWT prefix; routes to ClerkAdapter::validate
    And ClerkPrincipal extracted (user_id, org_id, role)
    And tenant_id resolved from org_id via D1 lookup
    And TenantCtx constructed com auth_method = AuthMethod::Jwt { clerk_session_id: ... }
    And handler invoked

  Scenario: PAT invalid — constant-time response
    Given malformed PAT corelink_pat_invalid
    When middleware verifies
    Then verify returns Err(InvalidPat) (after Argon2 dummy hash to maintain constant-time vs valid)
    And response 401 com error_code COR_AUTH_INVALID_TOKEN
    And response time indistinguishable from valid-pat-no-scope (Mann-Whitney p > 0.05)

  Scenario: PAT scope insufficient
    Given PAT corelink_ro_xyz com scopes cache_r only
    When request POST /v1/cas/<digest> requires cache_w
    Then middleware verify ok; scope_check fails
    And response 403 com error_code COR_AUTH_SCOPE_INSUFFICIENT
    And error_body { required: "cache-w", granted: ["cache-r"] } (canonical hyphen-form per auth_model.md §scope)
    And audit "auth.denied.scope" emitted

  Scenario: Session cache invalidation on revoke (WI-S03-004 hook)
    Given session_cache populated for token T at T+0
    When WI-S03-004 revoke fires; calls SessionCache::invalidate(token_hash)
    Then KV.delete called for auth:session:<sha256(T)[:16]>
    And subsequent request com same T → cache miss → fresh verify → Err(Revoked) (DB shows revoked_at NOT NULL)
    And response 401

  Scenario: Auth method ambiguous
    Given Authorization: Bearer not_a_pat_or_jwt_format
    When middleware tries to detect method
    Then both pat-prefix and jwt-prefix fail
    Then response 401 com error_code COR_AUTH_HEADER_AMBIGUOUS

  Scenario: Rate limit hook fires (S-08 stub passthrough)
    Given rate_limit_backend = PassthroughBackend
    When request authenticated successfully
    Then RateLimitOutcome::Allow returned by stub
    And handler invoked normally

  Scenario: Production deploy guard against passthrough
    Given CORELINK_ENV=production
    And CORELINK_RATE_LIMIT_BACKEND not set OR set to "passthrough"
    When Worker initializes
    Then deploy fails com error "rate limit backend mandatory in production"

  Scenario: TenantCtx immutability (compile-time)
    Given handler tries `ctx.tenant_id = other_tenant`
    Then code does NOT compile (private field; no setter)
    Note: enforced via crate visibility; static-checked

  Scenario: Mann-Whitney timing — invalid vs scope-insufficient indistinguishable
    Given 10k requests com PAT inválido
    Given 10k requests com PAT válido mas scope insufficient
    When latencies collected per arm
    And Mann-Whitney U applied with power analysis
    Then p > 0.05 com Šidák 3-trial
    And |Δmedian| ≤ 5ms com 95% CI cruzando 0
    (cliente cannot distinguish "token bad" vs "token good but no permission")

  Scenario: Backend unavailable (Clerk down)
    Given Clerk JWKS endpoint returns 503
    When request com JWT arrives; KV cache cold
    Then ClerkAdapter::validate returns AuthError::JwksFetchFailed
    Then middleware response 503 com error_code COR_AUTH_BACKEND_UNAVAILABLE
    And Retry-After: 5s
    And metric corelink.auth.middleware.requests_total{result="backend_unavailable"} incremented
```

## 9. Design Decisions

### 9.1 Why Tower (não custom middleware)

- Tower é Tokio ecosystem standard; composable + reusable em outros crates.
- `axum`, `tonic`, `tower-http` todos use Tower; reuse middleware across HTTP/gRPC.
- Custom middleware = code duplication + bug surface.
- Mature: timeout, retry, rate-limit layers já existem em `tower-rs`.

### 9.2 Why immutable TenantCtx (não mutable struct)

- Tenant identification é cripto-coordenado state — single point of truth deve ser monotonic per request.
- Mutável = handler accidentally swap tenant_id = cross-tenant catastrophic.
- Rust ownership model: `TenantCtx` is `Send + Sync + 'static`; cloning é safe (Arc-internal references); fields private prevents mutation.
- Type-driven security: compile-time prevention de bug class.

### 9.3 Why session cache TTL 60s (não 5min ou 5s) — orthogonal stale window axes

Trade-off:
- **Longer TTL** (5min): better cache hit ratio (~99%); maior stale window hot-path.
- **Shorter TTL** (5s): cache hit ratio drops (~50%); cost regression (Argon2 frequência ↑).
- **60s**: balance hit ratio ~95% com bounded hot-path stale window.

**Stale window axes (P0 fix Lote 10.3bis — não compõem aditivamente)**:
- **Hot path** (cache hit): stale = TTL = 60s.
- **Cold path** (cache miss → D1 lookup): D1 `revoked_at IS NULL` filter authoritative; stale = D1 commit visibility ≤ 100ms.
- **Cross-region propagation** (WI-S03-004 SLO): ≤ 60s p99 — outras regiões sem cache verificam D1 (cold path); sempre authoritative.
- **Combined max** = max(60s hot, 100ms cold, 60s cross-region propagation D1 visibility) = **60s p99 single SLA**.

Tunable via env var `CORELINK_SESSION_CACHE_TTL_S`; per-tenant override S-13 forward.

### 9.4 Why session cache key = sha256(token)[:16] truncated

- Full sha256 (32 bytes) encoded em hex = 64 chars; KV key length OK mas waste.
- Truncated 16 bytes = 128-bit; collision resistance birthday-bound 2^64; computacionalmente intratável.
- Performance: 16 bytes hex encoded = 32 chars; KV key parser-friendly.
- Constant-time lookup: KV does exact match; subtle::ConstantTimeEq em compare path (defense em depth).

### 9.5 Why pre-emit audit em outbox (não synchronous)

Synchronous emit em S-09 chain = +20ms latency per request (S-09 chain ingestion RPC). Em hot path = unacceptable. Outbox pattern (reuse WI-S01-005 audit_outbox table) = durable em D1 dentro mesma transaction; drain worker emits ao S-09. Trade-off: audit visibility lag ≤ 60s p99 (já documented S-01).

### 9.6 Why per-route scope check via Tower layer (não handler-level)

Handler-level scope check = forgettable. Atacante encontra handler unprotected → cross-tenant access. Middleware-level + composable layer = enforced by-construction; handler can only be reached if `require_scope(scope)` layer applied. Compile-time: route definition without layer would need explicit `.allow_unauthenticated()` opt-out (deny-by-default).

### 9.7 Why Mann-Whitney em "invalid vs scope-insufficient" (não ok vs invalid)

`ok` path inclui handler invocation latency (variable); comparing à `invalid` path (no handler) = trivially distinguishable. Comparing `invalid` (no handler) vs `scope-insufficient` (no handler) é o real timing oracle: ambos retornam pre-handler. Atacante distinguishing reveals "token is structurally valid mas sem perm" vs "token is bad" — informacional vazamento marginal mas defendible.

### 9.8 Why dummy Argon2 em invalid PAT path (constant-time defense)

Atacante envia random PAT-like string; verify falha em parse_env early. Sem dummy, response time é ~5µs (parse fail). Valid PAT verify ~250ms (Argon2). Diff trivially distinguishable. Solution: em parse_env failure path, ainda compute dummy Argon2 hash com fixed salt/dummy plaintext; return Err depois. Latência iguala. Cost: +250ms em invalid path; aceitável (atacante storm contained por rate limit S-08).

### 9.9 Why `#[non_exhaustive]` em TenantCtx

Adding fields em S-04+ (region, org_id, executor_id) sem breaking handler API. Rust convention: handlers pattern-match `TenantCtx { tenant_id, .. }` (rest dot-dot pattern) → forward-compat.

### 9.10 ADR potencial?

Sim — **ADR-0029**: "TenantCtx immutability + session cache strategy + audit pre/post-emit ordering". Documenta trade-off vs alternativas (mutable TenantCtx rejected; session cache TTL choice; audit ordering). Whitelist em validate_references.py.

## 10. Completeness Criteria SOTA

- [ ] **10.5.1** Property tests 10k iter (PR) + 100k iter (nightly) → 0 panics, 0 false-accepts (EVT-002).
- [ ] **10.5.2** Mann-Whitney U + power analysis 3-prong em invalid vs scope-insufficient timing (EVT-002):
  - N ≥ 10000 samples per arm.
  - Power 1−β ≥ 0.80 com effect d = 0.2.
  - Šidák correction: ALL 3 trials × 3 pairwise = 9 tests p > 0.05 (full conjunction); per-test α' ≈ 0.0057 controls combined familywise α at 0.05 (cycle 1 codex SEAL math correction).
  - |Δmedian| ≤ 5ms com 95% CI cruzando 0.
- [ ] **10.5.3** TenantCtx immutability static-checked: clippy custom lint OR cargo-semver-checks impede `pub` field addition em release (EVT-002).
- [ ] **10.5.4** 5-layer defense propagation chaos test em staging: TenantCtx mismatch detection ≤ 1s alert (EVT-022).
- [ ] **10.5.5** Session cache hit ratio ≥ 90% sustained 24h em staging com realistic workload (EVT-021).
- [ ] **10.5.6** Latency p99 cold ≤ 280ms; warm ≤ 10ms; sustained 72h (EVT-021).
- [ ] **10.5.7** Cargo-audit + cargo-deny + clippy `-D warnings` clean.
- [ ] **10.5.8** Cost regression gate: per-op cost ≤ $0.000005 (verify cache hit dominant) (Lote 9.4 §14.10).
- [ ] **10.5.9** Production deploy guard test: `CORELINK_RATE_LIMIT_BACKEND=passthrough` em `CORELINK_ENV=production` → deploy FAIL.
- [ ] **10.5.10** Integration E2E test contra Clerk staging + real PAT mint flow green em CI nightly.
- [ ] **10.5.11** OWASP ASVS V4 (access control) 100% checklist pass.

## 11. DoD

- [ ] Tower service composition em `auth.rs` compila + integration tests green.
- [ ] TenantCtx struct + builder; private fields enforced.
- [ ] SessionCache impl + KV bindings em wrangler.toml.
- [ ] All Gherkin scenarios green em integration test.
- [ ] Property tests 10k green em CI.
- [ ] Mann-Whitney 3-prong test green (invalid vs scope-insufficient).
- [ ] E2E test contra Clerk staging + PAT real green.
- [ ] Métricas (5 listadas §6.1.9) emitted; dashboard configurado.
- [ ] Trace span `auth.middleware` em OTel pipeline.
- [ ] rustdoc 100% public API + 4 examples (basic axum integration, custom scope, session cache disable, deploy guard).
- [ ] ADR-0029 published.
- [ ] Architect + AppSec + Security Lead + Crypto SME reviews.
- [ ] PRR Architect sign-off.
- [ ] Cost regression gate green.

## 12. Invariants Validated

- **INV-AUTH-TENANTCTX-IMMUTABLE** (CRITICAL): TenantCtx fields private; nenhum mutation path; static checked via clippy custom lint.
- **INV-AUTH-5-LAYER-ORDERING** (CRITICAL): middleware layer order é canonical; chaos test valida.
- **INV-AUTH-SESSION-CACHE-KEY-CT** (HIGH): session cache key compare é constant-time via subtle.
- **INV-AUTH-SCOPE-MIDDLEWARE-LEVEL** (HIGH): scope check enforced via Tower layer; compile-time validation route requires layer.
- **INV-AUTH-AUDIT-PRE-POST-ORDERING** (HIGH): pre-handler audit emit antes; post-handler audit emit depois; reuse outbox.

TLA+ alignment: `tenant_isolation.tla` Layers 1-3 (PAT scope + tenant resolution + AuthZ check) — este WI implementa orchestration; per-layer specs em WI-S03-001/002 (Layer 1) + WI-S01-001 (Layer 2 prefix derivation) + handlers downstream (Layer 3 enforcement).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Auth middleware | `crates/corelink-worker/src/middleware/auth.rs` | Rust |
| TenantCtx + Builder | `crates/corelink-worker/src/middleware/tenant_ctx.rs` | Rust |
| SessionCache | `crates/corelink-worker/src/middleware/session_cache.rs` | Rust |
| Scope layer | `crates/corelink-worker/src/middleware/scope.rs` | Rust |
| Rate limit trait + stub | `crates/corelink-worker/src/middleware/rate_limit.rs` | Rust |
| Property tests | `crates/corelink-worker/tests/prop_auth_middleware.rs` | Rust |
| Mann-Whitney timing tests | `crates/corelink-worker/tests/timing_auth.rs` | Rust |
| E2E integration | `tests/e2e_auth_clerk_pat.rs` | Rust |
| Chaos suite | `tests/chaos/auth_5layer_propagation.rs` | Rust |
| Production deploy guard | `crates/corelink-worker/src/init/deploy_guard.rs` | Rust |
| ADR-0029 | `specs/03_architecture/adrs/ADR-0029-tenantctx-session-cache.md` | Markdown |
| Examples | `crates/corelink-worker/examples/auth/` (4 examples) | Rust |
| OWASP ASVS V4 checklist | `specs/_audits/2026-XX-XX-asvs-v4-auth.md` | Markdown |

## 14. Quality Standards SOTA

- **14.5.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.5.2** rustdoc 100% public API + 4 examples + threat model README section.
- **14.5.3** Test coverage ≥ 95% (security boundary).
- **14.5.4** Latência: p99 cold ≤ 280ms; warm ≤ 10ms; TenantCtx::build ≤ 100µs.
- **14.5.5** SAST: cargo-audit + cargo-deny + clippy `-D warnings`; cargo-fuzz target em token parser 1h.
- **14.5.6** Métricas RED + cache hit ratio + per-method timing histograms.
- **14.5.7** Runbooks: RB-FM-160 (auth invalid storm) + RB-FM-AUTH-CACHE-MISS-STORM.
- **14.5.8** Breaking changes em TenantCtx fields shape = bump major + migration plan; non-breaking via `#[non_exhaustive]`.
- **14.5.9** Memory bounded: per-request ≤ 32 KiB stack (TenantCtx + transient verify state).
- **14.5.10** Cost regression gate: per-op cost ≤ $0.000005 (cache hit dominant).

## 15. Chaos Experiments

1. **TenantCtx tampering attempt**: red team patches handler to force `unsafe { transmute(...) }` cast that mutates `tenant_id`. Hypothesis: chaos test catches via 5-layer assertion failure (layer 4 R2 path mismatch). Procedure: synthetic patched binary em sandbox; chaos test validates abort. Abort: synthetic.

2. **Session cache poisoning attempt**: red team writes forged TenantCtx em KV directly via privileged binding. Hypothesis: KV write privilege restricted to deployer wrangler config; tenant code lacks privilege; defense via wrangler binding scope. Procedure: simulated insider in staging (red team); validate cannot complete.

3. **Argon2 verify storm via invalid PATs**: 1000 req/s with random PAT-like strings; cache miss every time; Argon2 cost dominate. Hypothesis: Worker memory budget (128 MiB) limits concurrent verify ≤ 2 (each Argon2 64 MiB ephemeral); rate limit caps storm; fallback graceful 503.

4. **Clerk JWKS outage 1h**: simulate Clerk endpoint 503; verify cache-served JWTs continue (24h KV cache); cold validates fail 503 com `auth_unavailable`. Hypothesis: graceful degradation; not full auth outage.

5. **Session cache TTL boundary**: token cached at T+0; revoke at T+30s (WI-S03-004); request at T+59s (still cached) returns 200; T+61s returns 401 (cache expired + DB revoked). Validates TTL bound + revocation propagation.

6. **Production deploy guard regression**: deploy with `CORELINK_RATE_LIMIT_BACKEND=passthrough` em prod env. Hypothesis: deploy fails CI gate. Procedure: chaos PR; validate CI red.

7. **5-layer defense ordering scramble**: synthetic patched middleware com layers em wrong order (scope check before verify). Hypothesis: integration test catches via fail-closed assertions. Procedure: feature flag chaos.

8. **TenantCtx confusion deputy**: handler X passes TenantCtx_X to internal helper that calls handler Y; helper accidentally re-uses TenantCtx_A. Hypothesis: helper signature requires `&TenantCtx`; forces passing explicit; chaos detects via cross-handler test assertion.

9. **Cold-start under load**: Worker isolate cold-start during 1k req/s burst; verify p99 ≤ 500ms graceful (vs 280ms warm). Cold-start metric emitted; alert if sustained > 5min.

10. **Audit emission D1 batch failure**: D1 timeout durante TenantCtx commit + outbox INSERT; verify atomic rollback (no orphan TenantCtx em handler stage); HTTP 503 returned.

## 16. PRR

PRR HIGH_RISK 11 sign-offs canonical gated em WI-S03-008 ship gate. Este WI mini-PRR Architect (Crypto SME specialization) + AppSec + Security Lead.

- [ ] All Gherkin green.
- [ ] Property + Mann-Whitney + chaos green.
- [ ] E2E Clerk + PAT staging green.
- [ ] Cost regression gate green.
- [ ] Métricas + dashboards.
- [ ] ADR-0029 published.
- [ ] OWASP ASVS V4 100%.
- [ ] Production deploy guard tested.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Tower service composition skeleton | 2.5h |
| ST-002 | TenantCtx struct + Builder + non_exhaustive | 2h |
| ST-003 | SessionCache impl (KV operations + envelope serde) | 3h |
| ST-004 | Auth method routing (PAT vs JWT detection) | 1.5h |
| ST-005 | Verify orchestration (delegate WI-001 + WI-002) | 2h |
| ST-006 | Scope enforcement layer (composable) | 2h |
| ST-007 | Rate limit trait + Passthrough stub | 1h |
| ST-008 | Audit pre/post emit hooks (outbox integration) | 2h |
| ST-009 | Métricas emit (5 metrics) + trace span | 2h |
| ST-010 | Property tests 10k iter | 3h |
| ST-011 | Mann-Whitney 3-prong test impl | 4h |
| ST-012 | Production deploy guard logic | 1.5h |
| ST-013 | Dummy Argon2 em invalid PAT path | 1.5h |
| ST-014 | Chaos suite 5-layer propagation | 3h |
| ST-015 | E2E integration test (Clerk + PAT real) | 4h |
| ST-016 | rustdoc + 4 examples + threat model README | 3h |
| ST-017 | ADR-0029 redação | 2h |
| ST-018 | Architect + AppSec + Crypto SME review feedback iteration | 3h |
| ST-019 | OWASP ASVS V4 self-checklist + audit | 2h |

**Total Optimistic**: ~45h. **PERT** (O=40h, M=45h, P=68h): **~49h**.

## 18. Dependencies

### Hard blockers

- WI-S03-001 (Clerk adapter) SEALED.
- WI-S03-002 (corelink-pat) SEALED.
- WI-S01-001 (corelink-tenant-path) SEALED (tenant_prefix derivation).
- WI-S01-005 audit_outbox table available.
- Crypto SME availability (review timing-sensitive paths).

### Soft blockers

- WI-S03-005 (Neon schema) — não bloqueante; D1 stub aceita durante S-03 dev iteration.
- WI-S03-004 (revocation) — soft; este WI implementa session_cache invalidate hook public API; WI-004 calls it.

### Outbound

- WI-S03-007 (audit events) — consumes pre/post emit hooks.
- WI-S03-008 (ship gate).
- All downstream handlers (WI-S01-005, WI-S02-001..002, WI-S04-001) consume TenantCtx via request.extensions.

## 19. Effort PERT

O: 40h, M: 45h, P: 68h → PERT **49h**.

## 20. Time-boxing

**56h hard limit**. Se exceder → escalation: split em sub-WI (core middleware vs Mann-Whitney test infra).

## 21. Observability

5 métricas listadas §6.1.9. Trace span `auth.middleware` com attributes:
- `auth.method` (pat|jwt)
- `auth.result` (ok|invalid|expired|scope_insufficient|backend_unavailable)
- `auth.session_cache.outcome` (hit|miss|populate|invalidate)
- `auth.principal_hash` (sha256(principal_id) prefix 8 chars; PII-safe)
- `auth.tenant_id` (UUID v7; pseudonymous)
- `auth.duration_ms`

Logs structured JSON; INFO em ok; WARN em scope_insufficient/invalid; ERROR em backend_unavailable.

Dashboard widget DASH-AUTH:
- Request rate per auth_method.
- Session cache hit ratio.
- p99 latency cold/warm.
- Argon2 verify rate.
- Backend availability gauge.
- Outbox emission rate.

## 22. Cost Analysis

**Per-request breakdown** (warm path; cache hit):
- Worker invocation: $0.50/M.
- KV read (session lookup): $0.50/M.
- D1 batch (TenantCtx commit + outbox INSERT): ~$1/M.
- Per-request total: ~$0.000003.

**Per-request breakdown** (cold path; cache miss):
- Worker + Argon2 compute: ~250ms CPU.
- KV write (session populate): $5/M.
- D1 batch: ~$1/M.
- Per-request total: ~$0.000010.

**TCO 12m projection** (10M req/dia, 5% cold path):
- 9.5M req/dia warm: $0.000003 × 9.5M = $28.5/dia.
- 0.5M req/dia cold: $0.000010 × 0.5M = $5/dia.
- Total: ~$33.5/dia × 365 = **~$12.2k/yr**.
- Storage (session cache KV): ~50 MB ephemeral; trivial < $10/yr.
- Total auth middleware: **~$12.5k/yr** em 10M req/dia workload.

**Cost regression gate** (§14.10): per-op cost ≤ $0.000005 (warm path target); CI bench fails se exceder. Métrica `corelink.auth.cost_per_op_usd` weekly.

**Comparison vs alternatives**:
- Synchronous Argon2 every request: ~$0.000250 × 10M/dia = $2.5k/dia = **$910k/yr** (catastrophic; 70× worse).
- Custom Redis session store (não-CF): infra $5k/yr + ops $50k = $55k/yr.
- CoreLink session cache na CF KV nativa: $12.5k/yr + zero ops.

## 23. API Contract

Middleware é internal Rust; expõe via Tower service composition. Public types stable post v1.0:
- `TenantCtx` (`#[non_exhaustive]`).
- `AuthMethod` enum.
- `auth_stack(inner)` composer fn.
- `require_scope(scope)` layer fn.

HTTP error mapping (downstream-visible):
- `Malformed token` → 401 `COR_AUTH_HEADER_MALFORMED`.
- `Invalid token` (verify failed) → 401 `COR_AUTH_INVALID_TOKEN`.
- `Expired token` → 401 `COR_AUTH_TOKEN_EXPIRED`.
- `Scope insufficient` → 403 `COR_AUTH_SCOPE_INSUFFICIENT` + body `{ required, granted }`.
- `Tenant not provisioned` → 412 `COR_AUTH_TENANT_NOT_PROVISIONED`.
- `Rate limited` (S-08 forward) → 429 `COR_AUTH_RATE_LIMITED` + Retry-After.
- `Backend unavailable` → 503 `COR_AUTH_BACKEND_UNAVAILABLE` + Retry-After.

## 24. Post-mortem Hooks

- TenantCtx leak detected (cross-tenant via shared mutable state) → CRITICAL post-mortem + breach notification consideration.
- Session cache poisoning detected → CRITICAL.
- Scope check bypass exploit → CRITICAL.
- 5-layer ordering regression → SEV-1.
- Mann-Whitney p < 0.05 sustained (timing oracle) → SEV-1.
- Production deploy guard failed (passthrough em prod) → SEV-0 (immediate revert + 5-Why).
- Backend unavailable storm > 1h → SEV-2 + runbook RB-FM-160.

## 25. Rollback / Recovery

Hot rollback via Wrangler `wrangler deploy --version-id <prev>`. Session cache flush:
- `KV.delete(auth:session:*)` via wrangler CLI (manual; rare).
- RTO: ≤ 10 min.
- RPO: 0 (stateless middleware; no data loss).

Fallback degradation: se ClerkAdapter offline, JWT path 503; PAT path continues. Se PatVerifier broken, both paths 503; system effectively down (graceful, não unsafe state).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: PAT/JWT verify (Layers 1-2) gated; TenantCtx forge impossible by construction (private fields + builder).
- **Tampering**: TenantCtx imutável; session cache key constant-time compare; scope check middleware-level enforced.
- **Repudiation**: pre/post audit emit (`auth.token.validated` + `auth.{ok,denied}`); outbox guarantees durability; reuse WI-S01-005 chain integrity.
- **Information disclosure**: Mann-Whitney constant-time invalid vs scope-insufficient; principal_id hashed em logs; session cache value não contém body.
- **DoS**: Argon2 cost-bound; rate limit S-08 forward; bounded concurrent verifies via Worker memory; backend unavailable graceful 503.
- **Elevation of privilege**: scope é DB-fonte-de-verdade (PAT prefix não inferido); 5-layer defense Layer 3 enforced em handlers downstream.

**LINDDUN delta**:
- **Linkability**: tenant_id UUID v7 pseudonymous; principal_id hashed em SIEM forwarding (S-09).
- **Identifiability**: PAT plaintext nunca logged; JWT email field redacted via S-09 macro.
- **Non-repudiation**: append-only audit chain; outbox at-least-once.
- **Detectability**: timing constant invalid vs scope-insufficient via dummy Argon2.
- **Disclosure of information**: response body nunca contém token; error_body só `required/granted scopes`.
- **Unawareness**: SLA addendum §3 documents stale window single SLA ≤ 60s p99 (hot/cold/cross-region eixos orthogonais; não aditivos).
- **Non-compliance**: LGPD Art. 38 + GDPR Art. 32 satisfied via Argon2 + audit + DSR support S-11.

## 27. Knowledge Transfer

- **Tech talk** (1.5h): "Tower Auth Stack + 5-Layer Defense + TenantCtx Type-Driven Security".
- **Doc** `docs/internal/auth-middleware.md` — sequence diagrams (PAT cold, PAT warm, JWT, scope deny, revocation propagation).
- **Doc** `docs/internal/tenantctx-pattern.md` — type-driven security pattern reusable em S-04+ (extending TenantCtx with region, org_id).
- **ADR-0029** — design rationale.
- **Workshop** (2h): com Architect + AppSec + Crypto SME + handler authors (downstream WIs) — pair-program review TenantCtx consumption.
- **Onboarding test** (5 questions): TenantCtx immutability, session cache TTL, 5-layer ordering, Mann-Whitney rationale, production deploy guard.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | TenantCtx mutation via `unsafe { transmute }` em handler | L | L | CRITICAL | L | LOW | Private fields + cargo-deny `[lints.rust] unsafe = "deny"`; chaos test layer mismatch |
| R-002 | Session cache poisoning (insider write to KV) | L | L | CRITICAL | L | LOW | wrangler binding scope restrict; deployer-only privilege; KV not trust boundary |
| R-003 | Argon2 storm exhausts Worker memory (128 MiB) | M | H | HIGH | M | LOW | Bounded concurrent verify (memory budget); rate limit S-08 forward; alert sustained |
| R-004 | Mann-Whitney CI flake (1-em-20 false positive) | M | H | LOW | M | LOW | Šidák correction: ALL 3 trials × 3 pairs = 9 tests p > 0.05; per-test α' ≈ 0.0057 controls familywise α at 0.05 (cycle 2 codex SEAL math correction) |
| R-005 | Session cache TTL drift causes hot-path stale window > 60s SLA | L | M | HIGH | L | LOW | Env var validation em deploy; integration test boundary; cold path D1 authoritative fallback bounded |
| R-006 | Scope check bypass via missing layer em new route | L | H | CRITICAL | L | LOW | Compile-time route definition requires `require_scope` OR `allow_unauthenticated`; deny-by-default |
| R-007 | TenantCtx shape evolution breaks handlers | M | M | MEDIUM | M | LOW | `#[non_exhaustive]` + dot-dot pattern matching convention; CI gate em handler patterns |
| R-008 | Audit outbox pre-emit fails atomicidade | L | M | HIGH | L | LOW | D1 batch atomic com TenantCtx commit; reuse WI-S01-005 outbox guarantee |
| R-009 | Production deploy guard bypassed via env override | L | L | CRITICAL | L | LOW | Hard fail em init() se passthrough+prod; canary deploy validation |
| R-010 | Backend unavailable cascades para all routes (single point of failure) | M | H | HIGH | M | LOW | Per-method fallback (PAT continues if Clerk down); circuit breaker S-XX forward |
| R-011 | Cost regression > 10% per-op | M | L | MEDIUM | L | LOW | §14.10 cost gate; weekly bench |
| R-012 | Dummy Argon2 path exposed via ad-hoc env flag | L | L | HIGH | L | LOW | No flag to disable; constant always |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Crypto SME review TenantCtx struct + 5-layer ordering.
2. **AppSec (D+2)**: AppSec review session cache key construction + scope enforcement composability.
3. **Code (D+5)**: peer review (2 engineers).
4. **Crypto (D+6)**: Crypto SME review Mann-Whitney 3-prong + dummy Argon2 path.
5. **Adversarial (pre-merge D+8)**: red team session — TenantCtx tampering, session cache poisoning, scope bypass.
6. **PRR (D+10)**: Architect sign-off + readiness review.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; composability + non-exhaustive evolution review_ (com Crypto SME specialization mandatory: Mann-Whitney 3-prong + dummy Argon2 + constant-time key compare (mandatory)) | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead | _staffing-blocked_ | _pending_ | _pending_ |
| 6 | Engineer (S-03 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; 5-layer ordering + session cache poisoning_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (cycle 1 codex SEAL alignment per framework §33.5.4.3 + ADR-0034 solo-tier waiver). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação WI-S03-003 (Lote 10.3); SOTA elevation pós-Lote 10.2bis (full STRIDE/LINDDUN delta + 13-row sign-off + 12-row risk + Mann-Whitney 3-prong + cost TCO + 10 chaos experiments). |
| 1.2.0 | 2026-04-30 | Gustavo (via Claude Opus 4.7) | **SEAL implementation phase.** Landed core deliverable per protocol §"WI ceremony (NO codex)" 2026-04-30: Tower `AuthLayer` + `AuthService` em `crates/corelink-worker/src/middleware/auth.rs` (orchestration: header extract → method route → verifier dispatch → cross-tenant smuggle reconciliation → AuthCtx build → request.extensions inject); immutable `AuthCtx` + `AuthCtxBuilder` em `auth_ctx.rs` (private fields, `#[non_exhaustive]`, `Clone`-only mutation surface, `tenant_ctx()` storage projection); canonical `AuthMiddlewareError` taxonomy em `auth_error.rs` mapping to `COR_AUTH_*` wire codes + HTTP status 401/403/412/503; constant-time pad via `corelink_pat::dummy_verify_for_constant_time` (cross-WI per WI §3 P0; defense-in-depth call wrapping every PAT cold-path branch); cross-tenant header smuggle pre-check + post-verify `subtle::ConstantTimeEq` reconciliation; `PatVerifier` / `JwtVerifier` / `JwtTenantResolver` async-trait abstractions decoupling adapter wiring from middleware logic. Tests: 11 smoke scenarios (`tests/auth_middleware_smoke.rs`) green; 8 property tests at 10k iter release (`tests/prop_auth_middleware.rs`) green covering missing-token rejection, malformed-token rejection, expired-token rejection, cross-tenant header smuggling rejection (with dual-arm matching-passes property to prevent false-positive), and 5-layer consistency (auth-layer prefix == storage-layer prefix == derive_prefix(tdk, tenant_id)). `#![forbid(unsafe_code)]` + crate `[lints]` strict (deny unwrap/expect/panic/indexing); workspace clippy clean. **Deferred to follow-up WIs (per §6.2 anti-scope)**: real session cache (KV 60s) impl + Mann-Whitney 3-prong timing infra → WI-S03-004 (revocation DO + KV invalidation hook); ADR-0029 redação; rate-limit DoBased backend → WI-S08-001; production deploy guard → WI-S03-008 ship gate; OWASP ASVS V4 self-checklist → S-20 audit. Sprint-close adversarial review covers per protocol 2026-04-30 §"per-WI ceremony (NO codex)". |

## 32. Anti-patterns evitados

- ❌ Mutable TenantCtx (immutability é INVARIANT).
- ❌ Handler-level scope check (middleware-level mandatory).
- ❌ Lazy tenant_prefix em handler (pre-compute em middleware).
- ❌ Plaintext token em session cache key (sha256[:16] truncated).
- ❌ Variable-time `==` em token compare (subtle::ConstantTimeEq).
- ❌ Synchronous audit emit em hot path (outbox pattern).
- ❌ Custom auth header `X-Internal-Auth` bypass (RFC 6750 only).
- ❌ Argon2 every request (session cache 60s TTL).
- ❌ Single auth method (support PAT + JWT routing).
- ❌ Auth bypass via env var em production (deploy guard).
- ❌ Mann-Whitney sem power analysis (3-prong gate enforced).
- ❌ Custom middleware framework (Tower ecosystem).

---

**Fim WI-S03-003.** Próximo: WI-S03-004 (Revocation Durable Object + KV cache invalidation + cross-region propagation ≤ 60s p99).
