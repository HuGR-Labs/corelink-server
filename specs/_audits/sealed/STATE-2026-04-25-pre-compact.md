# Estado pré-compact — 2026-04-25

> **Não comitar este arquivo.** É continuity scratch pra retomar após compaction.

## Onde estamos

CoreLink em modo **spec-first SOTA** (sem código ainda). Princípio: "todas as 134 WIs especificadas profissionalmente em SOTA, sem exceção, completo e impecável" antes de qualquer implementação.

Pace acordado: ~5-6 WIs por sessão. Não é corrida — é "esculpindo an engineering marvel".

## Progresso de specs

### Concluído
- ✅ Lotes 1-9.5 (framework v1.0.0-rc1) — 8 lotes + 4 audits adversariais
- ✅ 21 sprint contracts (S-00 a S-20) full SOTA — Lote 9.1
- ✅ 5 validators CI-ready (validate_specs jsonschema, validate_references, check_tla_obligations, check_error_taxonomy, check_cost_regression) — Lote 9.5c
- ✅ **Lote 10.1 — S-01 full WI spec (7 WIs HIGH_RISK CAS write path)** — committed
  - WI-S01-001-tenant-path-hmac (453 lines, baseline)
  - WI-S01-002-blake3-verify-at-write (491 lines)
  - WI-S01-003-r2-adapter-single-blob (464 lines)
  - WI-S01-004-d1-schema-blob-meta (373 lines)
  - WI-S01-005-reapi-batchupdateblobs (373 lines)
  - WI-S01-006-property-tests-10k (382 lines)
  - WI-S01-007-ci-tlc-gate-sbom (417 lines)
  - Total: 2953 lines
- ✅ **Lote 10.2 — S-02 full WI spec (6 WIs HIGH_RISK CAS read path)** — committed
  - WI-S02-001-bytestream-read (528 lines, baseline)
  - WI-S02-002-getblob-findmissing (439 lines)
  - WI-S02-003-corelink-client-verify-crate (454 lines)
  - WI-S02-004-constant-time-middleware (455 lines, references ADR-0023)
  - WI-S02-005-negative-cache-kv (437 lines)
  - WI-S02-006-property-tests-rb-prr (424 lines, ship-gate)
  - Total: 2737 lines

### Pendente
- ⏳ **Lote 10.3 — S-03 full WI spec** (auth real Clerk + PAT + WebAuthn — ~8 WIs HIGH_RISK)
- ⏳ Lotes 10.4-10.21 — sprints S-00, S-04 a S-20 (≈ 120 WIs)
- ⏳ Estimate: ~20-22 sessões restantes a 5-6 WIs cada

## Codex backgrounds em curso

### codex r4-S01 (Lote 10.1 review) — DONE com problema
- **Status**: terminou (exit 0) mas **NÃO escreveu o arquivo final** `specs/_audits/sealed/2026-04-25-codex-r4-s01-wi-review.md`
- **Log**: `/tmp/codex-r4-s01/gpt.log` (4846 linhas — codex leu specs mas não fechou o relatório)
- **Action item após retomar**: re-disparar com prompt mais enxuto, ou ler log e extrair findings manualmente
- **Prompt original**: `/tmp/codex-r4-s01/prompt.md`

### codex r4-S02 (Lote 10.2 review) — DONE com mesmo problema
- **Status**: terminou (exit 0) mas **NÃO escreveu o arquivo final** `specs/_audits/sealed/2026-04-25-codex-r4-s02-wi-review.md`
- **Log**: `/tmp/codex-r4-s02/gpt.log` (697 linhas — leu specs mas não fechou síntese)
- **Action item após retomar**: igual S-01 — re-disparar com prompt mais enxuto OU mudar pra `claude-code-guide`/agente diferente OU extrair manualmente do log

### Hipótese sobre os 2 codex falharem
Ambos consumiram contexto lendo ~5-7 WIs grandes (400-500 linhas cada) + auth_model + storage_semantics_matrix antes de chegar no relatório. Provável context exhaustion ou timeout da sandbox `codex exec`. Próxima vez:
- Reduzir scope da review (1 codex por WI, não 1 codex por lote)
- OU dar instrução mais explícita "WRITE FILE FIRST then iterate"
- OU usar Agent tool (Plan/Explore subagent) em vez de codex exec

## Validators state (todos green até último commit)

```bash
cd /Users/gustavoschneiter/Documents/HuGR/corelink-server
python3 scripts/validate_references.py        # green
python3 scripts/validate_specs.py             # green (jsonschema instalado via --break-system-packages)
python3 scripts/check_tla_obligations.py      # green
python3 scripts/check_error_taxonomy.py       # green
python3 scripts/check_cost_regression.py      # green
```

ADRs whitelisted (forward-looking):
- ADR-0021: S-04 — token storage hashing strategy
- ADR-0022: S-05 — chunk size vs multipart part size decoupling
- ADR-0023: S-02 WI-S02-004 — constant-time defense via timing padding middleware

## Padrão WI HIGH_RISK adotado

32 seções, ~400-500 linhas cada:
1-7: Intent → Narrativa (≥300 palavras) → Customer Impact → CAP → Escopo → Anti-Scope → Gherkin (8-9)
8-14: Design Decisions → Completeness Criteria SOTA → DoD → Invariantes → Artifacts → Quality 14.X.10 → Chaos
15-22: PRR → Sub-tasks PERT → Dependencies → Effort → Time-boxing → Observability → Cost → API Contract
23-32: Post-mortem Hooks → Rollback → Security STRIDE+LINDDUN → Knowledge Transfer → Risk Register 6-col → Review Checkpoints → Sign-off → Change Log → Anti-patterns

## Scope do projeto (recap)

- **CoreLink** = multi-tenant content-addressable cache (CAS)
- **Stack**: Cloudflare Workers + Rust/WASM + R2 + D1 + KV + Durable Objects
- **API**: REAPI v2 (Bazel Remote Execution API)
- **Customer zero**: Forge
- **Org GitHub**: humangr-labs
- **Naming**: decidido 2026-04-23

## Próxima ação ao retomar

1. Conferir output do codex r4-S02 (background b15276mlg). Se produziu `2026-04-25-codex-r4-s02-wi-review.md`, ler findings e remediar P0/P1.
2. Decidir sobre codex r4-S01 (re-disparar ou skipar — log tem material útil mas sem síntese final).
3. Se reviews OK, partir pro **Lote 10.3 — S-03 WIs auth real** (~8 WIs HIGH_RISK — Clerk integration + PAT lifecycle + WebAuthn L3 + token introspection + revocation propagation + scope enforcement + rate limit per-principal + audit emission).
