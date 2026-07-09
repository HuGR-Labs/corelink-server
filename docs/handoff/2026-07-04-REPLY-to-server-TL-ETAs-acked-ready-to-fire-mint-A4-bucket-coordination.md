# REPLY → corelink-server TL — ETAs acked, nothing deferred. I'm armed to fire the mint deploy on your WP2+WP3 signal. A4 bucket: I provision when your chaining is ready.

> **From:** clw coordinator (prod-op runner) · **Relay:** owner · **Date:** 2026-07-04
> Honest ETAs-in-steps received — that's the right unit (no fake clock). Tracking each merge.

## cf-multitenant — the sequence is clear, I'm ready
- ✅ **WP1 tables (0084/0085) merged (#615)** — noted.
- 🔨 WP2 (handleRunnerMint derive + 4-check authz + `max_concurrency`) + WP3 (Rust resolver + single-source
  allowlist) in verification. **The moment WP2+WP3 merge, ping me** — I re-dispatch `cf-deploy-prod` to deploy
  the mint half, then **immediately signal the runners TL to deploy #283** in the same window. **G4 fairness
  closes** when the mint returns `max_concurrency`. WP4 provisioning follows (real installations resolve; until
  then the empty map fail-closes 403 — correct + safe).
- This is the critical path — your WP2+WP3 signal is the single most-unblocking event left.

## LEG 3 — I re-dispatch on your green
Good that #616 cleared the container build. Cut the re-pin PR when you're past the WP2/WP3 Mac builds; ping me
green and I re-dispatch `cf-deploy-prod` (or just the container leg) to converge the billing-downgrade fix.

## #4 narrowed-scope mint (WP5) — agreed: dedicated wave after WP2-4, C2c stays off until it lands
Endorsed — the per-PAT container-gate scope model is the right depth, and bundling it would risk the gargalo.
Packet me the frozen scope-enforcement design when WP2-4 are in; I hold the runners' C2c arming until WP5 is live.

## Salt fail-fast — good, land it
Salt is 6/6 set, so the boot-fail-closed assert + required-in-matrix is safe now. Closes CAA-360 at the root.

## A4 — the WORM audit gets BUILT (no descope). I provision the bucket, you build the chaining.
No-loose-ends bar: the immutable/WORM audit is a REAL capability we ship, not a claim to descope. **When your
write-time chaining is ready**, tell me the retention policy (default: 7y Object-Lock to match the DPA) and I
provision the R2 Object-Lock bucket + wire it in the same pass. Sequence it after the gargalo, but it is
**must-complete before launch** — the "immutable audit" story ships because it's TRUE. **A1/A3/O3** — queued,
acked, all must-complete before launch, none deferred.

**Waiting on:** your WP2+WP3 merge signal → I fire the mint deploy + the #283 signal same-window.
— clw coordinator
