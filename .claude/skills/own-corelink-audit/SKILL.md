---
name: own-corelink-audit
description: >-
  Governa mudanças na taxonomia de auditoria, envelope CloudEvents, hash JCS,
  redaction, retention hints e interfaces de emissão de corelink-audit.
  Não atribui ownership de sinks produtivos, deploy, SIEM ou cadeia externa.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-audit"
  manifest: "crates/corelink-audit/Cargo.toml"
  source-commit: "59c76cf260bcdeb5246772f70821ac8b7e8a9780"
  evidence-set: "audit-static-graph-20260920"
---

# Ownership — corelink-audit

Candidata baseada somente em fonte e grafo estático da baseline. Não prova emissão
durável, entrega a SIEM, operação de retenção, runtime ou revisão independente.

[Escopo](#s01) · [Triagem](#s02) · [Envelope](#s03) · [Privacidade](#s04) ·
[Compatibilidade](#s05) · [Parada](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Escopo

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Alterar `events`, `link_hash`, `redact`, `retention`, `emitter`, `metrics`, `outbox` ou `ports` | Tratar como mudança desta unidade | [R03](../../../docs/ownership/crates/corelink-audit/REFERENCE.md#r03) | Se o efeito pertence a consumer ou sink |
| Alterar `analytics` ou `chain` | Coordenar com a crate reexportada | [B02](../../../docs/ownership/crates/corelink-audit/BLAST_RADIUS.md#b02) | Não assumir implementação local |

<a id="s02"></a>
## S02 — Triagem do contrato

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Tipo/payload `auth.*` muda | Mapear taxonomia, payload e consumer | [R04](../../../docs/ownership/crates/corelink-audit/REFERENCE.md#r04) | Variante ou leitor não enumerado |
| Interface pública muda | Traçar dependentes estáticos antes de editar | [B06](../../../docs/ownership/crates/corelink-audit/BLAST_RADIUS.md#b06) | Grafo reverso incompleto |

<a id="s03"></a>
## S03 — Envelope e hash

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Campo CloudEvents ou serialização muda | Preservar `specversion=1.0` e verificar vetores | [R04](../../../docs/ownership/crates/corelink-audit/REFERENCE.md#r04) | Compatibilidade canônica desconhecida |
| Hash de conteúdo ou link muda | Separar JCS do link SHA-256 | [B03](../../../docs/ownership/crates/corelink-audit/BLAST_RADIUS.md#b03) | Bytes persistidos/leitor N-1 sem plano |

<a id="s04"></a>
## S04 — Privacidade e retenção

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Campo pode conter PII/PAT | Usar newtype/macro de redaction e revisão de payload | [R05](../../../docs/ownership/crates/corelink-audit/REFERENCE.md#r05) | Campo bruto ou sem classificação |
| Tier ou hint muda | Comparar mapeamento e consumidor de retenção | [B04](../../../docs/ownership/crates/corelink-audit/BLAST_RADIUS.md#b04) | Política/worker não confirmados |

<a id="s05"></a>
## S05 — Compatibilidade e consumers

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Export/reexport/trait muda | Enumerar declarações diretas: container, CAS, AC, worker, adapter-host, auth, ops e e2e; classificar chain/analytics separadamente | [B06](../../../docs/ownership/crates/corelink-audit/BLAST_RADIUS.md#b06) | Consumer material não classificado |
| Mudança aparenta criar entrega durável | Exigir adapter/wiring concreto | [R08](../../../docs/ownership/crates/corelink-audit/REFERENCE.md#r08) | Só há import, trait ou comentário |

<a id="s06"></a>
## S06 — Paradas obrigatórias

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Baseline, manifesto ou fonte divergem | Reconciliar escopo e SHA | [M01](../../../docs/ownership/crates/corelink-audit/MAINTENANCE.md#m01) | Não editar nesta edição |
| Sink, replay ou retenção real é solicitado | Encaminhar ao owner da integração | [R08](../../../docs/ownership/crates/corelink-audit/REFERENCE.md#r08) | Não inferir operação da fonte |

<a id="s07"></a>
## S07 — Saída mínima

| Condição | Ação | Evidência | Parada |
|---|---|---|---|
| Mudança concluída | Registrar SHA, arquivos, contrato, consumers, comando e resultado | [M06](../../../docs/ownership/crates/corelink-audit/MAINTENANCE.md#m06) | Não chamar checks de aprovação |
| Lacuna permanece | Declarar desconhecido e rota de decisão | [R08](../../../docs/ownership/crates/corelink-audit/REFERENCE.md#r08) | Não converter ausência em garantia |

[Início](#s01)
