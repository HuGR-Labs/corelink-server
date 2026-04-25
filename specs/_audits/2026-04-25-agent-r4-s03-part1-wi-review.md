---
id: "AUDIT-2026-04-25-AGENT-R4-S03-PART1"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
created: "2026-04-25"
reviewer: "Agent R4 (Claude Opus 4.7, 1M context, independent reviewer)"
scope: "Lote 10.3 — Sprint S-03 Part 1 (WI-S03-001 .. WI-S03-004)"
baseline_template: "WI-S01-001 (não revisado; baseline reference)"
sprint_contract: "specs/04_sprints/S03/_spec_contract.md v1.1.0"
---

# Agent R4 — Lote 10.3 S-03 Part 1 (WIs 001-004) WI Review

> Reviewer: independent SOTA reviewer; sem viés ao autor.
> Escopo: 4 WIs (001..004) — primeira metade do auth-real sprint.
> Tom: crítico, técnico, sem "diplomacia". Spec final → código → produção. Auth = highest blast radius.

---

## Veredito Geral

**GO-WITH-FIXES.** Os 4 WIs do S-03 Part 1 representam um **salto qualitativo** vs S-02 baseline em três eixos: (a) **densidade técnica** (cada WI ≥ 700 linhas com ≥ 8 design decisions, ≥ 5 chaos experiments, ≥ 10 risk register rows, sign-off table 13-row consistente nos 4 WIs); (b) **integração cross-WI sólida** (WI-003 referência explícita às public APIs WI-001/002 + hook para WI-004; WI-004 declara hard-blocker em WI-002 + WI-003); (c) **reconhecimento explícito do trade-off cripto-cost** (Argon2 ~250ms verify + session cache TTL 60s + propagation 60s combinados em SLA 120s addendum). Porém **gaps materiais** que se materializariam como **bugs de produção em superfície de auth (highest blast radius)**: (1) **`p > 0.05` ainda mathematically backwards** em WI-002 (mesmo erro de WI-S02-004 não foi propagado consertado para WI-S03-002 narrative; só WI-S03-003 acerta com 3-prong + power); (2) **dummy Argon2 em invalid PAT path** (WI-003 §9.8) é defesa correta mas WI-002 §6.1 não menciona; lógica fica "esquecida" entre crates; (3) **session cache TTL 60s + propagation 60s = 120s combined** (WI-003/004) é declarado SLA mas WI-004 §8 último scenario tem **contradição interna** ("max effective stale window é ≤ 60s" diz uma coisa, e tudo o resto diz 120s — confusão sustentada); (4) **memory budget Worker 128 MiB vs Argon2 64 MiB ephemeral** = max 2 concurrent verifies — WI-002 §15 menciona mas não há rate limit declaration que codifique; risco DoS via cache miss storm; (5) **INV-AUTH-* declarados nos 4 WIs (12+ INVs novos) NÃO existem em invariant_registry.md canonical** — mesmo erro de Lote 6.2/8.x revisitado; (6) **Lazy JWKS refresh em KID miss (WI-001 §9.5) é defensável mas não trata "clock skew Clerk + KID rotation overlap window" race**; (7) **CF DO RPC fan-out vs Queue trade-off (WI-004 §9.2) bem articulado MAS** o cost analysis $62/yr é otimista; **mass revoke storm em incidente** (10k tenant scenarios) é peak load, não mean; capacity planning falta. Nenhum dos 4 é REJECT — todos têm intent claro, escopo limpo, Gherkin razoável (8-13 scenarios), 32 seções template fidelity. **Não selem S-03 Part 1 antes dos P0 fixes** (Mann-Whitney p-value statement, INV-AUTH registry sync, WI-004 stale-window contradiction, dummy Argon2 cross-WI consistency).

**Average Score**: **7.6/10** — GOOD-strong, S-03 está acima do ceiling dos NEW WIs S-02 (7.4 média) com WI-S03-003 escalando para 8.5/10 (best-in-class). Auth surface mandates HIGH bar; review-fixable em P0/P1 cycle; não bloqueante para sprint window.

---

## Per-WI Findings

### WI-S03-001 (Clerk adapter — JWKS + JWT validate + clock-skew)
**Score: 7.5/10** — GOOD, sólido foundation; 5 gaps materiais.

**Strengths:**
- Narrativa §2 (≥ 300 palavras) com **8 vulnerability classes mapeadas** (alg=none, key confusion, expired token, audience spoof, JWKS poisoning, stale JWKS, clock skew, org_id race) — densidade SOTA. CVE references concretas (CVE-2015-9235, CVE-2018-0114) ancorando o threat model.
- **`jsonwebtoken` 9.x version pin** (§9.1) com rationale explícito: explicit ALG allowlist API + audited 2024+ + WASM-compatible. Industry-correct choice.
- **Lazy JWKS refresh em KID miss** (§9.5) é o padrão correto — eager refresh storm em rotation cycle é o anti-pattern. Cost-justified: +1 fetch em raro miss, evita rejection storm.
- **9 Gherkin scenarios** cobrindo CVE regression (alg=none, key confusion), boundary (expired/leeway/audience/issuer), ops (KID rotation lazy refresh), hardening (no panic em malformed). Coverage adequada.
- **§9.6 `nbf` decision** + **§9.7 `jti` revocation defer** — engineering taste; explicit não-features documented.
- **Newtype IDs** (§9.8) — `ClerkUserId` distinct de `ClerkOrgId` distinct de `TenantId` previne accidental swap em compile time. Type-driven security.

**Gaps:**

1. **(P1) JWKS lazy refresh + Clerk rotation race window não tratado.** §9.5 diz "lazy refresh em KID miss; +1 fetch em raro miss". Mas considere: Clerk rotates key v2 às 12:00:00; kid_v1 marcado "deprecated" mas presence em JWKS por mais 30min (overlap window typical). Cliente apresenta JWT signed por v2 às 12:00:01. Fetch JWKS retorna `{kid_v1, kid_v2}`. Funciona. **Mas race**: se Clerk JWKS endpoint está sendo eventualmente consistente entre regions Clerk-side, fetch from Worker em região US pode retornar `{kid_v1}` apenas (sem v2 yet); refresh ainda no-op. Spec não trata o caso "JWKS fetch returned but KID still missing despite fresh fetch". §6.1.3 diz "if not found, refresh JWKS (rotation case) + retry; if still miss → AuthError::KidNotInJwks" — OK, mas o gap é **single retry insuficiente em eventual consistency window** Clerk-side. Mitigação propor: 2 retries com backoff (200ms + 800ms) ANTES de retornar KidNotInJwks, OR documentar explicitamente "Clerk JWKS eventual consistency fora do escopo deste WI".

2. **(P1) JWKS fetch authenticity é puramente TLS-dependent, não verified content.** §9.1 / chaos #4 reconhecem "JWKS poisoning attempt" e classificam como "documented limitation; mitigation = wrangler config". **Mas isso é insuficiente para HIGH_RISK auth boundary.** Se TLS pinning não é implementado (defer S-04 segundo §1 narrative), e CF Worker fetch usa apenas system root CAs, então DNS poisoning OR Clerk subdomain takeover via certificate misissuance = full bypass. JWT signed by attacker key validates. Mitigação concreta deveria propor: (a) **conhecimento JWKS hash baseline pinned em wrangler config** (`CLERK_JWKS_KEY_HASH_FINGERPRINTS`) — verify any newly-fetched key is in pinned set; (b) anomaly detection: novo KID inesperado em JWKS triggera SEV-2 alert. Spec atualmente delega para "S-04 cert pinning"; mas S-03 ships before S-04 — temporal gap.

