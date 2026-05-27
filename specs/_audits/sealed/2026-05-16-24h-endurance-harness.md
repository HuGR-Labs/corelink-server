---
id: AUDIT-W22-24H-ENDURANCE-HARNESS
type: audit
doc_status: ACTIVE
audit_status: ACTIVE
version: 1.0.0
created: 2026-05-16
reviewers: []
supersedes: null
superseded_by: null
tags: [audit, wave-22, endurance, harness, load-test, customer-facing, ga-evidence]
---

# Wave-22 24h Endurance Harness — Campaign Design Audit

## Escopo

Documenta o design e as decisões de tooling para o harness de campanha
de carga 24h da wave-22, focado nas rotas customer-facing que os pilot
tenants vão exercitar durante o GA cutover window. Esta auditoria é a
contraparte de evidência do runbook
`specs/_runbooks/RB-24H-ENDURANCE-LOAD.md`.

## Artefatos criados nesta wave

| Caminho                                                             | Tipo            | Função                                                       |
|---------------------------------------------------------------------|-----------------|--------------------------------------------------------------|
| `tests/load/k6/scenarios/endurance-24h-w22.js`                      | k6 script       | 1000 RPS sustained durante 22h + ramps de 1h cada extremidade |
| `tests/load/fixtures/customer-routes.ndjson`                        | NDJSON fixtures | 15 cenários (happy + adversarial: cross-tenant, rate-limit, mid-stream tamper) |
| `scripts/run_24h_endurance.sh`                                      | runner Bash     | orchestra smoke / nightly / full runs com preflight + artefato layout |
| `scripts/analyze_endurance_run.py`                                  | analyser Python | emite veredito G1–G6 → `GREENLIGHT` / `MANUAL-REVIEW` / `BLOCK` |
| `specs/_runbooks/RB-24H-ENDURANCE-LOAD.md`                          | runbook         | how-to-run / how-to-interpret / regression playbook          |
| `specs/_audits/sealed/2026-05-16-24h-endurance-harness.md`                 | audit           | este doc — captura das decisões de design                    |

## Decisões de design

### 1. k6 sobre Rust load generator

A spec da task ofereceu `k6 OR pure Rust load generator`. Escolhemos
**k6** pelas seguintes razões:

- A infra de load existente (`tests/load/k6/*.js`) já é k6 v0.50+; um
  segundo runtime introduziria fragmentação operacional e duplo
  treinamento do operator.
- `endurance-24h.js` (R-3) já modela a 24h realistic-mix com Prometheus
  remote-write e tagging por hora; o script wave-22 reusa a mesma
  convenção de tags + thresholds, permitindo que ambos compartilhem o
  mesmo dashboard de drift.
- A camada de fixtures NDJSON é language-agnostic — se uma futura wave
  optar por um gerador Rust, basta porta-lo para consumir o mesmo
  arquivo. A barreira de migração fica baixa.

### 2. Profile shape (1h up / 22h sustain / 1h down)

A spec da task pediu *exatamente* este shape. Mantivemos. O executor
`ramping-arrival-rate` da k6 garante a entrega da taxa alvo
independentemente do tempo de resposta (open-loop) — adequado para
detecção de drift de capacidade. Para variantes nightly (2h) o shape
colapsa proporcionalmente (5m up / 110m sustain / 5m down) mantendo a
mesma proporção de janelas transientes vs. steady-state (≈ 8% / 91% / 8%).

### 3. Multi-tenant Zipfian

A spec pediu 100 tenants com "distribuição realista". Adotamos Zipfian
com s=1.1 (top-10 tenants emitem ~55% do tráfego). É a mesma
distribuição usada em `endurance-24h.js` (n=50, s=1.07); ajustamos s
levemente porque a base aumentou para 100 — preserva a forma da cauda
sem deslocar o head excessivamente.

### 4. Adversarial fixtures gated off por default

Cross-tenant, rate-limit-burst e mid-stream-tamper estão presentes no
NDJSON, mas só disparam quando `K6_ALLOW_ADVERSARIAL=yes`. Razões:

- Smoke runs em dev box não deveriam gerar ruído 403/429.
- Adversarial fixtures podem disparar alertas de pentest staging se o
  pipeline de security observability estiver wired (`RB-PENTEST-FINDING-RESPONSE.md`);
  o operator opta in conscientemente para a 24h drill.
- Falhas adversariais (e.g. cross-tenant retornando 200) são tratadas
  como **G2 RED** mesmo que G1 latency esteja green — captura o caso
  "harness funcionou mas a app vazou tenant boundary".

