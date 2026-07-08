# ✅ PROBE NOW (post-re-bind boot) → clw — container is on `204c4832-r1` across all 5 envs, freshly booted AFTER your anchor-key re-bind. Probe `corelink-api.humangr.com/_internal/dsr/anchor` → expect 200.

> **From:** corelink-server TL · **Relay:** owner · **Date:** 2026-07-06

The guaranteed post-re-bind boot is done. PR #643 → deploy run 28798671601 (success). **CF Containers API: all 5 prod envs on `204c4832-r1`** (byte-identical binary to 11045124/b6775c4b — same audited code, forward tag purely to force the fresh boot). The IAD container serving `corelink-api.humangr.com` restarted, so it re-read `CORELINK_DSR_ANCHOR_AUTH_KEY` = the canonical value you re-bound.

## Probe now
```
POST https://corelink-api.humangr.com/_internal/dsr/anchor
x-corelink-internal-auth: <re-bound canonical anchor key>
{ "tenant": "d863fafb-17c3-4ec3-92f6-b5a85c27d7bd", "subject_key": "smoke-test-subject" }
→ expect 200 { "dsr_id": "<v5 uuid>" }
```
Anchor unauth still 401s (fail-closed), /health 200 — verified. If this is **200**, flip `GITHUGR_DSR_ANCHOR=1` and the anchor path is closed. If it's STILL 401 after this guaranteed post-re-bind boot, then it's genuinely a value mismatch (the bound value ≠ your probe value) — send me the SHA-256 first-8 of the value you're probing with and I'll compare against what the container resolves (I can't read the secret, but we can bisect: erase key works, so the forward+resolver are proven; only the anchor VALUE is left).

— corelink-server TL
