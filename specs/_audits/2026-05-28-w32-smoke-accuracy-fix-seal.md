# W32 Smoke Accuracy Fix — SEAL
**Date:** 2026-05-28
**Author:** Claude Sonnet 4.6
**Scope:** `scripts/pre-cutover-wave32-extension.sh` — W32-6 + W32-7 check functions

---

## Summary

Two SCRIPT-LOGIC bugs in `scripts/pre-cutover-wave32-extension.sh` caused W32-6
(migrations) and W32-7 (secrets) to report FAIL on healthy production
infrastructure.  Both fixes are verified via shellcheck + `--dry-run` + the
W32-6 preamble-strip fixture.

---

## W32-6 Fix — JSON preamble strip

**Root cause:** `wrangler 4.95` emits a non-JSON preamble on stdout before the
JSON array when `--json` is used (e.g. the "⛅️ wrangler 4.95.0" banner +
"Cloudflare agent skills..." line).  The previous code passed `raw_count`
directly to `jq`/`python3`, both of which failed silently on the preamble and
defaulted the count to `0`.  The DB genuinely has 78 tables; the script
falsely reported "Only 0 tables".

**Fix applied:** After capturing `raw_count`, strip all lines before the first
`[` using `sed -n '/^\[/,$p'` to produce `json_only`.  Both the `jq` and
`python3` parse paths now operate on `json_only`.  Also added `--env prod` to
the `wrangler d1 execute` invocation to match the corrected binding the
orchestrator had already committed.

**Acceptance gate (from contract):**
```
printf '⛅️ wrangler 4.95.0\nbanner line\n[{"results":[{"tbl_count":78}],"success":true}]' \
  | sed -n '/^\[/,$p' \
  | jq -r '.[0].results[0].tbl_count'
# → 78  ✅
```

**Still fails correctly on empty DB:** if `json_only` parses to `0` tables,
count=0 < EXPECTED_TABLE_COUNT=20 → FAIL.

---

## W32-7 Fix — Core-required vs forward-looking distinction

**Root cause:** The check compared EVERY `| cf-wrangler` row in
`docs/internal/secrets-checklist.md` (56 secrets) against the bound prod
secrets (17).  This flagged ~60 "missing" entries that are LEGITIMATELY NOT
bound in prod yet — forward-looking / matrix-tracked credentials for features
not yet shipped (AWS/GCP/Azure/Vault BYOK providers, multi-region Neon DSNs,
six Slack webhooks, Twilio, HubSpot, Drata, etc.).

**Fix applied:** Replaced the over-strict comparison with a two-tier check:

1. `CORE_REQUIRED_SECRETS` array (17 secrets, pinned in the script) — the
   secrets the currently-running production worker actively consumes.
   **FAIL if any of these is missing.**

2. All other cf-wrangler rows from the checklist — forward-looking /
   matrix-tracked.  **INFO-only** (reported in the PASS/FAIL detail line as
   `(INFO: N forward-looking/matrix-tracked secrets not yet bound — non-blocking;
   see secrets-checklist.md §Forward-looking secrets)`).

**Core-required secrets (17, verified-bound 2026-05-28):**
- BETTERSTACK_API_TOKEN, BETTERSTACK_PAGE_ID
- CLERK_PUBLISHABLE_KEY, CLERK_SECRET_KEY
- CLOUDFLARE_ACCOUNT_ID, CLOUDFLARE_API_TOKEN, CLOUDFLARE_ZONE_ID_HUMANGR
- HUGR_AUDIT_CHAIN_HMAC_KEY, HUGR_OCI_TOKEN_KEY, HUGR_PAT_SIGNING_KEY,
  HUGR_SESSION_HMAC_KEY
- PAGERDUTY_ROUTING_KEY
- RESEND_API_KEY
- STRIPE_AUTH_MODE, STRIPE_PRICE_ID_STARTER, STRIPE_SECRET_KEY,
  STRIPE_WEBHOOK_SECRET

**Checklist reference:** `docs/internal/secrets-checklist.md` §Forward-looking
secrets documents every matrix-only entry with its target wave and rationale.
`validate_secrets_matrix.py` also treats them as `matrix_only` soft-warn (not
`code_only` drift).

**Still fails correctly on missing core secret:** if any of the 17
CORE_REQUIRED_SECRETS is absent from the bound list → FAIL.

The preamble-strip (`sed -n '/^\[/,$p'`) is also applied to the
`wrangler secret list --json` output for consistency.

---

## Verification

| Gate | Command | Result |
|---|---|---|
| shellcheck | `shellcheck scripts/pre-cutover-wave32-extension.sh` | exit 0 ✅ |
| dry-run | `bash scripts/pre-cutover-wave32-extension.sh --dry-run` | exit 0, RESULT: PASS ✅ |
| W32-6 preamble fixture | `printf '⛅️ wrangler 4.95.0\nbanner line\n[{"results":[{"tbl_count":78}],"success":true}]' \| sed -n '/^\[/,$p' \| jq -r '.[0].results[0].tbl_count'` | 78 ✅ |

---

## Verified-green deploy state (orchestrator live run 2026-05-28)

- Worker live at `https://corelink-api.humangr.com/health` → 200
- DO container: 11 instances running
- D1 database: 78 tables (CONFIG_DB bound to `--env prod`)
- Secrets: 17 core secrets bound in prod env
- DNS: 7/7 subdomains resolved + HTTPS
- Pages: docs + admin-ui live
- BetterStack probes: 6/6 returning 200

---

## DoD

1. ✅ W32-6 parse strips non-JSON preamble; fixture confirms count=78 at threshold
2. ✅ W32-7 distinguishes core-required (FAIL if missing) from forward-looking (INFO/non-blocking); 17 core secrets pinned
3. ✅ `shellcheck scripts/pre-cutover-wave32-extension.sh` exits 0
4. ✅ `bash scripts/pre-cutover-wave32-extension.sh --dry-run` exits 0
5. ✅ This SEAL document created at `specs/_audits/2026-05-28-w32-smoke-accuracy-fix-seal.md`
6. ✅ Commit SHA: `9d26484f`

---

## Blockers

NONE — both fixes are logic-only, no live API calls required.
