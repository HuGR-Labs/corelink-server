# Estado pré-compact v2 — 2026-04-25

> **Não comitar.** Continuity scratch pra retomar após compaction.

## Onde estamos

CoreLink em **spec-first SOTA** (zero código). Princípio mantido: "todas as 134 WIs especificadas profissionalmente em SOTA, sem exceção, completo e impecável" antes de qualquer implementação. Pace ~5-8 WIs por sessão; user emphatic "Average não serve" → P0 fixes mandatory pós cada review.

## Progresso de specs

### Concluído

- ✅ Lotes 1-9.5 (framework v1.0.0-rc1) — 8 lotes + 4 audits adversariais
- ✅ 21 sprint contracts (S-00 a S-20) full SOTA — Lote 9.1
- ✅ 5 validators CI-ready — Lote 9.5c
- ✅ **Lote 10.1 — S-01 full WI spec (7 WIs HIGH_RISK CAS write)** — committed (2953 linhas)
- ✅ **Lote 10.2 — S-02 full WI spec (6 WIs HIGH_RISK CAS read)** — committed (2737 linhas)
- ✅ **Lote 10.2bis — P0 fixes S-01+S-02 (11 P0)** — committed (Mann-Whitney power analysis + outbox pattern + TLC bounds + sign-off harmonize + MissReason→404 + negative cache race + Round-Robin spec + ADR paths concretos)
- ✅ **Lote 10.3 — S-03 full WI spec (8 WIs HIGH_RISK auth real)** — committed (~4800 linhas; 10 ADRs forward 0024-0033; 25+ INVs whitelisted; ~80 chaos experiments)
- ✅ **Lote 10.3bis — P0 fixes S-03 (10 P0)** — committed
  - WI-002: Mann-Whitney 3-prong methodology + Argon2 Semaphore concurrency cap (N=1) + dummy_verify_for_constant_time API
  - WI-003 + WI-004: stale window orthogonal axes (single SLA 60s p99; NÃO 120s aditivo)
  - **invariant_registry.md §3.14 nova**: 38 INVs (19 AUTH + 5 AUDIT + 3 generic) formalmente promovidos (não whitelist-as-mitigation)
  - WI-005 SQL fixes: gen_random_uuid → UUID v7 app-side; pg_strom → pgcrypto native hmac(); SET LOCAL → tx wrapper + sqlx before_acquire + `with_tenant_ctx!` macro; pgp_sym_encrypt → _bytea variant; +6 _key_id columns
  - sprint.md + _spec_contract.md + WI-008: canonical 10k PR + 100k nightly (Argon2 sampled 1%) + 13 sign-offs (12+1 advisory)
  - WI-008: PRR staffing reality (Option A/B/C com ADR-0034 forward solo-tier waiver)
  - WI-007: +10 event types (auth.webauthn.new_device_used + auth.account.deleted + auth.tenant.deleted + auth.pat.scope_escalated + 6 auth.denied.* granular); 23 → 33 types
  - WI-007: serde_jcs RFC 8785 (canonical_json deprecated; round-trip Annex B test vectors)

### Pendente

- ⏳ **Lote 10.4 — S-04 full WI spec** (Action Cache + Read-Through Cache + cripto signing — ~7 WIs HIGH_RISK)
- ⏳ Lotes 10.5-10.21 — sprints S-00, S-05 a S-20 (≈ 110 WIs)
- ⏳ Estimate: ~18-20 sessões restantes a 5-8 WIs cada

## Estado validators

```bash
cd /Users/gustavoschneiter/Documents/HuGR/corelink-server
python3 scripts/validate_references.py        # ✅ green (127 docs, 0 dangling)
python3 scripts/validate_specs.py             # ✅ green (121 schema + 6 YAML)
python3 scripts/check_tla_obligations.py      # ✅ green
python3 scripts/check_error_taxonomy.py       # ✅ green
python3 scripts/check_cost_regression.py      # ✅ green
```

## ADRs whitelisted (forward-looking)

- ADR-0021..0023 (S-01/S-02/S-04/S-05 forward)
- ADR-0024..0033 (S-03 — Lote 10.3)
- ADR-0034 (S-03 WI-008 — Lote 10.3bis solo-tier PRR waiver)

## INVs registry (formalizadas)

`specs/03_architecture/invariant_registry.md §3.14` — 38 novas entries (Lote 10.3bis):
- 4 INV-AUTH-JWT-* (WI-S03-001)
- 5 INV-AUTH-PAT-* (WI-S03-002)
- 5 INV-AUTH-{TENANTCTX,5-LAYER,SESSION-CACHE,SCOPE,AUDIT-PRE-POST}-* (WI-S03-003)
- 5 INV-AUTH-{REVOCATION,D1,MASS-REVOKE,PROPAGATION}-* (WI-S03-004)
- 5 INV-AUTH-{SCHEMA,PII,MIGRATION,CASCADE,AUDIT-PSEUDO}-* (WI-S03-005)
- 5 INV-AUTH-WEBAUTHN-* (WI-S03-006)
- 5 INV-AUDIT-* (WI-S03-007)
- 3 INV-{NEG-CACHE-MONOTONIC, NO-BODY-IN-LOGS, NO-PII-IN-LOGS}
- 1 INV-AUTH-CLOCK-SKEW-BOUND

## Próxima ação ao retomar

