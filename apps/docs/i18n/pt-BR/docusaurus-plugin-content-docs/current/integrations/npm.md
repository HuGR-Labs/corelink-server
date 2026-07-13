---
id: npm
title: Espelho do registro npm
sidebar_position: 6
description: Aponte npm, pnpm, yarn ou bun para o CoreLink como um espelho de cache na frente do registro público do npm.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/npm.md`

# Espelho do registro npm

O CoreLink é um **espelho de cache** na frente do `registry.npmjs.org`. Aponte
o `npm` (ou `pnpm` / `yarn` / `bun`) para o CoreLink e os metadados de pacotes e tarballs
são cacheados no seu tenant, de modo que reinstalações repetidas — especialmente na CI — sejam mais rápidas e
resilientes a interrupções do upstream. Os tarballs são armazenados no CAS do seu tenant e
têm a integridade verificada contra o `dist.shasum` do publicador antes de serem cacheados.

Este é um **espelho somente leitura** para o `npm install`. Ele não hospeda pacotes
privados e não aceita `npm publish`.

## Pré-requisitos

- `npm` (ou um cliente compatível) instalado.
- Um PAT do CoreLink (`corelink_pat_...`).
- O UUID do seu tenant.

## Configurar o `.npmrc`

Adicione o registro do seu tenant e seu token de autenticação ao `.npmrc` (local do projeto ou
`~/.npmrc`):

```ini
registry=https://corelink-api.humangr.com/npm/<your-tenant-id>/
//corelink-api.humangr.com/npm/<your-tenant-id>/:_authToken=corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX
```

O npm envia o `_authToken` como `Authorization: Bearer <token>`, que é exatamente
o que o CoreLink espera. Depois, instale normalmente:

```bash
npm install
```

:::tip Mantenha o token fora do repositório
Faça commit apenas da linha `registry=`. Forneça a linha `_authToken` a partir de um
`~/.npmrc` específico do ambiente ou de um secret de CI, para que o PAT nunca seja commitado.
:::

## Verificar se funcionou

Exclua `node_modules` e reinstale; a segunda instalação deve ser servida a partir do
CoreLink:

```bash
rm -rf node_modules
npm install --loglevel http 2>&1 | grep corelink-api.humangr.com | head
```

Ver requisições para `corelink-api.humangr.com/npm/...` no log HTTP confirma que o
npm está usando o espelho.

## Resolução de problemas

| Sintoma | Causa provável | Correção |
|---|---|---|
| `401 Unauthorized` | Linha `_authToken` ausente ou malformada | O host + caminho da linha do token devem corresponder exatamente ao `registry=`, e o token deve ser um PAT `corelink_pat_...` |
| Instalações ainda acessam `registry.npmjs.org` | `registry=` não foi captado | Confirme o escopo do `.npmrc` (projeto vs. usuário) e execute novamente `npm config get registry` |
| `EINTEGRITY` | O tarball do upstream mudou | O CoreLink verifica o `dist.shasum` e falha de forma fechada em caso de divergência — tente novamente ou reporte o pacote do upstream |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
