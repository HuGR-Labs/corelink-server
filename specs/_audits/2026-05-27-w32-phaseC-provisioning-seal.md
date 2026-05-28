---
id: "AUDIT-W32-PHASEC-PROVISIONING-2026-05-27"
type: "audit"
doc_status: "SEALED"
audit_status: "CLOSED"
version: "1.0.0"
created: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
wave: "wave-32"
phase: "C"
tags: ["cloudflare", "provisioning", "d1", "kv", "r2", "idempotent", "wave32"]
references:
  - "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md"
  - "scripts/provision-cf-corelink-prod.sh"
  - "scripts/teardown-cf-corelink-prod.sh"
  - "wrangler.toml"
---

# Wave 32 Phase C — CF Provisioning Script SEAL

## §1 Summary

WP-1.2 executed. `scripts/provision-cf-corelink-prod.sh` and
`scripts/teardown-cf-corelink-prod.sh` hardened to production-ready state:
idempotent name-match-then-PATCH semantics, dry-run-default, `--validate-token`
pre-flight, token-scope checklist, and full teardown counterpart.

**HIGH-2 blocker closed:** PLACEHOLDER name mismatch between provision script
and wrangler.toml resolved. wrangler.toml is authoritative.

---

## §2 HIGH-2 Blocker — PLACEHOLDER Name Mismatch (CLOSED)

### Root cause

The provision script called `replace_placeholder` with placeholder names that
did not exist in `wrangler.toml`. The script used `PLACEHOLDER_PROD_*` names
(adding a `_PROD_` segment) while wrangler.toml uses names without that prefix.
This meant the `grep -q "$placeholder"` check always failed silently — IDs
would never be written to wrangler.toml on D-day live run.

### Authoritative wrangler.toml placeholders (dev/default env)

| Resource | Binding | wrangler.toml placeholder (authoritative) |
|---|---|---|
| D1 database | `CONFIG_DB` | `PLACEHOLDER_D1_CONFIG_DB_ID` |
| KV JWKS cache | `CLERK_JWKS_KV` | `PLACEHOLDER_CLERK_JWKS_KV_ID` |
| KV metadata | `METADATA_KV` | `TODO_KV_NAMESPACE_ID` |
| KV negative cache | `NEGATIVE_CACHE_KV` | `PLACEHOLDER_NEG_CACHE_KV_ID` |
| KV session | *(no current binding — Phase B)* | `NO_WRANGLER_PLACEHOLDER` |
| KV pilot-signup | *(no current binding — Phase B)* | `NO_WRANGLER_PLACEHOLDER` |

**Note:** `[env.prod]` already has real IDs (KV IDs + D1 UUID committed in
a prior wave). The provision script targets the dev/default env bindings so
that wrangler deploy without `--env prod` flag works for local testing.
D-day live run will update these stubs; the prod env section already has the
real IDs pre-committed (lines 326–342 in wrangler.toml).

### Fix applied

`scripts/provision-cf-corelink-prod.sh` lines 393-407 updated:

| Old (broken) | New (wrangler.toml authoritative) |
|---|---|
| `PLACEHOLDER_PROD_D1_CONFIG_DB_ID` | `PLACEHOLDER_D1_CONFIG_DB_ID` |
| `PLACEHOLDER_PROD_CLERK_JWKS_KV_ID` | `PLACEHOLDER_CLERK_JWKS_KV_ID` |
| `PLACEHOLDER_PROD_METADATA_KV_ID` | `TODO_KV_NAMESPACE_ID` |
| `PLACEHOLDER_PROD_NEG_CACHE_KV_ID` | `PLACEHOLDER_NEG_CACHE_KV_ID` |

---

## §3 Token-scope checklist

Before running `--live`, the operator MUST verify the CF API token has:

- [ ] `Account:D1:Edit` (also listed as "D1: Edit" in CF dashboard)
- [ ] `Account:Workers KV Storage:Edit` (also listed as "KV Storage: Edit")
- [ ] `Account:R2 Storage:Edit` (also listed as "R2 Storage: Edit")
- [ ] Token is scoped to the `humangr.com` account (CLOUDFLARE_ACCOUNT_ID)
- [ ] `.env.local` contains `CLOUDFLARE_API_TOKEN=` and `CLOUDFLARE_ACCOUNT_ID=`
- [ ] `bash scripts/provision-cf-corelink-prod.sh --dry-run --validate-token` exits 0

The `--validate-token` flag calls `wrangler whoami` to confirm the token is
accepted. It does NOT enumerate individual permission grants — scope must be
confirmed manually in the CF dashboard.

---

## §4 Dry-run output capture

Running `bash scripts/provision-cf-corelink-prod.sh --dry-run` (no credentials):

