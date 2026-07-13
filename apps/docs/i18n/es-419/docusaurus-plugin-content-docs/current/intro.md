---
id: intro
title: ¿Qué es CoreLink?
sidebar_position: 1
description: CoreLink es un caché multiinquilino direccionable por contenido para artefactos de compilación, paquetes, capas de contenedor y pesos de modelos de ML — alojado en Cloudflare.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/intro.md`

# ¿Qué es CoreLink?

CoreLink es un **caché direccionable por contenido** alojado y multiinquilino para artefactos de compilación. Almacena cualquier blob exactamente una vez por su digest SHA-256 y lo sirve desde el borde de Cloudflare más cercano a cada cliente.

Las herramientas de compilación que admiten la [Remote Execution API (REAPI)](https://github.com/bazelbuild/remote-apis) — Bazel, Buck2, NativeLink y otras — pueden apuntar directamente a CoreLink sin cambios de código. Turborepo se conecta mediante una sola variable de entorno. Los clientes HTTP sin procesar usan los endpoints REST.

## Para quién es

- **Equipos que ejecutan Bazel o Buck2** que quieren un caché remoto administrado sin operar buckets de S3, Redis ni `bazel-remote` por su cuenta.
- **Monorepos de Turborepo** que quieren un caché remoto personalizado fuera de la oferta alojada de Vercel.
- **Equipos de ingeniería de plataforma** que quieren aislamiento de inquilinos, registros de auditoría y cifrado BYOK en un solo servicio.

## Qué no es CoreLink

CoreLink no es un motor de ejecución remota. Almacena y recupera contenido por hash; no programa ni ejecuta acciones de compilación. Úselo junto con [BuildBarn](https://github.com/buildbarn/bb-storage) o [EngFlow](https://www.engflow.com) si necesita ejecución remota.

## Cómo funciona

```
build tool                CoreLink API (Cloudflare Worker)       R2 / KV
─────────────────────     ─────────────────────────────────     ─────────
PUT /v1/cas/<t>/<hash> ─► auth (PAT) → tenant isolation        → stored once
GET /v1/cas/<t>/<hash> ◄─ cache-hit lookup                     ← returned
```

Cada blob se direcciona por su digest SHA-256. Si dos inquilinos suben los mismos bytes, cada inquilino paga por una copia y tiene control de acceso independiente — el contenido se comparte en la capa de almacenamiento, el acceso no.

## Capacidades clave

| Capacidad | Detalles |
|---|---|
| Almacenamiento direccionable por contenido (CAS) | Almacén de blobs indexado por SHA-256. Deduplica automáticamente. |
| Action cache (AC) | Asigna `(action_digest) → (output_digest)` para que Bazel omita acciones idénticas. |
| Multiinquilino | Cada inquilino está aislado a nivel de PAT. Las lecturas entre inquilinos nunca son posibles. |
| Cifrado BYOK | Los inquilinos del plan Enterprise pueden suministrar su propia clave AES-256. |
| Registro de auditoría | Cada lectura y escritura se agrega a un registro inmutable y restringido al inquilino. |
| REAPI v2 | Servicios gRPC completos `ContentAddressableStorage` + `ActionCache` + `ByteStream`. |

## Siguiente paso

La ruta más rápida hacia su primer cache hit es el [quickstart de 5 minutos](./quickstart.md).
