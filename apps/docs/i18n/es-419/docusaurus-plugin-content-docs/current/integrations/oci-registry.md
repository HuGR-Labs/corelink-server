---
id: oci-registry
title: Integración con registro OCI (Docker / Podman)
sidebar_position: 5
description: Envíe y descargue imágenes de contenedor y artefactos OCI a CoreLink, un registro completo de la OCI Distribution Spec v1.1.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/oci-registry.md`

# Integración con registro OCI (Docker / Podman)

CoreLink es un registro completo de la **OCI Distribution Spec v1.1**. Cualquier cliente OCI
estándar — `docker`, `podman`, `buildah`, `crane`, `helm` (charts OCI), exportadores de
caché de BuildKit — puede hacer push y pull contra él. Los manifests se almacenan en
clave-valor por tenant; los blobs (capas y configs) residen en el CAS del tenant.

El host del registro es `corelink-api.humangr.com`. A diferencia de las otras superficies de
caché, el camino OCI **no tiene segmento de tenant** en la URL — su tenant se
deriva del token con el que se autentica, usando el flujo estándar de token bearer
de dos pasos del registro (`GET /token` y luego `Authorization: Bearer`).

## Requisitos previos

- `docker` (o `podman`) instalado.
- Un PAT de CoreLink (`corelink_pat_...`) con alcance de lectura + escritura de caché.

## Iniciar sesión

Inicie sesión con su PAT como contraseña. El nombre de usuario no se verifica — cualquier valor
(por ejemplo, `corelink`) funciona:

```bash
echo "$CORELINK_PAT" | docker login corelink-api.humangr.com \
  --username corelink --password-stdin
```

Docker realiza el intercambio de token automáticamente en el siguiente push o pull.

## Enviar una imagen

Etiquete la imagen con el host de CoreLink y haga push. El primer segmento de camino después del
host es el **nombre del repositorio** (no un tenant):

```bash
docker tag my-app:latest corelink-api.humangr.com/my-app:latest
docker push corelink-api.humangr.com/my-app:latest
```

## Descargar una imagen

```bash
docker pull corelink-api.humangr.com/my-app:latest
```

Podman usa la misma referencia:

```bash
podman pull corelink-api.humangr.com/my-app:latest
```

:::note El aislamiento es por tenant, con clave en el PAT
Dos tenants pueden hacer push de `my-app:latest` sin colisión — cada repositorio está
delimitado al tenant resuelto a partir del PAT autenticador. El endpoint `_catalog`
está deshabilitado de forma predeterminada.
:::

## Resolución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| `401 Unauthorized` en el push | No se inició sesión, o el PAT expiró | Ejecute `docker login` de nuevo con un PAT vigente |
| `denied: requested access to the resource is denied` | El PAT carece de alcance de escritura | Use un PAT con alcance de escritura de caché |
| `manifest unknown` en el pull | La imagen nunca se envió a este tenant | Haga push de ella primero, o verifique el host/nombre de la referencia |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