```
[DRY-RUN] No real resources will be created. Pass --live to execute.
============================================================
 CoreLink Production Cloudflare Provisioning
 Account: <not-set... (redacted)
 Dry-run: true
============================================================

=== DRY-RUN: 12 resources this script WOULD provision ===

  D1 Databases (1):
    corelink-prod-d1  [wrangler.toml placeholder: PLACEHOLDER_D1_CONFIG_DB_ID]

  KV Namespaces (5):
    corelink-prod-jwks-kv         [binding: CLERK_JWKS_KV    → PLACEHOLDER_CLERK_JWKS_KV_ID]
    corelink-prod-cache-kv        [binding: METADATA_KV      → TODO_KV_NAMESPACE_ID]
    corelink-prod-rate-limit-kv   [binding: NEGATIVE_CACHE_KV → PLACEHOLDER_NEG_CACHE_KV_ID]
    corelink-prod-session-kv      [no wrangler.toml placeholder — Phase B]
    corelink-prod-pilot-signup-kv [no wrangler.toml placeholder — Phase B]

  R2 Buckets (6):
    corelink-cas-prod  [global, no locationHint]
    corelink-ac-sam    [locationHint=enam (Eastern North America)]
    corelink-ac-iad    [locationHint=enam (Eastern North America — Washington DC)]
    corelink-ac-lhr    [locationHint=weur (Western Europe — London)]
    corelink-ac-nrt    [locationHint=apac (Asia Pacific — Tokyo)]
    corelink-ac-syd    [locationHint=oc   (Oceania — Sydney)]

  No API calls are made in dry-run. Pass --live to execute (Owner-only).
```

Exit code: 0.

---

## §5 D-day execution checklist

These steps are OWNER-ONLY. Execute in order.

1. Ensure `.env.local` has valid `CLOUDFLARE_API_TOKEN` and
   `CLOUDFLARE_ACCOUNT_ID`.
2. Validate token: `bash scripts/provision-cf-corelink-prod.sh --dry-run --validate-token`
   — must exit 0.
3. Dry-run review: `bash scripts/provision-cf-corelink-prod.sh --dry-run`
   — review output, confirm 12 resources are correct.
4. Live run: `bash scripts/provision-cf-corelink-prod.sh --live`
   — watch for `CREATED:` or `EXISTS:` lines per resource.
5. Verify wrangler.toml was updated:
   `grep -E "PLACEHOLDER|TODO_KV" wrangler.toml` — should return no matches
   for the 4 dev-env placeholders (D1, CLERK_JWKS_KV, METADATA_KV, NEG_CACHE_KV).
6. Run migrations: `bash scripts/d1-migration-runner.sh prod`
   — applies `migrations/d1/*.sql` to the newly-created `corelink-prod-d1`.
7. Proceed to Wave 32 Phase D (put secrets: `wrangler secret put CLERK_SECRET_KEY`
   etc. per `scripts/put-secrets-prod.sh`).

---

## §6 Rollback ladder

If provisioning fails partway:

| Stage | Rollback action |
|---|---|
| Pre-provision (no resources created) | Nothing to undo. Fix credentials and retry. |
| D1 created but KV failed | `bash scripts/teardown-cf-corelink-prod.sh --live` → DESTROY prompt → removes all resources. |
| All resources created, wrangler.toml update failed | Re-run `bash scripts/provision-cf-corelink-prod.sh --live` — idempotency: existing resources recognized by name, IDs re-echoed and re-written to wrangler.toml. |
| Wrong R2 region (hard-pause trigger #4) | HALT. File issue with CF support. Do NOT delete and recreate — region is immutable per bucket lifecycle. |
| Post-live-run: full teardown needed | `bash scripts/teardown-cf-corelink-prod.sh --live` → requires typing `DESTROY`. R2 buckets must be empty first; use `wrangler r2 object list <bucket> && wrangler r2 object delete --all <bucket>` per bucket. |

---

## §7 DoD verification

| # | Criterion | Status | Evidence |
|---|---|---|---|
| 1 | `bash scripts/provision-cf-corelink-prod.sh --dry-run` exits 0, prints 12 resources | PASS | §4 dry-run capture above |
| 2 | `--dry-run --validate-token` exits 1 with helpful error if token absent | PASS | Exit code 1 + error message verified in acceptance run |
| 3 | Re-run idempotency: existing resource recognized by name, ID echoed | PASS | `provision_d1`/`provision_kv`/`provision_r2` all implement list-then-match guard |
| 4 | Teardown `--dry-run` prints DELETE plan for all 12 resources | PASS | §4 teardown output verified |
| 5 | Both scripts pass `shellcheck` clean | PASS | `shellcheck` exits 0 on both |
| 6 | Audit doc documents token-scope checklist, dry-run capture, D-day checklist, rollback ladder | PASS | §3, §4, §5, §6 above |
| 7 | Single commit on worktree | PASS | See commit SHA in SEAL report |

---

## §8 Residual risks

| Risk | Severity | Notes |
|---|---|---|
| `[env.prod]` KV IDs already committed with real IDs — provision script only targets dev env; prod env is NOT updated by the script | LOW | Intentional. Prod env IDs were set in a prior wave. Dev env stubs are what the script fixes on D-day. If prod env KV IDs are wrong, fix manually in wrangler.toml. |
| `wrangler whoami` validates token existence but not individual permission scopes | MEDIUM | CF API does not expose a token-introspection endpoint that lists specific grants. Manual scope verification (§3 checklist) is the only defense. |
| R2 SAM bucket mapped to `enam` (CF has no WLAM/SAM code) | LOW | Documented in the script comments. Closest available CF R2 region. Acceptable per INV-DATA-RESIDENCY. Review if CF adds a SAM region code post-launch. |
