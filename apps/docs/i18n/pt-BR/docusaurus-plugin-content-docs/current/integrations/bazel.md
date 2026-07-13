---
id: bazel
title: Integração com o Bazel
sidebar_position: 1
description: Configure o Bazel para usar o CoreLink como seu cache remoto via .bazelrc.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/integrations/bazel.md`

# Integração com o Bazel

O CoreLink implementa o cache da **Bazel Remote Execution API v2 (REAPI v2)** como
um esquema REST ByteStream:

```text
https://corelink-api.humangr.com/bazel/v2/<your-tenant-id>/blobs/<hash>/<size>
```

O segmento de caminho `<instance>` é o UUID do seu tenant.

:::tip Duas formas de apontar o Bazel para o CoreLink — ambas ativas
- **Cache remoto plain-HTTP padrão (o mais simples):** aponte `--remote_cache` para o
  **alias stock-HTTP** `https://corelink-api.humangr.com/bazel/cache` — ele serve os
  caminhos `/cas/<sha256>` e `/ac/<sha256>` que o Bazel padrão emite (`PUT`→`204`, `GET`→`200`).
  Nenhum cliente REAPI é necessário.
- **REAPI v2 / ByteStream:** aponte `--remote_cache` para `/bazel/v2` (a configuração deste documento)
  para um cliente compatível com ByteStream.

Ambos são autenticados por Bearer-PAT. (O Bazel usa endereçamento por conteúdo com SHA-256, que as
rotas `/bazel/*` aceitam; o CAS REST *nativo* em `/v1/cas/...` é chaveado por BLAKE3 — veja
[HTTP bruto (curl)](./raw-curl).)
:::

## Pré-requisitos

- Um cliente Bazel compatível com REAPI/ByteStream.
- Um PAT do CoreLink (`corelink_pat_...`) com escopo de leitura + escrita de cache. Veja [criação de PAT](../concepts/tenancy.md).

## Configurar o `.bazelrc`

O repositório inclui uma configuração de referência versionada em
[`apps/examples/bazel/.bazelrc`](https://github.com/HumanGuardrail/corelink-server/blob/main/apps/examples/bazel/.bazelrc).
Ela aponta a instância REAPI do Bazel para o seu tenant:

```ini
# Point at the CoreLink REAPI v2 endpoint (the /bazel/v2 prefix is required).
build --remote_cache=https://corelink-api.humangr.com/bazel/v2

# Your tenant UUID becomes the REAPI :instance path segment.
build --remote_instance_name=${CORELINK_TENANT}

# Authenticate with your PAT.
build --remote_header=Authorization=Bearer ${CORELINK_PAT}

build --remote_upload_local_results=true
build --remote_timeout=60
```

Exporte ambos os valores antes de compilar; na CI, passe o PAT a partir de um secret para que ele nunca
apareça literalmente:

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXX"   # ${{ secrets.CORELINK_PAT }} in CI
export CORELINK_TENANT="acme-prod"
```

## Verificar se funcionou

Depois de executar uma compilação, verifique se o PAT e o tenant são reconhecidos:

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

Para verificar um acerto de cache, execute a mesma compilação duas vezes. Inspecione o log de execução
do Bazel (`--execution_log_json_file`) em busca de entradas `remoteCacheHit: true` na segunda
execução.

## Resolução de problemas específicos do Bazel

| Sintoma | Causa provável | Correção |
|---|---|---|
| Toda requisição retorna 404 | `--remote_cache` aponta para o prefixo errado | Use `/bazel/cache` (plain-HTTP padrão) ou `/bazel/v2` (REAPI/ByteStream) — ambos ativos; um host puro ou `/v1/cas` retornará 404 para os caminhos do Bazel |
| `UNAUTHENTICATED` | Cabeçalho `Authorization` ausente ou incorreto | Verifique se `CORELINK_PAT` está exportado no seu shell / ambiente de CI |
| `PERMISSION_DENIED` / 403 | O nome da instância não é o seu tenant | Defina `--remote_instance_name` como o UUID do seu tenant |
| Cache miss em toda compilação | `--remote_upload_local_results=false` | Defina como `true` em pelo menos um job de CI |

Referência completa de erros: [Resolução de problemas](../troubleshooting.md).
