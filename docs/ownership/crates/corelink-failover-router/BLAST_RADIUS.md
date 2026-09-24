---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-failover-router
manifest: crates/corelink-failover-router/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: failover-router-static-20260920
---

# corelink-failover-router — blast radius

Relações são atômicas e derivadas apenas de manifesto e fonte estática. Elas descrevem dependência ou fluxo de código, não Tower, Cloudflare, roteamento ao vivo, execução de probe ou produção. O OKF canônico não é redefinido nem revalidado.

[Superfície](#b01) · [Saúde](#b02) · [Auditoria](#b03) · [Residência](#b04) · [Failback](#b05) · [Lacunas](#b06).

<a id="b01"></a>
## B01 — Superfície própria

`health`, `probe`, `router`, `audit`, `error` e `failback` são módulos públicos reexportados por `src/lib.rs`. Impacto: mudança em tipo, trait, constante ou função reexportada pode alterar callers do package. Evidência: `src/lib.rs`. Pare antes de afirmar que todos os callers foram enumerados.

<a id="b02"></a>
## B02 — Saúde para decisão

`InMemoryFailoverRouter` recebe `Arc<dyn HealthProbe>` e converte o snapshot em decisão via `get_health`/`route_read`; erro do trait é convertido em snapshot degradado. Impacto: alterar sinais, limiares ou erro modifica a escolha do router em memória. Evidência: `probe.rs`, `health.rs` e `router.rs`. Pare: não há evidência de probe executado ou scheduler.

<a id="b03"></a>
## B03 — Decisão degradada para auditoria

No ramo não saudável, `route_read` resolve o sibling e chama `FailoverAuditSink::emit(FailoverDetected)` antes de construir a decisão de réplica; falha do emit retorna `FailoverError::Audit`. Impacto: mudar record, sink ou ordem afeta a capacidade do caller de receber a decisão degradada. Evidência: `router.rs` e `audit.rs`. Pare: entrega para um backend de auditoria não é observada.

<a id="b04"></a>
## B04 — Roteamento para residência

O router reexporta e instancia `corelink_replica_worker::ResidencyGraph`, usando `sibling(primary)` para a decisão degradada. Impacto: mudança de `Region`, de `sibling` ou do contrato do grafo exige coordenação com `corelink-replica-worker`. Evidência: manifesto, `lib.rs` e `router.rs`. Pare: topologia regional e uso em runtime não foram derivados.

<a id="b05"></a>
## B05 — Failback para outbox e observabilidade

`assert_outbox_drained_or_block` consulta `AuditOutboxRepository`; para contagem positiva ou erro ela tenta auditoria `FailoverResolved` e emite `FailbackBlockedCounter`, retornando erro de recusa. Impacto: uma alteração do gate, reason ou taxonomia muda esta fronteira de trait local. Evidência: `failback.rs`, símbolos `AuditOutboxRepository`, `FailbackBlockedCounter` e `assert_outbox_drained_or_block`. Pare: não há evidência de banco, métrica ou alerta executados.

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

Conhecido: dependência direta em `corelink-replica-worker` e módulos próprios reexportados por `src/lib.rs`. Desconhecido: consumidores, quais consumidores entram em request path, Tower, Cloudflare, live routing, execução de probe, status HTTP, tráfego, storage concreto, deploy, produção e grafo reverso completo. Não há observação desses elementos neste recorte; isto não prova ausência.
