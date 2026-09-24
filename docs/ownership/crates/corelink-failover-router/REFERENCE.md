---
schema: corelink-ownership/1.1
document: reference
package: corelink-failover-router
manifest: crates/corelink-failover-router/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: failover-router-static-20260920
---

# corelink-failover-router — referência de ownership

Esta referência é limitada ao manifesto e à fonte estática fixada. Ela não demonstra Tower, Cloudflare, roteamento ao vivo, execução de probes, tráfego, deploy ou produção. OKF canônico verificado é apenas referência externa; não é duplicado, redefinido nem revalidado.

[Identidade](#r01) · [Fronteiras](#r02) · [Mapa](#r03) · [Contratos](#r04) · [Invariantes](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade e função

| Campo | Evidência estática |
|---|---|
| Package / manifesto | `corelink-failover-router` / `crates/corelink-failover-router/Cargo.toml` |
| Função observada | Núcleo de decisão para saúde regional, decisão de leitura/escrita, trait de probe e gate de failback. |
| Fonte lida | `src/{lib,health,probe,router,audit,error,failback}.rs` e `tests/{prop_failover,failback_outbox_drain}.rs` |
| Dependência CoreLink | `corelink-replica-worker`; dela vêm `Region`, `ResidencyGraph` e tipos de auditoria reexportados. |
| Implementações locais | Traits e fixtures em memória; o manifesto não declara dependência Tower ou Cloudflare. |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Fronteira | Responsabilidade observada | Limite falsificável |
|---|---|---|
| `health.rs` | Calcula triggers e `Healthy`/`Degraded` de um snapshot | Não produz `Down` em `evaluate` |
| `probe.rs` | Declara `HealthProbe` e fixtures de sinal/erro | Trait e fixture não demonstram probe executado |
| `router.rs` | Decide primary/replica, write mode e emite auditoria antes da decisão degradada | Não contém montagem HTTP, middleware ou resposta 503 |
| `failback.rs` | Define leitura abstrata de outbox e gate de reengajamento | Não consulta D1/storage concreto |
| `corelink-replica-worker` | Fornece grafo e taxonomia de auditoria | A topologia e implementação dependente ficam fora desta crate |

<a id="r03"></a>
## R03 — Mapa da implementação

| Arquivo | Papel observado |
|---|---|
| `lib.rs` | Expõe módulos, constantes, traits, decisões e reexports regionais/auditoria. |
| `health.rs` | Define estados, triggers, limites e `RegionHealthSnapshot::evaluate`. |
| `probe.rs` | Define `HealthProbe`, implementação em memória e fixture que falha. |
| `router.rs` | Define `ReadMode`, `WriteMode`, `FailoverDecision`, trait e router em memória. |
| `audit.rs` | Reexporta sink/tipos de auditoria da dependência e adiciona wrapper de contexto. |
| `error.rs` | Define a taxonomia `FailoverError`. |
| `failback.rs` | Define outbox/counter abstraídos e `assert_outbox_drained_or_block`. |

<a id="r04"></a>
## R04 — Contratos públicos observados

| Contrato | Entrada e resultado | Limite falsificável |
|---|---|---|
| `RegionHealthSnapshot::evaluate` | Região, taxa 5xx, p99, falhas consecutivas e timestamp retornam snapshot com triggers | Só os três triggers declarados entram no vetor atual |
| `HealthProbe::probe` | Região e timestamp retornam snapshot ou `String` de erro | Não define transporte, cadence executada ou scheduler |
| `FailoverRouter::route_read` | Tenant, primária e timestamp retornam `FailoverDecision` ou `FailoverError` | Não recebe request HTTP nem produz response HTTP |
| `FailoverRouter::write_mode` | Região retorna `Allowed` ou `Blocked` | Consulta o probe no router em memória; não persiste lease |
| `assert_outbox_drained_or_block` | Repositório, sink, counter, regiões e timestamp retornam `Ok` ou erro | Depende de traits; não executa query concreta |

<a id="r05"></a>
## R05 — Estado e invariantes

| Invariante falsificável | Evidência estática | Não prova |
|---|---|---|
| `evaluate` marca `Degraded` quando os três thresholds contribuem triggers; caso contrário marca `Healthy` | `health.rs`, corpo de `RegionHealthSnapshot::evaluate` | Janela sustentada ou amostragem reais |
| `InMemoryFailoverRouter::get_health` transforma erro de probe em snapshot degradado conservador | `router.rs`, `get_health` | Que um probe real falhe da mesma forma |
| Snapshot saudável retorna leitura na primária, escrita permitida e `failover_active=false` | `router.rs`, ramo `Healthy` de `route_read` | Request routing em execução |
| Snapshot não saudável resolve sibling, emite `FailoverDetected`, e só então retorna decisão de réplica com escrita bloqueada | `router.rs`, ramo degradado de `route_read` | Entrega/auditoria externa ou status HTTP |
| Outbox com contagem zero retorna `Ok`; contagem positiva ou erro de consulta recusa o gate | `failback.rs`, `assert_outbox_drained_or_block` | Estado de um outbox real |
| `RegionHealth::Down` existe, mas `evaluate` atual não o produz | `health.rs`, enum e `evaluate` | Estados introduzidos por futuros callers |

<a id="r06"></a>
## R06 — Configuração e dependências

| Item | Leitura estática | Ação de mudança |
|---|---|---|
| `RATE_5XX_THRESHOLD_PCT` | `1.0`; trigger usa comparação estrita `>` | Revisar R05 e testes de limiar. |
| `LATENCY_SLO_CEIL_MS` | `300`; trigger usa comparação estrita `>` | Não alegar p99 medido sem evidência operacional. |
| `CONSECUTIVE_FAILURES_THRESHOLD` | `3`; trigger usa `>=` | Preservar relação com os outros dois triggers. |
| `SUSTAINED_WINDOW_SECS` / cadence | Constantes `5` e `300` em módulos distintos | Constante não demonstra mecanismo de janela/cadence em execução. |
| `SLO_FAILOVER_OVERHEAD_MS` | `50`; decisão em memória registra `5` | Não tratar valor simulado como medição de SLO. |
| `ResidencyGraph` e auditoria | Reexports/dependência de `corelink-replica-worker` | Congelar contratos com seu owner antes de alterar assinatura ou grafo. |

<a id="r07"></a>
## R07 — Falhas e recuperação semântica

| Condição | Sinal no package | Recuperação de ownership |
|---|---|---|
| Probe retorna erro | Router em memória fabrica sinais que avaliam degradado | Reter mudança como decisão conservadora estática; não inferir execução de probe. |
| Não há sibling | `FailoverError::NoReplicaAvailable` | Coordenar `ResidencyGraph`; não inventar rota alternativa. |
| Emit de auditoria falha na rota | `FailoverError::Audit` antes da decisão degradada retornar | Revisar sink/ordem de contrato; não alegar delivery externo. |
| Outbox sujo | `WriteBlockedDuringFailover` após tentativa de audit/counter | Manter gate bloqueado até fonte concreta confirmar limpeza. |
| Consulta de outbox falha | `Internal` se audit registra; `Audit` se esse emit falha | Tratar como recusa; investigar implementação do trait fora desta crate. |

<a id="r08"></a>
## R08 — Evidência e lacunas explícitas

Evidência disponível: manifesto, sete módulos de fonte e dois testes estáticos no baseline de fonte `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. O plano WAVE_007 classifica o package como perfil S. O OKF canônico é consultável como referência, mas não foi repetido, redefinido ou revalidado por estes artefatos.

Desconhecidos explícitos: Tower, Cloudflare, montagem de middleware, roteamento ao vivo, execução/cadence de probe, tráfego, métricas medidas, resposta HTTP/503, storage/outbox concreto, deploy, produção, configuração produtiva e grafo reverso completo. A ausência desta observação não prova inexistência.
