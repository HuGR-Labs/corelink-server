---
id: turborepo
title: Integração com o Turborepo
sidebar_position: 2
description: Configure o Turborepo para usar o CoreLink como seu cache remoto via TURBO_API e TURBO_TOKEN.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/turborepo.md`

# Integração com o Turborepo

O CoreLink implementa o protocolo `/v8/artifacts` do Vercel Remote Cache, de modo que o
Turborepo possa usar o CoreLink como um substituto direto para o cache remoto do Vercel.

## Como funciona

O Turborepo dá suporte a caches remotos personalizados por meio de duas variáveis de ambiente:

- `TURBO_API` — a **origem pura** do servidor de cache remoto. O Turborepo
  anexa seu próprio caminho `/v8/artifacts/...` — **não** adicione você mesmo nenhum caminho ou
  segmento de tenant.
- `TURBO_TOKEN` — o seu PAT do CoreLink, passado como `Authorization: Bearer`.

O seu tenant é resolvido a partir do PAT, **não** da URL. O `teamId`
que o Turborepo envia é tratado como um subnamespace lógico *dentro* do seu
tenant autenticado (times sob um mesmo tenant permanecem particionados) — ele não é uma
fronteira de segurança e não aparece na URL base.

## Configuração

### Opção A: Variáveis de ambiente (recomendado para CI)

```bash
export TURBO_API="https://corelink-api.humangr.com"
export TURBO_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
```

Depois, execute o Turborepo normalmente (passe um label `--team` para que o Turborepo habilite o cache
remoto):

```bash
npx turbo run build --team=acme --token="$TURBO_TOKEN"
```

### Opção B: `.turbo/config.json` (por repositório)

```json
{
  "teamId": "acme",
  "apiUrl": "https://corelink-api.humangr.com"
}
```

Com este arquivo na raiz do seu repositório, o Turborepo lê o label do time e a URL da API automaticamente. Ainda assim, defina `TURBO_TOKEN` como uma variável de ambiente — não faça commit do token.

### Opção C: configuração remota em `turbo.json`

```json
{
  "$schema": "https://turbo.build/schema.json",
  "remoteCache": {
    "enabled": true
  }
}
```

Isso habilita o cache remoto; a URL e o token vêm de variáveis de ambiente.

## Exemplo do GitHub Actions

```yaml
- name: Build with Turborepo + CoreLink cache
  env:
    TURBO_API: https://corelink-api.humangr.com
    TURBO_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: npx turbo run build test --team=acme --token="$TURBO_TOKEN"
```

Armazene o PAT em `Settings → Secrets and variables → Actions` como `CORELINK_PAT`.

## Verificar se funcionou

Após configurar, execute seu pipeline duas vezes. Na segunda execução, o Turborepo deve reportar acertos de cache remoto:

```text
• Packages in scope: web, api, shared
• Running build in 3 packages
• Remote caching enabled

web:build  cache hit, replaying output...  0.8s
api:build  cache hit, replaying output...  0.6s
shared:build  cache hit, replaying output...  0.3s
```

Você também pode confirmar que o token é válido:

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

## Resolução de problemas

| Sintoma | Causa provável | Correção |
|---|---|---|
| `Remote caching disabled` | `TURBO_TOKEN` não definido | Exporte `TURBO_TOKEN` no seu shell ou ambiente de CI |
| Erros `401` na saída do Turborepo | PAT incorreto ou expirado | Regenere o PAT no painel de administração |
| Falhas de cache em toda execução | `TURBO_API` tem um segmento de caminho extra | `TURBO_API` deve ser a **origem pura** `https://corelink-api.humangr.com` — sem `/turbo`, `/v8` ou sufixo de tenant |
| `400 Bad Request` no PUT/GET de artefato | Label de time ausente | Passe `--team=<label>` (ou defina `teamId` em `.turbo/config.json`) |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
