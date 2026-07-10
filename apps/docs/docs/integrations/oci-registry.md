---
id: oci-registry
title: OCI registry (Docker / Podman) integration
sidebar_position: 5
description: Push and pull container images and OCI artifacts to CoreLink, a full OCI Distribution Spec v1.1 registry.
---

# OCI registry (Docker / Podman) integration

CoreLink is a full **OCI Distribution Spec v1.1** registry. Any standard OCI
client — `docker`, `podman`, `buildah`, `crane`, `helm` (OCI charts), BuildKit
cache exporters — can push and pull against it. Manifests are stored in
per-tenant key-value; blobs (layers and configs) live in the tenant CAS.

The registry host is `corelink-api.humangr.com`. Unlike the other cache
surfaces, the OCI path has **no tenant segment** in the URL — your tenant is
derived from the token you authenticate with, using the registry's standard
two-leg bearer-token flow (`GET /token` then `Authorization: Bearer`).

## Prerequisites

- `docker` (or `podman`) installed.
- A CoreLink PAT (`corelink_pat_...`) with cache read + write scope.

## Log in

Log in with your PAT as the password. The username is not checked — any value
(for example `corelink`) works:

```bash
echo "$CORELINK_PAT" | docker login corelink-api.humangr.com \
  --username corelink --password-stdin
```

Docker performs the token exchange automatically on the next push or pull.

## Push an image

Tag the image with the CoreLink host and push. The first path segment after the
host is the **repository name** (not a tenant):

```bash
docker tag my-app:latest corelink-api.humangr.com/my-app:latest
docker push corelink-api.humangr.com/my-app:latest
```

## Pull an image

```bash
docker pull corelink-api.humangr.com/my-app:latest
```

Podman uses the same reference:

```bash
podman pull corelink-api.humangr.com/my-app:latest
```

:::note Isolation is per-tenant, keyed by the PAT
Two tenants can both push `my-app:latest` without collision — each repository is
scoped to the tenant resolved from the authenticating PAT. The `_catalog`
endpoint is disabled by default.
:::

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `401 Unauthorized` on push | Not logged in, or expired PAT | Re-run `docker login` with a current PAT |
| `denied: requested access to the resource is denied` | PAT lacks write scope | Use a PAT with cache write scope |
| `manifest unknown` on pull | Image was never pushed to this tenant | Push it first, or check the reference host/name |

Full error reference: [Troubleshooting](../troubleshooting.md).
