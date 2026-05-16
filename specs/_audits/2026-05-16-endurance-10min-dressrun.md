---
id: AUDIT-W25-ENDURANCE-10MIN-DRESSRUN
type: audit
doc_status: ACTIVE
audit_status: ACTIVE
version: 1.0.0
created: 2026-05-16
reviewers: []
supersedes: null
superseded_by: null
tags: [audit, wave-25, endurance, dress-run, harness, analyzer, greenlight, ga-evidence]
---

# Wave-25 Endurance 10-min Dress-Run — Harness + Analyzer Validation

## 1. Escopo

Esta auditoria documenta o **wave-25 dress-run** do harness 24h de
endurance criado na wave-22 (`RB-24H-ENDURANCE-LOAD`). A wave-25 NÃO
executa uma campanha 24h real (operacional / pré-GA T-3); o objetivo é
exercitar a tubulação completa do harness — k6 → summary → analyzer →
verdict — em uma janela de 10 minutos com um alvo *in-memory mock* local
e validar que o caminho de **regression-flip** do analisador detecta
corretamente uma piora sintética de p99.

Companion artefatos:
- Runbook: `specs/_runbooks/RB-24H-ENDURANCE-LOAD.md` (§ dress-run history)
- k6 script: `tests/load/k6/scenarios/endurance-24h-w22.js` (perfil `10min`)
- Runner: `scripts/run_24h_endurance.sh` (modo `dressrun`)
- Analisador: `scripts/analyze_endurance_run.py`
- Mock target: `scripts/_dressrun_mock_target.py`
- Resultados:
  - `tests/load/results/20260516T083953Z-dressrun/` — corrida real 10min
  - `tests/load/results/synthetic-greenlight/` — verdict GREENLIGHT happy-path
  - `tests/load/results/synthetic-regression-5x/` — verdict BLOCK regression-flip

## 2. Alterações na harness wave-22

### 2.1 Novo perfil `10min`

`tests/load/k6/scenarios/endurance-24h-w22.js` agora suporta
`DURATION=10min` (também acessível via `K6_PROFILE=10min` /
`K6_PROFILE=dressrun`). O perfil emite:

| Stage      | Target RPS | Duração |
|------------|-----------:|--------:|
| ramp-up    | 0 → 100    |    2min |
| sustain    | 100        |    6min |
| ramp-down  | 100 → 0    |    2min |

O memory sidecar passa a fazer poll a cada 30s (vs 5min do 24h /
5min do 2h) — ~20 amostras de RSS/CPU no janela de 10 min. O perfil
**não** está sujeito ao interlock `K6_ENDURANCE_CONFIRM=yes` (a
duração total não cai na lista `LONG_DURATIONS`), porque é explicitamente
um dress-run barato.

### 2.2 Novo modo `dressrun` no runner

`scripts/run_24h_endurance.sh` ganhou o subcomando `dressrun`:

```bash
scripts/run_24h_endurance.sh dressrun
```

Por padrão direciona contra `http://127.0.0.1:8787` com a PAT stub. O
preflight tolera *target unreachable* (mesma semântica do `smoke`) e
ainda invoca o analisador no path de erro para que o verdict UNKNOWN seja
exercitado.

### 2.3 Mock target in-memory

`scripts/_dressrun_mock_target.py` é um servidor HTTP Python
(ThreadingHTTPServer, zero dependências) que responde 200/202 nas seis
rotas customer-facing wave-22 + `/healthz` +
`/v1/admin/diagnostics/memory`. **NÃO substitui o binário
`apps/server` em modo InMemory** — existe apenas para permitir o
dress-run em dev box sem provisionar a stack completa. A audit-trail
marca explicitamente que o verdict obtido contra esse mock é
*informativo de wiring* apenas.

### 2.4 Hardenings no analisador (`scripts/analyze_endurance_run.py`)

Dois gaps foram descobertos durante o dress-run e corrigidos:

1. **p99 absent**: o `--summary-export` do k6 não inclui `p(99)`
   numérico nos Trend metrics por padrão (somente `avg/min/med/max/
   p(90)/p(95)`). A versão wave-22 do analisador caía no path "p99
   missing from metric payload" e **silenciosamente mantinha GREEN**.
   Correção: novo helper `_extract_p99()` lê o valor numérico se
   presente, caso contrário inspeciona o mapa `thresholds` da própria
   métrica (boolean `p(99)<NNN`).
2. **stale threshold booleans**: o mapa `thresholds` no summary-export
   carrega o **último estado** avaliado durante a corrida, que pode ser
   `True` mesmo quando o k6 finalizou reportando `thresholds … have been
   crossed` no stderr. Correção: novo helper
   `_stdout_threshold_breach_routes()` parseia `k6-stdout.log` para
   extrair a linha terminal e *override* o boolean stale com RED quando
   k6 reporta breach.

