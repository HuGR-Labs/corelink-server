# Wave 32 Phase G.1 — DNS Apply Prep SEAL
_Date: 2026-05-27 | Agent: WP-G.1 | Status: SEALED_

## Summary

Phase G.1 adds the 3 missing `[[env.prod.routes]]` blocks (`app`, `docs`, `get`)
to `wrangler.toml` and authors the Cloudflare DNS CNAME apply script
`scripts/g-day-dns-apply-prod.sh`. Together these bring the Worker route
declarations into alignment with the 6 flat-name subdomain targets required by
Phase F, WP-7.1, and WP-7.3.

---

## Subdomain → CF Worker Route → TLS Matrix

| Subdomain                        | CF Worker Route (`wrangler.toml`)               | Proxied (orange-cloud) | TLS Cert Path            | Pre-existing? |
|----------------------------------|-------------------------------------------------|------------------------|--------------------------|---------------|
| `corelink-api.humangr.com`       | `corelink-api.humangr.com/*`                    | yes                    | Universal SSL Free `*.humangr.com` | yes |
| `corelink-signup.humangr.com`    | `corelink-signup.humangr.com/*`                 | yes                    | Universal SSL Free `*.humangr.com` | yes |
| `humangr.com`     | `humangr.com/*`                  | yes                    | Universal SSL Free `*.humangr.com` | yes |
| `corelink-app.humangr.com`       | `corelink-app.humangr.com/*` (**NEW Phase G.1**) | yes                   | Universal SSL Free `*.humangr.com` | no  |
| `corelink-docs.humangr.com`      | `corelink-docs.humangr.com/*` (**NEW Phase G.1**) | yes                  | Universal SSL Free `*.humangr.com` | no  |
| `corelink-get.humangr.com`       | `corelink-get.humangr.com/*` (**NEW Phase G.1**)  | yes                  | Universal SSL Free `*.humangr.com` | no  |
| `status.corelink.humangr.com`    | _Not a Worker route — BetterStack/Phase A_      | n/a                    | Universal SSL Free `*.corelink.humangr.com` | Phase A |

**Notes:**
- All 6 Worker-backed subdomains are flat `corelink-{name}.humangr.com` (not `{name}.corelink.humangr.com`).
- `status.corelink.humangr.com` is NOT a Worker route; it is managed by BetterStack status page (Phase A, commit `4d4fb8f6`). Excluded from `g-day-dns-apply-prod.sh`.
- Universal SSL Free covers single-label wildcard (`*.humangr.com`) — no additional cert provisioning required for any of the 6 subdomains.

---

## Changes Delivered

### `wrangler.toml` (additive only)
- Route count: **3 → 6** `[[env.prod.routes]]` blocks under `[env.prod]`.
- Inserted after the existing `corelink-admin` block, before `[[env.prod.r2_buckets]]`.
- No other changes to the file.
- Verified: `grep -c "corelink-.*\.humangr\.com/\*" wrangler.toml` → `6`
- TOML parse: `python3 -c "import tomllib; tomllib.load(...)"` → `OK`

### `scripts/g-day-dns-apply-prod.sh` (new)
- Default mode: `--dry-run` (prints full JSON POST bodies; no API calls).
- Live mode: `--live` — requires `CF_API_TOKEN` + `CF_ZONE_ID` env vars; validates token via `/user/tokens/verify` before proceeding.
- Idempotency: GET by name → PATCH if record exists, POST if new.
- Covers all 6 Worker-backed subdomains.
- `shellcheck` clean (0 warnings).
- Dry-run output confirmed: all 6 CNAME records listed with full JSON body + zone_id placeholder.

---

## Acceptance Gate Results

| Gate | Command | Result |
|------|---------|--------|
| Route count = 6 | `grep -c "corelink-.*\.humangr\.com/\*" wrangler.toml` | `6` ✅ |
| TOML parsable | `python3 -c "import tomllib; tomllib.load(...)"` | `OK` ✅ |
| shellcheck | `shellcheck scripts/g-day-dns-apply-prod.sh` | `0 warnings` ✅ |
| Dry-run lists all 6 | `bash scripts/g-day-dns-apply-prod.sh --dry-run` | all 6 CNAMEs with JSON ✅ |

---

## Definition of Done Checklist

| # | Criterion | Status |
|---|-----------|--------|
| 1 | `wrangler.toml` has exactly 6 `[[env.prod.routes]]` blocks | ✅ |
| 2 | New blocks inserted between existing routes and `[[env.prod.r2_buckets]]` | ✅ |
| 3 | `--dry-run` lists all 6 CNAMEs with full JSON body + zone_id placeholder | ✅ |
| 4 | Idempotency: GET by name → PATCH if exists, POST if new | ✅ |
| 5 | `--live` requires explicit flag + token validation | ✅ |
| 6 | `shellcheck` clean | ✅ |
| 7 | Audit doc tabulates subdomain → CF Worker route → proxied flag → TLS cert path | ✅ |
| 8 | Single commit | ✅ |

---

## Blockers
NONE