3. **(P1) `ClerkPrincipal::email` é PII-claim mas not redacted no nível adapter.** §1 código declara `pub email: Email`. §26 LINDDUN diz "email claim is PII; consumed downstream com PII redaction macros (S-09)". **Mas downstream** (WI-S03-003) middleware passa `ClerkPrincipal` para handler via TenantCtx (pelo menos `principal_id` + indireto via `auth_method`). WI-S03-003 não menciona email; assumed scrubbed. Risco: handler logging `format!("{:?}", clerk_principal)` accidentalmente leaks email. Mitigação: `Email` newtype com `Debug` impl que mascara (`gust***@huma***`); consistente com PatPlaintext newtype protection (WI-002 §9.8).

4. **(P1) "412 TenantNotProvisioned" race em onboarding pode ser longa.** §1 / Persona 1 menciona "lazy provisioning — se org_id em JWT mas tenant não em D1/Neon, retorna 412". §3 diz cliente "retry após onboarding finaliza". Mas: quanto tempo é "onboarding finaliza"? Se Clerk emite JWT antes de webhook Clerk→CoreLink processar → race. WI não cita SLA para tenant provisioning lag. **Recomendação**: spec adicionar §X.X "tenant_provisioning_max_lag_ms = 5000ms; cliente retry exponential backoff up to 30s; se ainda 412 após 30s → SEV-2 alert". Caso contrário suporte humano vai ver "intermittent 412" tickets sem SLA atribuível.

5. **(P2) Clock-skew 60s leeway não distinguished entre `iat`/`nbf`/`exp`.** §9.3 fala "leeway 60s"; §9.6 menciona `nbf` "validate se presente, com mesma leeway 60s". OK. Mas: `exp` é **future-bounded** (atacante quer estender); `nbf` é **past-bounded** (atacante quer ativar antes); `iat` é **diagnostic** (logs). Industry conservadoramente: leeway em `exp` mais permissivo (atacante já não pode usar token expirado em runtime se network down); leeway em `nbf` mais estrito (early replay sensitive). Spec uniformiza 60s — defensável mas merece nota explicit em §9.3 "uniform 60s aceito como industry default; per-claim leeway tunable é over-engineering for current risk model".

6. **(P2) Property test em §6.1.8 lista 6 cases mas 10k iter em "alg=none rejected" é overkill.** Property tests devem fuzz inputs, não repetir mesmo case. Specifying "10k iter" para "alg=none rejected" significa ou (a) generating 10k different mal-formed JWTs com alg=none header — fine; (b) testing same JWT 10k times — wasteful. Spec deveria clarificar "10k iter sobre fuzz inputs (mutated JWT bytes)" vs deterministic regression cases. Methodology drift de S-02 baseline.

7. **(P2) "10k iter PR + 100k nightly" cost.** Nightly 100k iter sobre Argon2-equivalent path (este WI = JWT validate, ~5ms warm) é fine: 100k × 5ms = 500s. Mas WI-002 (100k Argon2 verify nightly) = 100k × 250ms = 25.000s = ~7h. CI budget? Não declarado. Should align nightly across S-03 WIs com explicit timeout.

---

### WI-S03-002 (`corelink-pat` Argon2id + timing-safe verify + scope bitset)
**Score: 7.0/10** — GOOD-but-Mann-Whitney-flaw recidiva. 6 gaps materiais.

**Strengths:**
- **Narrative §2 (≥ 300 palavras) com 8 vulnerability classes** + adversarial attacker model (offline crack, online brute-force, side-channel, insider). Cost analysis para offline crack (RTX 4090 ~30 hashes/sec @ 64 MiB Argon2 = intratável) é quantitativo concrete — best em S-03.
- **OWASP 2024 params m=65536/t=3/p=4** confirmados; cross-checked com OWASP Password Storage Cheat Sheet 2024 — **correto**.
- **Newtypes opacos** (§9.8): `PatPlaintext` sem `Display`/`Debug`/`Serialize`; único getter `into_string()` força conscious opt-in. Compile-time defense vs accidental leak.
- **Calibration tooling** (§6.1.9) com explicit deploy validation gate (200-350ms range; alert se outside) — production-grade. ADR-0025 dedicado.
- **Scope bitset u64** (§9.7) com 12+ scope constants enumerados (cache_r/w/find/delete + admin_tenant_r/w + admin_tokens/billing/audit/users + execute_action + report_result + cache_rw alias). Future-proof para até 64 scopes; ADR-0026 documenta migration path.
- **§6.1.11 Mann-Whitney U test em verify timing** + **§6.1.11 também em parse_env timing** — ambos paths covered (não só verify).
- **Adversarial regression tests** (§6.1.12): rainbow table (salt collision check), salt reuse, downgrade, timing oracle, salt < 16B — 5 classes covered.
- **§9.5 dual-layer constant-time defense**: Argon2 lib internal + `subtle::ct_eq` on PHC string. Defense em depth.
- **§9.9 CSPRNG fail-safe**: panic se getrandom unavailable. Cripto fail-loud, correto.

**Gaps:**

1. **(P0) Mann-Whitney `p > 0.05 indistinguishable` é mathematically backwards (ainda).** §10.5.2 + §10.5.3 + §6.1.11 + §8 Gherkin "verify constant-time" + Gherkin "parse_env constant-time" — todos dizem "p > 0.05 → distributions indistinguishable; H0 retained; constant-time confirmed". **Tecnicamente isto está errado.** `p > 0.05` = "fail to reject H0" = "evidence does NOT contradict null hypothesis". **Não prova distributions são same**. Especialmente sem power analysis: `p > 0.05` pode ser puramente low N. Mesmo erro flaggado por R4 review S-02 contra WI-S02-004, e ainda assim **não** propagado consertado para WI-S03-002. WI-S03-003 §10.5.2 (3-prong com power 1−β ≥ 0.80 + Šidák 3-trial + |Δmedian| ≤ 5ms 95% CI) **acerta** o pattern. WI-S03-002 deve copiar exato — sem isto, cargo-cult statistics.

2. **(P0) Memory budget Worker 128 MiB vs concurrent Argon2 mints/verifies sem rate limit.** §15 chaos #5 reconhece: "16 concurrent mints (all 64 MiB each = 1 GiB peak); CF Workers 128 MiB default → ≤ 2 concurrent mints; backpressure expected. **Mitigação documentada: rate limit + queue mint requests (S-13 admin plane)**." Mas **rate limit é S-08; queue admin mint é S-13**; ambos pós-S-03. Em S-03 ship state, **2 concurrent Argon2 verify storm =** (a) 3rd request gets isolate OOM; (b) 4th gets cold isolate spawn (~200ms) + OOM cycle; (c) cascading isolate restarts. Seria DoS via cache miss storm. WI-003 §15 chaos #3 endereça parcialmente ("Worker memory budget; rate limit caps storm; fallback graceful 503") — **mas falta:** (i) **concrete deploy guard** (env var or wrangler config validates expected concurrent verify cap < 2); (ii) bounded queue **dentro do** Worker (sem dependência S-08/S-13). Sem isto, primeiro burst de PATs randoms na produção = isolate OOM. Mitigação P0: **adicionar bounded `Semaphore::with_permits(1)` em Argon2 hot path** dentro do crate corelink-pat OR in WI-003 middleware orchestration; concurrency=1 force serialization; latency = max(N × 250ms) but no OOM.

