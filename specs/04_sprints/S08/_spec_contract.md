---
id: "SPEC-CONTRACT-S08"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.5.0"
created: "2026-04-24"
updated: "2026-05-02"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s08", "rate-limit", "quotas", "abuse-detection", "bulkhead", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-08: Rate Limiting Multi-Camada + Quotas + Abuse Detection (SOTA)

> **SOTA framing:** concorrentes (NativeLink, BuildBuddy) têm rate limit per-tenant baseline (token bucket). SOTA step: **4 camadas** (global → per-tenant → per-IP → per-PAT) + abuse detection heurístico multi-feature + `Retry-After` semântico distinto (within-quota vs over-quota, referido em `slo_catalog §4.2`). Isso distingue "bug nosso" (within_quota 429 ⇒ SLI failure) de "comportamento correto" (over-quota).

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-08 |
| Nome | Rate Limiting Multi-Camada + Quotas + Abuse Detection |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (CTRL-RATE-001 + CTRL-QUOTA-001 são controles de segurança formal; bypass = AVAIL isolation violation cross-tenant); FF-HR-002 (cross-tenant SLO degradation se rate limit deficit) |
| Duração estimada | 2.5 semanas (60–80h efetivas + 5d buffer) |
| WIs antecipados | 6 |

## 1. Objetivo

Implementar o **bulkhead principal do CoreLink multi-tenant**: isolamento comportamental. Rate limit 4-camadas + quota hard-limit + abuse heurístico garantem que um tenant abusivo, IP adversário, ou PAT comprometido não consomem recursos alheios nem degradam SLO dos demais (`INV-AVAIL-ISOLATION`). Foundation para SLA-backed enterprise contracts em S-14 e pricing real em S-10.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (11 sign-offs canonical per framework §33.5.4.3 + ADR-0034).
- **FF-HR-005**: CTRL-RATE-001 + CTRL-QUOTA-001 são controles formais de segurança; bypass via deficit = AVAIL-ISOLATION violation (INV-AVAIL-ISOLATION cross-tenant SLO degradation).
- **FF-HR-002**: per-tenant bulkhead falho permite cross-tenant degradation; equivalência à tenant isolation embora mitigável via degrade_mode.
- **Justificativa upgrade STANDARD → HIGH_RISK**: codex audit findings 2026-04-24 indica que CTRL-RATE-001 + CTRL-QUOTA-001 são security controls que ativam FF-HR-005 lane forcing factor.

## 3. Inherits_from

```yaml
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "SECURITY-MODEL"      # CTRL-RATE-001, CTRL-QUOTA-001, CTRL-AUTH-007 (replay nonce)
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"       # FM-250, FM-251, FM-255, FM-401 (thundering herd)
  - "SLO-CATALOG"         # SLI-AVAIL-CAS-GET precisa distinguir within vs over quota
  - "AUTH-MODEL"          # PAT-level rate limit depends on scope
  - "INVARIANT-REGISTRY"  # INV-RATE-LIMIT-PROPORTIONALITY, INV-AVAIL-ISOLATION, INV-QUOTA-ENFORCEMENT (Lote 9.4)
```

## 4. CAPs entregues

- **CAP-RATE-001**: Per-tenant token bucket stateful (DO `RateLimiter-<tenant_id>`).
- **CAP-RATE-002**: Per-IP rate limit edge CF (adversarial IP floods).
- **CAP-RATE-003**: Per-PAT rate limit (hijacked credential containment).
- **CAP-RATE-004**: Global rate limit circuit breaker (system-wide DoS mitigation).
- **CAP-QUOTA-001**: **Storage hard-block (≥ 100% — boundary com S-07 / ADR-0020)** — quando `tenant.bytes_used ≥ tenant.storage_limit`, write retorna 429 com `X-Rate-Limit-Type: over_quota` + `Retry-After: days-until-month-reset`. Soft (80%) e alert (95%) **transferidos para S-07 CAP-EVICT-003** (eviction-driven soft-pressure). S-08 entra em ação quando S-07 eviction não deu conta de manter `bytes_used < limit`.
- **CAP-QUOTA-002**: Bytes ingress/egress quota (monthly rolling) pra bandwidth tier.
- **CAP-ABUSE-001**: Abuse detection heurística multi-feature (`corelink_abuse_score{tenant_id}`).
- **CAP-ABUSE-002**: Automated response: downgrade silencioso, admin review trigger, extreme case suspend.

