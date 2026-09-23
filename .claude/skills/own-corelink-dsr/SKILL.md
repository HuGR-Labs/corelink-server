---
name: own-corelink-dsr
description: >-
  Assuma ownership da superfície pura de autoatendimento DSR; não trate traits,
  fakes ou referências estáticas como operação, conclusão legal ou runtime.
metadata:
  schema: "corelink-ownership/1.1"
  package: corelink-dsr
  manifest: crates/corelink-dsr/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-dsr-structural-normalization-20260921
---

# Ownership — corelink-dsr

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar taxonomia, trait, fake, recibo, SLA ou orquestrador DSR | Afirmar que rota CF, Neon, KMS/RS256, WebAuthn ou Email está ativo |
| Traçar MFA destrutiva, isolamento de ticket ou fence de audit | Decidir prazo legal, retenção, identidade autenticada ou política de produção |
| Mudar contrato público e consumidores estáticos | Executar Cargo, teste, rede, deploy ou operação de dados |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** `audit`, `calendar`, `endpoint`, `error`, `event`, `mfa`, `receipt` e `store`; traits e implementações em memória.
**Contrato público:** os reexports de `lib.rs`, `DsrEndpoint`, sinks/issuers/verifiers/stores, a taxonomia de seis direitos e decisões/tickets.
**Fora do território:** rota Cloudflare, chave RS256 real, WebAuthn real, store Neon e Email transacional. São integrações diferidas pela fonte, não capacidades observadas.
**Escalonamento:** owner de privacy/erasure, container, CF, identidade/credenciais, persistence e operação.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Qual contrato ou invariante muda? | [Referência](../../../docs/ownership/crates/corelink-dsr/REFERENCE.md#r04) e `crates/corelink-dsr/src/{event,endpoint}.rs` |
| Quem é afetado estaticamente? | [Relações](../../../docs/ownership/crates/corelink-dsr/BLAST_RADIUS.md#b03) |
| Qual procedimento é permitido nesta campanha? | [Manutenção](../../../docs/ownership/crates/corelink-dsr/MAINTENANCE.md#m02) |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação | Evidência | Pare quando |
|---|---|---|---|
| Direito, decisão ou SLA muda | Mapear enum/helper e consumidores antes de editar | `event.rs`; R04; B03 | O requisito depende de interpretação jurídica ou calendário operacional |
| Erasure ou Rectification muda | Preservar gate MFA e separar os quatro demais direitos | `is_destructive`; INV-001 | Token/identidade WebAuthn reais forem necessários |
| Audit/store/receipt muda | Preservar audit antes de `DsrRequestStore::insert` e falha propagada | `endpoint.rs`; INV-002 | A mudança tocar sink/ledger persistente externo |
| Trait ou reexport muda | Classificar import direto, reexport e referência adjacente | B03–B06 | Consumidor ou compatibilidade permanecer incerto |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme baseline, manifesto e módulo afetado.
2. Trace entrada → decisão → audit → store/recibo e registre a direção do impacto.
3. Expresse o predicado falsificável antes de editar a fonte ou o contrato.
4. Diferencie trait/fake de adaptador e runtime diferido.
5. Atualize referência, relações e manutenção; nesta campanha rode só verificações documentais estáticas.

<a id="s06"></a>
## S06 — Condições de parada

Pare diante de chave, credencial, WebAuthn, KMS, Neon, Email, binding/rota Cloudflare, tenant/dado real, rede, deploy, compilação Cargo ou teste. Pare também se não for possível demonstrar que uma mutação de store continua precedida pelos audits requeridos, ou se um consumidor só puder ser inferido por comentário/arquivo arquivado.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue baseline, caminhos-fonte lidos, contratos/relações alterados, consumidores estáticos classificados, comandos documentais e resultado literal. Declare desconhecidos e integrações diferidas. Não chame checagem documental de teste, revisão fria, certificação legal ou prova de runtime.

[Voltar ao início](#s01)
