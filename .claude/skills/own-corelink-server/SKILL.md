---
name: own-corelink-server
description: >-
  Assuma ownership de corelink-server ao alterar boot, composição HTTP, rotas,
  adapters nativos ou flags; não execute operações remotas.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-server"
  manifest: "crates/corelink-container/Cargo.toml"
  source-commit: "91630baebe3ae7abe686cd4e06a5621ecdc4ab73"
  evidence-set: "server-main-readback-20260923-91630"
---

# Ownership — corelink-server

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar `main.rs`, `main_runtime.rs`, router, GC, storage ou flags | Operar D1, R2, Stripe, BYOK ou Cloudflare |
| Diagnosticar boot, mount, fallback ou shutdown | Alterar crate dependente sem o owner dela |
| Mudar contrato HTTP, limite, segredo ou adapter | Inferir produção por compilação local |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** binários, boot nativo, composição Axum, rotas e adapters em `src`.
**Contratos públicos:** boot, montagem de rotas, seleção de adapter e contratos HTTP documentados.
**Fora do território:** semântica interna de handlers, Stripe, D1, R2, BYOK, Cloudflare e credenciais.
**Escalonamento:** owner do adapter ou operação correspondente. Esta skill não autoriza escrita remota,
deploy, custo, segredo ou dado de tenant.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Qual binário, flag ou rota compõe o comportamento? | [Referência](../../../docs/ownership/crates/corelink-server/REFERENCE.md#r03) |
| O que pode propagar para storage ou billing? | [Relações](../../../docs/ownership/crates/corelink-server/BLAST_RADIUS.md#b03) |
| Qual teste ou recuperação é segura? | [Manutenção](../../../docs/ownership/crates/corelink-server/MAINTENANCE.md#m02) |
| Qual contexto global se aplica? | [Container OKF](../../../docs/knowledge/planes/container.md) |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Mudar feature ou provider | Mapear target, fallback e relação; testar seleção local | Exigir credencial ou target entregue |
| Mudar mount ou limite HTTP | Mapear handler, auth e teste negativo | Rota ou recuperação indefinida |
| Sinal de produção incompleto | Preservar fail-closed e registrar env ausente | Exigir deploy ou escrita remota |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme package, baseline, binário, target e features.
2. Trace `main.rs` até o módulo dono da rota.
3. Classifique adapter como real, fake, stub ou não observado.
4. Defina teste negativo e recuperação antes de mudar wiring.
5. Execute apenas teste local autorizado, preservando output.
6. Atualize documentos e solicite cold review dos hashes alterados.

<a id="s06"></a>
## S06 — Condições de parada

Pare quando a tarefa exigir segredo, D1, R2, Stripe, KMS, Cloudflare, dado de cliente, custo,
deploy ou schema. Pare também com mais de uma flag BYOK real, target wasm divergente ou fallback
que torne o teste verde sem provar o caminho requerido. Não desative gate para fazer boot local.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue baseline, binário, target, features, rota e adapters afetados, comandos e resultados reais.
Registre mounts e pendências com owner. Build local não prova runtime ou produção; aprovação exige
quatro revisões frias independentes.

[Voltar ao início](#s01)
