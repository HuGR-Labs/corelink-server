---
id: troubleshooting
title: Solução de problemas
sidebar_position: 10
description: Códigos de erro comuns, o que significam e como corrigi-los.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/troubleshooting.md`

# Solução de problemas

## Referência de erros

### `401 Unauthorized`

**Significado**: O cabeçalho `Authorization` está ausente, malformado, ou o PAT foi revogado.

**Diagnóstico**:

```bash
# Confirm the PAT is set in your shell
echo $CORELINK_PAT

# Test the PAT directly
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Se `/v1/users/me` retornar `401`, o PAT é inválido. Possibilidades:

1. O PAT foi revogado no painel de administração.
2. O PAT nunca foi exportado (`export CORELINK_PAT=...` vs `CORELINK_PAT=...`).
3. Você está usando um PAT de teste (`clk_test_...`) contra a API de produção.

**Correção**: Gere um novo PAT no painel de administração. Armazene-o em um gerenciador de segredos antes de fechar a aba.

---

### `403 Forbidden`

**Significado**: O PAT é válido, mas não tem permissão para executar a operação solicitada.

Dois subcasos:

1. **Divergência de escopo**: o PAT foi criado apenas com `cas:read`, mas você está tentando escrever.
2. **Divergência de tenant**: o PAT pertence a `acme-prod`, mas a URL da requisição contém `acme-staging`.

**Diagnóstico**:

```bash
# Confirm your tenant
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# Check "tenant_id" in the response

# Confirm tenant in URL matches
echo $CORELINK_TENANT
```

**Correção**: Crie um PAT com os escopos corretos, ou corrija a variável de ambiente `CORELINK_TENANT`.

---

### `404 Not Found`

**Significado**: O blob com o digest informado não existe no CAS do tenant.

**Causas comuns**:

- Você está lendo de um tenant diferente daquele que fez o upload.
- O blob nunca foi enviado (comum em um tenant novo ou em um novo pipeline de CI).
- O digest foi calculado incorretamente.

**Diagnóstico**:

```bash
# Verify the digest
sha256sum ./my-file.bin
# Compare to the hash you are requesting
```

**Correção**: Envie o blob antes de buscá-lo. Confirme que o tenant na URL corresponde ao tenant que fez o upload.

---

### `422 Unprocessable Entity` (divergência de hash)

**Significado**: O SHA-256 no caminho da URL não corresponde ao SHA-256 do corpo da requisição.

Este é um erro do lado do cliente. O servidor calcula o digest dos bytes recebidos e o compara com o segmento de caminho. Se forem diferentes, o upload é rejeitado.

**Causas comuns**:

- O digest foi calculado antes da compressão (por exemplo, calculado em `file.tar` e depois enviado `file.tar.gz`).
- O digest foi calculado em uma leitura parcial.
- Ferramenta de upload multipart que adiciona bytes de framing.

**Correção**:

```bash
# Always compute the digest from the exact bytes being sent
DIGEST=$(sha256sum ./artifact.tar.gz | awk '{print $1}')
curl -X PUT ... --data-binary @./artifact.tar.gz \
  ".../v1/cas/$CORELINK_TENANT/$DIGEST"
```

---

### `429 Too Many Requests`

**Significado**: Você excedeu o limite de taxa para esta operação.

A resposta inclui um cabeçalho `Retry-After` indicando quantos segundos esperar.

**Correção**:

```bash
# Parse the retry delay from the response
curl -si ... | grep -i retry-after
```

Para throughput alto e sustentado, contate o suporte para aumentar os limites do seu plano.

---

### `503 Service Unavailable` com `audit_closed`

**Significado**: O período de auditoria do seu tenant foi fechado por uma operação de administração. As operações de escrita ficam suspensas até que o período de auditoria seja reaberto.

Isso normalmente é acionado durante uma auditoria de conformidade ou uma disputa de cobrança. Leituras (downloads) continuam disponíveis.

**Correção**: Contate o suporte do CoreLink em [support@corelink.humangr.com](mailto:support@corelink.humangr.com) informando o ID do seu tenant.

---

## Problemas específicos do Bazel

### `remote_cache: UNAUTHENTICATED`

O cabeçalho `authorization` não foi encaminhado. Verifique:

1. `CORELINK_PAT` está exportado no shell onde o Bazel roda.
2. Seu `.bazelrc` usa `${CORELINK_PAT}` (expansão de shell), não um placeholder literal.

### `remote_cache: PERMISSION_DENIED`

Divergência de tenant. Verifique se `x-corelink-tenant` no `.bazelrc` corresponde ao `tenant_id` de `/v1/users/me`.

### Todas as ações dão miss em builds repetidos

`--remote_upload_local_results` está como `false`. Adicione:

```text
build --remote_upload_local_results=true
```

---

## Problemas específicos do Turborepo

### `Remote caching disabled`

`TURBO_TOKEN` não está definido. No seu shell ou ambiente de CI:

```bash
export TURBO_TOKEN="$CORELINK_PAT"
```

### Cache misses em toda execução do Turborepo

Verifique se `TURBO_API` contém o sufixo de tenant correto:

```bash
echo $TURBO_API
# should be: https://corelink-api.humangr.com/turbo/v8/acme-prod
```

---

## Checklist de autodiagnóstico

Execute estes na ordem:

```bash
# 1. Network reachability
curl -s https://corelink-api.humangr.com/api/health
# {"status":"ok"}

# 2. PAT validity
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"...","token_prefix":"clk_live","route_kind":"cas"}

# 3. Write a test blob
echo "healthcheck" > /tmp/cl-test.txt
DIGEST=$(sha256sum /tmp/cl-test.txt | awk '{print $1}')
curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  --data-binary @/tmp/cl-test.txt \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
# {"hash":"sha256:<digest>"}

# 4. Read it back
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
# healthcheck
```

Se todos os quatro passos passarem, o CoreLink está funcionando. Qualquer falha antes do passo 4 significa que o problema está na configuração da sua ferramenta de build, não no CoreLink.

## Como obter ajuda

- GitHub Issues: [github.com/HumanGuardrail/corelink-server/issues](https://github.com/HumanGuardrail/corelink-server/issues)
- Suporte por e-mail: [support@corelink.humangr.com](mailto:support@corelink.humangr.com)

Ao abrir uma solicitação de suporte, inclua a saída dos passos de 1 a 4 acima e o ID do seu tenant.
