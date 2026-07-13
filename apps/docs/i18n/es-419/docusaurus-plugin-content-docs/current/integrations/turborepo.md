---
id: turborepo
title: Integración con Turborepo
sidebar_position: 2
description: Configure Turborepo para usar CoreLink como su caché remoto mediante TURBO_API y TURBO_TOKEN.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/turborepo.md`

# Integración con Turborepo

CoreLink implementa el protocolo `/v8/artifacts` del Vercel Remote Cache, de modo que
Turborepo puede usar CoreLink como un reemplazo directo del caché remoto de Vercel.

## Cómo funciona

Turborepo admite cachés remotos personalizados mediante dos variables de entorno:

- `TURBO_API` — el **origen puro** del servidor de caché remoto. Turborepo
  agrega su propio camino `/v8/artifacts/...` — **no** agregue usted mismo ningún camino ni
  segmento de tenant.
- `TURBO_TOKEN` — su PAT de CoreLink, pasado como `Authorization: Bearer`.

Su tenant se resuelve a partir del PAT, **no** de la URL. El `teamId`
que Turborepo envía se trata como un subespacio de nombres lógico *dentro* de su
tenant autenticado (los equipos bajo un mismo tenant permanecen particionados) — no es una
frontera de seguridad y no aparece en la URL base.

## Configuración

### Opción A: Variables de entorno (recomendado para CI)

```bash
export TURBO_API="https://corelink-api.humangr.com"
export TURBO_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
```

Luego ejecute Turborepo de forma normal (pase un label `--team` para que Turborepo habilite el caché
remoto):

```bash
npx turbo run build --team=acme --token="$TURBO_TOKEN"
```

### Opción B: `.turbo/config.json` (por repositorio)

```json
{
  "teamId": "acme",
  "apiUrl": "https://corelink-api.humangr.com"
}
```

Con este archivo en la raíz de su repositorio, Turborepo lee el label del equipo y la URL de la API automáticamente. Aun así, establezca `TURBO_TOKEN` como una variable de entorno — no haga commit del token.

### Opción C: configuración remota en `turbo.json`

```json
{
  "$schema": "https://turbo.build/schema.json",
  "remoteCache": {
    "enabled": true
  }
}
```

Esto habilita el caché remoto; la URL y el token provienen de variables de entorno.

## Ejemplo de GitHub Actions

```yaml
- name: Build with Turborepo + CoreLink cache
  env:
    TURBO_API: https://corelink-api.humangr.com
    TURBO_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: npx turbo run build test --team=acme --token="$TURBO_TOKEN"
```

Almacene el PAT en `Settings → Secrets and variables → Actions` como `CORELINK_PAT`.

## Verificar que funcionó

Después de configurar, ejecute su pipeline dos veces. En la segunda ejecución, Turborepo debería reportar aciertos de caché remoto:

```text
• Packages in scope: web, api, shared
• Running build in 3 packages
• Remote caching enabled

web:build  cache hit, replaying output...  0.8s
api:build  cache hit, replaying output...  0.6s
shared:build  cache hit, replaying output...  0.3s
```

También puede confirmar que el token es válido:

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

## Resolución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| `Remote caching disabled` | `TURBO_TOKEN` no establecido | Exporte `TURBO_TOKEN` en su shell o entorno de CI |
| Errores `401` en la salida de Turborepo | PAT incorrecto o expirado | Regenere el PAT desde el panel de administración |
| Fallos de caché en cada ejecución | `TURBO_API` tiene un segmento de camino extra | `TURBO_API` debe ser el **origen puro** `https://corelink-api.humangr.com` — sin `/turbo`, `/v8` ni sufijo de tenant |
| `400 Bad Request` en el PUT/GET de artefacto | Falta el label del equipo | Pase `--team=<label>` (o establezca `teamId` en `.turbo/config.json`) |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
