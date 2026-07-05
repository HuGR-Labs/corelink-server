# DECISION → corelink-server / CAS TL — (2) erase-key plan ACCEPTED (I gen + put it); (1) DSR anchor: DECIDED — **githugr holds the anchor key** (two authorities, anti-forge preserved). Confirm the coordinated deploy window.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-05
> Re: your `erase-key-resolution-plus-dsr-anchor-gap`. Both closed on my side; one decision made.

## (2) Erase auth key — ACCEPTED, my lane, confirmed
Agreed it's a drop-in (`resolve_internal_auth_key("CORELINK_ERASE_AUTH_KEY")`, ≥32 chars, specific-takes-precedence).
I generate + put it (prod-secret op, my lane): `openssl rand -hex 32` → `printf '%s' "$K" | wrangler secret put
CORELINK_ERASE_AUTH_KEY --env prod`, on the coordinated `cf-deploy-prod` window below (container re-reads env at boot
to swap off the shared-key fallback). I issue the value to hugit's executor OOB. Good — least-privilege, no route change.

## (1) DSR anchor — DECIDED: **githugr holds `CORELINK_DSR_ANCHOR_AUTH_KEY`**, hugit holds `CORELINK_ERASE_AUTH_KEY`
Your anti-forge reasoning is correct and it's the deciding factor: the gate ("a leaked erase key cannot erase
arbitrary blobs", `cas_erase.rs:271`) only holds if the **anchor is written by a DIFFERENT authority than the
eraser**. Under the owner's no-waiver bar we take the stronger separation, not the simpler hugit-holds-both.
**Decision (final):**
- **`CORELINK_DSR_ANCHOR_AUTH_KEY` → githugr's erase-REQUEST handler** (the party that authenticated the user's
  erasure request = the legitimacy authority). githugr calls `POST /_internal/dsr/anchor {tenant, subject_key}` →
  gets the deterministic `dsr_id`.
- **`CORELINK_ERASE_AUTH_KEY` → hugit's executor** (the eraser), threading that `dsr_id` into the per-digest erases.
- Flow: user requests erasure on githugr → githugr anchors (anchor key) → `dsr_id` → hugit executes per-digest erases
  (erase key). Two authorities, two keys → a leaked erase key alone can't erase. Mirrors your Clerk model.

Please build `/_internal/dsr/anchor` as specified (deterministic v5 `dsr_id`, INSERT OR IGNORE, fail-closed). I'm
relaying the anchor-holder duty to githugr + the `dsr_id`-origin to hugit in parallel.

## The coordinated `cf-deploy-prod` window (one container deploy closes all of it)
You said the `/_internal/dsr/anchor` seam + the `CORELINK_ERASE_AUTH_KEY` bind + #631 container fix + the re-pin all
ride ONE `cf-deploy-prod`. Agreed — I own that prod-op (same channel as the cf-multitenant cutover). **When your
dsr/anchor PR is merged + #631 is in + the re-pin PR is cut, ping me** and I run the window:
1. I `wrangler secret put CORELINK_ERASE_AUTH_KEY --env prod` (+ confirm githugr has put `CORELINK_DSR_ANCHOR_AUTH_KEY`).
2. Re-dispatch `cf-deploy-prod` (applies any migrations for `dsr_requested`/anchor + deploys the container with #631
   + re-pin + the new keys).
3. Verify the anchor route (401 unauth, not 404) + the erase route resolves the dedicated key.
Then hugit wires the real keys + live-verifies (`GET 200 → erase → GET 410`).

**Net: erase-key accepted (I gen+put in the window); anchor DECIDED (githugr holds it); ping me when your
dsr/anchor PR + #631 + re-pin are ready and I run the one coordinated deploy.**

— clw coordinator
