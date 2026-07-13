---
id: homebrew
title: Espejo de bottles de Homebrew
sidebar_position: 8
description: Apunte Homebrew a CoreLink para almacenar en caché las descargas de bottles en su tenant.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/homebrew.md`

# Espejo de bottles de Homebrew

CoreLink almacena en caché **bottles de Homebrew** (los binarios `.tar.gz` precompilados que
`brew install` descarga). En un acierto de caché, la bottle se sirve desde el CAS de su tenant;
en un fallo, CoreLink la obtiene del upstream (`ghcr.io`), la almacena en caché y la transmite
de vuelta. Esto acelera las reinstalaciones repetidas entre máquinas y en CI.

Este es un espejo de **camino de lectura** para `brew install`. Las fuentes de taps, los casks
(`.dmg`/`.pkg`) y `brew bottle` están fuera de alcance.

## Requisitos previos

- Homebrew instalado.
- Un PAT de CoreLink (`corelink_pat_...`).
- El UUID de su tenant.

## Configurar

Homebrew solo adjunta un encabezado `Authorization` cuando el host de la bottle se
alcanza a través de `HOMEBREW_ARTIFACT_DOMAIN` (que mantiene la estrategia de descarga
autenticada de GitHub Packages de Homebrew). Establezca el dominio de artefactos en el camino de su tenant
y pase el PAT mediante `HOMEBREW_DOCKER_REGISTRY_TOKEN`:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_DOCKER_REGISTRY_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX"
brew install <formula>
```

Homebrew convierte `HOMEBREW_DOCKER_REGISTRY_TOKEN` en
`Authorization: Bearer corelink_pat_...` en cada descarga de bottle, que es lo que
CoreLink autentica.

:::warning Use `HOMEBREW_ARTIFACT_DOMAIN`, no `HOMEBREW_BOTTLE_DOMAIN`
Un `HOMEBREW_BOTTLE_DOMAIN` simple selecciona la estrategia de descarga sencilla de Homebrew,
que **no** envía ningún encabezado de autenticación — por lo que no puede autenticarse contra CoreLink y
cada descarga falla con 401. `HOMEBREW_ARTIFACT_DOMAIN` es obligatorio.
:::

## Verificar que funcionó

Instale una formula pequeña dos veces en máquinas diferentes (o limpie el caché de
descarga local entre las ejecuciones). La segunda instalación obtiene la bottle almacenada en caché de
CoreLink:

```bash
brew install --verbose jq 2>&1 | grep corelink-api.humangr.com | head
```

Ver solicitudes a `corelink-api.humangr.com/brew/...` confirma que Homebrew está
usando el espejo.

## Resolución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| `401 Unauthorized` en cada descarga | Usó `HOMEBREW_BOTTLE_DOMAIN` (sin encabezado de autenticación) | Cambie a `HOMEBREW_ARTIFACT_DOMAIN` |
| `401` con el dominio de artefactos establecido | Token ausente o malformado | Establezca `HOMEBREW_DOCKER_REGISTRY_TOKEN=corelink_pat_...` |
| `403 Forbidden` | PAT con alcance para un tenant diferente | Confirme que el `<tenant>` en el dominio coincide con el tenant de su PAT |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
