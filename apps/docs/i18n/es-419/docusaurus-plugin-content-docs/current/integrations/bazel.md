---
id: bazel
title: Integración con Bazel
sidebar_position: 1
description: Configure Bazel para usar CoreLink como su caché remoto mediante .bazelrc.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/bazel.md`

# Integración con Bazel

CoreLink implementa el caché de la **Bazel Remote Execution API v2 (REAPI v2)** como
un esquema REST ByteStream:

```text
https://corelink-api.humangr.com/bazel/v2/<your-tenant-id>/blobs/<hash>/<size>
```

El segmento de ruta `<instance>` es el UUID de su tenant.

:::tip Dos formas de apuntar Bazel a CoreLink — ambas activas
- **Caché remoto plain-HTTP estándar (lo más simple):** apunte `--remote_cache` al
  **alias stock-HTTP** `https://corelink-api.humangr.com/bazel/cache` — sirve los
  caminos `/cas/<sha256>` y `/ac/<sha256>` que Bazel estándar emite (`PUT`→`204`, `GET`→`200`).
  No se necesita ningún cliente REAPI.
- **REAPI v2 / ByteStream:** apunte `--remote_cache` a `/bazel/v2` (la configuración de este documento)
  para un cliente compatible con ByteStream.

Ambos se autentican con Bearer-PAT. (Bazel usa direccionamiento por contenido con SHA-256, que las
rutas `/bazel/*` aceptan; el CAS REST *nativo* en `/v1/cas/...` usa claves BLAKE3 — vea
[HTTP sin procesar (curl)](./raw-curl).)
:::

## Requisitos previos

- Un cliente Bazel compatible con REAPI/ByteStream.
- Un PAT de CoreLink (`corelink_pat_...`) con alcance de lectura + escritura de caché. Vea [creación de PAT](../concepts/tenancy.md).

## Configurar `.bazelrc`

El repositorio incluye una configuración de referencia versionada en
[`apps/examples/bazel/.bazelrc`](https://github.com/HumanGuardrail/corelink-server/blob/main/apps/examples/bazel/.bazelrc).
Apunta la instancia REAPI de Bazel a su tenant:

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

Exporte ambos valores antes de compilar; en CI, pase el PAT desde un secret para que nunca
aparezca de forma literal:

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXX"   # ${{ secrets.CORELINK_PAT }} in CI
export CORELINK_TENANT="acme-prod"
```

## Verificar que funcionó

Después de ejecutar una compilación, verifique que el PAT y el tenant sean reconocidos:

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

Para comprobar un acierto de caché, ejecute la misma compilación dos veces. Inspeccione el registro
de ejecución de Bazel (`--execution_log_json_file`) en busca de entradas `remoteCacheHit: true` en la
segunda ejecución.

## Resolución de problemas específicos de Bazel

| Síntoma | Causa probable | Solución |
|---|---|---|
| Cada solicitud devuelve 404 | `--remote_cache` apunta al prefijo incorrecto | Use `/bazel/cache` (plain-HTTP estándar) o `/bazel/v2` (REAPI/ByteStream) — ambos activos; un host simple o `/v1/cas` devolverá 404 para las rutas de Bazel |
| `UNAUTHENTICATED` | Encabezado `Authorization` ausente o incorrecto | Verifique que `CORELINK_PAT` esté exportado en su shell / entorno de CI |
| `PERMISSION_DENIED` / 403 | El nombre de la instancia no es su tenant | Establezca `--remote_instance_name` como el UUID de su tenant |
| Caché fallido en cada compilación | `--remote_upload_local_results=false` | Establézcalo en `true` en al menos un job de CI |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
