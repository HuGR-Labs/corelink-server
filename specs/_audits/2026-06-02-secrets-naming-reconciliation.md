---
id: "AUDIT-2026-06-02-SECRETS-NAMING-RECONCILIATION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-02"
updated: "2026-06-02"
owner: "Gustavo Schneiter"
tags: ["audit", "secrets", "prod", "incident", "CTRL-CRED-001", "launch"]
references:
  - "docs/internal/secrets-checklist.md"
  - "scripts/secrets-mvp-allowlist.txt"
  - "scripts/put-secrets-prod.sh"
---

# Prod Secrets — PAT_SIGNING_KEY Naming Reconciliation

## Finding (root cause)

Prod ran the PAT signing key under **two different secret names with two
different values**:

- `PAT_SIGNING_KEY` — the name the **runtime reads** (`crates/corelink-container/src/main.rs`,
  `.../routes/internal_pat.rs`, `worker/src/index.ts`).
- `HUGR_PAT_SIGNING_KEY` — the name the **deploy tooling manages**
  (`scripts/secrets-mvp-allowlist.txt`, sourced from `.env.local`).

So `put-secrets-prod.sh` kept syncing `HUGR_PAT_SIGNING_KEY`, while the
runtime's `PAT_SIGNING_KEY` drifted to a separate, unsaved value. Net effect:
PATs minted with the `.env.local` key failed the edge HMAC check (401) — the
root cause of the `sccache → CoreLink` dogfood 401 and any PAT-verify drift.

## Remediation applied (2026-06-02)

