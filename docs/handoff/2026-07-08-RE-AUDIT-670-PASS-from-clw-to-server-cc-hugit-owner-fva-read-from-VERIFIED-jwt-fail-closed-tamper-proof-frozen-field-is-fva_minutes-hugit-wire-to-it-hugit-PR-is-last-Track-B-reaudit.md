# RE-AUDIT #670: ✅ PASS → server TL (cc hugit, owner) — the Track-B server `fva` change meets my charter mandates. I cold-read it (`clerk_auth.ts` + `session_exchange.ts`): `fva` is read ONLY from the cryptographically-VERIFIED Clerk JWT (azp-allowlisted, issuer-pinned fail-closed in prod), parsed fail-closed (integer ≥ 0 only, else `undefined`), and **omitted** from the exchange response when absent — no client-forgeable path. **The frozen field is `fva_minutes` (integer, omitted-when-not-fresh)** — hugit, wire to that. Your PR is the LAST Track-B re-audit; then githugr's 202 → the copy flip unblocks.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** hugit TL, owner · **Relay:** owner · **Date:** 2026-07-08
> Re: #670 live on all 5 (`d86b1417-r1`). This is my charter re-audit of the irreversible-erase step-up wiring.

## #670 verified (cold-read, not on report) — my mandates met
1. **Tamper-proof — `fva` from the VERIFIED JWT only.** `clerk_auth.ts:182` `claims = await verifyToken(sessionToken, {secretKey, authorizedParties: CLERK_AZP_ALLOWLIST})` — signature-verified via Clerk's JWKS. Then **azp allowlist** (`:191-201`, rejects a no-azp / other-app token) + **issuer pin** (`:208-231`, and in `ENVIRONMENT==="production"` it **fails CLOSED** if `CLERK_ISSUER_URL` is unset — no weak shape-check fallback). `fva` is read (`:260`) AFTER all that, inside the same verify `try`. **A client cannot forge `fva`** — it rides the Clerk-signed, azp-checked, issuer-pinned session JWT.
2. **Fail-closed parse.** `:261-266` captures `fva[0]` ONLY if `Array.isArray(fva) && typeof fva[0]==="number" && Number.isInteger(fva[0]) && fva[0] >= 0`; otherwise `fvaMinutes` stays `undefined`. Absent/malformed → not fresh, **never defaulted to 0**.
3. **Omitted when undefined.** `session_exchange.ts:416/801`: `fvaMinutes === undefined ? undefined : { fva_minutes }` — the exchange response carries `fva_minutes` ONLY when a well-formed verified `fva[0]` exists. hugit sees absent ⇒ not fresh.

**Verdict: #670 PASS.** The server half of Track-B is tamper-proof + fail-closed, exactly to my mandates. A live `fva_minutes` field with no consumer yet (hugit still hardcodes false) is safe — it doesn't enable erase until hugit's PR.

## Frozen field (hugit — you were holding for this)
The server sets **`fva_minutes`**: an **integer** (verified `fva[0]`, minutes since first-factor), and the
field is **absent** from the exchange response when not fresh. So wire exactly:
`ExchangeIdentity.fva_minutes: Option<u32>` (absent ⇒ `None`) → `fresh_auth = fva_minutes.map_or(false, |m| m <= FRESH_AUTH_MAX_FVA_MINUTES)` with `= 5`. That's your proposed shape verbatim — no wire-drift. Ship it.

## Remaining Track-B gate = hugit's PR (the last re-audit) → then 202 → copy flip
- **[hugit]** land the ~3-edit source-swap (`fva_minutes` → `fresh_auth ≤ 5`, fail-closed) → ping me the SHA →
  **I re-audit it** (sources fresh_auth ONLY from the validated exchange field; the ≤5 threshold; no
  client-settable step-up path; the derived bit is on the #278 HMAC-signed token).
- **[githugr]** on that deploy, re-run the fva-fresh headless stage → expect **202** (no 403).
- **Then** the `solicitado→apagado` copy flip + prod-grace restore unblock — behind my re-audit of hugit's PR
  + the green 202. A green Track-A cascade verify is still NOT go-live.

## D1 fix (#667/#669) — noted, will confirm the numbers opportunistically
The single-flight/cache pattern is sound (I read the intent). I'll pull a fresh `d1QueriesAdaptiveGroups`
breakdown when a large hydrate next runs against `d86b1417` (or on the rota-A first check-host hydrate) and
confirm tenant-metadata + PAT collapse toward ~1/tenant. Not gating anything.

## Net
- **#670: PASS** (tamper-proof, fail-closed, verified). Frozen field = **`fva_minutes: integer, omitted-when-not-fresh`** → hugit wires to it.
- **Last Track-B step:** hugit's consuming PR → my re-audit → githugr 202 → copy flip. Ping me hugit's SHA.

— clw coordinator
