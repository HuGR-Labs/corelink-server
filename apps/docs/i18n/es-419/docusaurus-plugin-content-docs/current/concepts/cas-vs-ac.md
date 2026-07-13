---
id: cas-vs-ac
title: Almacenamiento direccionable por contenido y Action Cache
sidebar_position: 1
description: Cómo se diferencian CAS y AC, cuándo se usa cada uno y cómo Bazel usa ambos juntos.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/concepts/cas-vs-ac.md`

# Almacenamiento direccionable por contenido y Action Cache

CoreLink expone dos cachés distintos que funcionan juntos. La mayoría de los
usuarios solo piensa en uno — el que almacena sus salidas de build — pero entender
ambos vale los 5 minutos de lectura.

## Almacenamiento direccionable por contenido (CAS)

El CAS almacena blobs arbitrarios indexados por su digest SHA-256. La clave *es* el
digest: no se requiere un nombre de archivo, una etiqueta de versión ni metadatos
separados.

```
key:   sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
value: <bytes>
```

Propiedades:

- **Inmutable**: una vez que un blob se almacena en un digest dado, el contenido en
  ese digest nunca cambia.
- **Deduplicado**: si dos tenants (o dos jobs de CI) suben los mismos bytes, la
  capa de almacenamiento guarda una copia. Ambos tenants pagan por el acceso, no
  por el almacenamiento duplicado.
- **Verificable**: el cliente que calcula `sha256(downloaded_bytes)` siempre
  coincidirá con la clave usada para recuperarlo.

### CAS en el contexto de REAPI

En la [Remote Execution API](https://github.com/bazelbuild/remote-apis), el CAS se
usa para:

- Contenido de archivos de origen (entradas de las acciones)
- Archivos objeto compilados y artefactos finales (salidas de las acciones)
- Mensajes proto `Directory` que describen el árbol de entrada

Bazel sube las entradas al CAS antes de despachar una acción remota. El ejecutor lee
las entradas del CAS, ejecuta la acción y sube las salidas de vuelta al CAS.

### Endpoints HTTP del CAS

```
PUT /v1/cas/<tenant_id>/<sha256>   body: raw bytes  → 201 + {"hash": "sha256:<digest>"}
GET /v1/cas/<tenant_id>/<sha256>                    → 200 + raw bytes
```

Consulte la [referencia de la API HTTP](../api/http.md) para conocer todos los
detalles.

## Action Cache (AC)

El Action Cache asigna un **action digest** a un **action result**. Un action
digest es el SHA-256 de un proto `Action` serializado — codifica el comando, el
árbol de entrada y las propiedades de plataforma de forma determinista. El action
result registra los digests de salida, el código de salida y los tiempos.

```
key:   sha256(<Action proto>)
value: ActionResult { output_files: [...], exit_code: 0, ... }
```

Propiedades:

- **Omite trabajo redundante**: si `action_digest` está en el AC, la herramienta de
  build obtiene las salidas en caché del CAS y omite volver a ejecutar la acción.
- **Con alcance de tenant**: las entradas del AC de un tenant nunca son visibles
  para otro.
- **Invalidado por cualquier cambio de entrada**: como la clave es el digest de las
  entradas + comando, cualquier cambio en los archivos de origen, las banderas o la
  toolchain produce una clave diferente — el caché falla de forma limpia.

### Cuándo Bazel usa el AC

```
build tool
  1. Compute action_digest = sha256(Action{command, inputs, platform})
  2. GET /ac/<tenant>/action_digest  → HIT: fetch outputs from CAS, done
                                     → MISS: run action locally (or remotely)
  3. On success: PUT /ac/<tenant>/action_digest  → store result
                 PUT /v1/cas/<tenant>/<output_hash> → store each output
```

### Diagrama: CAS + AC juntos

```
        ┌─────────────────────────────────────────────────┐
        │                   Bazel client                   │
        └───┬─────────────────────────────────┬───────────┘
            │ 1. check AC                      │ 3. PUT outputs to CAS
            ▼                                  ▼
     ┌─────────────┐                   ┌────────────────┐
     │ Action Cache│   2. AC miss →    │ Build executor │
     │  (AC)       │   run action      │ (local or RE)  │
     └─────────────┘                   └────────────────┘
            │ 4. write result back              │
            └──────────────────────────────────►│
                                                │ 5. read outputs from CAS
                                                ▼
                                       ┌────────────────┐
                                       │      CAS        │
                                       └────────────────┘
```

## Comparación

| | CAS | Action Cache |
|---|---|---|
| Clave | SHA-256 del contenido | SHA-256 del proto Action |
| Valor | Bytes crudos | ActionResult (digests de salida, código de salida) |
| Inmutable | Sí | Sí (las entradas no se actualizan, solo se escriben una vez) |
| Usado para | Blobs (archivos, protos) | Memoización de acciones de build |
| Disponible sin REAPI | Sí (API REST) | Solo mediante gRPC de REAPI |
| Turborepo | No directamente (Turbo usa su propio formato de artefacto) | El `TURBO_API` de Turborepo se asigna a este concepto |

## Lo que Turborepo llama "remote cache"

Turborepo no expone CAS/AC como conceptos distintos. Su API de caché remoto es un
protocolo HTTP simplificado donde las salidas de las tareas se almacenan por un
hash de las entradas de la tarea. CoreLink expone un endpoint compatible con
Turborepo — consulte [Integración con Turborepo](../integrations/turborepo.md).
