---
name: own-corelink-audit-chain-fuzz
description: Ownership routing for corelink-audit-chain-fuzz; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-audit-chain-fuzz
  manifest: crates/corelink-audit-chain/fuzz/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-audit-chain-fuzz-structural-normalization-20260921
---

# Ownership — corelink-audit-chain-fuzz

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar `fuzz/Cargo.toml`, os alvos `merkle_append` ou `jcs_canonicalize`, ou suas asserções | Alterar o contrato, serialização ou comportamento de produção de `corelink-audit-chain`; use [a skill da crate](../own-corelink-audit-chain/SKILL.md) |
| Investigar falha reproduzida pelo código de um desses alvos | Operar runner, schedule, cache, credenciais ou workflow; o owner humano/operacional não foi identificado |
| Alterar a lista de seleção do fuzzer dedicada a esses dois alvos | Interpretar nomes iguais em benches/perf como consumidores do package fuzz |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação deste pacote:** manifesto próprio e os dois binários de harness em `crates/corelink-audit-chain/fuzz/`.
**Contratos consumidos:** APIs de `corelink-audit-chain` e `corelink-analytics`; contratos externos de `libfuzzer-sys`, `serde_json`, `serde_jcs` e `uuid`.
**Composição invocadora:** `scripts/fuzz-all.sh` e `.github/workflows/fuzz-nightly.yml`; seus owners humanos não estão verificados.
**Operador/aprovador:** desconhecidos nos arquivos examinados; revisão independente ainda precisa ser designada.
Esta skill não autoriza builds, fuzz runs, CI, produção, gastos ou alterações fora da tarefa.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Quais entradas e predicados cada harness contém? | [Referência](../../../docs/ownership/crates/corelink-audit-chain-fuzz/REFERENCE.md#r03) |
| Quais dependências e invocadores podem ser afetados? | [Relações](../../../docs/ownership/crates/corelink-audit-chain-fuzz/BLAST_RADIUS.md#b03) |
| O que pertence ao contrato de produção ou à composição de `Region`? | [skill de `corelink-audit-chain`](../own-corelink-audit-chain/SKILL.md); [skill de `corelink-analytics`](../own-corelink-analytics/SKILL.md) |
| Qual contexto canônico OKF ou reconciliação transversal devo aplicar? | [`okf-context`](../okf-context/SKILL.md); [`okf-reconcile`](../okf-reconcile/SKILL.md) |
| Como validar ou preservar um achado? | [Procedimentos](../../../docs/ownership/crates/corelink-audit-chain-fuzz/MAINTENANCE.md#m02) |
| Qual evidência sustenta este mapa? | [Fontes](../../../docs/ownership/crates/corelink-audit-chain-fuzz/REFERENCE.md#r08) |

Carregue somente as seções pertinentes. Trate CO-1 v1.1 como candidato local até que sua publicação e hash imutável sejam verificados.

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Mudança no mapeamento de bytes ou nas asserções de um alvo | Identifique o `INV` e `PROC` correspondentes; valide apenas o alvo afetado e preserve o corpus/artefato | A propriedade proposta exceder o que a asserção e seus guardas realmente cobrem |
| Mudança de API importada de crate local | Trace o símbolo e encaminhe compatibilidade ao owner do contrato provedor | Owner humano, compatibilidade ou resultado real não estiver evidenciado |
| Mudança de invocador, runner ou persistência de artefatos | Encaminhe a revisão à composição/operador da configuração exata | Nenhuma rota humana verificada estiver disponível |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme o manifesto, os dois targets e a revisão imutável das fontes. 2. Leia o predicado e seus limites em [R04–R05](../../../docs/ownership/crates/corelink-audit-chain-fuzz/REFERENCE.md#r04) e os impactos em [B03–B06](../../../docs/ownership/crates/corelink-audit-chain-fuzz/BLAST_RADIUS.md#b03). 3. Selecione um procedimento e seu target; não execute a lista inteira por conveniência. 4. Registre baseline, corpus, ambiente isolado e recuperação antes de qualquer fuzz run. 5. Preserve saída de falha sem convertê-la em evidência de comportamento de produção. 6. Envie os

quatro hashes finais a uma revisão independente; autoria e checkers estruturais não aprovam o conteúdo.

<a id="s06"></a>
## S06 — Condições de parada

Pare se o pin ou manifesto divergir; se um target, feature ou contrato não estiver confirmado; se um achado depender de wiring/runtime de produção; se o runner exigir ação não autorizada; ou se corpus/crash-artifacts puderem ser sobrescritos antes de preservação. Não invente responsável: solicite designação ao owner do repositório.

<a id="s07"></a>
## S07 — Evidência e saída

Preserve os cinco axiomas do contrato: **success criteria** (novo maintainer localiza e escolhe o procedimento); **completeness criteria** (targets, dependências, invocadores e hits não-Cargo conciliados); **quality standards** (perfil S, navegação, evidência e sem duplicação); **definition of done** (quatro documentos revistos por hashes finais e gates pertinentes); **invariants** (sem mudança funcional disfarçada, autoridade inventada, prova de produção falsa, omissão ou autoaprovação).

Relate baseline `d80f245e0be3c61c250249de8292e42a6cd8ef5d`, package source pin `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` (the pin recorded in this artifact set), paths, `REL`/`INV`/`PROC`, commands actually executed, limits, and pending owner. Do not infer fuzz, CI, or runtime execution from static source.

[Voltar ao início](#s01)
