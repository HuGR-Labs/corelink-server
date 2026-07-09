# FOR clw coordinator — vetted one-shot: delete githugr orphan TEST tenants from prod CONFIG_DB (5 known + pre-launch zero-activity sweep). Authorized by githugr TL. Read-only candidate list FIRST → githugr ✓ → delete.

> **From:** corelink-server TL · **To:** clw coordinator (prod-op runner) · **cc** owner + githugr TL · **Relay:** owner · **Date:** 2026-07-03
> **Auth:** githugr TL AUTHORIZE doc (option 1+2). ⏱️ **Timing guard: run BEFORE the pilot opens** — pre-launch, every githugr-derived tenant is a test fixture (zero real users), so the sweep is safe today; after launch a zero-activity tenant could be a real new signup → revert to option-1-only.

## Context
Every githugr Track-B / e2e session that hit `/v1/session/exchange` provisioned an isolated CoreLink tenant = **5 rows** across `tenant`, `tier_selections`, `runners_entitlement`, `tenant_quota`, `tenant_org_map` (via `provisionOrLookupGithugrTenant`). Clerk users were deleted at harness cleanup; only these D1 rows persist (inert, no cost/security impact — clutter). Every githugr-derived tenant carries the unique marker `tier_selections.correlation_id = 'githugr-provision:'||tenant_id`.

All SQL below is run via: `worker/node_modules/.bin/wrangler d1 execute CONFIG_DB --env prod --remote --command "…"` (or `--file`).

## STEP 1 — candidate list (READ-ONLY; send the output to githugr TL to eyeball ✓)
```sql
SELECT ts.tenant_id,
       COALESCE(MAX(uc.cas_bytes_used),0)  AS cas_bytes,
       COALESCE(MAX(uc.ac_calls_used),0)   AS ac_calls,
       COALESCE(MAX(tss.bytes_used),0)     AS stored_bytes
FROM tier_selections ts
LEFT JOIN usage_counter uc         ON uc.tenant_id  = ts.tenant_id
LEFT JOIN tenant_storage_state tss ON tss.tenant_id = ts.tenant_id
WHERE ts.correlation_id LIKE 'githugr-provision:%'
GROUP BY ts.tenant_id;
```
The **zero-activity** rows (`cas_bytes=0 AND ac_calls=0 AND stored_bytes=0`) are the sweep candidates. The **5 known** tenants below MUST appear in this list (cross-check that the derivation matches live). Send the list to githugr TL; delete ONLY after their explicit ✓.

## STEP 2 — the 5 KNOWN test tenants (delete unconditionally — githugr TL gave these + I independently re-derived all 5 from their subs, EXACT match)
```
f7a4db66-1e2b-5bd5-97fa-acbaba97ab1b   (user_3FzoxYKTIMqBHOyMCqax4oOfHHA — Track-B A)
137fd98d-977c-5769-a6dc-b020f4a73a4e   (user_3Fzoxfko3VfVeTTPQsOGavuHZ22 — Track-B B)
6e496e67-db4b-5aa6-a8ba-3a5ea57ab2eb   (user_3FzpBplPPJPkqYSiRG43BQXpsII — e2e-push 1)
53b2a81e-73a1-5980-88f3-8a6016aed0df   (user_3FzpJd7CkAWGQ9thTu9lTktZmfo — e2e-push 2)
d8a4dc0c-c170-5b2d-bd50-b8cdc6d712d3   (user_3FzpXvldoU5Io9vBkfVWjxI2tQk — e2e-push 3)
```

## STEP 3 — delete (per authorized tenant_id `T` — children FIRST, `tenant` LAST for FK safety)
```sql
DELETE FROM tenant_storage_state WHERE tenant_id='T';
DELETE FROM usage_counter        WHERE tenant_id='T';
DELETE FROM tenant_quota         WHERE tenant_id='T';
DELETE FROM runners_entitlement  WHERE tenant_id='T';
DELETE FROM tier_selections      WHERE tenant_id='T';
DELETE FROM tenant_org_map       WHERE tenant_id='T';
DELETE FROM tenant               WHERE tenant_id='T';
```
(The first two are no-ops for a genuinely zero-activity tenant — included for completeness + FK order.) Run STEP 1 again after: the deleted tenants should no longer appear. **Safety:** delete ONLY the 5 known + the githugr-✓'d candidates; never a tenant_id not on that reconciled list.

## Heads-up (not blocking your D1 sweep)
githugr TL flagged that those same test subs created orphan **repos on the hugit engine** (`tb-e2e-*` / `e2e-push-*`) under the deleted tenants — that's hugit's prod to GC. githugr TL is sending the hugit TL a parallel note; nothing for you here.

Ping me / githugr the STEP-1 output; delete after their ✓. Routing via owner.

— corelink-server TL
