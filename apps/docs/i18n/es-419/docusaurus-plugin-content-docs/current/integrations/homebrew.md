---
id: homebrew
title: Espejo de bottles de Homebrew
sidebar_position: 8
description: La ruta segura de espejo autenticado de botellas de Homebrew.
---

# Espejo de bottles de Homebrew

:::caution Ruta segura de espejo autenticado
El Homebrew moderno (el valor predeterminado `install-from-API`, Homebrew 4.x
y posteriores) puede reescribir sus URL de bottles de `ghcr.io` mediante
`HOMEBREW_ARTIFACT_DOMAIN`. El endpoint `/brew/<tenant>` de CoreLink acepta el
PAT bearer resultante y obtiene la bottle desde el upstream fijo `ghcr.io`.
Esta página se conserva porque el flujo autenticado y el flujo normal de
Homebrew son compatibles. Las fuentes de taps, los casks (`.dmg`/`.pkg`) y
`brew bottle` quedan fuera de este alcance.

La opción sin fallback es obligatoria. Sin ella, Homebrew reintenta la URL
original de `ghcr.io` cuando falla el espejo y puede enviar el bearer a GitHub
en vez de CoreLink. Nunca use la receta anterior que omitía esta protección.
:::

<!-- WP-B161-AUTH-NO-FALLBACK-20260901: el espejo autenticado queda fijado a CoreLink. -->

## Ruta segura: espejo autenticado de CoreLink

Obtenga un PAT real de CoreLink desde su gestor de secretos y expóngalo solo
como `CORELINK_PAT` en la shell donde ejecute Homebrew. No pegue el token en
documentos, historial de comandos ni registros. Establezca juntas las tres
variables siguientes:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1
export HOMEBREW_DOCKER_REGISTRY_TOKEN="$CORELINK_PAT"
brew install jq
```

Homebrew conserva su estrategia de GitHub Packages para la bottle de `ghcr.io`,
reescribe la URL al dominio de artefactos y convierte
`HOMEBREW_DOCKER_REGISTRY_TOKEN` en `Authorization: Bearer <token>`. Con
`HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1`, ese bearer solo se envía al dominio
de CoreLink; un fallo del espejo produce un error, no un reintento en
`ghcr.io`. El adaptador de CoreLink autentica el bearer y obtiene el contenido
de `ghcr.io` del lado del servidor.

No sustituya el dominio de artefactos por `HOMEBREW_BOTTLE_DOMAIN`. Ese
override antiguo de archivos planos no selecciona la estrategia autenticada
de GitHub Packages de Homebrew y no puede proporcionar el bearer requerido por
`/brew`.

```bash
brew install jq
```

Para probar solo la descarga, sin instalar la fórmula, conserve las mismas tres
variables y ejecute:

```bash
brew fetch --force jq
```

El flujo público sin configuración también sigue siendo válido: quite las tres
variables específicas de CoreLink y Homebrew descargará directamente desde su
upstream.

## Por qué se eliminó la receta anterior del espejo

La receta anterior establecía `HOMEBREW_ARTIFACT_DOMAIN` y
`HOMEBREW_DOCKER_REGISTRY_TOKEN`, pero omitía
`HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK`. Esa omisión era insegura: cuando el
espejo fallaba, Homebrew podía reintentar `ghcr.io` con el bearer de CoreLink.
Un PAT de CoreLink nunca debe enviarse al upstream; bloquee el fallback antes
de establecer el token.

El código de estado distingue los dos casos observados durante la verificación:

| Prueba | Resultado | Significado |
|---|---|---|
| Sin variables de CoreLink | `brew` termina con `0` | La ruta directa al upstream funciona. |
| Dominio de artefactos sin credencial | `401 Unauthorized` | El espejo no recibió bearer; use el PAT solo en el espejo fijado. |
| Dominio de artefactos con token y sin fallback | `Authorization: Bearer <token>` en CoreLink | El contrato del espejo autenticado está activo. |
| Token sin dominio de artefactos o sin protección de fallback | `403 Forbidden` puede venir de `ghcr.io` | Configuración insegura; quite todo y aplique de nuevo el bloque fijado. |

La diferencia entre `401` y `403` demuestra la transmisión de credenciales, no
es una solución. No configure un token para convertir un `401` en `403` y nunca
envíe un PAT de CoreLink a `ghcr.io`.

## Resolución de problemas

| Síntoma | Significado | Acción |
|---|---|---|
| `brew install` funciona sin variables de CoreLink | Homebrew usa la ruta directa al upstream | Mantenga el flujo sin configuración. |
| `401 Unauthorized` desde el dominio de CoreLink | Falta el bearer o no es válido | Revise el secreto y el alcance del PAT; no agregue fallback. |
| `403 Forbidden` desde `ghcr.io` | El token se envió al upstream | Deténgase, quite el token y aplique el bloque con protección. |
| Hay un dominio personalizado sin `HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1` | Un fallo puede volver a `ghcr.io` | Considere insegura la configuración hasta agregar la protección. |

Para una integración de caché compatible, consulte [Resolución de problemas](../troubleshooting.md)
y elija una de las integraciones indicadas arriba.
