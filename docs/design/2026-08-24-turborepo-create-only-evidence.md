# Turborepo remote cache: create-only PUT — client evidence and decision (B-024)

**Status:** decided + implemented, 2026-08-24.
**Owner:** tech lead.
**Closes:** BACKLOG `B-024`.

## The question

`specs/_audits/2026-08-23-cache-integrity-coverage.md` (F-2) found Turborepo
artifacts overwritable. The obvious fix — make PUT create-only, as the Action
Cache already is — was deferred once, on purpose, because Turborepo (unlike the
AC, whose protocol says a result for an action digest is final) makes no such
promise, and nobody had measured what a real `turbo` client does. This repo has
already paid for guessing at a build tool's behaviour: rejecting `.sccache_check`
made sccache deem the backend unusable and disable itself entirely.

So the missing thing was evidence, not a patch. Two concrete questions:

1. **Does a real `turbo` client ever re-issue a PUT for a key that already
   exists?** If it does, create-only would refuse writes the client expects to
   succeed.
2. **What does `turbo` do when the server REFUSES an overwrite with 409?** Does
   it fail the build, disable the cache for the session (the sccache failure
   mode), or tolerate it?

Question 2 cannot be answered against production, because production *overwrites*
— it never refuses. It can only be answered against a server that refuses. So the
experiment drove the **real `turbo` binary** (v2.10.11) against a **local mock**
implementing the same `/v8/artifacts` surface `routes/turbo_v8.rs` exposes,
exercising both the accept-overwrite and refuse-overwrite (409) paths.

## Method

A minimal npm/turbo monorepo with one cacheable `build` task, and a ~60-line
Node mock implementing `GET`/`PUT /v8/artifacts/:hash`, `POST
/v8/artifacts/events`, `POST /v8/artifacts/status` (+ the `/v2/user`,
`/v2/teams` token pings some turbo versions issue). The mock logs every request
with whether the key already existed, and has a `REFUSE_OVERWRITE=1` mode that
returns `409 Conflict` on a PUT to an existing key. `turbo` was pointed at it
with `TURBO_API`/`TURBO_TOKEN`/`TURBO_TEAM`. (Harness archived with this note in
the session scratchpad; it needs no CoreLink credential and writes nothing to
prod — a deliberate improvement over hitting the live endpoint, which could not
answer question 2 at all.)

## Results

**Phase 1 — accept overwrites (`REFUSE_OVERWRITE=0`).**

```
RUN 1 (cold):            GET  <hash> hit=false      → turbo checks presence FIRST
                         PUT  <hash> existed=false  → uploads only on the miss
RUN 2 (local cache cleared, remote warm):
                         GET  <hash> hit=true       → downloads the hit
                         (no PUT)                    → never re-uploads an existing key
```

turbo **GETs before it PUTs**, and only uploads on a confirmed miss. With the
remote warm it downloads and does **not** re-PUT. Under normal operation it never
overwrites an existing key.

**Phase 2 — refuse overwrites (`REFUSE_OVERWRITE=1`).** To force a PUT of an
existing key at all, the second run used `turbo build --force` (an explicit user
override that re-runs the task and re-uploads):

```
PUT <hash> existed=true → mock returns 409 Conflict
turbo output:
  WARNING  failed to contact remote cache: ... HTTP status client error
           (409 Conflict) for url (.../v8/artifacts/<hash>?slug=team_test)
  Tasks:    1 successful, 1 total
  exit code: 0
```

A 409 is a **non-fatal warning**. The build **succeeds** (`exit 0`), the task is
counted successful, and the remote cache is **not** disabled for the session —
unlike sccache, which self-disables on an unexpected refusal.

## Decision

**Make Turborepo PUT create-only (`put_if_absent`): 409 on an existing key.**
The evidence removes the two risks that justified deferring it:

- turbo never re-PUTs an existing key in normal operation, so create-only refuses
  nothing the client expects to succeed. The only path that even attempts an
  overwrite is `--force`, a deliberate override.
