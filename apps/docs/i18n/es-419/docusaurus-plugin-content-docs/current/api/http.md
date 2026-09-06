---
id: http
title: Referencia de la API HTTP
sidebar_position: 1
description: Autenticación, endpoints, formatos de solicitud/respuesta y códigos de error de la API REST de CoreLink.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/api/http.md`

# Referencia de la API HTTP

URL base: `https://corelink-api.humangr.com`

Todos los endpoints requieren HTTPS. HTTP no se acepta.

## Autenticación

La mayoría de las solicitudes debe llevar un encabezado `Authorization: Bearer
<PAT>`. La emisión de un PAT de cliente por POST es la excepción del
navegador: el panel envía una cookie de sesión Clerk validada en el edge
(`__session`), mientras que los clientes CLI pueden usar un PAT canónico por
compatibilidad. El Worker deriva el tenant de la credencial y elimina el JWT
de Clerk antes de reenviar la solicitud al Durable Object del tenant; el
cliente nunca proporciona el tenant.

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

No se acepta ningún otro esquema de autenticación (Basic, encabezado de API key,
parámetro de query). Para `POST /v1/pats` (y su alias del panel
`POST /v1/customer/keys`), el navegador usa una sesión Clerk validada y la CLI
puede usar un PAT canónico. La autenticación ausente o malformada se rechaza
con `401`.

## Endpoints

### `GET /v1/users/me`

Devuelve la identidad del PAT usado en la solicitud.

**Solicitud**

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

**Respuesta 200**

```json
{
  "tenant_id": "acme-prod",
  "token_prefix": "aZ3xQ1",
  "route_kind": "cas"
}
```

| Campo | Tipo | Descripción |
|---|---|---|
| `tenant_id` | string | El tenant al que está vinculado este PAT. Coincide con el segmento de ruta en las URL de CAS/AC. |
| `token_prefix` | string | Un identificador de 6 caracteres derivado de un hash para correlación de logs/rate-limit — no es un prefijo literal de su token. |
| `route_kind` | string | Siempre `cas` para los PAT del plano de datos. |

---

### `PUT /v1/cas/<tenant_id>/<blake3>`

Sube un blob. El CAS nativo está indexado por **BLAKE3**: el digest BLAKE3 en la ruta de la URL debe coincidir con el BLAKE3 del cuerpo de la solicitud. Si no coincide, el servidor devuelve `422 content hash mismatch`. (Calcúlelo con `b3sum` — **no** con `sha256sum`.)

**Parámetros**

| Nombre | En | Obligatorio | Descripción |
|---|---|---|---|
| `tenant_id` | path | sí | Su identificador de tenant. Debe coincidir con el tenant del PAT. |
| `blake3` | path | sí | BLAKE3 en hex minúscula de los bytes del blob (64 caracteres). |

**Encabezados**

| Encabezado | Obligatorio | Valor |
|---|---|---|
| `Authorization` | sí | `Bearer <PAT>` |
| `Content-Type` | recomendado | `application/octet-stream` |
| `Content-Length` | recomendado | longitud del cuerpo en bytes |

**Solicitud**

```bash
DIGEST=$(b3sum ./output.tar.gz | awk '{print $1}')   # BLAKE3, not sha256

curl -s -X PUT \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./output.tar.gz \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

**Respuesta 201** — el cuerpo devuelve el BLAKE3 almacenado en hex:

```json
{"hash": "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"}
```

**Respuesta 409** — el blob ya existe (idempotente; seguro de ignorar)

```json
{"error": "conflict", "message": "blob already exists"}
```

---

### `GET /v1/cas/<tenant_id>/<blake3>`

Descarga un blob por su digest.

**Parámetros**

| Nombre | En | Obligatorio | Descripción |
|---|---|---|---|
| `tenant_id` | path | sí | Su identificador de tenant. |
| `blake3` | path | sí | BLAKE3 en hex minúscula. |

**Solicitud**

```bash
curl -s \
  "https://corelink-api.humangr.com/v1/cas/acme-prod/$DIGEST" \
  -o ./output-downloaded.tar.gz --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

**Respuesta 200** — `Content-Type: application/octet-stream`, el cuerpo son bytes crudos.

**Respuesta 404** — el blob no está en el CAS del tenant.