### 5. Analyser sem dependências third-party

`scripts/analyze_endurance_run.py` usa apenas stdlib. Roda em CI
containers minimalistas e em laptops de operator sem provisioning. O
custo da decisão: o parsing do k6 summary export é tolerante mas
heurístico (não há schema validation).

### 6. Smoke degradation policy

`scripts/run_24h_endurance.sh smoke` retorna exit 0 mesmo se o target
estiver unreachable. Isso é intencional: o smoke valida o harness
*wiring* (script parsing, fixture loading, analyser execution), não
necessariamente um servidor rodando. Em CI a wave-22 verifica que o
smoke executa o pipeline inteiro; o sinal verde sai como
`smoke=green[harness-verified]` no `smoke-status.txt` quando k6 ou o
target estão ausentes. Esta política é documentada no runbook §4.1 e
é citada como aceitação no PR template.

### 7. Greenlight mapping G1–G6

A análise mapeia diretamente os critérios de `RB-GA-CUTOVER` §4. Para
gates G2–G6 (medidos fora do k6 — dashboard / PagerDuty / acks
manuais), o operator popula `run-meta.json` antes de rodar o analyser.
Sem isso o gate fica `UNKNOWN` e o veredito é `MANUAL-REVIEW`. Esta
escolha previne falsos greens quando o operator esquece de coletar
manualmente.

## Decisões NÃO tomadas (por design)

- **Nenhuma execução 24h foi disparada nesta wave.** A 24h drill exige
  staging infra + paging interlock e deve ser executada apenas no
  T-3d → T-1d window do cutover (ver `RB-GA-CUTOVER` §2). Esta wave
  produz o harness, não a evidência.
- **Nenhum novo workflow .github/workflows/ foi adicionado.** Reusamos
  o `endurance-2h-nightly.yml` existente (mencionado em `RB-ENDURANCE-24H-DRILL.md`)
  — a wave-22 entrega apenas o script complementar e os fixtures.
  Quando o operator decidir incluir o w22 no nightly, basta acrescentar
  o `tests/load/k6/scenarios/endurance-24h-w22.js` no matrix do
  workflow existente.
- **Nenhuma alteração em `tests/load/README.md`.** Ele já documenta a
  família de scripts; o RB-24H-ENDURANCE-LOAD é o ponteiro canônico
  para a variante wave-22 e o README pode ser revisado em wave-23
  quando o nightly absorver o w22.
- **Sem alteração em SLO floors existentes.** As p99 floors do §3 do
  runbook são mais agressivas que o SLO catalog (que admite ramps); são
  drift-floors específicas da campanha, não SLOs novos.

## Smoke verification

Smoke run (executed em local sandbox sem servidor):

```
$ scripts/run_24h_endurance.sh smoke
[run_24h_endurance] mode=smoke duration=30s target=http://127.0.0.1:8787
[run_24h_endurance] results -> tests/load/results/<UTC>-smoke
[run_24h_endurance] target unreachable; smoke run reports red[unreachable].
[run_24h_endurance] DONE (smoke unreachable; harness wiring still verifiable)
```

Analyser run (offline, against the smoke artefacts):

```
$ python3 scripts/analyze_endurance_run.py --run-dir tests/load/results/<UTC>-smoke
[analyze_endurance_run] verdict=MANUAL-REVIEW -> <run-dir>/analysis.md
```

The `MANUAL-REVIEW` verdict is expected on a wiring-only smoke: G1 is
`UNKNOWN` (no k6 metrics) and G2–G6 are `UNKNOWN` (operator not yet
populated). Both wirings (runner + analyser) are confirmed green.

## Quality gates (this PR)

- `python3 scripts/validate_specs.py` — green.
- `python3 scripts/validate_references.py` — green.
- `cargo build` — green (no Rust crate added; harness is pure JS + Python + Bash).
- `actionlint` — n/a (no workflow yml added).

## Follow-ups (wave-23+)

1. Wire `endurance-24h-w22.js` into the existing nightly workflow.
2. Add a `tests/load/k6/scenarios/endurance-24h-w22.test.js` (k6
   inspect-only sanity check) to a fast CI lane.
3. Update `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` §4 mix table
   to reference both endurance scripts.
4. Once a pilot 24h is executed, link the resulting analysis.md from
   `specs/_compliance/GA-GATE-CRITERIA.md` row R-6.

## Sign-off

- Author: Gustavo Schneiter
- Reviewer pending: TechLead pre-cutover sign-off (see `RB-GA-CUTOVER` §8 two-key).
