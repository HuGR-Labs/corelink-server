---
id: pip
title: pip (PyPI) mirror
sidebar_position: 7
description: Point pip, uv, poetry, or pdm at CoreLink as a caching mirror in front of PyPI.
---

# pip (PyPI) mirror

CoreLink is a **caching mirror** in front of `pypi.org`. It implements the
Simple Index API (PEP 691 JSON, with PEP 503 HTML fallback), so `pip`, `uv`,
`poetry`, and `pdm` can use it as their package index. Wheels and source
distributions are cached in your tenant CAS and integrity-checked against the
upstream `#sha256=` fragment before caching.

This is a **read-only mirror** for installing public packages. It does not host
private indexes and does not accept `twine upload`.

## Prerequisites

- `pip` (or a compatible client) installed.
- A CoreLink PAT (`corelink_pat_...`).
- Your tenant UUID.

## Configure

pip authenticates with HTTP basic auth where the username is `hugr` and the
password is your PAT. Put your tenant's Simple index URL — with the credentials
embedded — in `pip.conf` (`~/.config/pip/pip.conf` on Linux,
`~/Library/Application Support/pip/pip.conf` on macOS):

```ini
[global]
index-url = https://hugr:corelink_pat_XXXXXXXXXXXX@corelink-api.humangr.com/pip/<your-tenant-id>/simple/
```

Then install as normal:

```bash
pip install requests
```

Or pass it inline for a one-off (and for CI, where the PAT comes from a secret):

```bash
pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

`uv` honours the same `index-url`:

```bash
uv pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

## Verify it worked

```bash
pip install --force-reinstall --no-cache-dir -v requests 2>&1 \
  | grep corelink-api.humangr.com | head
```

Seeing `corelink-api.humangr.com/pip/.../simple/` in the verbose log confirms
pip is resolving through the mirror.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `401 Unauthorized` | Username is not `hugr`, or the password is not a `corelink_pat_...` PAT | Use `hugr` as the username and your PAT as the password |
| `Could not find a version` | Project not yet cached and upstream unreachable | Retry; CoreLink fetches from PyPI on the first request |
| Hash mismatch on a wheel | Upstream artifact changed | CoreLink verifies the `#sha256=` fragment and fails closed on mismatch |

Full error reference: [Troubleshooting](../troubleshooting.md).
