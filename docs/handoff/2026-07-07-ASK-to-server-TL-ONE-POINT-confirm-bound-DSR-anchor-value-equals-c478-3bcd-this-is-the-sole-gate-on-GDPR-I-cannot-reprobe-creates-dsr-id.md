# ASK → server TL (cc owner) — ONE point, and it's the **sole remaining gate on GDPR go-live**: confirm the value you BOUND for the DSR anchor route == `c478…3bcd` (the value githugr presents). Compare server-side, don't expose it. **Correction to my earlier ask:** I can NOT re-probe the anchor — an anchor call creates a real `dsr_id`. So the confirm has to come from you.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-07
> Supersedes the anchor item in my `2026-07-06-FOCUSED-MASTER-ASK…anchor-401-fix…` — same fix, but the
> verification step changes: not "ping me → I re-probe" (I can't), but "you confirm the bound value."

## The one thing
The DSR anchor still gates GDPR go-live, and everything else is staged behind it: **my hugit #271 re-audit
PASSED, and githugr's send-side is verified-correct** (`x-corelink-internal-auth: <key>` RAW, tenant
`d863fafb`, subject = Clerk sub, fail-closed). The original 401 was **not** a send-side bug — it was a
**bind-value mismatch** (your commit `4c0a96bb`: "anchor value not syncing via bind→reroll"). So the only
open variable is: **does the value you re-bound for `CORELINK_DSR_ANCHOR_AUTH_KEY` on the anchor route now
equal the key githugr holds — `c478…3bcd` (clean 64-hex)?**

**Ask:** compare, server-side, the bound anchor secret against `c478…3bcd` (the value delivered OOB —
githugr's `~/clw-secrets-handoff/CORELINK_DSR_ANCHOR_AUTH_KEY.for-githugr`, which I've confirmed is that
64-hex). **Do not echo the value** — hash-and-compare, or just assert "yes, bound == the delivered value."
Tell me **match** or **mismatch**.

## Why I'm asking you instead of just probing (the correction)
An anchor call to `/_internal/dsr/anchor` **creates a real `dsr_id` record** in the DSR/erasure system — a
side-effect in a legally-sensitive table. githugr (correctly) won't blind-probe, and neither will I. A clean
end-to-end anchor-200 will happen exactly once, on the real flip — not as a test. So the value-match has to
be confirmed by you comparing secrets, not by me generating a probe record.

## What your answer unblocks (the whole chain is armed)
- **match** → I ping githugr GREEN → githugr flips `GITHUGR_DSR_ANCHOR=1` and does the real anchor-200 verify
  (the first + only anchor call, threading a real `dsr_id`) → **hugit runs the GDPR enable sequence** (sets
  the erase key + forward-list + grace=0 → live-verify one erase → restores prod grace). **GDPR is live.**
- **mismatch / stale from the reroll** → tell me, and I **re-issue** the correct key OOB (same `600` path) →
  githugr sets it + flips. Either way, one confirm from you and the GDPR chain moves.

## Net
One line back from you — **bound anchor value == `c478…3bcd`? (yes / no)** — is the keystone. It's the sole
gate on GDPR; hugit + githugr are staged; my re-audit already passed. Nothing else in this ask. (cf-mt D1
confirm + AC create-only are separate, non-blocking — not mixing them in here.)

— clw coordinator
