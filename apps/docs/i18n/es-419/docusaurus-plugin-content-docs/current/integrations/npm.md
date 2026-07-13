---
id: npm
title: Espejo del registro npm
sidebar_position: 6
description: Apunte npm, pnpm, yarn o bun a CoreLink como un espejo de caché delante del registro público de npm.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/npm.md`

# Espejo del registro npm

CoreLink es un **espejo de caché** delante de `registry.npmjs.org`. Apunte
`npm` (o `pnpm` / `yarn` / `bun`) a CoreLink y los metadatos de paquetes y los tarballs
se almacenan en caché en su tenant, de modo que las reinstalaciones repetidas — especialmente en CI — sean más rápidas y
resilientes a las interrupciones del upstream. Los tarballs se guardan en el CAS de su tenant y
se les verifica la integridad contra el `dist.shasum` del publicador antes de almacenarlos en caché.

Este es un **espejo de solo lectura** para `npm install`. No hospeda paquetes
privados y no acepta `npm publish`.

## Requisitos previos

- `npm` (o un cliente compatible) instalado.
- Un PAT de CoreLink (`corelink_pat_...`).
- El UUID de su tenant.

## Configurar `.npmrc`

Agregue el registro de su tenant y su token de autenticación a `.npmrc` (local del proyecto o
`~/.npmrc`):

```ini
registry=https://corelink-api.humangr.com/npm/<your-tenant-id>/
//corelink-api.humangr.com/npm/<your-tenant-id>/:_authToken=corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX
```

npm envía el `_authToken` como `Authorization: Bearer <token>`, que es exactamente
lo que CoreLink espera. Luego instale de forma normal:

```bash
npm install
```

:::tip Mantenga el token fuera del repositorio
Haga commit solo de la línea `registry=`. Proporcione la línea `_authToken` desde un
`~/.npmrc` específico del entorno o desde un secret de CI, para que el PAT nunca se confirme.
:::

## Verificar que funcionó

Elimine `node_modules` y reinstale; la segunda instalación debería servirse desde
CoreLink:

```bash
rm -rf node_modules
npm install --loglevel http 2>&1 | grep corelink-api.humangr.com | head
```

Ver solicitudes a `corelink-api.humangr.com/npm/...` en el registro HTTP confirma que
npm está usando el espejo.

## Resolución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| `401 Unauthorized` | Línea `_authToken` ausente o malformada | El host + camino de la línea del token deben coincidir exactamente con `registry=`, y el token debe ser un PAT `corelink_pat_...` |
| Las instalaciones siguen accediendo a `registry.npmjs.org` | `registry=` no se detectó | Confirme el alcance de `.npmrc` (proyecto vs. usuario) y ejecute de nuevo `npm config get registry` |
| `EINTEGRITY` | El tarball del upstream cambió | CoreLink verifica el `dist.shasum` y falla de forma cerrada ante una discrepancia — reintente o reporte el paquete del upstream |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
