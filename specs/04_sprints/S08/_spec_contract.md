---
id: "SPEC-CONTRACT-S08"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
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

- **Lane:** HIGH_RISK (10–12 sign-offs).
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
- **R-S08-3**: PAT-scoped rate check no middleware: `corelink_pat_rate{pat_id}` counter; cap per-PAT 10× refill-rate do tenant (detects PAT misuse).
- **R-S08-4**: Global circuit breaker DO `GlobalRateLimiter`: trip em error_rate > 50% sustained 5 min; `degrade_mode=emergency` fallback.
- **R-S08-5**: Quota checker middleware: pre-write check atomic em D1 `tenant_quota.used_bytes` vs `max_storage_bytes`; atomic CAS.
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
- [ ] **Abuse score calibration**: 10 synthetic workloads (5 benign, 5 abusive) → 0 false positives em benign, ≥ 80% true positives em abusive (EVT-004 benchmark).
- [ ] **RB-FM-250** (DDoS volumetric) dry-run (EVT-017).
- [ ] **Alerts armados**: per-tenant quota 95% (SEV-3 per-tenant), global circuit breaker trip (SEV-1 oncall), abuse score threshold (SEV-2).
- [ ] **Rate limit headers RFC 9331** (RateLimit, RateLimit-Policy) implementados — padrão IETF atualizado.
- [ ] **Property test** 10k iter cobrindo race conditions em DO token bucket update (EVT-002).
- [ ] **Coverage ≥ 90%**.
- [ ] **PRR HIGH_RISK** (10–12 sign-offs Lote 9.4 normalize): SRE lead + Security lead + Engineer + QA + Product + Compliance officer + Privacy officer + Architect + AppSec advisor + 2 peers (EVT-031).

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

---

**Fim de spec contract S-08 SOTA v1.1.**
