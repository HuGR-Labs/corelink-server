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

A maioria das requisições deve incluir um cabeçalho `Authorization: Bearer
<PAT>`. A emissão de PAT de cliente por POST é a exceção no navegador: o
dashboard envia um cookie de sessão Clerk validado no edge (`__session`),
enquanto clientes CLI podem usar um PAT canônico por compatibilidade. O Worker
deriva o tenant da credencial e remove o JWT do Clerk antes de encaminhar a
requisição ao Durable Object do tenant; o cliente nunca fornece o tenant.

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

Nenhum outro esquema de autenticação (Basic, cabeçalho de API key, parâmetro de
query) é aceito. Para `POST /v1/pats` (e seu alias do dashboard
`POST /v1/customer/keys`), o navegador usa uma sessão Clerk validada e a CLI
pode usar um PAT canônico. Autenticação ausente ou malformada é rejeitada com
`401`.

## Endpoints

### `GET /v1/users/me`

Retorna a identidade do PAT usado na requisição.

**Requisição**

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
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
  -H "Content-Type: application/octet-stream" \
  --data-binary @./output.tar.gz \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
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
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" \
  -o ./output-downloaded.tar.gz --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
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

### Emissão de PAT (`POST /v1/pats`)

Esta rota self-service ativa emite um PAT adicional vinculado ao tenant. O
navegador se autentica com uma sessão Clerk validada; a CLI pode usar um PAT
canônico. O token em texto puro é retornado exatamente uma vez. O alias
compatível com o dashboard, `POST /v1/customer/keys`, usa o mesmo fluxo de mint
e o mesmo limitador `pat-issue` por tenant (burst 10, depois 10/hora; um token a
cada 360 segundos), aplicado antes do mint e da auditoria. A listagem é
`GET /v1/customer/keys`; a revogação do dashboard é
`POST /v1/customer/keys/{pat_id}/revoke`. As operações públicas `GET /v1/pats`
e `DELETE /v1/pats/{pat_id}` continuam planejadas e não são aliases da rota do
dashboard.

O tipo de mídia do erro depende da camada. Um `401` rejeitado pelo Worker no
edge usa o envelope JSON `{error, message, request_id}`. Se a requisição chega
ao handler de cliente, ele usa `text/plain` para suas respostas
`400/401/403/429/500/503`. Scopes `admin`/`owner` inválidos ou não concedíveis
são `401` do handler, não `422`; um cliente somente leitura que peça um PAT de
escrita recebe `403`. Respostas limitadas incluem `Retry-After`.

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

As respostas de REAPI e dos endpoints de cliente, exceto a emissão de PAT,
normalmente compartilham este formato JSON. Na emissão de PAT, erros do edge
são JSON e erros do handler são `text/plain`, como descrito acima.

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