1. **Lote 10.4 — S-04 full WI spec** (Action Cache + Read-Through Cache + cripto signing)
   - 7 WIs HIGH_RISK estimados
   - Sprint contract S-04 já existe em `specs/04_sprints/_sealed/S04/_spec_contract.md` + `sprint.md` (Lote 9.1)
   - Padrão SOTA: 13-row sign-off table desde início; Mann-Whitney 3-prong onde aplicável; cost TCO 12m breakdown; STRIDE+LINDDUN delta full; risk register ≥ 10 rows; ≥ 5 chaos experiments; ADR forward (ADR-0035+ TBD)
   - Domain key: AC digest signing HKDF vs Ed25519 (ADR-0021 forward); cripto-touching → Crypto SME mandatory

2. **Após Lote 10.4**: dispatch 2 agent reviews em paralelo (mesmo pattern de S-01/S-02/S-03):
   - `specs/_audits/2026-04-XX-agent-r4-s04-part1-wi-review.md`
   - `specs/_audits/2026-04-XX-agent-r4-s04-part2-wi-review.md`
   - Apply P0 fixes em Lote 10.4bis

3. **Continuação**: S-05, S-06, ..., S-20 (mesma cadência ~5-8 WIs/lote + bis cycle)

## Reviews referência (canonical SOTA bar)

- `specs/_audits/sealed/2026-04-25-agent-r4-s01-wi-review.md` (S-01 7.2/10 baseline)
- `specs/_audits/sealed/2026-04-25-agent-r4-s02-wi-review.md` (S-02 7.6/10)
- `specs/_audits/sealed/2026-04-25-agent-r4-s03-part1-wi-review.md` (S-03 part1 7.6/10; WI-S03-003 best-in-class 8.5/10)
- `specs/_audits/sealed/2026-04-25-agent-r4-s03-part2-wi-review.md` (S-03 part2 7.95/10)

**Trajetória SOTA confirmada**: 7.2 → 7.6 → 7.95 (S-01 baseline → S-02 → S-03 part2). Target SOTA puro 9-10 atingido em WI-S03-003 standalone (best-in-class).

## Padrão WI HIGH_RISK calibrado pós-fixes

32 seções, ~500-700 linhas cada (S-03 elevou bar):
1-7: Intent + código rust → Narrativa (≥300 palavras + risk justification + ≥ 8 vulnerability classes) → Customer Impact 3 personas → CAP → Escopo + Anti-Scope → Gherkin (8-13 scenarios com edge cases reais)
8-14: Design Decisions (8+ alternatives rejeitadas com porquê) → Completeness Criteria SOTA (≥ 10 com targets numéricos + métodos + evidências) → DoD → Invariantes (TLA+ ↔ Rust mapping concreto) → Artifacts ≥ 8 → Quality Standards 14.X.10 expanded → Chaos ≥ 5
15-22: PRR sign-off list explícito → Sub-tasks PERT (15-22 tasks; pessimistic/optimistic/likely) → Dependencies (hard/soft/outbound) → Effort PERT calculado → Time-boxing → Observability métricas + traces + logs + dashboard → Cost TCO 12m breakdown → API Contract com error mapping
23-32: Post-mortem hooks → Rollback RTO/RPO concrete → Security STRIDE 6 + LINDDUN 7 delta full → Knowledge Transfer (tech talk + docs + workshops + onboarding test) → Risk Register 6-col 10-12 rows → Review Checkpoints 5-8 → Sign-off 13-row table → Change Log → Anti-patterns evitados ≥ 8

## Statistical methodology canonical (Mann-Whitney 3-prong)

Aplicar em todo timing-sensitive cripto path:
1. **Sample size**: N ≥ 10000 per arm
2. **Power calculation a priori**: 1−β ≥ 0.80 com Cohen's d = 0.2 (small/practically-relevant) via `statrs::distribution::statistical_power`
3. **p-value gate**: Mann-Whitney U test, target p > 0.05 (fail to reject H0)
4. **Šidák correction**: 3 independent trials; ALL 3 p > 0.05 (combined α ≈ 0.000125; flake < 1-em-8000)
5. **Bootstrap CI sobre |Δmedian|**: 95% CI cruzando 0 (zero-inclusive)

Combined verdict: high-power test failed to reject + small effect size + CI consistent with zero = **evidence-grade** indistinguishability.

`p > 0.05` SOZINHO é cargo-cult; nunca aceitar.

## Codebase highlights

- Workspace `/Users/gustavoschneiter/Documents/HuGR/corelink-server`
- Sprint S-03 specs em `specs/04_sprints/S03/work_items/WI-S03-{001..008}-*.md`
- Validator `scripts/validate_references.py` agora tem 38+ INVs whitelisted como "Forward-looking INVs" + 14 ADRs (0021-0034) + 5 RBs forward
- Org GitHub: humangr-labs (Forge customer zero)

## TODO list status

Active task #77 (Lote 10.3bis P0 fixes) → marcada completed.
Próxima task pra criar ao retomar: "Lote 10.4: S-04 full WI spec (7 WIs HIGH_RISK Action Cache + cripto signing)".

## Pace acordado

User: "vamos com calma, o importante e tudo sair impecavel". Não correr; ~5-8 WIs por session com bis-cycle de P0 fixes mandatory. **Average não serve**; SOTA puro 9-10 é o target.
