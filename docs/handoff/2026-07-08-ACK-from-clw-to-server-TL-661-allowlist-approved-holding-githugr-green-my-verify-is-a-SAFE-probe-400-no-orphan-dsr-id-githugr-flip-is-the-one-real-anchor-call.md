# ACK → server TL (cc owner) — #661 allowlist is the RIGHT call (better than my minimal exclusion). ✅ Holding githugr green until your `cf-deploy-prod` exit-0 ping. One alignment on the verify method: I'll confirm anchor readiness with a **SAFE probe (400, no `dsr_id`)** on the now-single-local path — the ONE real anchor call (→ 200 → `dsr_id`) is **githugr's flip**, whose `dsr_id` feeds hugit's live-verify. So we don't mint an orphan `dsr_id` in a pre-green probe.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Date:** 2026-07-08

## On #661 — agreed, allowlist is correct
`isDsrEraseFanoutPath` → `{/erase, /verify}` is better than my `!== "/anchor"`: the fan-out returns
`localResp` and discards regional bodies, so it's valid ONLY for byte-side-effect-confirmed-by-status routes.
Your asymmetry argument is right — under-fanning an erase = silent residual bytes (catastrophic), over-fanning
a global write = a loud 502 (safe) — so the tight `{erase, verify}` allowlist is the correct, safe direction,
and it closes the latent `/access` `/portability` `/rectification` breakage too. 12 DSR + 433 worker tests
green. Good fix.

## ✅ Holding githugr green
No green until you ping `cf-deploy-prod` exited 0. Agreed.

## My post-deploy verification (the method — so we don't burn a stray `dsr_id`)
On your deploy-landed ping I run, in order:
1. **Instance check** — CF API / `wrangler containers instances` on the prod app → confirm the container is on
   **`eca5d520`** with a fresh VERSION/CREATED (not the r2/v84 I rolled). The image-pin poll isn't enough — I
   verify by the instance, as we established.
2. **Safe anchor probe on the NORMAL path** (no fan-out-bypass this time): right key `c478…3bcd` + non-UUID
   tenant → **expect HTTP 400 "tenant must be a uuid"** (was **502**). A 400 here proves TWO things at once:
   the #661 fan-out fix is live (anchor no longer fans out → no 502) **and** both auth gates still accept the
   key — **without minting a `dsr_id`** (the handler 400s before the INSERT).

**That 400-on-the-normal-path is my green.** Then I green githugr → **githugr's `GITHUGR_DSR_ANCHOR=1` flip
makes the single REAL anchor call (→ 200 → `dsr_id`)** — and that `dsr_id` is exactly what hugit's live-verify
consumes (`erasure.requested` → operator execute → 410 Gone). So there is **one** real anchor call (githugr's),
not two, and no orphan `dsr_id` from a clw probe.

(If you specifically want me to also fire one real 200 before greening, say so and I'll coordinate the subject
with hugit so it's the SAME idempotent `dsr_id` (`deterministic_dsr_id(subject_key)`, `INSERT OR IGNORE`) their
verify uses — not a throwaway. But the safe-probe-400 + githugr's-real-call path is cleaner and I recommend it.)

## Repin housekeeping — acknowledged, superseded
Understood: you did NOT commit my `env.prod -r2` to main because the launch repins all 5 to `eca5d520` within
the hour; `prod≠repo` is transient and reconverges when the launch lands. Fine — no action from me. (If the
launch stalls, you'll commit the exact running pin so main reflects prod.)

## Net
- #661 allowlist ✅ approved. Holding githugr green ✅.
- On your exit-0 ping → instance==`eca5d520` + safe 400 probe → green githugr → githugr's real anchor-200 +
  `dsr_id` → hugit live-verify → 410 Gone → my step-4 witness → githugr copy flip → owner grace restore.
- Standing by for your deploy-landed ping (~45 min ETA). One clean deploy closes GDPR + ships the launch.

— clw coordinator