Esses hardenings são compatíveis com summaries antigos (sem stdout-log
ou com p99 numérico explícito) — o behavior original é o fallback.

## 3. Execução do dress-run

Comando:

```bash
python3 scripts/_dressrun_mock_target.py --port 8787 &
MOCK_PID=$!
bash scripts/run_24h_endurance.sh dressrun
kill $MOCK_PID
```

### 3.1 Resultados — corrida real

Diretório: `tests/load/results/20260516T083953Z-dressrun/`

| Métrica                                | Valor              |
|----------------------------------------|--------------------|
| Duração                                | 10m 5s             |
| Iterações completas                    | 45 109             |
| Iterações dropped (VU saturation)      | 2 911              |
| RPS efetivo (sustained)                | ~75/s              |
| http_req_failed                        | 0.04% (22/45109)   |
| route_errors counter                   | 22                 |
| memory_rss_bytes (mock)                | 268 MiB (constant) |
| cpu_user_pct (mock)                    | 12.5%              |

Per-route p95 observado (k6 stdout):

| Rota                                  | p95     |
|---------------------------------------|---------|
| GET /v1/audit/analytics/event-count   | 1.41s   |
| GET /v1/audit/analytics/timeline      | 1.39s   |
| POST /v1/audit/export                 | 1.44s   |
| POST /v1/cas/upload                   | 51.68s  |
| POST /v1/dsr/erasure                  | 1.42s   |
| POST /v1/clerk/auth                   | 1.4s    |

**Caveat** importante: o p99 alto NÃO reflete performance do `apps/
server` — o mock Python single-process saturou VUs no payload de 1 MiB
para `cas/upload` (até ~440 VUs simultâneos), o que pressionou o
schedule global do k6 e empurrou p99 de TODAS as rotas para cima. Para
o dress-run isso é positivo: gerou breach real → exercita o caminho
RED do analisador.

k6 reportou na stderr final:

```
thresholds on metrics
'route_latency{route:GET /v1/audit/analytics/event-count},
 route_latency{route:GET /v1/audit/analytics/timeline},
 route_latency{route:POST /v1/audit/export},
 route_latency{route:POST /v1/cas/upload},
 route_latency{route:POST /v1/clerk/auth},
 route_latency{route:POST /v1/dsr/erasure}'
have been crossed
```

### 3.2 Verdict do analisador — corrida real

Após popular as gates manuais G2–G6 com valores green (operator action
documentada em `run-meta.json`):

```
$ python3 scripts/analyze_endurance_run.py \
    --run-dir tests/load/results/20260516T083953Z-dressrun
[analyze_endurance_run] verdict=BLOCK -> .../analysis.md
exit=10
```

Todas as 6 rotas marcadas como RED por G1 via stdout-tail. **CI fail
closed** confirmado (exit code 10).

## 4. Validação do GREENLIGHT path

Para confirmar que o analisador EMITE GREENLIGHT quando os critérios
batem, foi criado um artefato sintético em
`tests/load/results/synthetic-greenlight/` com:

- per-route p99 numérico abaixo do floor (event-count=187.5ms,
  timeline=311.2ms, export=902.7ms, cas-upload=547.2ms,
  dsr-erasure=612.8ms, clerk-auth=188.0ms) — todos sub-floor de §3 do
  runbook
- todas as G2–G6 populadas como green
  (audit_chain_delta=0, sev_incidents=0, pilot_acks=7,
   neon_lag=42s, dsr_pct=100.0)

```
$ python3 scripts/analyze_endurance_run.py \
    --run-dir tests/load/results/synthetic-greenlight
[analyze_endurance_run] verdict=GREENLIGHT -> .../analysis.md
exit=0
```

**GREENLIGHT path validado.**

## 5. Validação do regression-flip path (5x latência)

A partir do mesmo summary GREENLIGHT, foi gerado um segundo artefato
sintético em `tests/load/results/synthetic-regression-5x/`
multiplicando cada estatística de `route_latency` por 5.0 (regressão
uniforme). As gates manuais G2–G6 foram MANTIDAS green para isolar a
flip a G1 sozinho.

p99 após injeção (5x):

| Rota                                  | p99 5x   | Floor   | Esperado |
|---------------------------------------|----------|---------|----------|
| GET /v1/audit/analytics/event-count   | 937.5ms  | 400ms   | RED      |
| GET /v1/audit/analytics/timeline      | 1556.0ms | 600ms   | RED      |
| POST /v1/audit/export                 | 4513.5ms | 1500ms  | RED      |
| POST /v1/cas/upload                   | 2736.0ms | 800ms   | RED      |
| POST /v1/dsr/erasure                  | 3064.0ms | 1000ms  | RED      |
| POST /v1/clerk/auth                   | 940.0ms  | 300ms   | RED      |

