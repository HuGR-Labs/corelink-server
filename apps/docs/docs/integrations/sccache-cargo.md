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

CoreLink is the **shared** layer — the one that lets a second machine, or a fresh
CI runner, reuse what someone else already compiled. It is meant to sit *behind*
a local disk layer, not to replace it. The configuration below sets up both.

## Prerequisites

- `sccache` **0.15 or newer** (`cargo install sccache` or your distribution's
  package). Check with `sccache --version` — the multi-level storage chain that
  gives you a local layer landed in 0.15.0.
- A CoreLink PAT (`corelink_pat_0123456789ABCDEF.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.BBBBBBBBBBBBBBBBBBBBBB`) with cache read + write scope.
- Your tenant UUID.

## Configure

Point sccache's WebDAV backend at your tenant path, pass the PAT as a bearer
token, and chain a local disk layer in front of it:

```bash
export SCCACHE_WEBDAV_ENDPOINT="https://corelink-api.humangr.com/cargo/<your-tenant-id>"
export SCCACHE_WEBDAV_TOKEN="corelink_pat_0123456789ABCDEF.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.BBBBBBBBBBBBBBBBBBBBBB"
export SCCACHE_MULTILEVEL_CHAIN="disk,webdav"   # requires sccache >= 0.15
export SCCACHE_DIR="$HOME/.cache/sccache"       # where the local layer lives
export RUSTC_WRAPPER=sccache
```

What each variable does:

| Variable | Role |
|---|---|
| `SCCACHE_WEBDAV_ENDPOINT` | The CoreLink backend — the **shared** layer, at your tenant path |
| `SCCACHE_WEBDAV_TOKEN` | The PAT sccache sends as `Authorization: Bearer` |
| `SCCACHE_MULTILEVEL_CHAIN` | The storage chain, nearest layer first. `disk,webdav` = local disk in front of CoreLink |
| `SCCACHE_DIR` | Filesystem path of the local disk layer. Only meaningful when the chain includes `disk` |
| `RUSTC_WRAPPER` | Makes cargo route every `rustc` invocation through sccache |

:::caution Do not omit `SCCACHE_MULTILEVEL_CHAIN`
sccache selects **exactly one** storage backend. Its `storage_from_config` only
falls through to the disk backend when *no* remote backend is configured — so
with `SCCACHE_WEBDAV_ENDPOINT` set and the chain unset, sccache is remote-only:
every cache read is an HTTPS round trip, and `SCCACHE_DIR` is inert.

`SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` instead builds a two-level chain — local
disk first, CoreLink behind it — and a hit found on CoreLink is backfilled into
the local layer in the background. Multi-level chains require sccache **0.15.0 or
newer**; on an older sccache the variable is ignored and you silently fall back to
remote-only, so verify `sccache --version`.
:::

Then build as normal:

```bash
cargo build --release
```

When a lookup misses the local layer, sccache uses the following six methods at
`<SCCACHE_WEBDAV_ENDPOINT>/<key>`:

| Method | Purpose |
|---|---|
| `GET` | Read a cached artifact. A miss returns `404`, so sccache compiles locally. |
| `PUT` | Store the compiled artifact after a local compilation. |
| `HEAD` | Check whether an artifact exists without downloading its body. |
| `PROPFIND` | WebDAV stat used by sccache to check a key and its byte length. |
| `MKCOL` | WebDAV parent-directory probe; CoreLink treats the implicit cache directories as a no-op. |
| `DELETE` | Remove a cache key; requires cache-write scope and a PAT with write capability. The operation is idempotent (`204`). |

CoreLink authenticates the bearer PAT, resolves your tenant from it, and serves
or stores each artifact in that tenant's CAS. The six methods above are the
published sccache contract: a proxy, WAF, or firewall in front of CoreLink must
allow all six. `DELETE` is a public authenticated write operation; it is not
internal-only. A normal PAT may remove an arbitrary cache key in its tenant.

During its startup/write health probe, sccache sends `PUT`, `GET`, and then
`DELETE` for `.sccache_check`. That key is reserved for this probe and cleanup:
it is **probe-only**, not a build-artifact key. This does not change the fact
that `DELETE` is also available for ordinary cache keys. A method-filtering
proxy must allow all six methods, including this control request.

:::warning A failed write makes sccache read-only
The sccache daemon performs a write probe when it starts. If that probe or a
later `PUT` fails, sccache marks the WebDAV backend **read-only for the rest of
the daemon's lifetime**. Builds can remain green, but new artifacts are not
stored, so a successful `GET`, `HEAD`, or `PROPFIND` does not prove that writes
are healthy. Fix the PAT, endpoint, or proxy, restart the daemon with
`sccache --stop-server`, and then verify the next build.

Always inspect `sccache --show-stats` after a write failure. Check `Cache errors`
alongside the hit and miss counters; a cold build with no new writes can look
normal while the daemon is latched read-only.
:::

:::note Tenant comes from the PAT
The `<tenant>` in the endpoint is used only for request routing; the
authoritative tenant is resolved from the PAT and re-verified server-side. A PAT
can only read and write its own tenant's cache.
:::

## Verify it worked

With the chain configured, a second build on the same machine is normally served
by the *local* layer — which is the point, but it proves nothing about CoreLink.
To confirm CoreLink itself is serving, drop the local layer between the two
builds:

```bash
cargo clean
cargo build --release        # cold — compiles, stores in both layers
cargo clean
sccache --stop-server        # stop the daemon before touching its disk layer
rm -rf "$SCCACHE_DIR"        # drop the local layer so the read must reach CoreLink
cargo build --release        # warm — the hit can only come from CoreLink
sccache --show-stats
```

`sccache --show-stats` reports the cache hit counts and the configured storage.
Non-zero "Cache hits" on the second build, with the local layer emptied,
confirms CoreLink served the artifacts. Only do the `rm -rf` for this check —
in normal use you want the local layer to persist.

## CI example (GitHub Actions)

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

Store the PAT as a repository secret (**Settings → Secrets and variables →
Actions**).

On an ephemeral runner the local layer starts empty and is discarded when the job
ends, so it only helps *within* one job — which still matters for a job that runs
several cargo invocations. Keep the chain configured either way; you can also
persist `SCCACHE_DIR` across runs with your runner's cache action.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Every build rebuilds | `RUSTC_WRAPPER` not set | Export `RUSTC_WRAPPER=sccache` in the same shell |
| `401 Unauthorized` in sccache logs | Missing or wrong `SCCACHE_WEBDAV_TOKEN` | Set it to your `corelink_pat_0123456789ABCDEF.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.BBBBBBBBBBBBBBBBBBBBBB` PAT |
| `403 Forbidden` | PAT scoped to a different tenant | Confirm the `<tenant>` in the endpoint matches your PAT's tenant |
| Cache misses persist | Non-deterministic build inputs | Pin toolchain + `CARGO_INCREMENTAL=0`; run `sccache --show-stats` to inspect |
| Hits are counted, but every one is a network request | `SCCACHE_MULTILEVEL_CHAIN` unset — sccache is remote-only | Set `SCCACHE_MULTILEVEL_CHAIN="disk,webdav"` **and** `SCCACHE_DIR` |
| `SCCACHE_DIR` appears to be ignored | Same cause, or an sccache older than 0.15 (chain variable ignored) | Set the chain; check `sccache --version` and upgrade to ≥ 0.15 |

Full error reference: [Troubleshooting](../troubleshooting.md).
