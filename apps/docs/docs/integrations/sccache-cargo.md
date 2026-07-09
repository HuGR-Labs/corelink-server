---
id: sccache-cargo
title: sccache (Rust / cargo) integration
sidebar_position: 4
description: Use CoreLink as an sccache WebDAV build cache so cargo builds share compiled artifacts across machines and CI.
---

# sccache (Rust / cargo) integration

[sccache](https://github.com/mozilla/sccache) is a compiler cache. When you set
`RUSTC_WRAPPER=sccache`, every `rustc` invocation is cached — so a `cargo build`
reuses compiled artifacts produced on another machine or a previous CI run.

CoreLink exposes an sccache **WebDAV** storage backend at
`/cargo/<tenant>`, backed by the same per-tenant content-addressable store the
native CAS uses. sccache and CoreLink both key artifacts by **BLAKE3**, so there
is zero digest translation.

## Prerequisites

- `sccache` installed (`cargo install sccache` or your distribution's package).
- A CoreLink PAT (`corelink_pat_...`) with cache read + write scope.
- Your tenant UUID.

## Configure

Point sccache's WebDAV backend at your tenant path and pass the PAT as a bearer
token:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export RUSTC_WRAPPER=sccache
```

Then build as normal:

```bash
cargo build --release
```

sccache issues `GET`, `PUT`, and `HEAD` requests to
`<SCCACHE_WEBDAV_ENDPOINT>/<key>`; CoreLink authenticates the bearer PAT,
resolves your tenant from it, and serves or stores each artifact in your tenant
CAS. A `GET`/`HEAD` miss returns 404 and sccache falls back to compiling
locally (then `PUT`s the result).

:::note Tenant comes from the PAT
The `<tenant>` in the endpoint is used only for request routing; the
authoritative tenant is resolved from the PAT and re-verified server-side. A PAT
can only read and write its own tenant's cache.
:::

## Verify it worked

Run a build twice (clear the local sccache first so the second build must hit
CoreLink):

```bash
cargo clean
cargo build --release        # cold — compiles and PUTs artifacts
cargo clean
cargo build --release        # warm — should read from CoreLink
sccache --show-stats
```

`sccache --show-stats` reports the cache hit ratio and the WebDAV backend URL.
Non-zero "Cache hits" on the second build confirms CoreLink served the
artifacts.

## CI example (GitHub Actions)

```yaml
- name: Build with sccache + CoreLink
  env:
    RUSTC_WRAPPER: sccache
    SCCACHE_WEBDAV_ENDPOINT: https://corelink-api.humangr.com/cargo/acme-prod
    SCCACHE_WEBDAV_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: cargo build --release
```

Store the PAT as a repository secret (**Settings → Secrets and variables →
Actions**).

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Every build rebuilds | `RUSTC_WRAPPER` not set | Export `RUSTC_WRAPPER=sccache` in the same shell |
| `401 Unauthorized` in sccache logs | Missing or wrong `SCCACHE_WEBDAV_TOKEN` | Set it to your `corelink_pat_...` PAT |
| `403 Forbidden` | PAT scoped to a different tenant | Confirm the `<tenant>` in the endpoint matches your PAT's tenant |
| Cache misses persist | Non-deterministic build inputs | Pin toolchain + `CARGO_INCREMENTAL=0`; run `sccache --show-stats` to inspect |

Full error reference: [Troubleshooting](../troubleshooting.md).
