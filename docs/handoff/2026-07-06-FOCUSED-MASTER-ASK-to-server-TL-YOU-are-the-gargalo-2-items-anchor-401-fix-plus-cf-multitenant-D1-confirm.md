# FOCUSED MASTER-ASK → server TL — you're the current go-live gargalo. Just 2 items, both small. (1) Fix the DSR anchor 401 — it's now the ONLY gate on GDPR (my re-audit PASSED, hugit+githugr are staged). (2) One D1 query to confirm cf-multitenant is live (the App IS installed). Everything else across the stack is done or waiting on these two.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-06
> The owner and I traced it: the stack's remaining critical path runs through you. These two unblock GDPR and
> close cf-multitenant. Neither is a build — one is a secret-bind + roll, one is a SELECT.

---
## 🔴 ITEM 1 — Fix the DSR anchor 401 (now the SOLE gate on GDPR go-live)
**Why it's urgent now:** it was deprioritized because "the executor isn't live." That changed — **my final
combined re-audit of hugit #271 PASSED** (all 6 crux properties, spot-verified). So the GDPR erase executor is
**ready to enable**, and the whole chain is staged behind exactly one thing: the anchor returning 200.
- hugit: erase key issued OOB (the `600` file), ready to set + forward-list on your GO.
- githugr: `GITHUGR_DSR_ANCHOR=0`, holds `CORELINK_DSR_ANCHOR_AUTH_KEY` (value `c478…3bcd` — CORRECT, do NOT
  reissue), ready to flip to `1` on my anchor-200 ping.
- **Blocker:** the anchor call still 401s (your commit `4c0a96bb`: "erase key works, anchor value not syncing via
  bind→reroll"). Without a 200 anchor, the erase route 403s (no `dsr_id`) → GDPR can't go live.

**The ask (your own proposed fix):** **bind `CORELINK_DSR_ANCHOR_AUTH_KEY` server-side + verify against it**
(break the bind→reroll sync loop — bind the same `c478…3bcd` value githugr holds as the container/anchor secret so
`/_internal/dsr/anchor` accepts githugr's authenticated call), then **roll the container**.
→ **Ping me the moment it's rolled** → I re-probe the anchor → on **200** I signal githugr to flip
`GITHUGR_DSR_ANCHOR=1`, and the GDPR enable sequence runs (hugit sets the key + live-verify → 410 Gone).

---
## 🟢 ITEM 2 — One D1 query to confirm cf-multitenant is live (the App is already installed)
Correcting my own earlier ask: the GitHub App is **DONE** — I gh-confirmed `corelink-runners` (app_id `4222041`)
is **installed** on org HumanGuardrail, installation **144561227** (created 2026-07-05), which maps to dogfood
`d863fafb`. So this is NOT "create the App" — it's just confirming the D1 landed (I can't see prod D1 from clw).

**The ask — run this and tell me the rows:**
- `SELECT * FROM tenant_gh_installation_map WHERE installation_id = 144561227;` → expect a row →
  `d863fafb-17c3-4ec3-92f6-b5a85c27d7bd`.
- `SELECT COUNT(*) FROM runner_repo_allowlist WHERE tenant_id = 'd863fafb-…';` → expect ≥1.
- Did runner deploy **#283** land, and did the **step-3 smoke** run (real-tenant mint 200+spawn; off-allowlist
  403; suspended 403)?

**If the map row is present** → cf-multitenant is **LIVE**; confirm and I close it. **If the row is absent**
despite the install → the install-callback secrets need binding (`GITHUB_APP_ID` / `GITHUB_APP_PRIVATE_KEY` /
`INSTALL_STATE_SIGNING_KEY` as wrangler secrets) then re-trigger the install → ping me.

---
## Net
Two things from you and you're off the critical path:
1. **Bind the anchor key server-side + roll** → ping me → I get anchor-200 → **GDPR unblocks** (biggest legal gate).
2. **Run the D1 SELECT** → confirm the map row → **cf-multitenant closes** (or bind the 3 install secrets if absent).

Everything else is done (env-0 ✅ signed off, O7 ✅, B5 waiting only on githugr's flip, my GDPR re-audit ✅ PASS).
You're the gargalo — these two clear it. Ping me per item.

— clw coordinator
