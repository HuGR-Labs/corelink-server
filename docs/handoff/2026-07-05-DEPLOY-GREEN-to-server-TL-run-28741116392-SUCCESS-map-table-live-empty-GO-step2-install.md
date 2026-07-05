# DEPLOY-GREEN → corelink-server TL — run 28741116392 SUCCESS. Migrations applied, map table live+empty, all 5 envs deployed. GO for step 2 (App create + install).

> **From:** clw coordinator (prod-op runner) · **Date:** 2026-07-05

## Step 1 COMPLETE ✅ (run 28741116392, conclusion: success)
- **Migrations applied** (shared prod D1): 0084 map / 0085 allowlist / 0086 ac-key / 0087 runner_billing.
- **All 5 envs deployed** (prod + prod-{sam,lhr,nrt,syd}), each past the secrets gate; container re-pin folded in (LEG 3).
- **Verified read-only:** `tenant_gh_installation_map` now EXISTS and is EMPTY (`map_rows: 0`, `allow_rows: 0`) —
  the correct pre-install state (before this deploy the table did not exist at all).

## GO — your step 2
Create the "CoreLink Runners" App → bind signup-worker + admin-ui secrets → install on dogfood tenant
`d863fafb-17c3-4ec3-92f6-b5a85c27d7bd`. The callback provisions the map row + allowlist.
**Tell me the moment the install fires** and I run step 3: verify the `d863fafb` row → signal runners #283 → smoke.

**#283 remains HELD until I see a non-empty map row.** Standing by.

— clw coordinator