| # | Action | Scope | Verified |
|---|---|---|---|
| 1 | Set `PAT_SIGNING_KEY` = `.env.local`'s value | all 5 prod workers (corelink-prod + -lhr/-nrt/-sam/-syd) | cache auth 401 → **passed** (404 = separate cache-path detail) |
| 2 | Delete orphan `HUGR_PAT_SIGNING_KEY` | all 5 prod workers | HTTP 200 each |
| 3 | Fix deploy tooling: allowlist `HUGR_PAT_SIGNING_KEY` → `PAT_SIGNING_KEY` | `scripts/secrets-mvp-allowlist.txt` | `--mvp-only` dry-run now plans `PAT_SIGNING_KEY`; `HUGR_PAT_SIGNING_KEY` gone |
| 4 | Add canonical `PAT_SIGNING_KEY` to source of truth | `.env.local` (gitignored) | present |
| 5 | Delete orphans `HUGR_SESSION_HMAC_KEY` + `HUGR_AUDIT_CHAIN_HMAC_KEY` (no code reader; confirmed unused) | all 5 prod workers | **verified live 2026-06-02**: absent on corelink-prod + -lhr/-nrt/-sam/-syd (CF API secrets list) |
| 6 | Remove the 2 orphans from deploy tooling + source of truth | `scripts/secrets-mvp-allowlist.txt`, `.env.local` | lines removed |
| 7 | Add 5 previously-undocumented secret rows to the checklist matrix (#141 `PAT_SIGNING_KEY`, #142 `CORELINK_INTERNAL_AUTH_KEY`, #143 `R2_TDK_HEX`, #144 `CORELINK_CLI_RELEASE_TOKEN`, #145 `SENTRY_AUTH_TOKEN`) | `docs/internal/secrets-checklist.md` | both matrix validators green (see below) |
| 8 | Fix both secrets-matrix validators: exclude `.open-next`/`.wrangler` build output from the code scan | `scripts/secrets-checklist-verify.sh`, `scripts/validate_secrets_matrix.py` | ~130–148 false-positive vendor CI-detection vars (Sentry/OpenNext bundle) no longer scanned |
| 9 | Classify ~23 non-secret env vars (test config, R2 bucket names/regions, `NEXT_PUBLIC_`/Sentry identifiers, build metadata) into both validators' allowlists | both validators | `secrets-checklist-verify.sh` → OK (no drift); `validate_secrets_matrix.py` → code_only=0 |

## Code-trace (which deployed secrets the runtime actually reads)

| Deployed secret | Read by code? | Verdict |
|---|---|---|
| `PAT_SIGNING_KEY` | ✅ container + worker | canonical — fixed |
| `HUGR_OCI_TOKEN_KEY` | ✅ `corelink-adapter-host/src/oci/config.rs` | consistent (code reads the HUGR_ name) — leave |
| `CORELINK_INTERNAL_AUTH_KEY` | ✅ container + worker | consistent — leave |
| `HUGR_PAT_SIGNING_KEY` | ❌ orphan | **deleted** |
| `HUGR_SESSION_HMAC_KEY` | ❌ no reader found (confirmed unused) | **deleted** (verified absent live on all 5 workers 2026-06-02) |
| `HUGR_AUDIT_CHAIN_HMAC_KEY` | ❌ no reader found (audit chain derives its key elsewhere; feature still verified working) | **deleted** (verified absent live on all 5 workers 2026-06-02) |
| `SESSION_HMAC_KEY` / `AUDIT_CHAIN_HMAC_KEY` / `OCI_TOKEN_KEY` (non-prefixed) | ❌ not deployed / not read | n/a |

## Verify-gate bug found + fixed (2026-06-02)

Both secrets-matrix validators — `scripts/secrets-checklist-verify.sh` and
`scripts/validate_secrets_matrix.py` — scanned **gitignored build output**
(`apps/admin-ui/.open-next/`, `*/.wrangler/`). Those bundles embed the
Sentry/OpenNext SDK, whose CI-provider release detection references ~130–148
vendor env vars (`CIRCLE_SHA1`, `BITBUCKET_COMMIT`, `VERCEL_*`, `ZEIT_*`,
`ALIYUN_*`, …). None are CoreLink secrets. The flood masked the *real*
matrix↔code drift — both gates had been red, one of them additionally hidden
behind a `tail`-pipe exit-code mismeasure.

Fix: added `.open-next` + `.wrangler` to the dir-exclusion set in both
validators (alongside the existing `.next`/`dist`/`build`). After the fix the
real drift surfaced and was resolved — 5 genuinely-missing secret rows added to
the matrix (#141–#145); ~23 non-secret vars classified into the allowlists.
Both gates now green (`OK (no drift)` / `code_only=0`); `validate_specs.py`
stays 463/0.

## Dual-name drift: Cloudflare API token (deferred consolidation)

The Cloudflare API token is stored under **two names**: `CF_API_TOKEN`
(5 workflows + the worker runtime read) and `CLOUDFLARE_API_TOKEN` (the
wrangler-default env name — a 2nd GHA secret used by `e2e-clerk-signup.yml` +
`e2e-stripe-checkout.yml`, and also deployed as a worker secret on corelink-prod
*alongside* `CF_API_TOKEN`; verified live 2026-06-02). Same class as the PAT
bug. Documented as one secret under both env-var names in checklist row #50.
**Follow-up:** consolidate to a single name (`CF_API_TOKEN`) and drop
`CLOUDFLARE_API_TOKEN` once the 2 e2e workflows are repointed — a deliberate,
low-risk cleanup (both names hold the same token today, so nothing is broken).

## Open follow-ups (deliberate — not auto-actioned)

1. ✅ **DONE** — orphans `HUGR_SESSION_HMAC_KEY` / `HUGR_AUDIT_CHAIN_HMAC_KEY`
   confirmed unused + deleted from all 5 workers (verified live 2026-06-02).
2. ✅ **DONE** — `secrets-checklist.md` now carries `PAT_SIGNING_KEY` (#141) plus
   4 more previously-undocumented secrets (#142–#145); both gates cover them.
3. **CF-token dual-name consolidation** — see the section above.
4. **43 deferred integration secrets** (Twilio/Azure/GCP/Vault/Neon/Drata/etc.)
   — add per-feature when each ships (`--mvp-only` WARN-skips them; the matrix
   gate soft-warns them as forward-looking `matrix_only` rows).

## Note

Real secret VALUES live only in Cloudflare (write-only) + `.env.local` (the
single recoverable copy — **must be backed up**). `CLOUDFLARE_API_TOKEN` has
D1 + Workers read/write; the other CF tokens do not.
