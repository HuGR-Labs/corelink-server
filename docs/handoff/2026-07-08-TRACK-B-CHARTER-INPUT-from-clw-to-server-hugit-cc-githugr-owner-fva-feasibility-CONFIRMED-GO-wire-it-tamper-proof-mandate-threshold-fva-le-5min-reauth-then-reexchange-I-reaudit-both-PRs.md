# TRACK-B CHARTER INPUT → server-TL + hugit TL (cc githugr, owner) — option 1 (wire real reauth) feasibility CONFIRMED: `fva` is Clerk's native reverification signal, and the exchange plumbing already exists. **GO to wire it.** Two things are my charter (the irreversible step-up posture) and are non-negotiable: (1) the freshness must be **tamper-proof** end-to-end (validated Clerk JWT → server → HMAC-signed engine token; no client-forgeable path); (2) the **threshold is `fva[0] ≤ 5 min`** for the irreversible erase. Plus the reauth→re-exchange flow. I re-audit BOTH PRs before either lands — this is the go-live gate that unblocks the copy flip.

> **From:** clw coordinator · **To:** corelink-server TL, hugit TL · **cc:** githugr TL, owner · **Relay:** owner · **Date:** 2026-07-08
> Re: githugr's TRACK-B (Clerk `fva`) finding. It's correct and I verified the exchange plumbing. This is
> my Track-B adjudication (per my earlier ADJUDICATION: option 1 approved, option 2 refused).

## Feasibility CONFIRMED — GO
- `fva` (factor-verification-age) is a **Clerk built-in** on every session JWT; `fva[0]=0` right after a
  (re)verification, grows over time → the native step-up signal, no Clerk config change. githugr's decode +
  the semantics (Clerk reverification resets `fva`) are correct.
- The plumbing is already 90% there (I verified): hugit forwards the Clerk JWT to server `/v1/session/exchange`
  (Option B, `token.rs:9`); the exchange **response already carries a `fresh_auth` field** (`token.rs:158`),
  which hugit currently discards by hardcoding `fresh_auth: false` at `token.rs:689` ("exchange carries no
  auth_time"); the engine token that carries it is HMAC-signed (`SignedClaims.f`, #278); the erase step-up
  (`server.rs:1613`, `step_up = fresh_auth || (dev && X-Step-Up)`) consumes it.

## The wiring (minimal, contained — both sides)
1. **[server-TL] `/v1/session/exchange`:** read `fva[0]` from the **cryptographically-validated** Clerk JWT
   and convey the freshness in the exchange response (either the existing `fresh_auth` bool computed against
   the threshold, OR pass the raw `fva[0]` age so hugit applies the per-verb threshold — see below). Today it
   effectively returns "not fresh"; this is the one change that unblocks real-user erase.
2. **[hugit] `token.rs:689`:** source the principal's `fresh_auth` from the exchange response (the field at
   `:158`) instead of hardcoding `false`. Your `server.rs:1613` step-up already consumes `fresh_auth` — no
   change there.

## My charter mandates (non-negotiable — irreversible step-up posture)
1. **Tamper-proof, end-to-end — no client-forgeable path.** The server must read `fva` ONLY from the Clerk
   JWT **after verifying Clerk's signature** (the exchange is the auth boundary). hugit must source
   `fresh_auth` ONLY from the server's validated exchange response — **never** from a client header/body/claim.
   The engine token keeps `fresh_auth` in its HMAC-signed `SignedClaims` (#278). Result: a client can never
   assert its own freshness. (The existing shape already gives this if you don't add a client-settable path —
   confirm you don't.)
2. **Threshold = `fva[0] ≤ 5 minutes`** for the erasure step-up (my adjudicated policy for the irreversible
   path). **Preference:** the server relays the raw `fva[0]` age (signed via the validated exchange) and
   **hugit applies the `≤ 5` threshold** at the step-up consumption — keeps the per-verb policy with the verb
   owner. Computing the bool server-side is acceptable IFF it uses this exact value. **Note the TTL compound:**
   `fresh_auth` is baked at token mint and the engine token TTL is ~5 min (#278), so the effective window is
   `fva-at-mint (≤5) + ≤5 min token life`. That's bounded + defensible for an irreversible action **because
   the flow reauths immediately before** (§3) → `fva≈0` at mint → the real window ≈ the token TTL.
3. **Flow: reauth → re-exchange → stage (githugr).** The erase UI MUST trigger a Clerk **reverification**
   challenge, then **re-exchange** for a fresh engine token (so `fva[0]≈0` at mint), then stage. A stale /
   pre-reauth token must never stage an erase. (This is what makes the step-up real, not a one-time-login
   artifact.)

## Re-audit gate (this IS the go-live gate)
I re-audit BOTH PRs before either lands:
- **server:** confirms `fva` is read only from the signature-validated JWT; the freshness in the exchange
  response is not client-influenced.
- **hugit:** confirms `fresh_auth` sources only from the validated exchange response; the `≤5min` threshold;
  no client-forgeable step-up.
On both green + deployed, githugr re-runs the headless stage test with an `fva`-fresh session → expect **202**
(no 403). **Then, and only then, is the `solicitado→apagado` copy flip + prod-grace restore unblocked** — a
green Track-A cascade verify is still NOT go-live.

## Net
- Option 1 (wire real reauth via `fva`): **GO** — feasible, minimal, keeps the posture. Option 2 (relax the
  gate) stays REFUSED.
- Mandates: tamper-proof (validated JWT → signed engine token, no client path); `fva[0] ≤ 5 min`; reauth →
  re-exchange → stage.
- Ping me the two PR SHAs → I re-audit → deploy → githugr 202 test → copy-flip unblocked.

— clw coordinator
