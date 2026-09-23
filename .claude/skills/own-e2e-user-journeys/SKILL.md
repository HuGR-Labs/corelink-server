---
name: own-e2e-user-journeys
description: >-
  Route changes to CoreLink's e2e-user-journeys binary, HTTP assertions,
  personas, and local runner; use the canonical owner for server behavior and
  explicit operational authorization for any live or stateful run.
metadata:
  schema: "corelink-ownership/1.1"
  package: "e2e-user-journeys"
  manifest: "tests/e2e-user-journeys/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "source-inspection-1177dad2"
---

# Ownership — e2e-user-journeys

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| A mudança toca o manifesto, runner, harness, personas, `README.md`, `JOURNEY-MATRIX.md` ou qualquer módulo em `tests/e2e-user-journeys/`; também quando altera a associação desse package no `Cargo.toml` raiz, `Cargo.lock` ou a composição em `journeys/mod.rs`. | A mudança é comportamento de API, Worker, provider, deployment ou provisionamento externo; uma referência em README/matriz/OKF é apenas contexto e não transfere a implementação para este package. |

<a id="s02"></a>
## S02 — Território e autoridade

Este package é um bin de verificação black-box; seu território inclui o bin, o harness, personas, os 23 dispatches da composição em `src/journeys/mod.rs`, `README.md`, `JOURNEY-MATRIX.md` e o vínculo declarativo no workspace. Não implementa a API, não tem dependência Cargo `corelink-*`, e não prova que um endpoint está implantado.

Seu código emite HTTP quando executado e pode alterar estado remoto. Esta skill cobre inspeção e alterações locais; não autoriza chamar APIs, executar o CLI de produção, criar contas, enviar convites, mudar billing, apagar dados ou tocar storage.

`Cargo.toml` raiz e `Cargo.lock` são superfícies de composição/dependência compartilhadas: este package pode propor a correção do seu membro ou nó, mas o owner do workspace/lockfile aprova alterações globais. `README.md` e `JOURNEY-MATRIX.md` são contratos operacionais/documentais do harness; rotas, handlers, Worker, provider e dados continuam sob seus owners canônicos.

Roteie o contrato de servidor pelo [manifesto canônico OKF](../../../docs/internal/okf-wiki/concept-manifest.yaml). Ele classifica este harness como ferramenta de teste fora do escopo de arquitetura; esta skill não cria outro conceito nem substitui a rota OKF.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Papel, módulos e contratos | [Referência](../../../docs/ownership/crates/e2e-user-journeys/REFERENCE.md#r01) |
| README, matriz e composição dos 23 dispatches | `tests/e2e-user-journeys/README.md`, `JOURNEY-MATRIX.md`, `src/journeys/mod.rs` |
| Consumidores e efeitos | [Relações](../../../docs/ownership/crates/e2e-user-journeys/BLAST_RADIUS.md#b03) |
| Procedimentos e gates | [Manutenção](../../../docs/ownership/crates/e2e-user-journeys/MAINTENANCE.md#m02) |
| Contrato canônico e owner externo | [OKF](../../../docs/internal/okf-wiki/concept-manifest.yaml) |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Um journey muda rota, status ou dado esperado. | Trace o builder, chamada HTTP, assert e `REL` da superfície. | Owner do contrato ou ambiente de validação não estiver claro. |
| Persona, flag ou gate muda. | Leia `Config::from_env`, `TokenKind` e o predicado final em `main.rs`. | Uma ausência passar a ser silenciosa ou permitir GREEN sem PASS. |
| Um módulo é adicionado/removido da composição. | Refaça o censo `pub fn run`/`out.extend` e atualize o mapeamento API/REL e a matriz. | A contagem do source não reconciliar com o dispatcher. |
| A alteração exercita DSR, quota, billing, convite ou outra escrita remota. | Classifique o estado final e a recuperação em M05; use apenas tenant descartável. | Não houver recuperação real ou autorização operacional explícita. |
| A descrição “black-box” for usada como prova de rede, deploy ou provider. | Exija evidência de execução do ambiente correspondente. | Houver somente manifesto, comentário ou código-fonte. |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme `e2e-user-journeys`, o manifesto, o source pin e o target `e2e-user-journeys`. 2. Leia apenas as fichas API/INV/REL/PROC relevantes e a rota OKF canônica. 3. Separe chamadas HTTP, subprocessos, declarações Cargo e referências de texto. 4. Identifique efeitos remotos e defina tenant, credenciais, flags e recuperação antes de propor validação. 5. Faça a alteração autorizada, preserve o diff e use somente os gates do procedimento aplicável. 6. Informe resultados executados

e não executados separadamente; envie os hashes finais para revisão independente.

<a id="s06"></a>
## S06 — Condições de parada

Pare se o source pin divergir, o tenant/endereço não for dedicado, uma rota interna exigir autenticação indisponível, uma falha de isolamento revelar dado de outro tenant, ou uma operação puder cobrar, apagar, convidar ou gravar fora do escopo. Não contorne gates com acesso a banco/storage, segredos inline, chamadas diretas a provider ou edição do owner do servidor. Encaminhe contrato não identificado pelo OKF sem inventar um responsável.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue objetivo, baseline, API/INV/REL/PROC afetados, caminhos e símbolos lidos, package/target/features, comandos e ambiente previstos, resultados reais ou `NOT_EXECUTED`, efeitos remotos possíveis, recuperação, desconhecidos e responsável resolvido pelo OKF. Um teste declarado, um bin selecionável ou uma URL pública não prova execução nem produção.

[Voltar ao início](#s01)
