---
id: npm
title: npm registry mirror
sidebar_position: 6
description: Point npm, pnpm, yarn, or bun at CoreLink as a caching mirror in front of the public npm registry.
---

# npm registry mirror

CoreLink is a **caching mirror** in front of `registry.npmjs.org`. Point
`npm` (or `pnpm` / `yarn` / `bun`) at CoreLink and package metadata and tarballs
are cached in your tenant so repeat installs — especially in CI — are faster and
resilient to upstream outages. Tarballs are stored in your tenant CAS and
integrity-checked against the publisher's `dist.shasum` before caching.

This is a **read-only mirror** for `npm install`. It does not host private
packages and does not accept `npm publish`.

## Prerequisites

- `npm` (or a compatible client) installed.
- A CoreLink PAT (`corelink_pat_...`).
- Your tenant UUID.

## Configure `.npmrc`

Add your tenant registry and its auth token to `.npmrc` (project-local or
`~/.npmrc`):

```ini
registry=https://corelink-api.humangr.com/npm/<your-tenant-id>/
//corelink-api.humangr.com/npm/<your-tenant-id>/:_authToken=corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX
```

npm sends the `_authToken` as `Authorization: Bearer <token>`, which is exactly
what CoreLink expects. Then install as normal:

```bash
npm install
```

:::tip Keep the token out of the repo
Commit only the `registry=` line. Provide the `_authToken` line from an
environment-specific `~/.npmrc` or a CI secret so the PAT is never committed.
:::

## Verify it worked

Delete `node_modules` and re-install; the second install should be served from
CoreLink:

```bash
rm -rf node_modules
npm install --loglevel http 2>&1 | grep corelink-api.humangr.com | head
```

Seeing requests to `corelink-api.humangr.com/npm/...` in the HTTP log confirms
npm is using the mirror.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `401 Unauthorized` | Missing or malformed `_authToken` line | The token line's host + path must match `registry=` exactly, and the token must be a `corelink_pat_...` PAT |
| Installs still hit `registry.npmjs.org` | `registry=` not picked up | Confirm the `.npmrc` scope (project vs. user) and re-run `npm config get registry` |
| `EINTEGRITY` | Upstream tarball changed | CoreLink verifies `dist.shasum` and fails closed on mismatch — retry, or report the upstream package |

Full error reference: [Troubleshooting](../troubleshooting.md).