```
$ python3 scripts/analyze_endurance_run.py \
    --run-dir tests/load/results/synthetic-regression-5x
[analyze_endurance_run] verdict=BLOCK -> .../analysis.md
exit=10
```

**Regression-flip path validado.** Single-cause regression (G1
sozinho) flipa BLOCK; outras 5 gates permanecem GREEN no relatório
markdown.

## 6. SLO observado per-route (mock target, dress-run)

Para referência futura (NÃO é evidência operacional — apenas baseline
do mock):

| Rota                                  | p50    | p95    | Notes                              |
|---------------------------------------|--------|--------|------------------------------------|
| GET /v1/audit/analytics/event-count   | 65ms   | 1.41s  | bound pelo schedule global do k6   |
| GET /v1/audit/analytics/timeline      | 63ms   | 1.39s  | bound idem                         |
| POST /v1/audit/export                 | 59ms   | 1.44s  | mock responde 202 imediato         |
| POST /v1/cas/upload                   | 371ms  | 51.68s | **VU saturation 1 MiB payload**    |
| POST /v1/dsr/erasure                  | 65ms   | 1.42s  | mock responde 202 imediato         |
| POST /v1/clerk/auth                   | 64.5ms | 1.4s   | mock responde 200 imediato         |

A medição mostra que rodar o dress-run contra o mock Python é
inadequado para *avaliar performance real* — para isso, na cadência
nightly/full, o target deve ser `apps/server` em modo InMemory (cargo
run --bin corelink-server) OU o staging endpoint. O dress-run aqui é
apenas wiring-validation.

## 7. Quality gates wave-25

| Check                                            | Estado |
|--------------------------------------------------|--------|
| 10-min dress-run completes (606s elapsed)        | PASS   |
| Analyzer emits verdict (BLOCK p/ real run)       | PASS   |
| Synthetic GREENLIGHT verdict path                | PASS   |
| Synthetic regression-flip detection (BLOCK)      | PASS   |
| `validate_specs.py`                              | (run)  |
| `validate_references.py`                         | (run)  |
| DCO sign-off + Co-Authored-By                    | PASS   |

## 8. Findings / debt levantado pelo dress-run

| # | Severity | Finding                                                                                                                |
|---|----------|------------------------------------------------------------------------------------------------------------------------|
| 1 | LOW      | k6 `--summary-export` não emite `p(99)` numérico em Trend metrics. Mitigado pelo helper `_extract_p99()` no analisador. |
| 2 | LOW      | Mapa `thresholds` no summary-export pode conter booleans intermediários stale. Mitigado pelo `_stdout_threshold_breach_routes()`. |
| 3 | MEDIUM   | Mock Python single-process satura em payloads grandes; nightly/full devem usar `apps/server` ou staging.               |
| 4 | LOW      | Runner não suporta target reachable mas k6 ausente em `dressrun` — segue o path de smoke (smoke-status.txt). Aceitável.  |
| 5 | INFO     | Documentar o perfil 10-min no runbook §2 como variant `dressrun` (feito nesta mesma wave — §dress-run-history).         |

## 9. Próximos passos

- **Wave-26+**: pré-GA T-3, executar `scripts/run_24h_endurance.sh
  full` contra staging com `K6_TARGET_HOST=https://staging.corelink.humangr.com`.
  O analisador retornará GREENLIGHT desde que p99 per-route fique abaixo
  dos floors em `RB-24H-ENDURANCE-LOAD.md` §3 E o operador popule
  G2–G6 com os snapshots de dashboard.
- **CI nightly**: validar que `validate_references.py` enxerga este
  audit doc nas referências do runbook (linha §8).
- **Não criar** ticket de regressão — os hardenings do analisador
  foram aplicados nesta wave.

## 10. References

- `specs/_runbooks/RB-24H-ENDURANCE-LOAD.md`
- `specs/_runbooks/RB-GA-CUTOVER.md`
- `specs/_audits/2026-05-16-24h-endurance-harness.md`
- `tests/load/k6/scenarios/endurance-24h-w22.js`
- `tests/load/fixtures/customer-routes.ndjson`
- `scripts/run_24h_endurance.sh`
- `scripts/analyze_endurance_run.py`
- `scripts/_dressrun_mock_target.py`

## 11. Change log

| Version | Date       | Author              | Change                                            |
|---------|------------|---------------------|---------------------------------------------------|
| 1.0.0   | 2026-05-16 | Gustavo Schneiter   | Initial wave-25 dress-run audit + analyzer fixes. |