- turbo tolerates the 409 gracefully (build succeeds, cache stays enabled), so
  there is no `.sccache_check`-style self-disable to fear.

Create-only also makes the surface consistent with the Action Cache and matches
turbo's own model, in which a cache entry is authoritative once written (turbo
never re-fetches-and-compares — on a hit it simply uses the cached bytes).

**What it closes.** The residual named in B-024: a credential with cache-write
scope for a tenant could REPLACE the bytes behind that tenant's own Turborepo
keys — within-tenant cache poisoning the content envelope cannot detect, because
the key is not a preimage of the content. Create-only removes the overwrite, so
the poisoning is impossible.

## Implementation

- `corelink-turbo-bridge`: new `TurboBridgeError::AlreadyExists`; the adapter
  probes presence before the write and returns it on a confirmed existing key.
  The probe runs under the route's per-`(tenant, team, hash)` write lock, so the
  probe→refuse-or-write is serialized per stored object. A probe *backend* error
  fails OPEN (proceeds to write) so a transient R2 error never blocks a
  legitimate first insert; only a confirmed existing key is refused.
- `routes/turbo_v8.rs`: `map_err` maps `AlreadyExists` → `409 Conflict`; the
  route's up-front byte reservation is released on the refusal.
- The old overwrite byte-delta reconciliation (rt34) is superseded: with
  overwrites refused there is no grow/shrink delta to account, and the "PUT 1 B
  then PUT 100 MiB under the same key" free-storage exploit shape can no longer
  be issued. The reconciliation code is kept as defensive dead code in case
  create-only is ever relaxed.

## Appendix — the mock (reproducible, no CoreLink credential)

Scaffold a one-package turbo monorepo (`npm i -D turbo@2.10.11`, a `turbo.json`
with `remoteCache.enabled` and one `build` task), then run the real client
against the mock below:

```js
// node mock-cache.js  (REFUSE_OVERWRITE=1 for phase 2)
const http = require('http');
const store = new Map();
const REFUSE = process.env.REFUSE_OVERWRITE === '1';
http.createServer((req, res) => {
  const u = new URL(req.url, 'http://localhost'), p = u.pathname, cs = [];
  req.on('data', c => cs.push(c));
  req.on('end', () => {
    const body = Buffer.concat(cs);
    if (p === '/v8/artifacts/events') return res.writeHead(200).end('[]');
    if (p === '/v8/artifacts/status') return res.writeHead(200).end('{"status":"enabled"}');
    if (p === '/v2/user') return res.writeHead(200).end('{"user":{"id":"u"}}');
    if (p.startsWith('/v2/teams')) return res.writeHead(200).end('{"teams":[{"id":"team_test","slug":"team_test"}]}');
    const m = p.match(/^\/v8\/artifacts\/([^/]+)$/);
    if (m) {
      const h = m[1], existed = store.has(h);
      if (req.method === 'PUT') {
        console.log(`PUT ${h} existed=${existed}`);
        if (existed && REFUSE) return res.writeHead(409).end('{"error":"exists"}');
        store.set(h, body); return res.writeHead(200).end(`{"urls":["local/${h}"]}`);
      }
      if (req.method === 'GET') {
        console.log(`GET ${h} hit=${existed}`);
        if (!existed) return res.writeHead(404).end();
        const b = store.get(h);
        return res.writeHead(200, {'Content-Length': b.length}).end(b);
      }
    }
    res.writeHead(404).end();
  });
}).listen(8787);
```

```sh
# phase 1: turbo build ; rm -rf .turbo node_modules/.cache/turbo packages/*/dist ; turbo build
# phase 2: REFUSE_OVERWRITE=1 node mock-cache.js ; turbo build ; turbo build --force
export TURBO_API=http://localhost:8787 TURBO_TOKEN=t TURBO_TEAM=team_test
```
