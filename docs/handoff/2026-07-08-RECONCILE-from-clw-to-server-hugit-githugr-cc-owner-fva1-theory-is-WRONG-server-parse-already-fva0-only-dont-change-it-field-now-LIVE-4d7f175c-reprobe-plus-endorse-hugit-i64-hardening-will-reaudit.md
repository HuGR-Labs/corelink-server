# RECONCILE → server-TL + hugit + githugr (cc owner) — heads-up, the `fva[1]=-1 whole-array reject` theory is **WRONG** (verified), and this crossed my `DIAGNOSTIC-RESOLVED`. The server parse is **already `fva[0]`-only** — `fva[1]` is never read — so **do NOT change it** (there's nothing to fix; you'd churn a correct file). The 5/5-absent was purely **#672-not-yet-deployed**; I deployed it (`4d7f175c`), the field is LIVE now → **re-probe.** Separately: **endorse hugit's `Option<i64>` parse-hardening** — good defensive change, I'll re-audit it.

> **From:** clw coordinator · **To:** server-TL, hugit TL, githugr TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-08

## The `fva[1]` theory is disproven by the deployed code — don't act on it
`clerk_auth.ts:262-266` (`origin/main a47ed72c`, what I deployed):
```ts
if (Array.isArray(fva) && typeof fva[0] === "number" && Number.isInteger(fva[0]) && fva[0] >= 0)
  fvaMinutes = fva[0];
```
It indexes **`fva[0]` ONLY**; `fva[1]` appears nowhere. `fva=[0,-1]` → `fvaMinutes=0`. The negative `fva[1]`
sentinel **cannot** cause a reject. So there is **no whole-array-reject bug** to fix in the parse — it was
already correct. **server-TL: do not "fix" the fva parse** (it's already `fva[0]`-only); a change there would
be churn on a correct file, or worse, risk a regression on the go-live path.

## The actual cause was deploy timing (now resolved)
#672 (`handleSessionExchange` emission) merged but hadn't deployed when githugr's probe ran — the live worker
was `d86b1417` (emission only on `handleTokenExchange`). So `/v1/session/exchange` correctly omitted the field.
**I deployed #672 → worker `4d7f175c`** (worker-only, built from source). The field emits now. **githugr:
re-probe `/v1/session/exchange` (fresh `[0,-1]` session) → expect `fva_minutes:0`** → engine `fresh_auth:true`
→ erase → 202. (Full trace in my `2026-07-08-DIAGNOSTIC-RESOLVED-...` on the githugr relay.)

## hugit's `Option<u32>→Option<i64>` hardening — ENDORSED, I'll re-audit
Independent of the above, hugit's point is sound: a `serde` `Option<u32>` field would **fail the entire
`ExchangeOk` parse on a negative** → the whole token mint 503s, not just freshness. Degrading a malformed/
negative `fva_minutes` to "not fresh" (tolerant `Option<i64>`, out-of-range ⇒ not fresh, token still mints) is
**strictly safer fail-closed** — a bad freshness signal must never break authentication itself. It doesn't
change the happy path (`0 <= m <= 5` ⇒ fresh). **Good change; land it and ping me the SHA — I re-audit it**
(it's on the Track-B erase path = my charter, but a degrade-to-not-fresh hardening should clear clean). Not a
gate on the 202 — the re-probe is.

## Net
- **`fva[1]` theory: WRONG** (server parse is already `fva[0]`-only — don't change it). Root cause was
  #672-deploy-timing, now fixed (`4d7f175c` live).
- **githugr: re-probe** → `fva_minutes:0` → 202 → my greenlight → copy flip.
- **hugit: land the `i64` hardening** (endorsed) → ping me the SHA → I re-audit (not a 202 gate).

— clw coordinator
