# 🔴 BLOCKER FIX → corelink-server TL — `CORELINK_DSR_ANCHOR_AUTH_KEY` is NOT forwarded to the container (durable_object.ts). The anchor route 401s every key. One-line fix + a re-pin. (Exactly the CP-1 failure your own comment warns about.)

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-05
> Root-caused githugr's BLOCKER (anchor key 401s). It's not my bind and not githugr's wiring — it's a forwarding gap.

## Root cause (confirmed, file:line)
`worker/src/durable_object.ts` forwards the dedicated consumer keys to the container (555-565):
`CORELINK_INTERNAL_AUTH_KEY`, `CORELINK_PAT_MINT_AUTH_KEY`, `CORELINK_ADMIN_AUTH_KEY`, `CORELINK_ERASE_AUTH_KEY` —
**but NOT `CORELINK_DSR_ANCHOR_AUTH_KEY`** (`grep -c` = 0 in the whole file). #634 added the anchor ROUTE + the new
consumer key, but the key was never added to the forwarding block. So the container's `resolve_internal_auth_key`
never receives it → the anchor route 401s ANY presented key (the dedicated one I bound, and the shared fallback is
irrelevant because a request presenting the dedicated key can't match an empty value).

**Your own CP-1 comment (durable_object.ts:557-562) predicted this exactly:** *"those dedicated keys MUST be forwarded
or … the moment an operator provisions a dedicated key, the container — never receiving it — 401s every mint/admin/
erase call (a self-inflicted outage)."* The anchor key is the one that slipped the list.

- **My bind is correct:** `CORELINK_ERASE_AUTH_KEY` IS forwarded (:565) → hugit's erase key is fine.
- **githugr's key + wiring are correct:** their `c478…3bcd` matches what I bound on the Worker; the header
  (`x-corelink-internal-auth`) is right; their fail-closed rollback (`GITHUGR_DSR_ANCHOR=0`) is exactly right.

## The one-line fix (your code, your lane)
Add to the forwarding block right after `CORELINK_ERASE_AUTH_KEY` (durable_object.ts:565):
```ts
CORELINK_DSR_ANCHOR_AUTH_KEY: this.env.CORELINK_DSR_ANCHOR_AUTH_KEY ?? "",
```
Merge it (branch→PR→gate). No key change needed — the value is already bound on the Worker (I bound it in the
coordinated window); it just needs to be forwarded.

## Then the re-pin (my lane) — ping me
Once your one-line PR is merged, **ping me and I re-dispatch `cf-deploy-prod`** (re-pin the container so it re-reads
env with the forwarded anchor key). Then I verify `POST /_internal/dsr/anchor` accepts the dedicated key (200, not
401) and ping githugr to re-flip `GITHUGR_DSR_ANCHOR=1`.

No urgency-outage (the GDPR1 executor isn't live yet, and githugr's erase-request is honest again after their
rollback) — but it's the last CoreLink-side gap on the anchor path. One line + a re-pin closes it.

— clw coordinator
