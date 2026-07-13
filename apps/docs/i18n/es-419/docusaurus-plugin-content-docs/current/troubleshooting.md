---
id: troubleshooting
title: Solución de problemas
sidebar_position: 10
description: Códigos de error comunes, qué significan y cómo solucionarlos.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/troubleshooting.md`

# Solución de problemas

## Referencia de errores

### `401 Unauthorized`

**Significado**: El encabezado `Authorization` falta, está malformado, o el PAT fue revocado.

**Diagnóstico**:

```bash
# Confirm the PAT is set in your shell
echo $CORELINK_PAT

# Test the PAT directly
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Si `/v1/users/me` devuelve `401`, el PAT es inválido. Posibilidades:

1. El PAT fue revocado desde el panel de administración.
2. El PAT nunca se exportó (`export CORELINK_PAT=...` vs `CORELINK_PAT=...`).
3. Está usando un PAT de prueba (`clk_test_...`) contra la API de producción.

**Solución**: Regenere un PAT desde el panel de administración. Guárdelo en un gestor de secretos antes de cerrar la pestaña.

---

### `403 Forbidden`

**Significado**: El PAT es válido pero no tiene permiso para realizar la operación solicitada.

Dos subcasos:

1. **Discrepancia de scope**: el PAT se creó solo con `cas:read`, pero está intentando escribir.
2. **Discrepancia de tenant**: el PAT pertenece a `acme-prod` pero la URL de la solicitud contiene `acme-staging`.

**Diagnóstico**:

```bash
# Confirm your tenant
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# Check "tenant_id" in the response

# Confirm tenant in URL matches
echo $CORELINK_TENANT
```

**Solución**: Cree un PAT con los scopes correctos, o corrija la variable de entorno `CORELINK_TENANT`.

---

### `404 Not Found`

**Significado**: El blob con el digest indicado no existe en el CAS del tenant.

**Causas comunes**:

- Está leyendo desde un tenant distinto al que hizo la carga.
- El blob nunca se subió (común en un tenant nuevo o un nuevo pipeline de CI).
- El digest se calculó incorrectamente.

**Diagnóstico**:

```bash
# Verify the digest
sha256sum ./my-file.bin
# Compare to the hash you are requesting
```

**Solución**: Suba el blob antes de descargarlo. Confirme que el tenant en la URL coincida con el tenant que hizo la carga.

---

### `422 Unprocessable Entity` (discrepancia de hash)

**Significado**: El SHA-256 en la ruta de la URL no coincide con el SHA-256 del cuerpo de la solicitud.

Este es un error del lado del cliente. El servidor calcula el digest de los bytes recibidos y lo compara con el segmento de ruta. Si difieren, la carga se rechaza.

**Causas comunes**:

- El digest se calculó antes de la compresión (p. ej., calculado sobre `file.tar` y luego se subió `file.tar.gz`).
- El digest se calculó sobre una lectura parcial.
- Herramienta de carga multipart que agrega bytes de framing.

**Solución**:

```bash
# Always compute the digest from the exact bytes being sent
DIGEST=$(sha256sum ./artifact.tar.gz | awk '{print $1}')
curl -X PUT ... --data-binary @./artifact.tar.gz \
  ".../v1/cas/$CORELINK_TENANT/$DIGEST"
```

---

### `429 Too Many Requests`

**Significado**: Ha excedido el límite de tasa para esta operación.

La respuesta incluye un encabezado `Retry-After` que indica cuántos segundos esperar.

**Solución**:

```bash
# Parse the retry delay from the response
curl -si ... | grep -i retry-after
```

Para un throughput alto y sostenido, contacte a soporte para elevar los límites de su plan.

---

### `503 Service Unavailable` con `audit_closed`

**Significado**: El período de auditoría de su tenant ha sido cerrado por una operación de administración. Las operaciones de escritura quedan suspendidas hasta que el período de auditoría se reabra.

Esto suele desencadenarse durante una auditoría de cumplimiento o una disputa de facturación. Las lecturas (descargas) siguen disponibles.

**Solución**: Contacte a soporte de CoreLink en [support@corelink.humangr.com](mailto:support@corelink.humangr.com) indicando el ID de su tenant.

---

## Problemas específicos de Bazel

### `remote_cache: UNAUTHENTICATED`

El encabezado `authorization` no se reenvió. Verifique:

1. `CORELINK_PAT` está exportado en el shell donde se ejecuta Bazel.
2. Su `.bazelrc` usa `${CORELINK_PAT}` (expansión de shell), no un placeholder literal.

### `remote_cache: PERMISSION_DENIED`

Discrepancia de tenant. Verifique que `x-corelink-tenant` en `.bazelrc` coincida con `tenant_id` de `/v1/users/me`.

### Todas las acciones fallan en builds repetidos

`--remote_upload_local_results` está en `false`. Agregue:

```text
build --remote_upload_local_results=true
```

---

## Problemas específicos de Turborepo

### `Remote caching disabled`

`TURBO_TOKEN` no está definido. En su shell o entorno de CI:

```bash
export TURBO_TOKEN="$CORELINK_PAT"
```

### Cache misses en cada ejecución de Turborepo

Verifique que `TURBO_API` contenga el sufijo de tenant correcto:

```bash
echo $TURBO_API
# should be: https://corelink-api.humangr.com/turbo/v8/acme-prod
```

---

## Lista de autodiagnóstico

Ejecute estos en orden:

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

Si los cuatro pasos pasan, CoreLink está funcionando. Cualquier fallo antes del paso 4 significa que el problema está en la configuración de su herramienta de build, no en CoreLink.

## Cómo obtener ayuda

- GitHub Issues: [github.com/HumanGuardrail/corelink-server/issues](https://github.com/HumanGuardrail/corelink-server/issues)
- Soporte por correo electrónico: [support@corelink.humangr.com](mailto:support@corelink.humangr.com)

Al abrir una solicitud de soporte, incluya la salida de los pasos 1 a 4 anteriores y el ID de su tenant.
