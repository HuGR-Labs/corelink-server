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

CoreLink es la capa **compartida**: la que permite que otra máquina, o un runner de CI
recién creado, reutilice lo que alguien más ya compiló. Está pensada para ubicarse *detrás*
de una capa local en disco, no para reemplazarla. La configuración de abajo arma las dos.

## Requisitos previos

- `sccache` **0.15 o posterior** (`cargo install sccache` o el paquete de su
  distribución). Verifíquelo con `sccache --version`: la cadena de almacenamiento
  multinivel que le da una capa local llegó en la 0.15.0.
- Un PAT de CoreLink (`corelink_pat_...`) con alcance de lectura + escritura de caché.
- El UUID de su tenant.

## Configurar

Apunte el backend WebDAV de sccache al camino de su tenant, pase el PAT como un token
bearer y encadene una capa local en disco delante de él:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export SCCACHE_MULTILEVEL_CHAIN="disk,webdav"   # requiere sccache >= 0.15
export SCCACHE_DIR="$HOME/.cache/sccache"       # dónde vive la capa local
export RUSTC_WRAPPER=sccache
```

Qué hace cada variable:

| Variable | Función |
|---|---|
| `SCCACHE_WEBDAV_ENDPOINT` | El backend de CoreLink: la capa **compartida**, en el camino de su tenant |
| `SCCACHE_WEBDAV_TOKEN` | El PAT que sccache envía como `Authorization: Bearer` |
| `SCCACHE_MULTILEVEL_CHAIN` | La cadena de almacenamiento, de la capa más cercana a la más lejana. `disk,webdav` = disco local delante de CoreLink |
| `SCCACHE_DIR` | Ruta en el sistema de archivos de la capa local en disco. Solo tiene efecto cuando la cadena incluye `disk` |
| `RUSTC_WRAPPER` | Hace que cargo enrute cada invocación de `rustc` a través de sccache |

:::caution No omita `SCCACHE_MULTILEVEL_CHAIN`
sccache selecciona **exactamente un** backend de almacenamiento. Su `storage_from_config`
solo llega al backend de disco cuando *ningún* backend remoto está configurado: así que con
`SCCACHE_WEBDAV_ENDPOINT` definido y la cadena sin definir, sccache queda en modo
solo-remoto — cada lectura de caché es una ida y vuelta HTTPS, y `SCCACHE_DIR` es inerte.

En cambio, `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` arma una cadena de dos niveles — disco
local primero, CoreLink detrás — y un acierto encontrado en CoreLink se copia a la capa
local en segundo plano. Las cadenas multinivel requieren sccache **0.15.0 o posterior**; en
un sccache más antiguo la variable se ignora y usted vuelve en silencio al modo solo-remoto,
así que verifique `sccache --version`.
:::

Luego compile de forma normal:

```bash
cargo build --release
```

Cuando una búsqueda falla en la capa local, sccache emite solicitudes `GET`, `PUT` y
`HEAD` a
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

Con la cadena configurada, un segundo build en la misma máquina normalmente lo atiende la
capa *local* — que es justamente el objetivo, pero eso no prueba nada sobre CoreLink. Para
confirmar que el propio CoreLink está sirviendo, descarte la capa local entre los dos
builds:

```bash
cargo clean
cargo build --release        # frío — compila y guarda en ambas capas
cargo clean
sccache --stop-server        # detenga el daemon antes de tocar su capa en disco
rm -rf "$SCCACHE_DIR"        # descarte la capa local para que la lectura deba llegar a CoreLink
cargo build --release        # tibio — el acierto solo puede venir de CoreLink
sccache --show-stats
```

`sccache --show-stats` reporta los conteos de aciertos de caché y el almacenamiento
configurado. "Cache hits" distinto de cero en el segundo build, con la capa local vaciada,
confirma que CoreLink sirvió los artefactos. Haga el `rm -rf` solo para esta verificación:
en el uso normal usted quiere que la capa local persista.

## Ejemplo de CI (GitHub Actions)

```yaml
- name: Build with sccache + CoreLink
  env:
    RUSTC_WRAPPER: sccache
    SCCACHE_WEBDAV_ENDPOINT: https://corelink-api.humangr.com/cargo/acme-prod
    SCCACHE_WEBDAV_TOKEN: ${{ secrets.CORELINK_PAT }}
    SCCACHE_MULTILEVEL_CHAIN: disk,webdav
    SCCACHE_DIR: ${{ runner.temp }}/sccache
  run: cargo build --release
```

Almacene el PAT como un secret de repositorio (**Settings → Secrets and variables →
Actions**).

En un runner efímero la capa local arranca vacía y se descarta cuando termina el job, así
que solo ayuda *dentro* de un mismo job — lo que igual cuenta en un job que corre varias
invocaciones de cargo. Mantenga la cadena configurada de todos modos; también puede
persistir `SCCACHE_DIR` entre ejecuciones con la action de caché de su runner.

## Resolución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| Cada build recompila | `RUSTC_WRAPPER` no establecido | Exporte `RUSTC_WRAPPER=sccache` en la misma shell |
| `401 Unauthorized` en los registros de sccache | `SCCACHE_WEBDAV_TOKEN` ausente o incorrecto | Establézcalo con su PAT `corelink_pat_...` |
| `403 Forbidden` | PAT con alcance para un tenant diferente | Confirme que el `<tenant>` en el endpoint coincide con el tenant de su PAT |
| Los fallos de caché persisten | Entradas de build no deterministas | Fije el toolchain + `CARGO_INCREMENTAL=0`; ejecute `sccache --show-stats` para inspeccionar |
| Se contabilizan aciertos, pero cada uno es una solicitud de red | `SCCACHE_MULTILEVEL_CHAIN` sin definir — sccache está en modo solo-remoto | Defina `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` **y** `SCCACHE_DIR` |
| `SCCACHE_DIR` parece ignorarse | La misma causa, o un sccache anterior a 0.15 (la variable de la cadena se ignora) | Defina la cadena; verifique `sccache --version` y actualice a ≥ 0.15 |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