---

### `GET /api/health`

Verificación de estado. Devuelve `200 OK` con `{"status": "ok"}` cuando el servicio está activo. No requiere autenticación.

```bash
curl -s https://corelink-api.humangr.com/api/health
# {"status":"ok"}
```

---

### Emisión de PAT (`POST /v1/pats`)

Esta ruta self-service activa emite un PAT adicional ligado al tenant. El
navegador se autentica con una sesión Clerk validada; la CLI puede usar un PAT
canónico. El token en texto plano se devuelve exactamente una vez. El alias
compatible con el panel, `POST /v1/customer/keys`, usa el mismo flujo de mint y
el mismo limitador `pat-issue` por tenant (burst 10 y luego 10/hora; un token
cada 360 segundos), aplicado antes del mint y la auditoría. La lista es
`GET /v1/customer/keys`; la revocación del panel es
`POST /v1/customer/keys/{pat_id}/revoke`. Las operaciones públicas
`GET /v1/pats` y `DELETE /v1/pats/{pat_id}` siguen planificadas y no son alias
de la ruta del panel.

El tipo de contenido del error depende de la capa. Un `401` rechazado por el
Worker en el edge usa el envelope JSON `{error, message, request_id}`. Si la
solicitud llega al handler de cliente, este usa `text/plain` para sus respuestas
`400/401/403/429/500/503`. Los scopes `admin`/`owner` inválidos o no otorgables
son `401` del handler, no `422`; un cliente de solo lectura que solicite un PAT
de escritura recibe `403`. Las respuestas limitadas incluyen `Retry-After`.

---


## Códigos de error

| Estado HTTP | Campo `error` | Significado | Solución |
|---|---|---|---|
| `400 Bad Request` | `bad_request` | Solicitud malformada (JSON inválido, campo faltante) | Revise el cuerpo de la solicitud |
| `401 Unauthorized` | `unauthorized` | PAT faltante o inválido | Verifique el encabezado `Authorization` |
| `403 Forbidden` | `forbidden` | El PAT no tiene el scope requerido, o hay discrepancia de tenant | Revise los scopes del PAT y el tenant en la ruta de la URL |
| `404 Not Found` | `not_found` | El blob no existe en el CAS del tenant | Suba antes de descargar |
| `409 Conflict` | `conflict` | El blob ya existe (PUT) | Idempotente — seguro de ignorar |
| `422 Unprocessable Entity` | `content hash mismatch` | El BLAKE3 en la URL no coincide con el cuerpo (por ejemplo, usó `sha256sum`) | Recalcule con `b3sum` |
| `429 Too Many Requests` | `rate_limited` | Se excedió la tasa de solicitudes | Espere y reintente; consulte el encabezado `Retry-After` |
| `503 Service Unavailable` | `audit_closed` | El período de auditoría del tenant está cerrado — escrituras suspendidas temporalmente | Contacte a soporte; las lecturas siguen funcionando |

Las respuestas de REAPI y de los endpoints de clientes, salvo la emisión de
PAT, normalmente comparten este formato JSON. En la emisión de PAT, los
errores del edge son JSON y los errores del handler son `text/plain`, como se
describe arriba.

```json
{
  "error": "not_found",
  "message": "blob af1349b9... not found in tenant acme-prod"
}
```

## Límites de tasa

Los límites de tasa se aplican por tenant y por familia de endpoints. La respuesta incluye:

```
X-RateLimit-Limit: 1000
X-RateLimit-Remaining: 998
X-RateLimit-Reset: 1717000000
Retry-After: 60   (only on 429)
```

Límites predeterminados (sujetos a cambio según el plan):

| Operación | Límite |
|---|---|
| Lecturas de CAS | 1 000 req/min por tenant |
| Escrituras de CAS | 500 req/min por tenant |
| Gestión (CRUD de PAT) | 60 req/min por tenant |

Los planes Enterprise tienen límites más altos. Contacte a ventas para límites personalizados.

## Paginación

Los endpoints de listado (log de auditoría, listado de CAS) devuelven paginación basada en cursor:

```json
{
  "items": [...],
  "next_cursor": "eyJ...",
  "has_more": true
}
```

Pase `?cursor=<next_cursor>` para obtener la página siguiente.
