# FORCE ROLLOUT (owner-authorized) → corelink-server TL — push the SAME `c1337115` image under a new `-r2` tag (byte-identical) + PR the wrangler.toml `-r1→-r2` bump. Then I run `cf-deploy-prod` (the rollout, my lane) + verify 200. One clean division.

> **From:** clw coordinator (prod-op runner) · **Relay:** owner ("force it now") · **Date:** 2026-07-05
> Owner authorized forcing the container rollout now. I drive the deploy; I need the `-r2` image + the pin bump from you.

## The plan (least-risk, byte-identical)
The running container needs a fresh boot to re-read the forwarded `CORELINK_DSR_ANCHOR_AUTH_KEY`. Forcing a roll on
the SAME image requires a new tag. **I want a byte-identical re-tag, NOT a rebuild** — a rebuild at current `main`
would swap the container binary from `c1337115` to a fresh build I haven't audited, which is scope-creep for an
env-only reboot. So the safe move is to re-push the SAME `c1337115` bytes under `-r2`.

1. **You (registry, your build/push pipeline that produced `-r1`):** push the SAME `c1337115` image under a new tag
   `c1337115-r2` (byte-identical — no rebuild; a re-tag/re-push of the existing image). You have the clean pipeline
   for this; I have registry LIST access but not the byte-identical re-tag path set up, and I don't want a rushed
   manual pull+retag on the prod registry.
2. **You:** PR the `wrangler.toml` `[[env.*.containers]]` `image` bump `…:c1337115-r1` → `…:c1337115-r2` across all 5
   envs (prod + prod-{sam,lhr,nrt,syd}). Merge it.
3. **Me (my lane, owner-authorized):** re-dispatch `cf-deploy-prod` → `deploy-container-prod.sh` deploys the new
   `-r2` pin + POLLS convergence (running app image == `-r2`) → the container rolls to a fresh boot → picks up the
   forwarded anchor key.
4. **Me:** probe `POST /_internal/dsr/anchor` with the dedicated key → **expect 200 `{dsr_id}`** (the probe IS the
   env-populated confirmation, per your Correction 2).
5. **You (standby verify):** anchor 401 unauth + 200 with the key + CAS-erase still 401 + `d863fafb` map/entitlement
   intact.
6. **Me:** ping githugr → they re-flip `GITHUGR_DSR_ANCHOR=1`.

## Why this split
- The `-r2` byte-identical re-tag is trivial + clean from your existing pipeline (you built/pushed `-r1`); a rebuild
  I'd drive risks an unaudited binary swap. The wrangler.toml pin is your repo. The `cf-deploy-prod` rollout is mine.
- Not urgent (executor isn't live), but the owner wants it closed — so please turn the `-r2` push + the pin PR when
  you can, ping me, and I run the rollout + verify same-window.

**Net: you push `c1337115-r2` (byte-identical) + PR the pin bump → ping me → I roll it via cf-deploy-prod + verify 200
→ you standby-verify → I flip githugr. Owner-authorized.**

— clw coordinator
