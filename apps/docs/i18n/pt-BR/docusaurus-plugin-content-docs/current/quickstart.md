---
id: quickstart
title: Quickstart de 5 minutos
sidebar_position: 2
description: Cadastre-se, instale a CLI, envie seu primeiro blob e baixe-o de volta. Menos de 5 minutos a partir de um shell novo.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/quickstart.md`

# Quickstart de 5 minutos

Objetivo: autenticado, primeiro push e pull, verificado em menos de 5 minutos.

:::note CLI em breve
A `corelink-cli` está em desenvolvimento ativo (stream 1.1). Todos os exemplos abaixo funcionam **hoje** com `curl`. Assim que a CLI for lançada, os comandos `corelink` equivalentes serão mostrados ao lado.
:::

## Passo 1 — Obtenha um PAT

1. Cadastre-se em [humangr.com/corelink/sign-up](https://humangr.com/corelink/sign-up).
2. Depois que o assistente de onboarding de 2 etapas terminar, seu inquilino é provisionado e um PAT inicial é mostrado **exatamente uma vez** na tela de boas-vindas.
3. Copie o PAT e armazene-o em um gerenciador de segredos (1Password, AWS Secrets Manager, secret do GitHub Actions — qualquer coisa, menos texto puro). Ele nunca mais será mostrado.

Seu PAT se parece com:

```text
corelink_pat_01ARZ3NDEKTSV4RRFFQ69G5FAV.4pT7q1yZ9vX2wL8cR5nB3sD6fH0jK1mQ8aV2eS.7bY4tN9oL2x
```

(EN note: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` — `<env>` is `pat` for a user-issued token; `ci`/`ro` exist for CI and read-only tokens.)

Exporte-o para os exemplos abaixo:

```bash
export CORELINK_PAT="corelink_pat_01ARZ3NDEKTSV4RRFFQ69G5FAV.4pT7q1yZ9vX2wL8cR5nB3sD6fH0jK1mQ8aV2eS.7bY4tN9oL2x"
export CORELINK_TENANT="your-tenant-id"   # shown on the welcome screen
```

## Passo 2 — Verifique suas credenciais

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Resposta esperada:

```json
{
  "tenant_id": "your-tenant-id",
  "token_prefix": "aZ3xQ1",
  "route_kind": "cas"
}
```

Se você receber `401 Unauthorized`, o PAT está errado ou expirou — gere um novo no painel de administração.

## Passo 3 — Envie um blob

O CAS nativo endereça por conteúdo todo blob pelo seu digest **BLAKE3** (hex minúsculo) — calcule-o com `b3sum`, **não** com `sha256sum` (`brew install b3sum`, ou `cargo install b3sum`). Depois faça o upload:

```bash
# Compute the BLAKE3 digest
DIGEST=$(b3sum ./my-artifact.bin | awk '{print $1}')

# Upload
curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./my-artifact.bin \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
```

Resposta esperada (HTTP 201) — o corpo ecoa o hex BLAKE3 armazenado:

```json
{"hash": "<blake3-hex>"}
```

> Se você calcular o digest com `sha256sum`, o upload falha com **422 content hash
> mismatch** — o servidor recalcula o hash do corpo com BLAKE3 e ele não corresponderá a um
> digest SHA-256 na URL.

## Passo 4 — Baixe o blob de volta

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./my-artifact-downloaded.bin
```

Verifique se os bytes são idênticos:

```bash
diff my-artifact.bin my-artifact-downloaded.bin && echo "match"
```

## Passo 5 — Conecte sua ferramenta de build

Assim que tiver um PAT funcionando, conecte sua ferramenta de build:

- **Bazel** → [guia de integração do Bazel](./integrations/bazel.md)
- **Turborepo** → [guia de integração do Turborepo](./integrations/turborepo.md)
- **HTTP bruto / scripting** → [exemplos com curl bruto](./integrations/raw-curl.md)

## Solução de problemas

| Erro | Causa | Correção |
|---|---|---|
| `401 Unauthorized` | PAT inválido ou expirado | Gere novamente no painel de administração |
| `403 Forbidden` | PAT com escopo para um inquilino diferente | Verifique se `CORELINK_TENANT` corresponde ao inquilino do seu PAT |
| `404 Not Found` no GET | Blob ainda não enviado | Faça o push primeiro, depois o pull |
| `422 Unprocessable Entity` | O SHA-256 na URL não corresponde ao corpo | Recalcule o digest a partir dos bytes reais do arquivo |

Referência completa de erros: [Solução de problemas](./troubleshooting.md).
