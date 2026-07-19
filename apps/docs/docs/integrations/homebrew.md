---
id: homebrew
title: Homebrew bottle mirror
sidebar_position: 8
description: Point Homebrew at CoreLink to cache bottle downloads in your tenant.
---

# Homebrew bottle mirror

:::caution Experimental — not functional with current Homebrew (roadmap)
Modern Homebrew (the `install-from-API` default, Homebrew ≥ 4.x) fetches bottles
as **OCI blobs directly from `ghcr.io`** and **ignores `HOMEBREW_ARTIFACT_DOMAIN`
and `HOMEBREW_BOTTLE_DOMAIN`** for that path — so the env-var recipe below does
**not** route bottle downloads through CoreLink (verified 2026-07-19 against
Homebrew 6.0.11: `brew` fetched the bottle blob straight from `ghcr.io`, 0
requests to the mirror). The mirror endpoint itself works (it serves bottle
manifests over HTTP with a `Bearer` PAT), but no Homebrew env var currently
redirects the client onto it.

**Supported path (roadmap):** re-host bottles as OCI artifacts on the CoreLink
OCI registry (`corelink-oci.humangr.com`) behind a **custom tap** that rewrites
bottle URLs. Tracked as an expansion-backlog item. Until then, treat Homebrew as
**not yet a supported cache surface** — use the [OCI registry](./oci-registry.md),
[npm](./npm.md), [pip](./pip.md), or [sccache/cargo](./sccache-cargo.md)
surfaces, which are exercised by their real clients.
:::

CoreLink is designed to cache **Homebrew bottles** (the pre-built binaries `brew
install` downloads): on a hit the bottle is served from your tenant CAS; on a
miss CoreLink fetches it from upstream (`ghcr.io`), caches it, and streams it
back — speeding up repeat installs across machines and CI.

This is a **read-path** mirror for `brew install`. Tap sources, casks
(`.dmg`/`.pkg`), and `brew bottle` are out of scope.

## Prerequisites

- Homebrew installed.
- A CoreLink PAT (`corelink_pat_...`).
- Your tenant UUID.

## Configure (experimental — does not currently route; see the caution above)

Homebrew only attaches an `Authorization` header when the bottle host is
reached through `HOMEBREW_ARTIFACT_DOMAIN` (which keeps Homebrew's authenticated
GitHub Packages download strategy). Set the artifact domain to your tenant path
and pass the PAT via `HOMEBREW_DOCKER_REGISTRY_TOKEN`:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_DOCKER_REGISTRY_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX"
brew install <formula>
```

Homebrew turns `HOMEBREW_DOCKER_REGISTRY_TOKEN` into
`Authorization: Bearer corelink_pat_...` on every bottle download, which is what
CoreLink authenticates.

:::warning Use `HOMEBREW_ARTIFACT_DOMAIN`, not `HOMEBREW_BOTTLE_DOMAIN`
A bare `HOMEBREW_BOTTLE_DOMAIN` selects Homebrew's plain download strategy,
which sends **no** auth header — so it cannot authenticate against CoreLink and
every download fails with 401. `HOMEBREW_ARTIFACT_DOMAIN` is required.
:::

## Verify it worked

Install a small formula twice on different machines (or clear the local
download cache between runs). The second install pulls the cached bottle from
CoreLink:

```bash
brew install --verbose jq 2>&1 | grep corelink-api.humangr.com | head
```

Seeing requests to `corelink-api.humangr.com/brew/...` confirms Homebrew is
using the mirror.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `401 Unauthorized` on every download | Used `HOMEBREW_BOTTLE_DOMAIN` (no auth header) | Switch to `HOMEBREW_ARTIFACT_DOMAIN` |
| `401` with the artifact domain set | Token missing or malformed | Set `HOMEBREW_DOCKER_REGISTRY_TOKEN=corelink_pat_...` |
| `403 Forbidden` | PAT scoped to a different tenant | Confirm the `<tenant>` in the domain matches your PAT's tenant |

Full error reference: [Troubleshooting](../troubleshooting.md).
