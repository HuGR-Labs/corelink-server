---
id: sccache-cargo
title: Integración con sccache (Rust / cargo)
sidebar_position: 4
description: Use CoreLink como un caché de build WebDAV de sccache para que los builds de cargo compartan artefactos compilados entre máquinas y en CI.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/sccache-cargo.md`

# Integración con sccache (Rust / cargo)

[sccache](https://github.com/mozilla/sccache) es un caché de compilador. Cuando establece
`RUSTC_WRAPPER=sccache`, cada invocación de `rustc` se almacena en caché — de modo que un `cargo build`
reutiliza artefactos compilados producidos en otra máquina o en una ejecución de CI anterior.

CoreLink expone un backend de almacenamiento **WebDAV** de sccache en
`/cargo/<tenant>`, respaldado por el mismo almacenamiento direccionable por contenido por tenant que usa el
CAS nativo. Tanto sccache como CoreLink usan claves **BLAKE3** para los artefactos, por lo que
no hay traducción de digest.

## Requisitos previos

- `sccache` instalado (`cargo install sccache` o el paquete de su distribución).
- Un PAT de CoreLink (`corelink_pat_...`) con alcance de lectura + escritura de caché.
- El UUID de su tenant.

## Configurar

Apunte el backend WebDAV de sccache al camino de su tenant y pase el PAT como un token
bearer:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export RUSTC_WRAPPER=sccache
```

Luego compile de forma normal:

```bash
cargo build --release
```

sccache emite solicitudes `GET`, `PUT` y `HEAD` a
`<SCCACHE_WEBDAV_ENDPOINT>/<key>`; CoreLink autentica el PAT bearer,
resuelve su tenant a partir de él y sirve o almacena cada artefacto en el CAS de su
tenant. Un fallo de `GET`/`HEAD` devuelve 404 y sccache recurre a compilar
localmente (y luego hace `PUT` del resultado).

:::note El tenant proviene del PAT
El `<tenant>` en el endpoint se usa solo para el enrutamiento de solicitudes; el
tenant autoritativo se resuelve a partir del PAT y se reverifica en el servidor. Un PAT
solo puede leer y escribir en el caché de su propio tenant.
:::

## Verificar que funcionó

Ejecute un build dos veces (limpie el sccache local primero para que el segundo build deba acceder
a CoreLink):

```bash
cargo clean
cargo build --release        # cold — compiles and PUTs artifacts
cargo clean
cargo build --release        # warm — should read from CoreLink
sccache --show-stats
```

`sccache --show-stats` reporta la tasa de aciertos de caché y la URL del backend WebDAV.
"Cache hits" distinto de cero en el segundo build confirma que CoreLink sirvió los
artefactos.

## Ejemplo de CI (GitHub Actions)

```yaml
- name: Build with sccache + CoreLink
  env:
    RUSTC_WRAPPER: sccache
    SCCACHE_WEBDAV_ENDPOINT: https://corelink-api.humangr.com/cargo/acme-prod
    SCCACHE_WEBDAV_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: cargo build --release
```

Almacene el PAT como un secret de repositorio (**Settings → Secrets and variables →
Actions**).

## Resolución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| Cada build recompila | `RUSTC_WRAPPER` no establecido | Exporte `RUSTC_WRAPPER=sccache` en la misma shell |
| `401 Unauthorized` en los registros de sccache | `SCCACHE_WEBDAV_TOKEN` ausente o incorrecto | Establézcalo con su PAT `corelink_pat_...` |
| `403 Forbidden` | PAT con alcance para un tenant diferente | Confirme que el `<tenant>` en el endpoint coincide con el tenant de su PAT |
| Los fallos de caché persisten | Entradas de build no deterministas | Fije el toolchain + `CARGO_INCREMENTAL=0`; ejecute `sccache --show-stats` para inspeccionar |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
