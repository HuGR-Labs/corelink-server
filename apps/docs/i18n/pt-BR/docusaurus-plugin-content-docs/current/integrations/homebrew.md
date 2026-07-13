---
id: homebrew
title: Espelho de bottles do Homebrew
sidebar_position: 8
description: Aponte o Homebrew para o CoreLink para cachear downloads de bottles no seu tenant.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/homebrew.md`

# Espelho de bottles do Homebrew

O CoreLink cacheia **bottles do Homebrew** (os binários `.tar.gz` pré-compilados que o `brew
install` baixa). Em um acerto de cache, a bottle é servida a partir do CAS do seu tenant;
em uma falha, o CoreLink a busca do upstream (`ghcr.io`), a cacheia e a transmite
de volta. Isso acelera reinstalações repetidas entre máquinas e na CI.

Este é um espelho de **caminho de leitura** para o `brew install`. Fontes de taps, casks
(`.dmg`/`.pkg`) e `brew bottle` estão fora do escopo.

## Pré-requisitos

- Homebrew instalado.
- Um PAT do CoreLink (`corelink_pat_...`).
- O UUID do seu tenant.

## Configurar

O Homebrew só anexa um cabeçalho `Authorization` quando o host da bottle é
alcançado por meio de `HOMEBREW_ARTIFACT_DOMAIN` (que mantém a estratégia de download
autenticado do GitHub Packages do Homebrew). Defina o domínio de artefatos para o caminho do seu tenant
e passe o PAT via `HOMEBREW_DOCKER_REGISTRY_TOKEN`:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_DOCKER_REGISTRY_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX"
brew install <formula>
```

O Homebrew transforma `HOMEBREW_DOCKER_REGISTRY_TOKEN` em
`Authorization: Bearer corelink_pat_...` em cada download de bottle, que é o que
o CoreLink autentica.

:::warning Use `HOMEBREW_ARTIFACT_DOMAIN`, não `HOMEBREW_BOTTLE_DOMAIN`
Um `HOMEBREW_BOTTLE_DOMAIN` puro seleciona a estratégia de download simples do Homebrew,
que **não** envia nenhum cabeçalho de autenticação — então ela não consegue se autenticar no CoreLink e
todo download falha com 401. `HOMEBREW_ARTIFACT_DOMAIN` é obrigatório.
:::

## Verificar se funcionou

Instale uma formula pequena duas vezes em máquinas diferentes (ou limpe o cache de
download local entre as execuções). A segunda instalação puxa a bottle cacheada do
CoreLink:

```bash
brew install --verbose jq 2>&1 | grep corelink-api.humangr.com | head
```

Ver requisições para `corelink-api.humangr.com/brew/...` confirma que o Homebrew está
usando o espelho.

## Resolução de problemas

| Sintoma | Causa provável | Correção |
|---|---|---|
| `401 Unauthorized` em todo download | Usou `HOMEBREW_BOTTLE_DOMAIN` (sem cabeçalho de autenticação) | Mude para `HOMEBREW_ARTIFACT_DOMAIN` |
| `401` com o domínio de artefatos definido | Token ausente ou malformado | Defina `HOMEBREW_DOCKER_REGISTRY_TOKEN=corelink_pat_...` |
| `403 Forbidden` | PAT com escopo para um tenant diferente | Confirme que o `<tenant>` no domínio corresponde ao tenant do seu PAT |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
