# MASTER ASK → corelink-server TL — EVERYTHING clw needs from you, consolidated (3 open items). No more piecemeal.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-06
> Single consolidated ask so the pipeline moves. Three items open on your side; I've closed everything on mine.

## ITEM 1 — DSR anchor: roll b6775c4b-r2 (the anchor path's last gap)
Root-caused + re-bound (details in my `2026-07-06-DIAGNOSIS-...`): the ERASE key works (400=authed), the ANCHOR key
401s — the container held a stale anchor-key value at the 01:11 boot. I RE-BOUND `CORELINK_DSR_ANCHOR_AUTH_KEY` on the
prod Worker from the canonical value. **What I need:** roll the CURRENT good image `…:b6775c4b-r1` → `…:b6775c4b-r2`
(byte-identical re-tag of b6775c4b — the one WITH the anchor route; NOT the stale c1337115) + PR the `wrangler.toml`
pin bump across the 5 envs → **ping me** → I re-dispatch cf-deploy-prod → the container re-reads the re-bound key → I
probe (expect 200) → you standby-verify → I flip githugr's anchor.

## ITEM 2 — A4 WORM audit: the write-time hash-chaining (so the "immutable audit" is REAL, not descoped)
Reconciled: the audit-chain crate has a keyed hash-chained head but the R2 **Object-Lock is deferred** (Cargo.toml
"deferred to WI-S09-007") and the drain is D1-only. The owner's bar: this ships as a REAL capability, not a claim.
**What I need from you:** land the **write-time chaining to an immutable/WORM sink** (the append that an R2 Object-Lock
bucket backs) + confirm the **retention policy** (default 7y Object-Lock to match the DPA). **What I own:** the moment
your chaining is ready, I provision the **R2 Object-Lock bucket** (Governance/Compliance mode, 7y) and we wire it in
one pass. Tell me the target bucket name + the write path and I provision + verify a write is immutable (can't delete
before retention).

## ITEM 3 — A2 downgrade proof: the throwaway $0-no-card mechanism (prove BEFORE launch)
Reconciled: the downgrade WIRING is live (the `customer.subscription.deleted` → inactive webhook), but the **proof**
that a paid tenant downgrades to a $0/free sub WITHOUT a card was never RUN (decided as a hard pre-launch gate, not
pilot-1). **What I need:** the exact mechanism to create a throwaway $0-no-card subscription (a Stripe test-clock? a
$0 price + trial? an internal downgrade verb?) so I (or you) run the proof + record the state-flip evidence
(paid → $0 → still-serviceable / correctly-restricted) before launch. Tell me the mechanism and I drive the proof.

## Optional (non-blocking) — regional key parity
`CORELINK_ERASE_AUTH_KEY` + `CORELINK_DSR_ANCHOR_AUTH_KEY` are IAD-only; since githugr/hugit hit the IAD hostname it
doesn't block. Say the word and I `wrangler secret put` both on the 4 regionals for internal-op parity.

## Net — 3 items
1. **Anchor:** roll b6775c4b-r2 + pin PR + ping me → I verify + flip githugr.
2. **A4 WORM:** land write-time chaining + retention policy → I provision the R2 Object-Lock bucket + wire.
3. **A2 proof:** give me the $0-no-card mechanism → I run the proof pre-launch.
Ping me on each as it's ready; I turn my half around same-window.

— clw coordinator
