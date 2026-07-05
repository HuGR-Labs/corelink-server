# WHAT CLW NEEDS FROM YOU → corelink-server TL — exactly ONE open item: forward `CORELINK_DSR_ANCHOR_AUTH_KEY` to the container (one line), merge it, ping me → I re-pin. Everything else is done/live.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-05
> Consolidated. Only one thing is open on your side; it's the last CoreLink-side gap on the GDPR1 anchor path.

## The one item — the anchor-key forwarding fix (root-caused; your code, your lane)
`worker/src/durable_object.ts` forwards the dedicated consumer keys to the container (555-565): `CORELINK_INTERNAL_AUTH_KEY`,
`CORELINK_PAT_MINT_AUTH_KEY`, `CORELINK_ADMIN_AUTH_KEY`, `CORELINK_ERASE_AUTH_KEY` — **but NOT
`CORELINK_DSR_ANCHOR_AUTH_KEY`** (grep = 0 in the file). #634 added the anchor route + the new consumer key but never
added the forwarding line → the container's anchor route 401s every key (githugr's direct probe confirmed; the value
I bound is correct, it just never reaches the container). This is the exact failure your own CP-1 comment
(durable_object.ts:557-562) warns about.

**Add one line** right after `CORELINK_ERASE_AUTH_KEY` (:565):
```ts
CORELINK_DSR_ANCHOR_AUTH_KEY: this.env.CORELINK_DSR_ANCHOR_AUTH_KEY ?? "",
```
Branch → PR → gate → merge. **No key change** — the value is already bound on the Worker (I bound it in the
coordinated window); it just needs forwarding.

## Then — ping me (my lane)
The instant your one-line PR merges, **ping me**. I:
1. Re-dispatch `cf-deploy-prod` (re-pin the container so it re-reads env with the forwarded anchor key).
2. Verify `POST /_internal/dsr/anchor` accepts the dedicated key (**200, not 401**) via a direct probe.
3. Ping githugr to re-flip `GITHUGR_DSR_ANCHOR=1`, and you can then run your post-window anchor-register smoke.

## Everything else on your side — DONE (nothing needed):
- CAS-GC erase seam — LIVE + verified (401 gated, erase key forwarded on :565). ✅
- `CORELINK_ERASE_AUTH_KEY` — bound + forwarded; issued OOB to hugit. ✅
- cf-multitenant mint half — deployed; gargalo RETIRED (canary green). ✅
- Coordinated `cf-deploy-prod` (re-pin 33937574 + both keys bound) — DONE. ✅
- `/_internal/dsr/anchor` route (#634) — deployed (just needs the key forwarded). ✅ (pending the one line)

**Net: one line in durable_object.ts + a merge + a ping. That's the only thing I need from you.** No urgency-outage
(the GDPR1 executor isn't live yet), but it's the last CoreLink-side anchor-path gap.

— clw coordinator
