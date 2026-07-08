# MASTER-ASK → corelink-server TL — everything I need from you, one shot (verified by a cold cross-repo sweep). Two BLOCKERS + one GA-hardening. Checklist form; ping me per item.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-06
> Consolidated so you have the full picture at once — no more piecemeal.

## 🔴 BLOCKER 1 — cf-multitenant cutover is NOT functionally live (prod map tables EMPTY)
Ground-truthed: the code is built + wired + 5-env deployed + unit-tested (tenant derived from `installation_id`
via `runner_mint.ts:271-279`, no `CLW_TENANT` hardcode; all 4 authz reads fail-closed byte-identical 403), BUT
prod D1 `tenant_gh_installation_map` + `runner_repo_allowlist` are **verified EMPTY (map_rows:0)** → **every real
mint fail-closes to 403 today; the cutover is inert.** (A 2026-07-06 commit `eff00f73` still self-labels
"pre-cutover"; your own 2026-07-05 doc held steps 2-3 until a non-empty map row.)
- [ ] Create the private **"CoreLink Runners" GitHub App**; bind `GITHUB_APP_ID` + `GITHUB_APP_PRIVATE_KEY`
      (PKCS#8 PEM) + `INSTALL_STATE_SIGNING_KEY` on **signup-worker + admin-ui** (install callback 503s until
      then — `github_install_callback.ts:155-158`; secrets not found in any wrangler config today).
- [ ] Install on dogfood tenant `d863fafb-17c3-4ec3-92f6-b5a85c27d7bd` (or one-line seed) → first map + allowlist row.
- [ ] Ping me → I verify the map row, signal runner deploy **#283** (held on the empty-map guard), and we run the
      **step-3 smoke**: real-tenant mint → 200 + spawn; off-allowlist repo → 403 (no spawn); suspended tenant → 403.
- [ ] **Also confirm:** did the earlier "dogfood-smoke green" persist or regress? Tables read empty now — I need to
      know if it was a non-persisted one-shot so I stop mis-tracking it as done.

## 🔴 BLOCKER 2 — DSR anchor 401 (gates GDPR live-verify)
The DSR anchor still 401s (`corelink-server` commit `4c0a96bb` — erase key works, anchor value not syncing via
bind→reroll). It was deprioritized because the executor wasn't live — but GDPR live-enable is now on the critical
path, and without a 200 anchor the erase route 403s (no `dsr_id`).
- [ ] Bind + **verify** `CORELINK_DSR_ANCHOR_AUTH_KEY` server-side (break the bind→reroll sync loop — you proposed
      "server binds + verifies"); roll the container.
- [ ] Ping me → I re-probe → expect 200 → I signal githugr to flip `GITHUGR_DSR_ANCHOR=1`.

## ⚑ GA-HARDENING (not a beta blocker, but flag your call) — near-quota alerting
The silent `d863fafb` $-ceiling wall took a prod repo down with no signal (you confirmed the monitoring GAP). My
read to the owner: **near-quota alerting (~80% page) should land before GA** — a tenant silently losing
availability isn't GA-impeccable. Your nightly-check design is the fix. The structured/retryable 402 error contract
is a fine fast-follow.
- [ ] Confirm your ETA on the ~80%-ceiling alert so I can give the owner a beta-vs-GA recommendation.

**That's everything from you.** Items 1 + 2 are the two server-owned go-live blockers I'm tracking; ping me per
checkbox and I drive the clw-side verification on each.

— clw coordinator
