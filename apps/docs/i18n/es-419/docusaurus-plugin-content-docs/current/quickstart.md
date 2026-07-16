---
id: quickstart
title: Quickstart de 5 minutos
sidebar_position: 2
description: Regístrese, instale la CLI, suba su primer blob y descárguelo de vuelta. Menos de 5 minutos desde un shell nuevo.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/quickstart.md`

# Quickstart de 5 minutos

Objetivo: autenticado, primer push y pull, verificado en menos de 5 minutos.

:::note CLI próximamente
`corelink-cli` está en desarrollo activo (stream 1.1). Todos los ejemplos a continuación funcionan **hoy** con `curl`. Una vez que se lance la CLI, se mostrarán al lado los comandos `corelink` equivalentes.
:::

## Paso 1 — Obtenga un PAT

1. Regístrese en [humangr.com/corelink/sign-up](https://humangr.com/corelink/sign-up).
2. Después de que finalice el asistente de onboarding de 2 pasos, su inquilino queda aprovisionado y se muestra un PAT inicial **exactamente una vez** en la pantalla de bienvenida.
3. Copie el PAT y guárdelo en un gestor de secretos (1Password, AWS Secrets Manager, secret de GitHub Actions — cualquier cosa menos texto plano). No se vuelve a mostrar nunca más.

Su PAT se ve así:

```text
clk_live_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

Expórtelo para los ejemplos a continuación:

```bash
export CORELINK_PAT="clk_live_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export CORELINK_TENANT="your-tenant-id"   # shown on the welcome screen
```

## Paso 2 — Verifique sus credenciales

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Respuesta esperada:

```json
{
  "tenant_id": "your-tenant-id",
  "token_prefix": "clk_live",
  "route_kind": "cas"
}
```

Si obtiene `401 Unauthorized`, el PAT es incorrecto o está vencido — genere uno nuevo desde el panel de administración.

## Paso 3 — Suba un blob

El CAS nativo direcciona por contenido cada blob por su digest **BLAKE3** (hex en minúsculas) — calcúlelo con `b3sum`, **no** con `sha256sum` (`brew install b3sum`, o `cargo install b3sum`). Luego suba:

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

Respuesta esperada (HTTP 201) — el cuerpo devuelve el hex BLAKE3 almacenado:

```json
{"hash": "<blake3-hex>"}
```

> Si calcula el digest con `sha256sum`, la subida falla con **422 content hash
> mismatch** — el servidor vuelve a hashear el cuerpo con BLAKE3 y no coincidirá con un
> digest SHA-256 en la URL.

## Paso 4 — Descargue el blob de vuelta

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./my-artifact-downloaded.bin
```

Verifique que los bytes sean idénticos:

```bash
diff my-artifact.bin my-artifact-downloaded.bin && echo "match"
```

## Paso 5 — Conecte su herramienta de compilación

Una vez que tenga un PAT funcionando, conecte su herramienta de compilación:

- **Bazel** → [guía de integración de Bazel](./integrations/bazel.md)
- **Turborepo** → [guía de integración de Turborepo](./integrations/turborepo.md)
- **HTTP sin procesar / scripting** → [ejemplos con curl sin procesar](./integrations/raw-curl.md)

## Solución de problemas

| Error | Causa | Solución |
|---|---|---|
| `401 Unauthorized` | PAT inválido o vencido | Vuelva a generarlo desde el panel de administración |
| `403 Forbidden` | PAT con alcance a un inquilino diferente | Verifique que `CORELINK_TENANT` coincida con el inquilino de su PAT |
| `404 Not Found` en GET | Blob aún no subido | Haga el push primero, luego el pull |
| `422 Unprocessable Entity` | El SHA-256 en la URL no coincide con el cuerpo | Vuelva a calcular el digest a partir de los bytes reales del archivo |

Referencia completa de errores: [Solución de problemas](./troubleshooting.md).
