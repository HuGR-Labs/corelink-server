---
id: homebrew
title: Espelho de bottles do Homebrew
sidebar_position: 8
description: O caminho seguro do espelho autenticado de bottles do Homebrew.
---

# Espelho de bottles do Homebrew

:::caution Caminho seguro do espelho autenticado
O Homebrew moderno (o padrão `install-from-API`, Homebrew 4.x ou posterior)
pode reescrever as URLs de bottles do `ghcr.io` por meio de
`HOMEBREW_ARTIFACT_DOMAIN`. O endpoint `/brew/<tenant>` do CoreLink aceita o
PAT bearer resultante e obtém a bottle do upstream fixo `ghcr.io`. Esta página
é mantida porque tanto o fluxo autenticado quanto o fluxo normal do Homebrew
são suportados. Fontes de taps, casks (`.dmg`/`.pkg`) e `brew bottle` ficam
fora deste escopo.

A opção sem fallback é obrigatória. Sem ela, o Homebrew tenta novamente a URL
original do `ghcr.io` quando o espelho falha e pode enviar o bearer ao GitHub em
vez do CoreLink. Nunca use a receita antiga que não tinha essa proteção.
:::

<!-- WP-B161-AUTH-NO-FALLBACK-20260901: o espelho autenticado fica preso ao CoreLink. -->

## Caminho seguro: espelho autenticado do CoreLink

Obtenha um PAT real do CoreLink no seu gerenciador de segredos e exponha-o
somente como `CORELINK_PAT` no shell em que o Homebrew será executado. Não cole
o token em documentos, no histórico do shell nem em logs. Defina as três
variáveis juntas:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1
export HOMEBREW_DOCKER_REGISTRY_TOKEN="$CORELINK_PAT"
brew install jq
```

O Homebrew mantém sua estratégia de GitHub Packages para a bottle do `ghcr.io`,
reescreve a URL para o domínio de artefatos e transforma
`HOMEBREW_DOCKER_REGISTRY_TOKEN` em `Authorization: Bearer <token>`. Com
`HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1`, esse bearer é enviado somente ao
domínio do CoreLink; uma falha do espelho produz um erro, não uma tentativa no
`ghcr.io`. O adaptador do CoreLink autentica o bearer e busca o conteúdo do
`ghcr.io` no lado do servidor.

Não substitua o domínio de artefatos por `HOMEBREW_BOTTLE_DOMAIN`. Esse override
legado de arquivos planos não seleciona a estratégia autenticada do GitHub
Packages do Homebrew e não consegue fornecer o bearer exigido pelo `/brew`.

```bash
brew install jq
```

Para exercitar somente o download, sem instalar a fórmula, mantenha as mesmas
três variáveis e execute:

```bash
brew fetch --force jq
```

O fluxo público sem configuração também continua válido: remova as três
variáveis específicas do CoreLink e o Homebrew baixará diretamente do upstream.

## Por que a receita antiga do espelho foi removida

A receita anterior definia `HOMEBREW_ARTIFACT_DOMAIN` e
`HOMEBREW_DOCKER_REGISTRY_TOKEN`, mas omitia
`HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK`. Essa omissão era insegura: quando o
espelho falhava, o Homebrew podia tentar o `ghcr.io` novamente com o bearer do
CoreLink. Um PAT do CoreLink nunca deve ser enviado ao upstream; bloqueie o
fallback antes de definir o token.

O código de status diferencia os dois casos observados na verificação:

| Teste | Resultado | Significado |
|---|---|---|
| Sem variáveis do CoreLink | `brew` termina com `0` | O caminho direto para o upstream funciona. |
| Domínio de artefatos sem credencial | `401 Unauthorized` | O espelho não recebeu bearer; use o PAT somente no espelho fixado. |
| Domínio de artefatos com token e sem fallback | `Authorization: Bearer <token>` no CoreLink | O contrato do espelho autenticado está ativo. |
| Token sem domínio de artefatos ou sem proteção de fallback | `403 Forbidden` pode vir do `ghcr.io` | Configuração insegura; remova-a e reaplique o bloco fixado. |

A diferença entre `401` e `403` comprova a transmissão de credencial, não é
uma solução. Não configure um token para transformar `401` em `403` e nunca
envie um PAT do CoreLink ao `ghcr.io`.

## Solução de problemas

| Sintoma | Significado | Ação |
|---|---|---|
| `brew install` funciona sem variáveis do CoreLink | O Homebrew está usando o caminho direto do upstream | Mantenha o fluxo sem configuração. |
| `401 Unauthorized` do domínio do CoreLink | O bearer está ausente ou inválido | Verifique a origem e o escopo do PAT; não adicione fallback. |
| `403 Forbidden` do `ghcr.io` | O token foi enviado ao upstream | Pare, remova o token e reaplique o bloco com proteção. |
| Há domínio personalizado sem `HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1` | Uma falha pode voltar ao `ghcr.io` | Trate a configuração como insegura até adicionar a proteção. |

Para uma integração de cache compatível, consulte [Solução de problemas](../troubleshooting.md)
e escolha uma das integrações listadas acima.