3. **(P1) Calibration drift trigger não tem auto-rebaseline.** §15 chaos #4 menciona "deploy em distinct HW (CF Workers UK vs US vs JP); verify calibration ∈ range across regions. Outlier > 500ms triggers WI-S03 review (params rebaseline)." OK. Mas: o que acontece se prod hardware **upgrades** (CF Workers ARM rollout)? Calibration cai p50 200ms → 80ms (HW 3× faster). Currently in-range alert NÃO triggers (still > 200ms threshold? maybe). But: 80ms = OWASP rec **violated** (target ≥ 250ms cripto-cost). Spec deveria add: "two-sided alert (lower bound ALSO checked); calibration p50 < 200ms = Argon2 params **underprovisioned** for current HW; trigger ADR-0025 update + bump m_cost." Sem isto, HW upgrade silenciosamente weakens auth.

4. **(P1) "PHC string format" assumption sobre upstream `argon2` lib API.** §6.1.4 diz "Use `argon2::PasswordHasher::verify_password` (PHC-string-aware; constant-time by design)". OK. Mas: `argon2` crate 0.5 API specifically uses `argon2::Argon2::verify_password` em uma signature different de `PasswordHasher` trait. Spec assume API stable; mas major version bump (0.5 → 0.6) frequently breaks. Should pin: `argon2 = "=0.5.x"` (exact) + cargo-deny `[bans]` blocking upgrade sans review. Mesmo assim, **API drift** between minor versions é risk. Recommend: spec adiciona §X "Argon2 wrapper layer in `corelink-pat::internal::argon2_compat` que abstrai upstream API; bump downstream Cargo.toml lib só após review."

5. **(P1) Dummy Argon2 em invalid PAT path NÃO mencionado em WI-002.** WI-S03-003 §9.8 declara: "Solution: em parse_env failure path, ainda compute dummy Argon2 hash com fixed salt/dummy plaintext; return Err depois. Latência iguala. Cost: +250ms em invalid path; aceitável (atacante storm contained por rate limit S-08)." **Mas WI-S03-002** (que owns parse_env + verify) não menciona dummy Argon2. **Inconsistency cross-WI**: dummy Argon2 deve ser **API afforded por crate `corelink-pat`** (e.g., `pub fn dummy_verify_for_constant_time()`), não orchestration em WI-003 que tem que reach into argon2 lib directly. Mitigação: WI-S03-002 deve adicionar na §6.1 "Item 14: `dummy_verify(plaintext: &str)` API que executa Argon2 verify em fixed dummy hash; **timing identical to real verify**; called by middleware (WI-003) em invalid PAT path para constant-time defense." Caso contrário, WI-003 vai implementar manualmente outside of corelink-pat e duplicar código; bug surface.

6. **(P1) Latency p99 budget vs Argon2 cost.** §14.5.4 diz: "mint p99 ≤ 350ms; verify p99 ≤ 350ms; parse_env p99 ≤ 100µs". Calibration target ~250ms (§9.2). **Mas WI-S03-003** §3 SLA addendum diz: "Auth middleware p99 ≤ 10ms warm path (cache hit); ≤ 280ms cold path (Argon2 verify)". **Discrepância:** WI-002 p99 350ms vs WI-003 p99 cold 280ms. Isso assume Argon2 == 280-350ms (worst case 100ms diff = 30% margin). Sem reconcile, cost regression gates podem disparar erroneously: CI test passes em WI-003 (≤ 280ms warm) mas WI-002 standalone passes (≤ 350ms = looser). Should align: ambos = ≤ 350ms p99 OR explicitly note "WI-003 path includes parse + dispatch overhead → upper bound 280ms para Argon2 component apenas; WI-002 standalone = 350ms inclui error handling tail".

7. **(P2) Property test "mint produces unique plaintext" (10k samples).** §6.1.10 + §8 Gherkin. Sample size 10k vs 256-bit keyspace = collision birthday-bound 2^128 expected → 10k samples = effectively zero-collision test (não-test of entropy, just smoke). Make explicit: "test verifies collision-free in 10k sample (smoke); not meaningful entropy validation; entropy validated via getrandom OS interface assumption". Caso contrário future engineer pode aumentar para 100M iter unnecessarily.

8. **(P2) Sub-tasks ST-016 "Crypto SME pair review (mandatory) 4h".** Realista? Pair-program de Argon2 + Mann-Whitney + adversarial em 4h é tight. S-02 review notava similar issue. Should bump to 8h OR split into design (2h) + code (4h) + adversarial (2h) sub-buckets.

---

### WI-S03-003 (Tower middleware + immutable TenantCtx + 5-layer defense)
**Score: 8.5/10** — GOOD-toward-SOTA, **best-in-class S-03 Part 1**. 4 gaps materiais.

