# REPLY → corelink-server TL — ✅ DONE: container endpoint tightened 236 → 10 (matrix). Webhook-events front closed.

> **From:** clw coordinator (prod-op runner) · **To:** corelink-server TL · **cc** owner · **Date:** 2026-07-03

## ✅ Container endpoint `we_1Tfh8P…` — 236 → 10, verified
- **Gate check first:** confirmed **PR #601 is MERGED** (2026-07-03T16:39Z) and the local
  `crates/corelink-billing-stripe-materializer/container-webhook-events.json` is on main, **matches origin/main**
  (10 events) — so the JSON source-of-truth is canonical before I touched anything.
- **Dry-run** (`--live`, `--url` container, `--json` matrix): **ADD=0, REMOVE=226** — the STOP condition
  (any `+ ADD`) did NOT fire, exactly as expected. Spot-checked that the 10 kept are the matrix (the removes
  never touched `charge.dispute.created`, `charge.refunded`, `customer.created`, the four
  `customer.subscription.{created,deleted,trial_will_end,updated}`, `invoice.created/paid/payment_failed`).
- **Applied** (`--yes --live`, restricted `rk_live_`) → `✅ DONE`.
- **Independently verified** (re-read `we_1Tfh8P…`): **exactly 10 events**, matching
  `EVENT_MATERIALIZATION_MATRIX`. Every removed event was a no-op ack in the container → no behavior change,
  pure least-privilege. `Resolved Stripe account: ?` was the expected restricted-key cosmetic, as you noted.

## Net — webhook-events front CLOSED
- signup-worker `we_1Tolig…`: **7** (canonical, async grant path live).
- container `we_1Tfh8P…`: **236 → 10** (matrix, least-privilege).
- `api.humangr.com/_wallet` `we_1TJ1YZ…`: 4 (untouched — not in scope; flag if you want it reviewed).
- No stray endpoints exist.

That's the entire Stripe webhook surface reconciled to its code-owned sources of truth. Ping if you want the
`_wallet` endpoint looked at; otherwise this front is done.

— clw coordinator
