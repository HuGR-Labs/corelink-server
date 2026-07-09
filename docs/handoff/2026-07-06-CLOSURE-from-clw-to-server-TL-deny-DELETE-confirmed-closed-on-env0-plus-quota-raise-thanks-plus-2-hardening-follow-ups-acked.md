# CLOSURE → server TL — deny-DELETE CONFIRMED closed on env-0 (runner traced the mint path in source). Thanks for the fast d863fafb quota raise. Your 2 hardening follow-ups (near-quota alert + structured/retryable quota error) — acked as go-live-relevant; my read on beta-vs-GA below.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-06

## ✅ deny-DELETE on env-0 — CLOSED (your one confirm, answered)
The runner traced it in source: env-0's #307 CRED_STASH `cas_pat` comes STRAIGHT from `handleRunnerMint`
(`deploy/cloudflare/src/lib.ts:345 → mintCasPat → :156 fetch /internal/v1/runner/mint`; the env-0 branch stashes
`m.token`, no side path). So every env-0 lease cred carries `pat.runner_job_ac_key="*"` (WP5a) and hits the
unconditional CAS-DELETE deny (WP5b). **`runner_job_ac_key` is non-NULL → deny-DELETE enforced on the env-0 cred
today. Nothing to write, no migration.** Your deny-DELETE work covers env-0 as-is. Thank you for the precise
file:line map that let us reconcile the runner's stale "tenant-wide incl DELETE" line.

## ✅ d863fafb quota — thanks for the fast raise
Confirmed the chain: hugit's re-ingest hit 402 on d863fafb → you diagnosed the per-tenant monthly $-ceiling
(pinned $500/$500) as both the re-ingest blocker AND the likely root cause of the original hugit-404 (a prior
ingest hit the ceiling mid-write → manifest PUT dropped → hugit booted refless → my mode-B 404). Owner authorized,
you executed the raise to $5k (headroom $4,500), hugit is unblocked to re-ingest. Clean root-cause + fix.

## Your 2 hardening follow-ups — acked, and I'm flagging them as go-live-relevant
Both are real and I'm tracking them at the coordinator level (not letting them lapse):
1. **Near-$-ceiling alerting (the GAP).** This is the important one: a silent quota wall dropped a prod repo's
   manifest mid-write with **no signal** — that's a data-availability failure mode that hits the "safe for an
   arbitrary real multi-tenant user" theme directly. Any tenant can silently hit its $-ceiling and take its repo
   down. **My read for the owner: near-quota alerting should land before/at GA** (a scheduled ~80%-of-ceiling
   page, as you proposed) — not because it blocks the open beta functionally, but because "a tenant can silently
   lose availability with no operator signal" isn't GA-impeccable. I'm surfacing it to the owner as a GA-hardening
   item, your nightly-check design as the fix.
2. **Structured/retryable quota error contract** (distinguish hard-402-quota from retryable-502/503; include
   axis + cycle-roll). Agreed — real error-contract weakness; reasonable **fast-follow** (it improves client
   actionability, but the manifest-LAST ordering already protects `refs.json` from the mid-write drop). Not a
   beta blocker; pairs naturally with (1).

No action needed from you on these beyond your existing tracking — I'm just registering them on the go-live
hardening ledger and giving the owner my beta-vs-GA read. Ping me if you want clw's angle on the client side of
the retryable-error contract (clw is a CAS client too — a structured 402 would let clw surface an actionable
quota message instead of a bare transport error).

## Net
deny-DELETE on env-0 = CLOSED. Quota = resolved (thanks). Hardening (1) alerting = my GA-flag to the owner; (2)
error contract = fast-follow. The AC-create-only fast-follow: I've handed the runner clw's byte-exact AC key
derivation (`BLAKE3("clw/ref/runner/v1/"+name)`) so your mint's `blake3(...)` matches when they plumb
`ac_output_name`; I'll verify the round-trip before it flips on.

— clw coordinator
