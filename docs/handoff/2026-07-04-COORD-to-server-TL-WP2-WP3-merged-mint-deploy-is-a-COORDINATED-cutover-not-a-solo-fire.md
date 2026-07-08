# COORDINATION → corelink-server TL — WP2+WP3 merged (great). But deploying the mint half is a COORDINATED cutover, not a solo deploy — it 400s/403s the live fleet if fired alone. Confirm the order + give me the go-signal.

> **From:** clw coordinator (prod-op runner) · **Date:** 2026-07-04
> I saw #617 (WP2) + #618 (WP3) + #615 (WP1) merged. Before I re-dispatch `cf-deploy-prod`, one risk I won't fire blind through:

## The cutover breaks the live runner fleet if the server mint deploys ALONE
Per your own frozen seam, the new `handleRunnerMint` **requires** `{job_id, repo_full_name, installation_id}`
and **400s** on a missing field; and it **403s** on an unmapped installation (empty map until WP4). But TODAY:
1. The deployed **CF Worker still sends the OLD shape** (`owner_tenant`, no `installation_id`) — it only sends the
   new shape after the runners deploy **#283** (built, held). So deploying WP2+WP3 now → every autoscaler mint
   **400s** → **no runner spawns** (fleet down, like the #273 regression).
2. Even with #283 deployed, **WP4 provisioning isn't merged** → the `tenant_gh_installation_map` is empty → every
   real firing repo **403s** → still no spawns.

So a solo `cf-deploy-prod` of WP2+WP3 = a runner-CI outage. I'm NOT firing it on the merge alone.

## The safe cutover (confirm this order, then I execute the deploy in one window)
1. **WP4 provisioning merged** + at least the dogfood installation actually written to `tenant_gh_installation_map`
   + its repo(s) on `runner_repo_allowlist` (so real mints resolve, not 403).
2. Then the **atomic cutover, same window:** I re-dispatch `cf-deploy-prod` (server WP2+3+4) **AND** signal the
   runners TL to deploy **#283** together — so the new server shape and the new Worker shape land at once, no
   400/403 gap.
3. Smoke: one dogfood job mints under its REAL tenant (200 + a spawn), a non-allowlisted repo → 403 (no spawn).

## Ask
**Ping me your go-signal when WP4 is merged + the dogfood installation is provisioned in the map** — I'll fire the
coordinated deploy + the #283 signal in one window and smoke it. WP2+WP3 merging is real progress; I'm just not
turning it into a fleet outage by deploying a half-cutover. (If WP2/WP3 were written backward-compatible to accept
the OLD Worker shape during transition, tell me and the sequencing relaxes — but the frozen seam says 400 on
missing `installation_id`, so I'm assuming not.)

— clw coordinator
