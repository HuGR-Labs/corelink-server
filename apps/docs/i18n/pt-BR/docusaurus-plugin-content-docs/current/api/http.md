---
id: http
title: Referência da API HTTP
sidebar_position: 1
description: Autenticação, endpoints, formatos de requisição/resposta e códigos de erro da API REST do CoreLink.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/api/http.md`

# Referência da API HTTP

URL base: `https://corelink-api.humangr.com`

Todos os endpoints exigem HTTPS. HTTP não é aceito.

## Autenticação

Toda requisição deve incluir um cabeçalho `Authorization: Bearer <PAT>`.

```bash
curl -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Nenhum outro esquema de autenticação (Basic, cabeçalho de API key, parâmetro de query) é aceito. Se o cabeçalho estiver ausente ou malformado, a API retorna `401`.

## Endpoints

### `GET /v1/users/me`

Retorna a identidade do PAT usado na requisição.

**Requisição**

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

**Resposta 200**

```json
{
  "tenant_id": "acme-prod",
  "token_prefix": "aZ3xQ1",
  "route_kind": "cas"
}
```

| Campo | Tipo | Descrição |
|---|---|---|
| `tenant_id` | string | O tenant ao qual este PAT está vinculado. Corresponde ao segmento de caminho nas URLs de CAS/AC. |
| `token_prefix` | string | Um identificador de 6 caracteres derivado de hash para correlação de log/rate-limit — não é um prefixo literal do seu token. |
| `route_kind` | string | Sempre `cas` para PATs do plano de dados. |

---

### `PUT /v1/cas/<tenant_id>/<blake3>`

Envia um blob. O CAS nativo é chaveado por **BLAKE3**: o digest BLAKE3 no caminho da URL deve corresponder ao BLAKE3 do corpo da requisição. Se não corresponder, o servidor retorna `422 content hash mismatch`. (Calcule-o com `b3sum` — **não** com `sha256sum`.)

**Parâmetros**

| Nome | Em | Obrigatório | Descrição |
|---|---|---|---|
| `tenant_id` | path | sim | Seu identificador de tenant. Deve corresponder ao tenant do PAT. |
| `blake3` | path | sim | BLAKE3 em hex minúsculo dos bytes do blob (64 caracteres). |

**Cabeçalhos**

| Cabeçalho | Obrigatório | Valor |
|---|---|---|
| `Authorization` | sim | `Bearer <PAT>` |
| `Content-Type` | recomendado | `application/octet-stream` |
| `Content-Length` | recomendado | tamanho do corpo em bytes |

**Requisição**

```bash
DIGEST=$(b3sum ./output.tar.gz | awk '{print $1}')   # BLAKE3, not sha256

curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./output.tar.gz \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST"
```

**Resposta 201** — o corpo ecoa o BLAKE3 armazenado em hex:

```json
{"hash": "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"}
```

**Resposta 409** — o blob já existe (idempotente; seguro ignorar)

```json
{"error": "conflict", "message": "blob already exists"}
```

---

### `GET /v1/cas/<tenant_id>/<blake3>`

Baixa um blob pelo digest.

**Parâmetros**

| Nome | Em | Obrigatório | Descrição |
|---|---|---|---|
| `tenant_id` | path | sim | Seu identificador de tenant. |
| `blake3` | path | sim | BLAKE3 em hex minúsculo. |

**Requisição**

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" \
  -o ./output-downloaded.tar.gz
```

**Resposta 200** — `Content-Type: application/octet-stream`, o corpo são bytes brutos.

**Resposta 404** — blob não está no CAS do tenant.

---

### `GET /api/health`

Verificação de saúde. Retorna `200 OK` com `{"status": "ok"}` quando o serviço está no ar. Não requer autenticação.

```bash
curl -s https://corelink-api.humangr.com/api/health
# {"status":"ok"}
```

---

### Gerenciamento de PATs (`GET`/`POST /v1/pats`, `DELETE /v1/pats/:pat_id`) — planejado, ainda não em produção

Essas rotas são especificadas para uma futura superfície self-service de
gerenciamento de PATs (listar, criar, revogar), mas **não estão conectadas
hoje** — chamar qualquer uma delas retorna `404`. O único caminho de criação
de PAT em produção é o PAT inicial automático emitido pelo assistente de
cadastro. Existe uma superfície admin-only, somente leitura, para o suporte
inspecionar os PATs de um tenant (`GET /v1/admin/tenants/{tenant_id}/pats`),
mas ela exige um PAT de administrador e não é chamável com um token comum de
cliente.

Até que o gerenciamento self-service de PATs esteja disponível, escreva para
[support@humangr.com](mailto:support@humangr.com) para emitir um PAT
adicional ou revogar um existente.

---


## Códigos de erro

| Status HTTP | Campo `error` | Significado | Correção |
|---|---|---|---|
| `400 Bad Request` | `bad_request` | Requisição malformada (JSON inválido, campo ausente) | Verifique o corpo da requisição |
| `401 Unauthorized` | `unauthorized` | PAT ausente ou inválido | Verifique o cabeçalho `Authorization` |
| `403 Forbidden` | `forbidden` | O PAT não tem o escopo necessário, ou há divergência de tenant | Verifique os escopos do PAT e o tenant no caminho da URL |
| `404 Not Found` | `not_found` | O blob não existe no CAS do tenant | Envie antes de baixar |
| `409 Conflict` | `conflict` | O blob já existe (PUT) | Idempotente — seguro ignorar |
| `422 Unprocessable Entity` | `content hash mismatch` | O BLAKE3 na URL não corresponde ao corpo (por exemplo, você usou `sha256sum`) | Recalcule com `b3sum` |
| `429 Too Many Requests` | `rate_limited` | Taxa de requisições excedida | Recue e tente novamente; veja o cabeçalho `Retry-After` |
| `503 Service Unavailable` | `audit_closed` | O período de auditoria do tenant está fechado — escritas temporariamente suspensas | Contate o suporte; leituras continuam funcionando |

Todas as respostas de erro compartilham este formato:

```json
{
  "error": "not_found",
  "message": "blob af1349b9... not found in tenant acme-prod"
}
```

## Limites de taxa

Os limites de taxa são aplicados por tenant e por família de endpoints. A resposta inclui:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 998
X-RateLimit-Reset: 1717000000
Retry-After: 60   (only on 429)
```

Limites padrão (sujeitos a alteração por plano):

| Operação | Limite |
|---|---|
| Leituras de CAS | 1 000 req/min por tenant |
| Escritas de CAS | 500 req/min por tenant |
| Gerenciamento (CRUD de PAT) | 60 req/min por tenant |

Planos Enterprise têm limites mais altos. Contate vendas para limites personalizados.

## Paginação

Os endpoints de listagem (log de auditoria, listagem de CAS) retornam paginação baseada em cursor:

```json
{
  "items": [...],
  "next_cursor": "eyJ...",
  "has_more": true
}
```

Passe `?cursor=<next_cursor>` para buscar a próxima página.
