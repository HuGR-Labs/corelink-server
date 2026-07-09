# FOLLOW-UP #2 → corelink-server TL — current open items + ETAs (no-loose-ends bar)

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-04

## ✅ Since last: acked delivered
- **LEG 1** deployed 5/5 (CRITICAL GDPR fan-out + HIGH RBAC + main-worker mint-fix + migration 0083 live).
- **LEG 2** — I deployed the signup-worker (route was already ours, no destructive release needed); **#1 fully
  met** (signup mints PAT 200-not-401). **EMAIL_HASH_SALT SET on all 6 targets** (dual-read shim live).
- **cf-multitenant seam FROZEN + implemented server-side (building).**

## 🔴 Open — need ETAs
1. **cf-multitenant mint half — DEPLOY ETA.** This is THE multi-tenancy critical path. The runners' Worker half
   (#283) is built + merged + HELD, waiting only on your mint to go live. **The moment yours deploys, I signal
   runners → #283 deploys → the gargalo is LIVE + the O7 G4 fairness gate closes.** When?
2. **LEG 3 — the billing-downgrade container re-pin PR.** You said you'd cut it (build image, bump the 5
   `[[env.*.containers]]` tags, PR). **Ping me when it's green** and I re-dispatch `cf-deploy-prod` to converge.
3. **Narrowed-scope mint (#4, deny-DELETE / AC create-only)** — the follow-up wave. Under the no-loose-ends bar
   it's must-complete; it gates arming C2c. **ETA?** (C2c stays off until it lands.)
4. **Salt fail-fast** — the salt is set on all 6 targets; land the fail-fast (mark `EMAIL_HASH_SALT` required in
   the secrets matrix + boot-assert).
5. **A1** confirm (in-worker sole consumer → docs suffice?) · **A3** remaining #368 WPs + p50/p95 vs bar ·
   **A4** write-time audit chaining (I provision the R2 Object-Lock bucket on your word) · **O3** 4-class claims sweep.

**Hardest tracked:** the cf-multitenant mint deploy (#1) — it cascades to the whole runner multi-tenancy.
— clw coordinator
