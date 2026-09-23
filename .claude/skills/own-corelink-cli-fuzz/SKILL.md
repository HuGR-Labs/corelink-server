---
name: own-corelink-cli-fuzz
description: Ownership routing for corelink-cli-fuzz; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-cli-fuzz
  manifest: tools/cli/fuzz/Cargo.toml
  source-commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
  evidence-set: corelink-cli-fuzz-structural-normalization-20260921
---

# Ownership — corelink-cli-fuzz

Rascunho baseado na fonte fixada. Declarações de bin, workflow ou assertion são
evidência de intenção e desenho; não provam build, execução, cobertura ou runtime.

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use como owner principal quando |
|---|---|
| Alterar `tools/cli/fuzz/Cargo.toml`, seus cinco `fuzz_targets/*.rs`, entradas, oráculos ou dependências diretas | Alterar `corelink-cli` ou `fuzz_api`: coordene com o package pai e confira os consumidores |
| Revisar separação do workspace independente, targets, lockfile ou seleção de CI | Alterar parser, auth, configuração, rede ou contrato da CLI: `corelink-cli` é o owner de implementação |
| Avaliar uma falha, corpus, minimização ou claim de 0 panics/0 leaks | Declarar que um target executou, que a CLI foi publicada, ou que comportamento de produção foi observado |

<a id="s02"></a>
## S02 — Território e autoridade

O território é o package `corelink-cli-fuzz` em `tools/cli/fuzz/Cargo.toml`:
manifesto, lockfile e os bins `cli_input`, `config_toml`, `json_deserialize`,
`auth_resolution` e `secret_redaction_check`. O package pai `corelink-cli` é
dono da implementação e dos contratos `CorelinkConfig`, `validate_pat_shape` e
`fuzz_api`; a reexportação não transfere esse ownership. Não há operador ou rota
nominal verificada nesta fonte. Esta skill não autoriza providers, credenciais,
produção, publicação, gastos, alteração de dados ou aprovação.

Para mudanças no package pai, assuma a rota de coordenação
[own-corelink-cli](../own-corelink-cli/SKILL.md): essa skill é a autoridade de
implementação/contrato de `tools/cli`, enquanto esta skill permanece owner dos
harnesses e dos oráculos fuzz.

`cli_input` é um harness de bytes dividido em `key`/`value`, não um parser de
argv, flags ou subcomandos; o `arbitrary` declarado não prova geração via
`Arbitrary`. `json_deserialize` testa parsing de entrada local, não formatter ou
schema de saída `--output=json`. `secret_redaction_check` cobre apenas
`ConfigError` formatados por dois helpers; não cobre todo stderr da CLI.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler |
|---|---|
| Quais bins, entradas e oráculos existem? | [R03](../../../docs/ownership/crates/corelink-cli-fuzz/REFERENCE.md#r03) e [R05](../../../docs/ownership/crates/corelink-cli-fuzz/REFERENCE.md#r05) |
| O que é API do pai versus harness próprio? | [R02](../../../docs/ownership/crates/corelink-cli-fuzz/REFERENCE.md#r02) e [API-001](../../../docs/ownership/crates/corelink-cli-fuzz/REFERENCE.md#api-001) |
| Quais consumidores, workflows e inversas importam? | [B03](../../../docs/ownership/crates/corelink-cli-fuzz/BLAST_RADIUS.md#b03) e [B04](../../../docs/ownership/crates/corelink-cli-fuzz/BLAST_RADIUS.md#b04) |
| Como revisar ou validar sem confundir configuração com prova? | [M02](../../../docs/ownership/crates/corelink-cli-fuzz/MAINTENANCE.md#m02) e [M04](../../../docs/ownership/crates/corelink-cli-fuzz/MAINTENANCE.md#m04) |
| A mudança é no parent `corelink-cli`/`fuzz_api`? | [skill parent](../own-corelink-cli/SKILL.md) S03–S05, depois REL-001 e cold review coordenada |
| Qual referência canônica OKF aplicar? | [OKF engineering onboarding](../../../docs/knowledge/ops/engineering-onboarding.md) e o perfil em `docs/internal/okf-wiki/` |

Abra somente os registros pertinentes e preserve os conceitos OKF; não replique
políticas do OKF dentro da skill.

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição | Ação e evidência | Parar quando |
|---|---|---|
| Muda assinatura ou semântica de `fuzz_api` | Coordene com `corelink-cli`, atualize API/REL/INV e recenseie todos os cinco targets | O contrato do pai ou a compatibilidade do harness não está resolvida |
| Muda bytes, cortes ou oráculo de um target | Preserve input/target original, atualize FLOW/INV e valide o diff no pin | O predicado não é falsificável ou a reprodução foi perdida |
| Workflow ou script seleciona um target | Separe YAML/script declarado de execução observada e registre o comando exato | A seleção aponta para caminho stale ou falta evidência de execução |
| Há claim de 0 panic ou 0 leak | Trate como propriedade a ser exercitada em ambiente isolado; capture saída e artefato | O resultado é apenas comentário, bin declarado ou green genérico |
| Pedido envolve token, provider, release ou produção | Encaminhe para owner operacional verificado e registre bloqueio | Não existe autoridade e evidência autorizada |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme package pelo nome Cargo, manifesto e source pin `38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`.
2. Leia os cinco harnesses e classifique cada fatia de entrada, chamada e assertion.
3. Recenseie dependências, inversas, lockfile, scripts e workflows fora de Cargo.
4. Para cada mudança, selecione procedimento em M02 e registre ambiente e recuperação.
5. Separe implementado, wired e runtime_verified; não promova um estado ao outro.
6. Atualize somente os quatro artefatos deste package e solicite cold review independente dos bytes finais.

<a id="s06"></a>
## S06 — Condições de parada

Pare com manifesto/package divergente, fonte stale, target sem entrada conhecida,
contrato pai contraditório, seleção CI sem owner, crash sem input preservado ou
recovery indefinido. Pare se um YAML, `cargo metadata`, build ou assertion for
apresentado como runtime. Não execute provider, produção, release, secret ou
fuzz não autorizado; escale a relação fora de autoridade ao owner verificado.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue baseline, paths, targets, APIs, INV/REL/PROC afetados, comandos realmente
executados, hashes, estado `not run` quando aplicável, limites e pendências. Nomeie
revisor somente com atribuição verificável. O autor não aprova os artefatos; bytes
alterados invalidam a revisão correspondente.

[Voltar ao início](#s01)
