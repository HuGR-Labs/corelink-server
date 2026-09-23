---
schema: corelink-ownership/1.1
document: reference
package: corelink-replication-coordinator
manifest: crates/corelink-replication-coordinator/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: replication-coordinator-static-20260920
---

# corelink-replication-coordinator — referência de ownership

Referência estritamente de manifesto e fonte no commit fixado. Ela não demonstra coordenador ativo, estado replicado, Durable Object, lock distribuído, auditoria entregue, tráfego, deploy ou produção. OKF verificado é referência externa, não uma política repetida aqui.

[Identidade](#r01) · [Fronteira](#r02) · [Heartbeat](#r03) · [Coordenação](#r04) · [Axiomas](#r05) · [Falhas](#r06) · [Dependências](#r07) · [Lacunas](#r08).

<a id="r01"></a>
## R01 — Identidade e função

| Fonte falsificável | Observação estática | Não prova |
|---|---|---|
| Manifesto | Package é `corelink-replication-coordinator`; declara `serde`, `thiserror`, `corelink-replica-worker` e `corelink-failover-router` | Uma instância em execução |
| `src/lib.rs` | Reexporta os contratos de papel, heartbeat, lag, auditoria, erro e coordenador | Integração de `/health` ou dashboards |
| Limite | O package contém implementação denominada `InMemoryReplicationCoordinator` | Coordenador externo ou estado entre processos |

<a id="r02"></a>
## R02 — Mapa e fronteira

| Fonte falsificável | Contrato observado | Limite |
|---|---|---|
| `state.rs` | Enum não exaustivo `Primary`, `HotStandby`, `Replica` e constante de cooldown | Não define topologia real |
| `heartbeat.rs` / `lag.rs` | Traits e valor de heartbeat; bundle de quatro números de lag | Não realiza sondagem ou medição |
| `coordinator.rs` / `audit.rs` | Trait, estado `Arc<Mutex<HashMap<…>>>` e trait de sink | Não liga DO, bus ou armazenamento |
| `error.rs` | Taxonomia não exaustiva de recusas e falhas internas | Não classifica incidentes reais |

<a id="r03"></a>
## R03 — Heartbeat e lag

| Fonte falsificável | Invariante atual | Contraprova possível |
|---|---|---|
| `Heartbeat::is_fresh` | Timestamp no futuro é stale; idade menor que `60` segundos é fresca | Um corpo que aceite timestamp futuro ou `>= 60` |
| `InMemoryHeartbeatRegistry` | `record` substitui o valor pela chave de região; `latest` devolve opcional | Persistência externa ou entrega por worker |
| `LagBundle::within_slo` | R2, D1 e KV devem ser `<= 60`; Neon não bloqueia esse método | Uma quarta condição hard ou lag medido |

<a id="r04"></a>
## R04 — Contratos do coordenador

| Fonte falsificável | Entrada e resultado estático | Limite |
|---|---|---|
| `ReplicationCoordinator::register` | Registra papel; segundo primário de outra região retorna `SplitBrainRejected` | Exclusão mútua distribuída |
| `route_write` | Só retorna `Ok` a primário registrado, fresco e dentro dos SLOs hard | Uma escrita encaminhada de verdade |
| `evaluate` | Sem mutar, retorna manter, promover réplica elegível ou nenhuma elegível | Decisão executada em produção |
| `promote` / `failback` | Operam no mapa local e retornam `Result` | Transição de estado replicado |
| `replication_status` | Constrói snapshot a partir de mapa e registry locais | Endpoint ou consumidor ativo |

<a id="r05"></a>
## R05 — Cinco axiomas estáticos

| Axioma | Fonte | Falsificador |
|---|---|---|
| A1: só `Primary` aceita escrita | `RegionRole::accepts_writes`, `route_write` | Um ramo que aceite outro papel |
| A2: promoção permite não haver `Primary` atual, mas rejeita primário atual diferente de `primary`; ainda exige `primary` inelegível e réplica fresca dentro dos SLOs hard | `primary_in_guard`, `promote` | Rejeitar ausência de primário ou promover com primário diferente |
| A3: o mapa local rejeita outro primário ao registrar ou promover | `register`, `primary_in_guard`, `promote` | Dois papéis `Primary` aceitos no mesmo mapa |
| A4: emit de promoção/demissão precede a mutação codificada | `promote` | Inserção no mapa antes de `audit.emit` |
| A5: failback antes de `86_400` segundos retorna `CooldownNotElapsed` somente se o emit bloqueado passar; emit bloqueado falho retorna `Audit` | `failback`, `HOT_STANDBY_COOLDOWN_SECONDS` | Retorno `Ok` antes do limiar ou `CooldownNotElapsed` após falha do emit |

<a id="r06"></a>
## R06 — Falhas e recuperação semântica

| Condição fonte | Resultado codificado | Limite |
|---|---|---|
| Região ausente | `UnknownRegion` | Não identifica uma região externa |
| Heartbeat ausente, stale ou hard-lag | Escrita recusa; avaliação não mantém primário | Não prova failover real |
| Réplica inelegível | `NoEligibleReplica` | Não chama runbook |
| Emit falha | `Audit`; o caminho de promoção não chega às inserções posteriores | Não prova entrega ou atomicidade remota |
| Cooldown precoce | Tenta evento bloqueado e retorna recusa se o emit passa | Não prova relógio compartilhado |

<a id="r07"></a>
## R07 — Configuração e dependências

| Fonte falsificável | Valor/ligação | Ação de mudança |
|---|---|---|
| `state.rs` | `HOT_STANDBY_COOLDOWN_SECONDS = 86_400` | Revisar A5 e callers estáticos |
| `heartbeat.rs` | `HEARTBEAT_STALE_SECONDS = 60`; comparação é estrita | Revisar R03 e R06 |
| `lag.rs` | Hard ceilings R2/D1/KV `60`; Neon soft `5` | Não converter Neon em gate sem contrato |
| Manifesto e `lib.rs` | `Region` vem de `corelink-replica-worker`; `corelink-failover-router` é dependência declarada | Congelar assinatura intercrate antes da mudança |

<a id="r08"></a>
## R08 — Lacunas e desconhecidos

Cinco desconhecidos explícitos: (1) existência de coordenador ativo, (2) estado replicado entre instâncias, (3) DO ou outro lock distribuído, (4) origem/atualidade de heartbeat e lag, e (5) entrega de auditoria. Também não foram observados rota HTTP, dashboard, tráfego, deploy, configuração produtiva ou produção. Ausência neste recorte não prova ausência no sistema.
