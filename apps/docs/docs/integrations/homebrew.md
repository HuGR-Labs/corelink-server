---
id: homebrew
title: Homebrew bottle mirror
sidebar_position: 8
description: The safe authenticated Homebrew bottle mirror path.
---

# Homebrew bottle mirror

:::caution Safe authenticated mirror configuration
Modern Homebrew (the `install-from-API` default, Homebrew 4.x and later) can
rewrite its `ghcr.io` bottle URLs through `HOMEBREW_ARTIFACT_DOMAIN`. The
CoreLink `/brew/<tenant>` endpoint accepts the resulting bearer PAT and reads
the bottle from the fixed `ghcr.io` upstream. Tap sources, casks
(`.dmg`/`.pkg`), and `brew bottle` are out of scope here.

This page is retained because both the authenticated mirror and the ordinary
no-configuration Homebrew workflow are supported.

The no-fallback setting is mandatory. Without it, Homebrew retries the original
`ghcr.io` URL when the mirror fails, and the registry token can be sent to
GitHub instead of CoreLink. Never use the old recipe that omitted this pin.
:::

<!-- WP-B161-AUTH-NO-FALLBACK-20260901: authenticated mirror is pinned to CoreLink; no upstream fallback. -->

## Safe path: authenticated CoreLink mirror

Obtain a real CoreLink PAT from your secret manager and expose it as
`CORELINK_PAT` only in the shell running Homebrew. Do not paste a token into a
document, command history, or log. Set all three variables below together:

```bash
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/<your-tenant-id>"
export HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1
export HOMEBREW_DOCKER_REGISTRY_TOKEN="$CORELINK_PAT"
brew install jq
```

Homebrew keeps its GitHub Packages download strategy for the `ghcr.io` bottle,
rewrites the URL to the artifact domain, and turns
`HOMEBREW_DOCKER_REGISTRY_TOKEN` into `Authorization: Bearer <token>`. With
`HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1`, that bearer is sent only to the
CoreLink artifact domain; a mirror failure is an error rather than a retry to
`ghcr.io`. The CoreLink adapter authenticates that bearer and fetches upstream
content from `ghcr.io` server-side.

Do not substitute `HOMEBREW_BOTTLE_DOMAIN` for the artifact domain. That legacy
flat-file override does not select Homebrew's authenticated GitHub Packages
strategy and cannot provide the bearer required by `/brew`.

To exercise only the download path without installing the formula, keep the
same three variables and run:

```bash
brew fetch --force jq
```

The public no-configuration path also remains valid: unset all three
CoreLink-specific variables and Homebrew downloads directly from its upstream.

## Why the old mirror recipe was removed

The former recipe set `HOMEBREW_ARTIFACT_DOMAIN` and
`HOMEBREW_DOCKER_REGISTRY_TOKEN` but omitted
`HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK`. That omission was unsafe: when the
mirror failed, Homebrew could retry `ghcr.io` with the CoreLink bearer. A
CoreLink PAT must never be used as a GitHub Packages credential; pin the mirror
before supplying the token.

The status code distinguishes the two failure modes seen during verification:

| Probe | Result | Meaning |
|---|---|---|
| No CoreLink variables | `brew` exits `0` | The direct upstream path works. |
| Artifact domain without a credential | `401 Unauthorized` | The mirror received no bearer; provide the PAT only to the pinned mirror. |
| Artifact domain with token and no fallback | `Authorization: Bearer <token>` at CoreLink | The authenticated mirror contract is active. |
| Token without artifact domain or without no-fallback | `403 Forbidden` may come from `ghcr.io` | Unsafe configuration; remove it and reapply the pinned block. |

The `401`/`403` distinction is evidence about credential transmission, not a
fix to apply. Do not configure a token to try to turn a `401` into a `403`, and
never send a CoreLink PAT to `ghcr.io`.

## Troubleshooting

| Symptom | Meaning | Action |
|---|---|---|
| `brew install` succeeds with no CoreLink variables | Homebrew is using the direct upstream path | Keep the no-env workflow. |
| `401 Unauthorized` from the CoreLink domain | The bearer is missing or invalid | Check the PAT source and token scope; do not add a fallback. |
| `403 Forbidden` from `ghcr.io` | The token was sent to the upstream host | Stop, unset the token, and reapply the artifact domain plus no-fallback block. |
| A custom domain is set without `HOMEBREW_ARTIFACT_DOMAIN_NO_FALLBACK=1` | A failed mirror can fall back to `ghcr.io` | Treat the configuration as unsafe until the pin is present. |

For a supported cache integration, see [Troubleshooting](../troubleshooting.md)
and choose one of the integrations listed above.