**Strengths:**
- **Narrative §2 (8 vulnerability classes mapeadas)** com explicit risk justification; granularidade (TenantCtx mutável, cache poisoning, scope bypass, ordering lazy derivation, audit ordering, session stale, shape evolution, rate limit bypass). Densidade SOTA.
- **TenantCtx imutável compile-time enforced** (§9.2 + §12 INV-AUTH-TENANTCTX-IMMUTABLE): private fields + builder pattern + `#[non_exhaustive]` + Send+Sync+'static. Type-driven security — mode correto.
- **Mann-Whitney 3-prong com power analysis** (§10.5.2): N ≥ 10000 + power ≥ 0.80 + Šidák 3-trial gate + |Δmedian| ≤ 5ms 95% CI. **Statistically rigorous** — único WI S-03 que acerta.
- **§9.7 escolha "invalid vs scope-insufficient" comparison** é o **right oracle to test** (não "ok vs invalid" trivial). Reasoning explicit; reflects prior audit feedback.
- **§9.8 dummy Argon2 em invalid PAT path** — defesa correta vs distinguishable timing. Spec aware do trade-off (+250ms invalid).
- **§9.6 scope check via Tower layer (não handler-level)**: composability + deny-by-default route definition. Compile-time validation `axum::routing` requires layer.
- **Production deploy guard** (§6.1.7) impede `passthrough` em prod env — **defensive engineering**. Single most underrated feature; many breaches from "stub left in prod".
- **Cost analysis com TCO 12m** (§22): per-op breakdown warm vs cold + alternativas comparison ($910k/yr synchronous Argon2 vs $12.5k/yr session cache vs $55k/yr custom Redis). Quantitative + grounded.
- **10 chaos experiments** (§15) — cobertura mais ampla S-03; inclui confused deputy (#8) + cold-start under load (#9) + audit emission D1 batch failure (#10).
- **12-row risk register 6-col** com explicit `Mann-Whitney CI flake` (R-004), `dummy Argon2 path env flag exposed` (R-012) — paranoia justificada.

**Gaps:**

1. **(P0) Combined stale window contradiction WI-003 + WI-004.** §3 SLA: "≤ 120s combined (60s session_cache TTL + 60s WI-S03-004 propagation)". §9.3 Why session cache TTL 60s diz: "Combined max stale window 120s". OK consistente em WI-003. **Mas WI-S03-004 §8 último Gherkin scenario** ("Combined stale window bound") tem statement contradictory: "max effective stale window is **≤ 60s** (session cache TTL alone; D1 always authoritative)". WI-004 §9.4 reforça: "verify path ... D1 lookup api_tokens com revoked_at IS NULL constraint. D1 returns row OR not-found; revoke handled at D1 level." Reconciliation: **se** middleware verify-path checks D1 revoked_at em cold-path (cache miss), then revocation effective ≤ 60s (cache TTL alone, not + propagation). **Mas WI-003 hot path** (cache hit) bypasses D1 lookup entirely (that's the optimization point). Hot path stale window = session cache TTL = 60s, **independent** of propagation. Hot path stale window **does NOT compose** with propagation (orthogonal axes). Then **why** does WI-003 §3 say "≤ 120s combined"? Misleading. Recomendação P0: align both WIs on "**hot-path** stale window ≤ 60s session cache TTL; **cross-region cold-path** propagation ≤ 60s p99; both ≤ 60s axes; **NOT additive**." Customer-facing SLA = single number 60s p99. Actual contradiction if not fixed = customer claim "you said 60s, my session held 120s, that's a breach lawsuit".

2. **(P1) Session cache key sha256 truncated 16 bytes — collision space evaluation incomplete.** §9.4: "Truncated 16 bytes = 128-bit; collision resistance birthday-bound 2^64; computacionalmente intratável." Correct in pure mathematical isolation. **But in operational context**: 10M req/dia × 365 dias = 3.65B sessions/yr; 2^64 ≈ 1.8 × 10^19. Birthday collision probability at 3.65B = ~3.7 × 10^-10. OK. **Mas:** if attacker can **influence** session keys (e.g., generate PATs com chosen prefixes via mint API), then targeted collision attack is reduced complexity. PAT plaintext is full-entropy 32 bytes random → sha256(plaintext)[:16] also random; chosen-prefix infeasible. **OK overall mas**: should make explicit "sha256 truncation safe **because** PAT plaintext entropy guaranteed full 32 bytes via getrandom (cf. WI-002 §9.6); truncation NOT safe if input has attacker-influenced entropy." Defense in depth: bump to 24 bytes truncation (192-bit); negligible KV key cost.

3. **(P1) Audit pre/post-emit ordering com handler error path.** §6.1.8: "Pre-handler: auth.token.validated ... Post-handler: auth.{ok,denied} based on response status code. Both inside D1 batch with TenantCtx commit (single transaction)." But: **what happens if handler panics**? Panic em axum handler → tokio task abort → response cleanup; post-emit `auth.{ok,denied}` may never fire. Outbox has `auth.token.validated` (pre) but no `auth.ok|denied` (post) → audit chain has orphan validation event. WI-S01-005 outbox guarantee é write-side; reads expect pre+post pairs. Mitigação: spec adiciona §X "panic recovery: tokio `catch_unwind` em middleware; force-emit `auth.error{panic}` em outbox; SEV-1 alert; runbook RB-FM-AUTH-PANIC". OR: pre+post audit pair via single committed atomic (post-emit deferred until response observed; outbox writer `flush_after_response` hook). Currently spec ambiguous.

4. **(P1) "Verify count drop ≥ 99%" projection optimista.** §1: "Verify count drop ≥ 99% em workload típico (10M req/dia × 0.5% cache miss = 50k Argon2/dia vs 10M; massive cost reduction)." 0.5% cache miss assumption = 99.5% hit ratio. **But:** real workload (CI bursts; deploy hits): cache miss spikes during deploy windows (~30 min/day). Realistic mean: 95% hit ratio (~5% miss = 500k Argon2/dia). 10× more cost than projection. WI-002 §22 cost analysis assumes "0.5% verify (cache miss; rest hit session cache em WI-S03-003 middleware)" — same optimistic 0.5%. Should reconcile: spec uses **conservative** 5% baseline (95% hit ratio); document 99.5% as "best case sustained steady-state, not deploy/burst". Cost regression gate at 5% miss baseline; alert at 10% miss sustained.

5. **(P2) §17 sub-tasks ST-011 "Mann-Whitney 3-prong test impl 4h" + ST-014 "Chaos suite 5-layer propagation 3h".** Mann-Whitney test infra (statistics + power analysis + Šidák correction + 3-trial gate + flake-resilient CI hook) is genuinely 8h+ work, especially first-time. Chaos suite 5-layer propagation requires 5 chaos test files + harness. Expand to 6h + 5h respectively or note "estimate optimistic — first time impl, will iterate".

6. **(P2) §6.1.4 auth method routing "Detect by token prefix: corelink_<env>_* → PAT path; eyJ... → JWT path".** Current `eyJ` JWT detection é base64url-of-`{"alg`. **But**: malformed token starting with `eyJ` then garbage = `JWT path` triggered; `ClerkAdapter::validate` returns Malformed; flow returns 401 — fine BUT: ambiguity check §6.1.4 "ambíguo → 401 com COR_AUTH_HEADER_AMBIGUOUS". When does "ambíguo" trigger? Token starting with neither pattern. So tokens like "Bearer xyz" (no prefix) hit ambiguous. Tokens like "Bearer eyJxxx" (looks JWT, parses bad) hit JWT-malformed (different error code). Two codes for "looks-like-JWT-fails" vs "no-recognizable-prefix" — UX confusion. Should align to single error code OR document distinguishing rationale.

---

### WI-S03-004 (Revocation DO + KV invalidation + ≤ 60s propagation)
**Score: 7.5/10** — GOOD, sólido orchestration; 5 gaps materiais including a P0 contradiction.

**Strengths:**
- **Narrative §2 (8 vulnerability classes)**: broadcast loss, DO migration data loss, KV invalidation race, propagation lag, mass revoke storm, audit gap, dashboard race, hot path latency. CF-specific risks (DO migration WAL; Queue at-least-once) addressed.
- **D1 SoT explícito** (§9.4): "verify path ... D1 returns row OR not-found; revoke handled at D1 level. DO query (`is_revoked`) é admin path only; not in critical verify path." Single-writer transactional truth — correct architecture.
- **§9.2 Queue vs RPC fan-out trade-off** quantified: "Pure DO RPC: latency = max(rpcs) = 4×region-RTT ~200ms p99 best case + retries; fragile vs partition. CF Queue: durable; at-least-once; ≤ 60s p99 SLO." Engineering rigor.
- **§9.5 mass revoke rate limit 100/sec/tenant** justified: "anti-abuse + queue backpressure. 10k = 100s SLO acceptable vs DoS risk." Operacional grounding.
- **§9.6 batch 100 entries/queue message**: CF Queue 256 KiB limit + RevokedEntry ~200 bytes = 100 fits comfortably. Cost reduction quantified.
- **§9.7 explicit reason enum** (UserInitiated/AdminInitiated/SecurityIncident/Expired/ScopeChanged/MassRevoke) com `#[non_exhaustive]` future-proof. SOC 2 + LGPD compliance categorized.
- **§9.9 reconciliation Cron daily** + drift alert SEV-2 + runbook RB-FM-REVOKE-DRIFT — operational discipline.
- **10 chaos experiments**: cross-region stress, queue outage, DO migration, mass revoke storm, race verify-vs-revoke, combined stale window probe, reconciliation drift, queue consumer offline, audit emission gap, replay attack post-propagation. **Comprehensive**.
- **Gherkin "Race condition revoke vs verify"** (§8) é critical scenario well-spec'd: Argon2 mid-flight + revoke fires at T+5ms → middleware checks D1 revoked_at pós-Argon2 → 401. **Spec correctness defensável.**

**Gaps:**

1. **(P0) §8 último Gherkin "Combined stale window bound" contradiz §3 SLA + WI-003.** Quote literal: "max effective stale window is **≤ 60s** (session cache TTL alone; D1 always authoritative)". **Mas §3** Persona 2 + 3 SLA addendum dizem "Combined max stale window (session cache + propagation) ≤ 120s p99". §9.8 reforça 120s. **Contradição interna**: same WI says both 60s AND 120s. Combined com WI-003 §3 ("≤ 120s combined"), customer-facing inconsistency. P0 clarify (cf. WI-003 gap #1): hot-path = 60s session cache TTL; cross-region propagation = 60s axis independent; **NOT additive** (D1 authoritative em cold path). SLA addendum should be single 60s p99 with footnote on hot-path session cache decoupling.

2. **(P1) Mass revoke 100/sec/tenant rate limit não enforced em código spec.** §6.1.5 Mass revoke endpoint description não menciona rate limit enforcement. §9.5 design decision says "100/sec rate limit" but enforcement mechanism (D1 lock? DO atomic counter? KV bucket?) não especificado. Without explicit enforcement, "100/sec" é aspirational. Recommend: §6.1.5 adds "rate limit enforced via DO atomic counter `mass_revoke_window_count` per tenant; 1-second sliding window; exceed → 429 com Retry-After" OR similar concrete impl path.

3. **(P1) DO ↔ D1 reconciliation Cron daily — but transient drift detection 5min budget incoherent.** §9.9 + §10.5.8: "Reconciliation Cron daily detects drift sustained > 5min." Mas Cron runs **daily** (24h cycle); como detecta drift "5min sustained"? Either (a) Cron stores per-day diff history, alerts if same drift pendente em 2 successive runs (= 24h-48h detection); OR (b) Cron has multi-pass: per minute polling. (a) is the common interpretation but means **drift detection lag = 24h-48h sustained, not 5min**. Reconcile spec: "drift > 5min sustained = real bug (not transient); Cron daily detects + alerts; **detection latency = 24h cycle, not 5min**." OR: implement Cron a cada hora (not daily); reconcile $0.10 → $2.40/dia × 365 = $876/yr (still negligible).

4. **(P1) DO migration cross-region partition handling.** §15 chaos #3 "DO migration during in-flight revoke" tests **single-region migration** (CF region failover within same region). But: **cross-region partition** (e.g., wnam DO unreachable from enam consumer for 2min) — what happens? Queue consumer attempts RPC `DO_wnam.ingest_remote_revocation` → timeout → DLQ → retry → still fail → SEV-2. Mas durante essa janela, revocation propagation lag = duration of partition. SLO ≤ 60s p99 violated. Spec não enumera explicit "cross-region partition recovery time". §28 R-001 menciona "Propagation > 60s p99 sustained" mas mitigation é "Tiered alerts" (passive). **Active mitigation needed**: per-region failover queue (DLQ-of-DLQ); manual reconciliation script (RB-FM-REVOKE-LAG runbook). Currently runbook listed but not detailed in WI.

5. **(P1) Cost analysis $62/yr é mean, não peak.** §22 TCO: "100k revokes/yr + 5 mass revokes 10k each". Realistic peak: tenant breach incident → mass revoke 100k PATs em 1 hour. Cost: 100k × $0.000003 = $0.30 incident + $5 batch base = $5.30 per incident. Cumulative incidents: depends. **Mas operational cost during incident = queue burst storage + DLQ accumulate + reconciliation drift detection**. Peak load capacity planning missing. Should add §22 sub: "Peak load model: mass revoke 100k em 1h = 28 ops/s sustained; queue capacity (default CF Queue 5000 msg/s) handles; DO storage peak +10 MiB transient (acceptable < 100 MiB cap §14.5.9)."

6. **(P2) §6.1.7 reconciliation Cron daily with "manual sync runbook" — RB-FM-REVOKE-DRIFT not detailed in WI.** Listed em §13 Artifacts but content not described. Should add §X "RB-FM-REVOKE-DRIFT outline: 1) verify D1 vs DO diff; 2) decide source of truth (D1 wins typical); 3) manual sync script `corelink admin revocation reconcile --tenant X`; 4) verify post-sync; 5) audit event `revocation.reconciliation.manual`; 6) close incident."

7. **(P2) §6.1.10 audit events list 3 types (`auth.token.revoked`, `auth.session.revoked`, `auth.mass_revoke.triggered`).** But §3 + §12 INV-AUTH-D1-IS-SOT discuss "revoke" generically. Reconcile: explicitly map "single revoke → emits `auth.token.revoked`"; "mass revoke → emits `auth.mass_revoke.triggered` + N×`auth.token.revoked`"; "session invalidate hook → emits `auth.session.revoked` only"; "session emitted by hook in WI-003 OR by this WI?". Chain semantics unclear; auditor compliance reports may double-count.

8. **(P2) Sign-off table row 13 "Crypto SME advisory; non-crypto-touching".** This is **defensible** (revocation is data plane orchestration, not crypto primitive). But sprint contract §14 lists Crypto SME mandatory for S-03. Reconcile: row 13 say "advisory not mandatory" but sprint mandates 13 sign-offs total. If Crypto SME advisory is optional, then this WI = 12 mandatory + 1 advisory; total 12 mandatory matches sprint contract spirit. Should clarify "advisory" semantics: does sign-off table need 13 signatures or 12 + advisory note?

---

## Cross-WI Consistency

**Strengths:**

- **Crate paths consistent** across the 4 WIs:
  - `crates/corelink-clerk/` (WI-001)
  - `crates/corelink-pat/` (WI-002)
  - `crates/corelink-worker/src/middleware/auth.rs` (WI-003)
  - `crates/corelink-worker/src/auth/revocation.rs` (WI-004)
  - Hierarchy clean; no naming drift.

- **Dependencies matrix coerente**:
  - WI-003 declares hard blockers WI-001 + WI-002 (correct; consumes ClerkAdapter + PatVerifier).
  - WI-004 declares hard blockers WI-002 + WI-003 (correct; needs PatId types + SessionCache::invalidate hook).
  - WI-001 declares only S-01 SEALED (correct; standalone).
  - WI-002 declares S-01 SEALED + Crypto SME availability (correct).

- **ADR allocation 0024-0030 corretamente whitelisted** em `scripts/validate_references.py` (verified):
  - ADR-0024 (WI-001 Clerk JWKS cache)
  - ADR-0025 (WI-002 Argon2 calibration)
  - ADR-0026 (WI-002 PatScopes bitset)
  - ADR-0029 (WI-003 TenantCtx + session cache)
  - ADR-0030 (WI-004 Revocation propagation)
  - **ADR-0027 + ADR-0028 reservados** para outros WIs (S-01/S-02). **No conflicts.**

- **Sign-off tables**: All 4 WIs have **13-row** sign-off tables consistent with sprint contract §14 ("12 mandatory + 1 optional Crypto SME = 13"). **WI-S02 inconsistency (11-row vs 13-row) NÃO replicada em S-03 — correct fix.**

**Gaps:**

1. **(P0) `INV-AUTH-*` invariants declarados em WIs NÃO existem em `specs/03_architecture/invariant_registry.md`.** Concrete check (verified via grep `INV-AUTH-` registry): **0 hits**. WIs declarate:
   - WI-001: INV-AUTH-JWT-VALIDATE-RS256-ONLY, INV-AUTH-CLOCK-SKEW-BOUND, INV-AUTH-ISS-EXACT-MATCH, INV-AUTH-KID-RESOLUTION (4)
   - WI-002: INV-AUTH-PAT-HASH-ARGON2ID-2024, INV-AUTH-PAT-PLAINTEXT-NEVER-PERSISTED, INV-AUTH-PAT-VERIFY-CONSTANT-TIME, INV-AUTH-PAT-SALT-PER-TOKEN, INV-AUTH-PAT-SCOPE-DB-IS-SOT (5)
   - WI-003: INV-AUTH-TENANTCTX-IMMUTABLE, INV-AUTH-5-LAYER-ORDERING, INV-AUTH-SESSION-CACHE-KEY-CT, INV-AUTH-SCOPE-MIDDLEWARE-LEVEL, INV-AUTH-AUDIT-PRE-POST-ORDERING (5)
   - WI-004: INV-AUTH-REVOCATION-IDEMPOTENT, INV-AUTH-REVOCATION-SLO-60S, INV-AUTH-D1-IS-SOT, INV-AUTH-MASS-REVOKE-ATOMIC, INV-AUTH-PROPAGATION-AT-LEAST-ONCE (5)
   - **Total: 19 NEW INVs** declared without registry sync. Mesmo pattern de S-01/S-02 audit findings (Lote 6.2) — registry update PR is mandatory **before seal**.

2. **(P0) Combined stale window declared inconsistently across WI-003 + WI-004.** Already detailed em WI-003 gap #1 + WI-004 gap #1. WI-003 says "≤ 120s combined"; WI-004 §3 says "≤ 120s p99 (session cache + propagation)" but Gherkin says "≤ 60s (session cache TTL alone; D1 always authoritative)". **Resolução required:** customer-facing SLA é unique number; specs internamente devem clarify hot-path vs cold-path semantics. Recomendação:
   - Hot path (cache hit) stale = 60s (session TTL).
   - Cold path (cache miss) stale = ≤ 60s (D1 authoritative, propagation independent).
   - Cross-region propagation SLO = ≤ 60s (separate SLO).
   - **Combined "stale window" = max of hot/cold = 60s; NOT additive**.

3. **(P1) Dummy Argon2 cross-WI ownership unclear.** WI-003 §9.8 specs dummy Argon2 in middleware level. WI-002 doesn't expose `dummy_verify` API. Either:
   - (a) WI-002 adds `pub fn dummy_verify_for_constant_time()` (recommended; encapsulation).
   - (b) WI-003 implements internally (fast path; bug surface high).
   Currently spec ambiguous → engineer will discover at impl time. P1 to resolve in spec.

4. **(P1) Session cache TTL config: env var name inconsistency.** WI-003 §6.1.3: `CORELINK_SESSION_CACHE_TTL_S`. WI-003 §22 cost analysis assumes 60s default. **OK**. But WI-004 §3 references "60s session cache TTL" as if hard-coded; references env var não. Recomendação minor: WI-004 §3 explicitly cite "subject to `CORELINK_SESSION_CACHE_TTL_S` (default 60s; defined in WI-003 §6.1.3)".

5. **(P1) Outbox table integration referenced in 3 WIs.** WI-003 §6.1.8 + WI-004 §6.1.2 + WI-004 §15 all reference "WI-S01-005 audit_outbox". WI-S01-005 SEALED status assumed. Should verify WI-001 audit emission also goes through outbox (spec actually says "trace span `clerk.validate`" only — implicit assumption that WI-001 emits via outbox in WI-S03-007). Recomendação: WI-001 §6.1.7 explicitly call `audit_outbox.insert()` em validate path; ou WI-S03-007 owns auth.token.* events emission stack-wide.

6. **(P2) Métricas naming consistency.** Cross-WI metrics:
   - WI-001: `corelink.auth.clerk.*`
   - WI-002: `corelink.auth.pat.*`
   - WI-003: `corelink.auth.middleware.*` + `corelink.auth.session_cache.*` + `corelink.auth.tenant_ctx.*`
   - WI-004: `corelink.auth.revocation.*`
   - **OK pattern consistente** (`corelink.auth.<sub_module>.<metric>`).
   - Minor: WI-003 has cardinality concern — `corelink.auth.middleware.duration_ms_bucket{path}` with path enum 4 values × Worker isolate count = bounded; OK.

---

## Technical Accuracy Issues

### WI-001
- **alg=none rejection**: §6.1.3 + Gherkin "alg=none rejected" — explicit `Validation::new(Algorithm::RS256)` enforce; **correct**. `jsonwebtoken` 9.x API enforces ALG allowlist regardless of header content.
- **`jsonwebtoken` 9.x version pin**: §6.1.1 "= 9.3" — correct (9.3+ has explicit `Validation::set_required_spec_claims()` API).
- **Lazy JWKS refresh em KID miss**: defensável (cf. gap #1 race window). Industry-correct.
- **Issuer allowlist exact match**: §9.4 prevents `https://clerk.corelink.dev.attacker.com` confusion; **correct**.

### WI-002
- **Argon2id m=65536/t=3/p=4**: OWASP Password Storage Cheat Sheet 2024 explicitly recommends `(m=46 MiB, t=1, p=1)` OR `(m=19 MiB, t=2, p=1)` as floors. WI's m=65536 (= 64 MiB) **exceeds** floor; t=3 OK; p=4 OK. **Correct (strict floor surpassed)**.
- **PHC string format**: "$argon2id$v=19$m=65536,t=3,p=4$<salt>$<hash>" — **correct PHC format** (RFC-flavored).
- **`subtle::ConstantTimeEq`**: §9.5 — correct; subtle 2.5+ provides `ct_eq` for byte slices.
- **`getrandom` (não `OsRng`)**: §9.6 — **correct**; `getrandom` 0.2+ has `js` feature for WASM Workers.
- **Mann-Whitney U vs t-test**: choice OK for long-tailed latency. **But p-value statement direction is WRONG** (cf. gap #1).
- **Salt 16 bytes**: OWASP recommends ≥ 16; **correct**.

### WI-003
- **TenantCtx imutável compile-time**: private fields + only public via getters + builder pattern + `#[non_exhaustive]`. **Compile-time enforced**, not runtime. **Correct**.
- **Session cache TTL 60s**: Tunable via env. Combined com WI-004 = the contradiction documented (gap #1).
- **Dummy Argon2 em invalid path**: defensible constant-time defense. Cost: +250ms in invalid path. **Correct trade-off** (rate limit handles storm).
- **Mann-Whitney 3-prong**: power 1−β ≥ 0.80 + Šidák 3-trial + |Δmedian| ≤ 5ms 95% CI = **statistically rigorous** (only WI in S-03 to do this right).

### WI-004
- **DO + Queue propagation ≤ 60s p99 4 regions × queue lag**: feasible. CF Queue typical consumer lag p99 = 5-30s; per-region RPC p99 = 200ms-2s; 4 regions sequential = 1-8s. Headroom 50-55s for retries. **Realistic if queue consumer push-based** (not Cron 10s). Needs explicit consumer config (push vs pull).
- **Mass revoke 100/sec/tenant rate limit**: defensible (anti-abuse + queue backpressure). Enforcement mechanism not spec'd (gap #2).
- **D1 IS SoT (not DO storage)**: trade-off well-articulated (§9.4). Verify path checks D1 in cold path; DO is admin/audit query only. **Correct architecture**.
- **CF DO WAL persistence**: assumed via "CF DO storage tem write-ahead log; data loss impossible se commit returned" (§2 #2). **Correct (CF documented behavior).**

---

## Missing Gaps for Production

Categorized by surface:

### Auth surface

1. **JWKS rotation racing com S-04 PAT signing key rotation**: WI-001 + future WI-S04-X both rotate keys 24h. If they rotate simultaneously com transient mismatch (DO_wnam has new PAT key v2 but Clerk JWKS only has v1), validation cascade failures. Spec não cross-references S-04 timing.

2. **Argon2 memory budget Worker isolate (128 MiB limit)** NOT CODIFIED beyond chaos test. Current spec assumes "rate limit S-08 + queue admin S-13 forward" but those are post-S-03. P0 actionable: bounded `Semaphore::with_permits(N)` em corelink-pat verify path or middleware orchestration; default N=1 (serialize); upgrade later.

3. **WebAuthn step-up token timing attack**: WI-S03-006 forward but interaction with WI-003 middleware not addressed (step-up requires re-MFA freshness; how does middleware integrate?). Out of scope for S-03 Part 1 review but flag for Part 2 (WI-005..008).

4. **JWT replay window pós-revoke**: hot path session cache 60s is the de facto JWT replay window (revocation propagation independent). Customer-facing message clarification needed.

5. **DO migration cross-region partition handling** (cf. WI-004 gap #4): single-region migration tested; cross-region partition (Queue consumer can't reach DO_wnam) lacks explicit recovery procedure beyond "SEV-2 alert".

### Compliance surface

6. **Audit emission cost spike em mass revoke storm**: 10k tenant breach mass revoke = 10k audit events em 1 batch + 10k S-09 chain integrity hashes. S-09 forward but spike modelo not in §22 cost analysis.

7. **LGPD Art. 47 (consent revogation)** mentioned em WI-004 §26 but no explicit DSR-export integration. WI-S03-008 supposedly handles DSR PAT export but coordination across sprint não detailed.

8. **GDPR Art. 17 (erasure)** WI-004 §26 references but actual erasure pipeline (delete vs anonymize PAT records) deferred to S-11. Should explicit "PAT records retained 7y post-revoke for SOC 2; anonymized via S-11 pipeline; DSR erasure overrides retention após manual review".

### Cripto surface

9. **`subtle` crate version pin**: WI-002 §6.1.1 "subtle = 2.5". Correct. But ARM-specific timing attack research (2023 papers) suggests `subtle` may not fully harden ARM cache timing. Acceptable for current x86/WASM but flag for nightly cachegrind audit (mentioned WI-002 §2 mas not enforced).

10. **Argon2 lib pin**: WI-002 `argon2 = "0.5"` (not exact). Should be `=0.5.x` exact pin OR `[bans]` policy preventing 0.6 upgrade sans review (WI-002 gap #4).

### Operational surface

11. **Calibration drift two-sided alert** (WI-002 gap #3): only upper-bound flagged; HW upgrade silently weakens auth. Needs lower-bound alert + auto-rebaseline workflow.

12. **Backend unavailable cascade behavior** (WI-003): if Clerk + PatVerifier both down → 503 across all auth — entire system down. Spec mentions "graceful degradation; not unsafe state" but no per-method circuit breaker; one common timeout cascades. WI-003 §28 R-010 lists "Per-method fallback" mas mitigation says "circuit breaker S-XX forward" — not in S-03.

13. **Production deploy guard for OTHER mandatory env vars**: WI-003 §6.1.7 guards `CORELINK_RATE_LIMIT_BACKEND`. But: `CLERK_JWKS_URL`, `CLERK_AUDIENCE`, `CLERK_ISSUER_ALLOWLIST` (WI-001) — also mandatory in prod, no explicit deploy guard listed.

---

## Comparison vs Sprint Contract S-03

Sprint contract §5 deliverables S03-D1..D8; Part 1 (this review) = WIs 001-004 covers D1..D4.

| Deliverable | WI | Coverage | Notes |
|---|---|---|---|
| **S03-D1** Clerk adapter | WI-S03-001 | ✅ Complete | JWKS cache 24h KV + JWT validate clock-skew 60s + RS256 only + 5 CVE regression tests. **Aligned**. |
| **S03-D2** corelink-pat (Argon2 + scope + timing-safe) | WI-S03-002 | ✅ Complete | OWASP 2024 params + ConstantTimeEq + benchmark calibration ~250ms + scope bitset 13+ scopes. **Aligned**. |
| **S03-D3** Tower middleware + TenantCtx | WI-S03-003 | ✅ Complete + bonus | 5-layer defense propagation + integration test + session cache 60s + dummy Argon2 + Mann-Whitney 3-prong + production deploy guard. **Exceeds contract** (best-in-class). |
| **S03-D4** Revocation DO + KV invalidation | WI-S03-004 | ✅ Complete with gap | Cross-region ≤ 60s p99 + chaos test mid-flight revoke + idempotent + mass revoke 100/sec/tenant + reconciliation Cron. **Stale window contradiction (P0 fix needed)**. |

**Capability mapping**:
- CAP-AUTH-001 (Clerk SSO) → WI-001 primary ✅
- CAP-AUTH-002 (PAT lifecycle) → WI-002 (mint+verify) + WI-004 (revoke) ✅
- CAP-AUTH-003 (Scope enforcement) → WI-003 primary + WI-002 bitset construct ✅
- CAP-AUTH-004 (Tenant resolution + propagation) → WI-003 primary + WI-004 partial ✅
- CAP-AUTH-005 (Tenant provisioning) → WI-001 partial (TenantNotProvisioned hook); main em S-13 ✅ (out-of-scope ack)
- CAP-AUTH-006 (Audit events PAT) → WI-S03-007 (Part 2) — flagged as soft blocker em WI-003 audit emission ⚠️
- CAP-AUTH-007 (Argon2id PAT hash) → WI-002 primary ✅

**Coverage assessment: 4/4 deliverables addressed, 3 with full alignment, 1 (D4) with internal contradiction P0.**

**Sprint contract §6.1 PAT signing key 24h overlap (key_management §3.2.1 + ADR-0018)**: WI-002 §9.10 mentions "ADR-0026 forward + ADR-0025"; **mas** PAT signing key rotation overlap NOT addressed in WI-002 (focus is PAT plaintext hash via Argon2, not signing key). Sprint contract R-S03-9 explicitly: "PAT signing key rotation: 24h overlap per ADR-0018". WI-002 conflates "PAT plaintext hash" with "PAT signing key" — these are different. **PAT signing key rotation** missing as coverage; should be in WI-005 (Neon schema) or new WI. This is partial sprint contract gap.

---

## Recommendations

### P0 — bloqueia seal (Must fix before WI SEAL):

1. **Mann-Whitney `p > 0.05` semântica em WI-002** (§10.5.2 + §10.5.3 + §6.1.11 + §8 Gherkin "constant-time"). Adopt WI-003 3-prong methodology (power ≥ 0.80 + Šidák + |Δmedian| ≤ 5ms 95% CI).
2. **Combined stale window contradiction WI-003 + WI-004**. Resolve customer-facing SLA: hot/cold paths não aditivos; documented as ≤ 60s p99 single SLA com hot-path footnote. Remove "120s" claims. Update WI-003 §3, §9.3 + WI-004 §3, §8 last Gherkin, §9.4, §9.8.
3. **`INV-AUTH-*` registry sync** (19 new INVs across 4 WIs). PR adicionando rows ao `specs/03_architecture/invariant_registry.md` mandatory antes de SEAL. Parallel work: same-day fix.
4. **Argon2 memory DoS bounded `Semaphore` em corelink-pat OR middleware** (default N=1; concurrency cap explicit, não dependendo de S-08/S-13).

### P1 — sprint window (recommended fix em S-03 sprint window before ship):

5. **Dummy Argon2 API afford in WI-002** (`pub fn dummy_verify_for_constant_time()`); WI-003 calls instead of internal impl.
6. **JWKS retry policy WI-001 §6.1.3** (2 retries with 200ms+800ms backoff before AuthError::KidNotInJwks; OR document Clerk eventual consistency as scope-out).
7. **JWKS hash baseline pinned** WI-001 (`CLERK_JWKS_KEY_HASH_FINGERPRINTS` em wrangler config; defense em depth vs DNS poisoning).
8. **Mass revoke rate limit enforcement mechanism** WI-004 §6.1.5 (DO atomic counter or KV bucket; concrete impl path).
9. **Reconciliation Cron cadence** WI-004 §9.9 (24h cycle vs 5min sustained drift detection — clarify or upgrade to hourly).
10. **Cross-region partition recovery** WI-004 (DLQ-of-DLQ strategy + RB-FM-REVOKE-LAG runbook detail).
11. **Calibration two-sided alert** WI-002 (lower-bound trigger; HW upgrade detection + auto-rebaseline workflow).
12. **`Email` newtype com redacted `Debug`** WI-001 (consistent com PatPlaintext protection WI-002).
13. **TenantNotProvisioned race SLA** WI-001 (provisioning lag ≤ 5s; cliente retry exp backoff up to 30s; SEV-2 alert em > 30s).
14. **Session cache key 24-byte truncation** WI-003 §9.4 (192-bit collision space; defense em depth).
15. **Audit pre/post-emit panic recovery** WI-003 §6.1.8 (`tokio::catch_unwind` + force-emit `auth.error{panic}` em outbox).
16. **Realistic cache miss baseline** WI-003 §22 (5% mean, not 0.5%; cost regression at 5% baseline; alert at 10% sustained).
17. **PAT signing key rotation 24h overlap** sprint contract R-S03-9 — não em WIs 001-004; should be em WI-005 OR new WI.
18. **`Semaphore` para concurrent verify** (P1 if not already P0 fixed).

### P2 — next sprint (improvements, low-priority):

19. **Property test wording** WI-001 §6.1.8 (clarify "10k iter sobre fuzz inputs" not "same case 10k times").
20. **Argon2 lib exact version pin** WI-002 (`= "=0.5.x"` exact + `cargo-deny [bans]`).
21. **Auth method routing ambiguity** WI-003 §6.1.4 (single error code OR explicit rationale for two codes).
22. **Métricas cardinality audit** all WIs (cross-check observability_model.md §11.2 budget).
23. **Knowledge transfer workshop** all WIs — schedule pre-merge with Crypto SME + AppSec + downstream WI authors.
24. **Sub-task estimation reality check** all WIs (Crypto SME 4h, Mann-Whitney 4h likely under-estimate; bump to 6-8h each).
25. **Sign-off Crypto SME advisory semantic** WI-004 §30 (does row 13 require signature or note?).
26. **Reconciliation runbook RB-FM-REVOKE-DRIFT detail** WI-004 §13 artifacts (outline body in WI not just listed path).

---

## Final Verdict

**GO-WITH-FIXES.**

Os 4 WIs do S-03 Part 1 são **production-ready com fixes**. WI-S03-003 (8.5/10) é best-in-class do framework — densidade técnica + Mann-Whitney 3-prong + production deploy guard + 12-row risk register estabelece novo SOTA bar. WI-S03-001 (7.5) e WI-S03-004 (7.5) são GOOD-solid foundations com gaps endereçáveis em sprint window. WI-S03-002 (7.0) é GOOD mas com **regressão crítica** de Mann-Whitney p-value semântica que já foi flaggada em S-02 audit e não foi propagada consertada — requires P0 fix antes de seal. **Average score 7.6/10** — acima do S-02 NEW WIs (7.4 average), confirmando trajetória SOTA elevation; abaixo de SOTA puro 9-10 que demanda zero P0 + ≤ 2 P1 gaps.

**Auth surface = highest blast radius**: P0 fixes (Mann-Whitney semântica + stale window contradiction + INV-AUTH registry sync + Argon2 memory budget bounded) **devem** ser resolved antes de SEAL para evitar bugs em produção que se materializam como (a) cargo-cult statistics em CI flaky, (b) customer SLA breach lawsuit ("you said 60s, my session held 120s"), (c) audit chain ungoverned (INVs declarados sem registry; future TLA+ specs reference broken invariants), (d) DoS via cache miss storm em primeiro burst load. Os 4 WIs juntos cobrem deliverables S03-D1..D4 (4/4) com partial gap em PAT signing key rotation (R-S03-9 sprint contract; addressed em WI-005 esperado). **Sprint contract S-03 Part 1 ON TRACK**, with 4 P0 fixes mandatory + 14 P1 recommended + 8 P2 deferrable. Sprint should not seal sem Part 2 review (WIs 005-008) — concorrent track work.

**Recommendation: WI authors apply P0 fixes (≤ 2 dias work paralelo dos 4 WIs); R4 re-review post-fix; if clean → GO seal Part 1 + proceed Part 2 audit.**

---

**Fim audit.** Reviewer: Agent R4 (Claude Opus 4.7, 1M context). Independent; sem viés ao autor. Spec final → código → produção. Auth = highest blast radius; criticality enforced.