## 5. Requirements específicos

- **R-S08-1**: DO `RateLimiter-<tenant_id>` com token bucket (refill rate = plan × multiplier); state persistente; sub-ms latency via DO sticky placement.
- **R-S08-2**: CF edge per-IP rate rules via Workers API ou CF Ruleset Engine (não-atrelado a tenant; IP blocklist 429 silencioso pra CIDR abusivos).
- **R-S08-3 (Lote 10.8bis P0-C corrected)**: PAT-scoped misuse DETECTION (NOT enforcement): `corelink_quota_pat_misuse_detected_total{pat_id}` SEV-2 alert when per-PAT observed rate > 10× tenant_refill (compromised credential signal). **Aggregate per-tenant rate enforcement em camada 1** (R-S08-1 DO RateLimiter); per-PAT camada 3 é observability-on-PATs apenas. Original "cap" framing rejected porque single PAT cannot exceed tenant aggregate (camada 1 already 429s).
- **R-S08-4**: Global circuit breaker DO `GlobalRateLimiter`: trip em error_rate > 50% sustained 5 min; `degrade_mode=emergency` fallback.
- **R-S08-5 (Lote 10.7bis P0-2 + Lote 10.8bis P1-6 corrected)**: Quota checker middleware: pre-write check atomic via DO actor (NOT D1 SQLite atomic CAS — D1 sem native CAS); canonical `tenant_storage_state.bytes_used` vs `tenant_quota.max_storage_bytes`. Original phantom column `tenant_quota.used_bytes` rejected (Lote 10.7bis P0-2: column doesn't exist in data_model.md; canonical é separate `tenant_storage_state` table for STATE vs `tenant_quota` for POLICY).
- **R-S08-6**: Monthly bandwidth quota: counter aggregated em DO `BandwidthTracker-<tenant>-<YYYY-MM>`; reset monthly.
- **R-S08-7**: Abuse score calculator: features = `{cpu_wallclock_ratio, egress_bytes_per_min, action_digest_entropy, concurrent_exec_count}`; score via weighted sum; threshold calibrado com real workload em staging.
- **R-S08-8**: Response codes tipados:
  - `429 + X-Rate-Limit-Type: tenant_quota` (within plan; SLI failure — bug nosso)
  - `429 + X-Rate-Limit-Type: per_ip` (edge; IP abusivo)
  - `429 + X-Rate-Limit-Type: per_pat` (PAT misuse)
  - `429 + X-Rate-Limit-Type: over_quota` (tenant excedeu plan; SLI pass — legítimo)
  - `429 + X-Rate-Limit-Type: global_circuit_open` (emergency)
- **R-S08-9**: `Retry-After` sempre presente com seconds-until-refill realistic.

## 6. Definition of Done

Universal (`_sprint_creation_contract §7`) **+**:

- [ ] 6 WIs SEALED.
- [ ] **Chaos test isolation**: 1 tenant flood 10k QPS sustentado → vizinhos mantêm SLO-AVAIL-CAS-GET (EVT-023).
- [ ] **429 distinction implemented**: `rate_limited_within_quota` vs `rate_limited_over_quota` emitido separadamente; SLI-AVAIL-CAS-GET usa apenas within-quota no numerador (EVT-002).
- [ ] **Load test bandwidth quota**: tenant consome 100% de bandwidth monthly → writes rejected with `over_quota` 429 + Retry-After = days-until-month-reset.
- [ ] **Abuse score calibration (Lote 10.8bis P0-E corrected)**: n=50 benign + n=50 high-intensity abusive workloads → 95% CI FP rate ≤ 5% upper-bound (≤ 2 FP em 50 acceptable; target ideal 0 FP) + 95% CI TP rate ≥ 80% lower-bound (≥ 40 TP em 50; target ≥ 45 TP) (EVT-004 benchmark; statistical methodology validated by Data Scientist advisor).
- [ ] **RB-FM-250** (DDoS volumetric) dry-run (EVT-017).
- [ ] **Alerts armados**: per-tenant quota 95% (SEV-3 per-tenant), global circuit breaker trip (SEV-1 oncall), abuse score threshold (SEV-2).
- [ ] **Rate limit headers RFC 9331** (RateLimit, RateLimit-Policy) implementados — padrão IETF atualizado.
- [ ] **Property test** 10k iter cobrindo race conditions em DO token bucket update (EVT-002).
- [ ] **Coverage ≥ 90%**.
- [ ] **PRR HIGH_RISK 11 sign-offs canonical** (per framework §33.5.4.3 + ADR-0034): Owner + Final Approver + Architect (Crypto SME specialization for race correctness atomic CAS) + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance + Privacy + AppSec advisor (peer reviewers contribuem em PR review sem sign-off canonical separado) (EVT-031).

## 7. Completeness Criteria SOTA (delta local)

- [ ] **10.s08.1 SLI correctness**: rate limit response NÃO contabiliza no denominador do SLI quando é legítimo (over_quota); contabiliza quando within_quota (corretude crítica pra error budget).
- [ ] **10.s08.2 Rate limit overhead**: middleware adiciona p99 ≤ 3ms (criterion benchmark).
- [ ] **10.s08.3 Abuse response humane (Lote 9.4 Opus H-06 LGPD Art. 20 alignment)**: nenhum tenant é suspended sem human-in-the-loop. Automated downgrade/throttle DEVE expor: (a) endpoint `GET /v1/admin/abuse_score` para customer self-service inspection do score atual + features que contribuíram; (b) endpoint `POST /v1/admin/abuse_appeal` com payload de explanation + timeline → roteia para human reviewer ≤ 24h; (c) audit log emit para cada decisão automatizada (CTRL-AUDIT-003 alignment). Compliance: LGPD Art. 20 (revisão de decisões automatizadas) + GDPR Art. 22.
- [ ] **10.s08.4 Per-tenant isolation formal**: property test proves 1 tenant's rate limit state não afeta outros.
- [ ] **10.s08.5 Monthly reset determinístico**: bandwidth quota reset em 1º dia UTC do mês seguinte; não em sliding window.

## 8. Invariants

**Mantidas:**
- **INV-AVAIL-ISOLATION** (HIGH, `invariant_registry §3.8`): tenant DoS não afeta outros (bulkhead enforcement).
- **INV-QUOTA-ENFORCEMENT** (HIGH, `invariant_registry §3.11`): quota real-time check atomic.
- **INV-TENANT-ISOLATION** (CRITICAL): rate limit state per-tenant; nunca cross-contaminate.

**Nova deste sprint:**
- **INV-RATE-LIMIT-PROPORTIONALITY** (novo HIGH): refill_rate × window sempre consistente com tenant plan; mudança de plan reflete em ≤ 5 min via DO config sync.

## 9. Quality Standards SOTA (delta local)

- **14.s08.1 Rate limit p99 overhead ≤ 3ms** (criterion) — principal hot path.
- **14.s08.2 Quota check atomic**: DO actor model guarantees; property test proof.
- **14.s08.3 Zero false negative em abuse**: adversarial workloads simulated; recall measurable.
- **14.s08.4 Retry-After reflete realidade**: 2 clients com mesmo Retry-After realmente ambos voltam a passar no próximo tick.
- **14.s08.5 RFC 9331 compliance**: headers padrão IETF (não custom X-*).
- **14.s08.6 Observability full**: cada 429 emitir log `{tenant_id, type, current, limit, remaining, refill_eta_ms}`.
- **14.s08.7 Cost regression gate** (Lote 9.5b — meta §14.10): rate limit middleware $USD/million ops baseline; PR > 10% cost regression bloqueia merge sem ADR. DO storage growth + KV ops cost projection per tenant tier.

## 10. Anti-scope

- ❌ **Automated tenant suspend** sem human-in-the-loop. Abuse score → admin review; não auto-kill.
- ❌ **ML-based abuse detection**. Heurística simples primeiro; ML só se baseline insuficiente.
- ❌ **Priority queuing** (diferentes latencias por tier em congestion). Future sprint S-14+ se enterprise demand.
- ❌ **Cost-based rate limit** (ex: limite de $$ per hour). Fora de escopo S-08; cobrança efetiva em S-10.
- ❌ **Regional rate limit federation** (sync cross-region). Per-region independent em S-08; S-14 trata edge case.

## 11. Dependencies

- **Blocker (hard):** S-03 SEALED — auth context (tenant_id, pat_id).
- **Blocker (hard):** S-09 em paralelo OU SEALED primeiro — observability pra métricas rate/abuse.
- **Soft:** S-13 admin plane para config tuning (interim via env vars).
- **Bloqueia:** S-10 billing (quota enforcement é dependência de pricing correctness); S-14 enterprise tier (custom rate limits).

## 12. WIs antecipados (PERT)

| ID | Título | PERT (h) |
|---|---|---|
| WI-S08-001 | DO RateLimiter token bucket + state machine | 20 |
| WI-S08-002 | CF edge per-IP rules + blocklist CIDR | 12 |
| WI-S08-003 | Quota checker middleware (atomic D1 CAS) | 16 |
| WI-S08-004 | Abuse detection heurística + scoring | 20 |
| WI-S08-005 | Response code types + RFC 9331 headers | 8 |
| WI-S08-006 | DASH-RATE dashboard + alerts + SLI distinction | 12 |

Total: ~88h. Buffer: 3 dias úteis.

## 13. Duração + Timeline

- **Start:** após S-03 + S-09 SEALED. Assume 2026-08-31 (Mon).
- **Mid-check:** 2026-09-04 (Fri).
- **Target complete:** 2026-09-11 (Fri).

## 14. Critérios de promoção

- DoD complete.
- Chaos isolation test clean.
- 429 SLI distinction validated.
- Abuse scoring calibrated (measurable recall/precision).

## 15. Riscos (Prob/Det/Impacto/Exposure/Residual)

| ID | Risco | Prob | Det | Imp | Exposure | Mitigação | Residual |
|---|---|---|---|---|---|---|---|
| R-S08-001 | DO quota exceeded (FM-059) em tenant muito ativo | M | H | MEDIUM | Tenant blocked | DO sharding plan (pós-GA); monitor storage usage | MEDIUM |
| R-S08-002 | Rate limit too aggressive → legitimate clients 429 | M | M | MEDIUM | Customer trust | Tunable via admin (S-13); 2-week bake | LOW |
| R-S08-003 | Abuse heurística FP em legítimo ML workload | H | H | LOW | Automated downgrade wrong | Human-in-loop obrigatório pra suspend | LOW |
| R-S08-004 | Global circuit breaker trip false-positive → full outage | L | M | CRITICAL | All tenants down | Multi-signal trigger (não single metric); manual override | MEDIUM |
| R-S08-005 | Monthly reset race condition (end-of-month edge) | L | L | MEDIUM | Cliente 429 no 1º do mês | Atomic reset via DO transaction; integration test | LOW |
| R-S08-006 | Edge IP rate limit afeta NAT legitimate (multi-user atrás de IP) | M | M | MEDIUM | Customer complaints | Warn mode primeiro; opt-in hard-block | LOW |

## 16. Benchmarks SOTA externos

- **AWS API Gateway** throttling: 10k RPS default + burst — CoreLink targets comparable.
- **Stripe API** rate limit pattern: **livemode** 25 RPS default + **testmode** 100 RPS — inspiração pro Retry-After semantics.
- **GitHub REST API v3**: 5k/hour authenticated, 60/hour unauthenticated — structure análoga a PAT scoped.

## 17. References

- **RFC 9331**: RateLimit, RateLimit-Policy headers (IETF draft stable).
- **RFC 6585**: HTTP 429 Too Many Requests.
- **Stripe Engineering Blog**: "Scaling Stripe's API: rate limits in practice" — design pattern reference.
- **Cloudflare Rate Limiting** docs — CF Ruleset Engine reference.
- **Netflix OSS Hystrix** (legacy) — circuit breaker conceptual (Resilience4j Rust equivalent emerging).
- **Google SRE Workbook** Ch. 7 "Load Shedding" — degrade pattern.

## 18. Post-mortem hooks

Trigger post_mortem se:

- Global circuit breaker tripped falsamente (nenhum real DoS, só rate spike normal).
- Tenant reportar 429 persistente estando under quota (FM-251 bug ou CTRL-RATE-001 regression).
- Abuse score flagga tenant enterprise reputable → customer loss.

## 19. Waiver policy

Nenhum waiver previsto. Rate limit / quota são enforcement primitives; waiver aqui é perigoso. Exceção custom: enterprise tier com contrato specific requer ADR + CAP-RATE-004.

## 20. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.2.0 | 2026-04-24 | Gustavo (initial design) | Initial spec contract S-08 (Rate Limiting Multi-Camada + Quotas + Abuse Detection; HIGH_RISK lane; FF-HR-005 + FF-HR-002). |
| 1.3.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S08-001 SEAL) | **WI-S08-001 SEALED — `crates/corelink-ratelimit/` v0.1.0 shipped + `migrations/d1/0010_ratelimit_buckets.sql` (NEW table; durable mirror of DO singleton in-memory token-bucket state machine).** New crate ships 8 source modules (~1850 LOC + ~660 LOC tests): `key` (KeyDimension `#[non_exhaustive]` 3-canonical literal `per_tenant`/`per_ip`/`per_tenant_per_endpoint` + BucketKey composite tenant-leftmost), `tier` (5-tier refill ladder canonical free/solo/team/business/enterprise per Lote 10.7bis P0-7; refill_rate_for_tier resolver), `bucket` (TokenBucketState f64-precision + `try_acquire` lazy-refill canonical formula + monotonic clock clamp + Lote 10.8bis P1-1 division-by-zero guard for canceled tenants 7-day saturation), `audit` (3-event taxonomy `corelink.ratelimit.{allowed,denied_429,bucket_refilled}` fail-closed envelope per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`), `metrics` (7-canonical metric ladder: check_total + tokens_remaining + refill_rate + plan_sync_lag_ms + do_cold_start_total + middleware_duration_us + cross_tenant_violation_total SEV-1), `error` (`#[non_exhaustive]` taxonomy with explicit TenantMismatch + CostExceedsCapacity arms), `config` (RFC 6585 §4 1s floor + 86400s live-tenant ceiling + 7d canceled-tenant retry; team-tier defaults), `limiter` (RateLimiter trait + InMemoryTokenBucketRateLimiter orchestrator with per-instance `Arc<Mutex<HashMap<BucketKey, TokenBucketState>>>` mirrors DO actor model serialisation; F-001 closure preserved). Migration `0010_ratelimit_buckets.sql`: composite PK `(tenant_id, key_dimension, scope_key)` tenant-leftmost; 8 inline CHECK constraints; idempotent + additive; 2 secondary indices. Tests: 73 inline lib unit + 18 prop_ratelimit @ 10k iter (100k via PROPTEST_CASES override) + 10 migration canonical = **101 tests across all targets, 0 failures, parallel-safe**. Property tests pin INV-AVAIL-ISOLATION (`prop_tenant_isolation` + `prop_concurrent_acquire_does_not_double_spend` — DO actor serialisation), INV-RATE-LIMIT-PROPORTIONALITY (`prop_token_bucket_proportionality` + `prop_token_bucket_never_exceeds_capacity`), token non-negativity, refill monotonicity in time, RFC 6585 §4 Retry-After lower bound, idempotent zero-cost acquire, audit emit per decision arm, monotonic clock clamp safety, < 3ms p99 hot path probe. **Trait-abstraction-defer per charter**: real CF DO singleton + real D1 atomic batch + real Tower layer (gated `corelink-worker/tower-middleware`) + 100k nightly + chaos 12 + RB-FM-401 dry-run — all consolidated alongside WI-S08-006 (DASH-RATE + alerts + PRR ship gate). Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-ratelimit` member + workspace dep + reuses `corelink-eviction::Tier`. Quality gates verde. No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-08 corpus. |

---

| 1.5.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M; autonomous WI-S08-003 SEAL) | **WI-S08-003 SEALED — `crates/corelink-quota-cas/` v0.1.0 shipped + `migrations/d1/0012_quota_cas_attempts.sql` (NEW table; durable audit ring of every CAS attempt — successful / denied / race-detected — for SEV-3 race-detection observability + admin-plane forensics).** New crate ships the canonical 100% hard-block path (CAP-QUOTA-001) per ADR-0020 FROZEN, superseding the S-07 PROVISIONAL transitional 429 + Retry-After arm: 7 source modules (`retry_after` Howard Hinnant Gregorian primitives — wasm32-clean no `chrono` dep; `state` AtomicCasState trait + InMemoryAtomicCasState with monotone `cas_version` + race-aware strict-< boundary; per-instance `Mutex` envelope mirrors DO actor model byte-for-byte; F-001 closure preserved; `cas` AtomicQuotaChecker trait + InMemoryAtomicQuotaChecker bounded CAS retry loop canonical 3 attempts + audit-emit-BEFORE-write fail-closed mirroring S-07 sprint-close P1-1 fix; QuotaCasDecision `#[non_exhaustive]` Allow / Deny429 carrying canonical `retry_after_secs: days-until-month-reset`; idempotent zero-byte read-path; `audit` canonical 6-event taxonomy `corelink.quota.cas_{check_passed, denied_429_hard_block, race_detected, commit_succeeded, release_idempotent, retry_after_emitted}` `#[non_exhaustive]`; `metrics` 5-canonical metric ladder; `error` `#[non_exhaustive]` taxonomy; `config` QuotaCasConfig hard_block_pct 1.0 + max_cas_attempts 3 with defensive clamps). Migration `0012_quota_cas_attempts.sql`: PK `attempt_id` UUIDv7 surrogate; 12 inline CHECK constraints + 3 indices including SEV-1 partial index on `'CasDenied429HardBlock'` + SEV-3 partial index on `'CasRaceDetected'`; idempotent + additive; canonical schema version 12. Tests: 72 inline lib unit + 16 migration_canonical_0012 + 12 prop_quota_cas @ 10k iter (100k via PROPTEST_CASES override) = **100 tests across all targets, 0 failures**. Property tests pin INV-QUOTA-ENFORCEMENT race-aware strict-< predicate (`prop_cas_no_double_spend`), CAS race retry happy path (`prop_cas_race_detected_retry_succeeds` — OneShotRaceState wrapper + bounded retry succeeds on attempt 2), canonical Retry-After bounds (`prop_retry_after_days_until_month_reset` — always within `[1, 31×86_400]`), tenant isolation, audit-emit-per-decision-arm, idempotent zero-byte, saturating-arithmetic safety, informational SLO probe ≤ 50ms wall-clock. Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-quota-cas` member + workspace dep; reuses `corelink-eviction::EvictionRegion`; composes with `corelink-quota` (S-07 PROVISIONAL stays intact for transitional window per ADR-0020 FROZEN). **Trait-abstraction-defer per charter**: real CF DO singleton + real D1 atomic batch + Tower-layer wiring (gated `corelink-worker/tower-middleware`) + 100k nightly + chaos 12 + crypto SME advisory race correctness — all consolidated alongside WI-S08-006 PRR ship gate. Quality gates verde: `cargo test -p corelink-quota-cas --all-targets` 100 tests 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (284 docs); `check_migrations_additive.py` clean (12 migrations). No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-08 corpus. |

---

| 1.4.0 | 2026-05-02 | Gustavo (via Claude Opus 4.7 1M; orchestrator-finalized after agent rate-limit) | **WI-S08-002 SEALED — `crates/corelink-edge/` v0.1.0 shipped + `migrations/d1/0011_edge_blocklist.sql` (NEW table; durable source-of-truth for CF Edge blocklist with CF List replica deferred to WI-S08-006).** New crate ships 7 source modules (~3554 LOC + tests): `cidr` (CIDRv4/v6 prefix + longest-prefix-match hand-rolled bit-level matcher; wasm32-clean), `policy` (EdgePolicy trait + InMemoryEdgePolicy orchestrator + CidrBlocklist trait + InMemoryCidrBlocklist with per-tenant trie-equivalent storage; per-instance `Arc<Mutex<>>` F-001 closure), `audit` (5-event taxonomy `corelink.edge.{allowed, denied_blocklist, denied_abuse, blocklist_added, blocklist_removed}` + fail-closed envelope), `metrics` (canonical metric ladder for edge decisions + blocklist mutations), `error` (`#[non_exhaustive]` taxonomy), `config` (default-action knob + max blocklist size), `lib` (public re-exports + `MIGRATION_0011_EDGE_BLOCKLIST` const + `edge_schema_version() = 11`). EdgeDecision `#[non_exhaustive]` 3-arm enum (Allow / DenyBlocklisted / DenyAbuse). Migration `0011_edge_blocklist.sql`: composite PK + idempotent + additive. Tests: 68 inline lib unit + 11 prop_edge @ 10k iter + 17 migration canonical = **96 tests across all targets, 0 failures, parallel-safe**. Property tests pin longest-prefix-match correctness, IPv6 support, idempotent add, remove round-trip, tenant isolation, audit emit per decision arm, deterministic decisions, default-allow on empty blocklist. **Camada-2 composition** documented in module rustdoc: edge blocklist consulted FIRST (zero-cost drop of adversarial IPs); ratelimit per-IP bucket SECOND (camada-1 of WI-S08-001 KeyDimension::PerIp). **Trait-abstraction-defer per charter**: real CF List replica binding + real D1 binding + 100k nightly + cargo-fuzz target — all consolidated alongside WI-S08-006 PRR ship gate. Cross-module patches: workspace `Cargo.toml` adds `crates/corelink-edge` member + workspace dep. Quality gates verde: `cargo test -p corelink-edge --all-targets` 96 tests 0 failures; `cargo clippy --workspace --all-targets --features corelink-worker/tower-middleware -- -D warnings` clean; `validate_specs.py` clean (284 docs); `check_migrations_additive.py` clean (11 migrations including new 0011). **Note**: agent hit Anthropic rate limit at 31 tool uses (initial dispatch); finalizer agent then completed lib.rs + policy.rs + tests + workspace wiring but did not commit (cargo test --workspace timed out on pre-existing prop_webauthn statistical test 6:31min); orchestrator-finalized SEAL ceremony per charter `Real bug detected; don't paper over` pattern. No per-WI codex per 2026-04-30 protocol; sprint-close Sonnet review covers full S-08 corpus. |

---

**Fim de spec contract S-08 SOTA v1.5.0** (Upgrade history: v1.2 initial design; v1.3 WI-S08-001 SEAL; v1.4 WI-S08-002 SEAL; v1.5 WI-S08-003 SEAL).
