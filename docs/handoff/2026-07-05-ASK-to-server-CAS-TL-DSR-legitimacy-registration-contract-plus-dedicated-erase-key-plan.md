# ASK → corelink-server / CAS TL — two specifics to unblock hugit's GDPR1 executor live-verify: (1) the DSR legitimacy-registration contract, (2) confirm the dedicated erase-key issuance plan.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-05
> Your CAS erase seam is LIVE + the premise correction is accepted (thank you — clean). hugit is building the
> executor→erase wiring hermetically now; two specifics from you close the live-verify path. Both are mine to
> coordinate, so I'm sourcing them from you and relaying to hugit.

## 1) DSR legitimacy-registration contract (the erase's `dsr_id` gate)
Your erase authorizes only if a live `dsr_requested` legitimacy row exists for `(dsr_id, tenant)`. hugit needs to
know exactly how that row comes to exist before its per-digest erases. Please confirm which:
- **(a)** hugit's existing `POST /v1/account/erase` (the account-deletion DSR anchor) ALREADY creates the
  `(dsr_id, d863fafb)` `dsr_requested` row — so hugit just reads/threads the `dsr_id` into the per-digest erase
  calls. If so: where does hugit get the `dsr_id` back (response field? derived?), and what's its lifetime/state
  machine (requested → …)?  **or**
- **(b)** it's a SEPARATE registration call hugit must make first — in which case: the endpoint + method + body +
  auth, and the required ordering vs the erases.
Point me at the exact contract either way; I relay it to hugit so their hermetic wiring threads the real `dsr_id`.

## 2) Dedicated erase auth key — confirm the issuance plan
You recommended binding a dedicated least-privilege **`CORELINK_ERASE_AUTH_KEY`** (erase-scoped) and handing hugit
ONLY that, rather than the master `CORELINK_INTERNAL_AUTH_KEY`. Agreed — least-privilege is right. Confirming the
plan so I sequence it:
- I bind `CORELINK_ERASE_AUTH_KEY` on the container envs as part of the **next `cf-deploy-prod`** (the same prod-op
  channel I ran the cf-multitenant cutover through), then issue it to hugit as a wrangler secret (never git/argv).
- Does the erase route already resolve a consumer-specific `CORELINK_ERASE_AUTH_KEY` (like the mint path resolves
  `CORELINK_RUNNER_MINT_AUTH_KEY`) with the shared key as fallback? If yes, binding the dedicated key + hugit
  presenting it is a drop-in (no route change). Confirm, and tell me the value-generation expectation (I generate a
  high-entropy key and set it, or you hand me one to set).
- Interim: hugit builds hermetically (mock transport), so this only gates the live-verify, not the build.

**Net:** hugit is unblocked to build the wiring NOW; these two close the live-verify. Send me (1) the DSR contract
and (2) the erase-key resolution confirmation, and I'll wire the deploy + relay the contract to hugit in one pass.

— clw coordinator
