# DIRECTIVE → corelink-server TL — OWNER OVERRIDE: the narrowed-scope mint (WP5) is PRE-LAUNCH, not a post-WP2-4 deferred wave.

> **From:** clw coordinator · **Relay:** owner (Gustavo) · **Date:** 2026-07-04
> **Owner ruling, explicit, today:** NO waivers, NO loose ends, 100% complete.

## WP5 (deny-DELETE / AC create-only per-PAT scope) — PRE-LAUNCH
You scoped WP5 as a dedicated wave AFTER WP2-4. Under the owner's no-waiver bar it becomes a **pre-launch gate**,
because it pairs with the runner env-0 fix I've just directed the runners TL to make pre-launch:
- The CF runner path will (pre-launch) stop putting the PAT in the untrusted env and instead redeem a
  **scope-narrowed** per-job PAT via the cred-ticket broker. That narrowing is YOUR mint enforcement: the minted
  per-job PAT must **deny DELETE** (CAS+AC) + be **AC create-only**, enforced server-side / at the container
  CAS/AC gate (a per-PAT scope model, as you noted). If the output workspace name is known at spawn, restrict AC
  writes to the exact `BLAKE3("clw/ref/runner/v1/"‖name)` key.
- **Why pre-launch:** without it, an in-window stolen per-job PAT can r/w the tenant's WHOLE cache (poison-blast).
  Launching with that open = a waiver the owner refuses.

## Sequencing
WP2-4 (the gargalo authz) proceed as planned — deploy the mint half on your signal (I fire #283). **WP5 (the
narrowed scope) must land before the CF runner path serves real untrusted users.** Packet me the frozen
scope-enforcement design as soon as you can — it's no longer a post-launch wave, it's on the launch critical path.
Everything else in your ETA reply (LEG 3, salt fail-fast, A1/A3/A4-WORM/O3) stays must-complete-before-launch.

— clw coordinator
