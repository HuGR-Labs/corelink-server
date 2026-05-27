---
id: AUDIT-W32-PHASEC-CF-PROVISION-20260526
type: phase-audit
doc_status: SEALED
audit_status: GREEN
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: gustavo@humangr.com
tags:
  - wave-32
  - phase-c
  - cloudflare
  - d1
  - kv
  - r2
  - infra-provision
references:
  - specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md
  - scripts/provision-cf-corelink-prod.sh
  - scripts/teardown-cf-corelink-prod.sh
  - wrangler.toml
---

# Wave 32 Phase C — CF Infra Provision SEAL Audit

## §1 Scope

Wave 32 Phase C provisions the Cloudflare production infrastructure required for CoreLink deployment, as specified in `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md §3 Phase C`. Runs in parallel with Phase B (Worker shim). All operations are API-level; no Rust crates or Worker files were touched.

Resources provisioned on Cloudflare account `6a1fc1c6...` (humangr-labs primary):

- 1 D1 database
- 5 KV namespaces
- 6 R2 buckets

Deliverables:
1. `scripts/provision-cf-corelink-prod.sh` — idempotent provisioning script
2. `scripts/teardown-cf-corelink-prod.sh` — rollback script with DESTROY confirmation gate
3. `wrangler.toml` — 4 prod-env placeholders replaced with real IDs
4. This SEAL audit doc

## §2 Provisioned Resources

| Type | Name | ID / UUID | Region / Scope |
|------|------|-----------|----------------|
| D1 | corelink-prod-d1 | `d64742ea-e102-40b2-a844-ff02e3f94562` | Global (CF managed) |
| KV | corelink-prod-jwks-kv | `924c6c0f9ee4439f96ec3a75a98eef8b` | Global |
| KV | corelink-prod-cache-kv | `56f8e99f36ad4ec2aa3b876562409ccc` | Global |
| KV | corelink-prod-rate-limit-kv | `b024aef6d57f4a49a4c1715563794392` | Global |
| KV | corelink-prod-session-kv | `d2cda13c9a974ae5ae734d5f0b71a071` | Global |
| KV | corelink-prod-pilot-signup-kv | `13005d94c4404387b21c46960ea05380` | Global |
| R2 | corelink-cas-prod | — (name-addressed) | ENAM (global CAS, no pin required) |
| R2 | corelink-ac-sam | — (name-addressed) | ENAM (see §6 note) |
| R2 | corelink-ac-iad | — (name-addressed) | ENAM (Eastern N. America) |
| R2 | corelink-ac-lhr | — (name-addressed) | WEUR (Western Europe) |
| R2 | corelink-ac-nrt | — (name-addressed) | APAC (Asia Pacific) |
| R2 | corelink-ac-syd | — (name-addressed) | OC (Oceania) |

**KV binding mapping (spec name → wrangler.toml [env.prod] binding):**

| Spec KV name | wrangler.toml binding | Placeholder replaced |
|---|---|---|
| corelink-prod-jwks-kv | CLERK_JWKS_KV | PLACEHOLDER_PROD_CLERK_JWKS_KV_ID |
| corelink-prod-cache-kv | METADATA_KV | PLACEHOLDER_PROD_METADATA_KV_ID |
| corelink-prod-rate-limit-kv | NEGATIVE_CACHE_KV | PLACEHOLDER_PROD_NEG_CACHE_KV_ID |
| corelink-prod-session-kv | (no current wrangler binding — provisioned for Phase B) | — |
| corelink-prod-pilot-signup-kv | (no current wrangler binding — provisioned for Phase B) | — |

## §3 Gate Evidence

All verification calls made with token scope `Account: Workers Scripts/KV/R2/D1 Edit` — no Pages scope required for Phase C.

**D1 list (confirms corelink-prod-d1 present):**
```
GET /accounts/6a1fc1c6.../d1/database
→ success: true
→ corelink-prod-d1  uuid=d64742ea-e102-40b2-a844-ff02e3f94562
```

**KV list (all 5 corelink namespaces present):**
```
GET /accounts/6a1fc1c6.../storage/kv/namespaces
→ success: true
→ corelink-prod-pilot-signup-kv  id=13005d94c4404387b21c46960ea05380
→ corelink-prod-cache-kv         id=56f8e99f36ad4ec2aa3b876562409ccc
→ corelink-prod-jwks-kv          id=924c6c0f9ee4439f96ec3a75a98eef8b
→ corelink-prod-rate-limit-kv    id=b024aef6d57f4a49a4c1715563794392
→ corelink-prod-session-kv       id=d2cda13c9a974ae5ae734d5f0b71a071
```

**R2 buckets (all 6 present with location pinning):**
```
GET /accounts/6a1fc1c6.../r2/buckets/<name>  (individual GET per bucket)
→ corelink-cas-prod   location=ENAM
→ corelink-ac-sam     location=ENAM  (see §6 SAM note)
→ corelink-ac-iad     location=ENAM
→ corelink-ac-lhr     location=WEUR
→ corelink-ac-nrt     location=APAC
→ corelink-ac-syd     location=OC
```

**Token verify (redacted — status only):**
```
GET https://api.cloudflare.com/client/v4/user/tokens/verify
→ status: active
→ id: d2222f6b... (token ID, not value)
```

## §4 wrangler.toml Diff Summary

Four lines changed, all in `[env.prod.*]` stanzas. Strict substitution — no line additions, no line removals.

```diff
-id = "PLACEHOLDER_PROD_METADATA_KV_ID"
+id = "56f8e99f36ad4ec2aa3b876562409ccc"

-id = "PLACEHOLDER_PROD_CLERK_JWKS_KV_ID"
+id = "924c6c0f9ee4439f96ec3a75a98eef8b"

-id = "PLACEHOLDER_PROD_NEG_CACHE_KV_ID"
+id = "b024aef6d57f4a49a4c1715563794392"

-database_id = "PLACEHOLDER_PROD_D1_CONFIG_DB_ID"
+database_id = "d64742ea-e102-40b2-a844-ff02e3f94562"
```

Remaining placeholders (dev-env + staging) are out of Phase C scope and will be addressed in Phase D (dev KV/D1) or during staging setup. The `main = ...` line (Phase B scope) is untouched.

## §5 Rollback Verification

Teardown script dry-run output (no changes executed):

```
[DRY-RUN] No real resources will be deleted.
DELETE PLAN:

  D1 Databases:
    DELETE corelink-prod-d1: corelink-prod-d1

  KV Namespaces:
    DELETE KV namespace: corelink-prod-jwks-kv
    DELETE KV namespace: corelink-prod-cache-kv
    DELETE KV namespace: corelink-prod-rate-limit-kv
    DELETE KV namespace: corelink-prod-session-kv
    DELETE KV namespace: corelink-prod-pilot-signup-kv

  R2 Buckets:
    DELETE R2 bucket: corelink-cas-prod  (WARNING: all objects will be lost)
    DELETE R2 bucket: corelink-ac-sam    (WARNING: all objects will be lost)
    DELETE R2 bucket: corelink-ac-iad    (WARNING: all objects will be lost)
    DELETE R2 bucket: corelink-ac-lhr    (WARNING: all objects will be lost)
    DELETE R2 bucket: corelink-ac-nrt    (WARNING: all objects will be lost)
    DELETE R2 bucket: corelink-ac-syd    (WARNING: all objects will be lost)

[DRY-RUN] Skipping confirmation prompt. Above is the DELETE plan.
[DRY-RUN] Re-run without --dry-run to execute (confirmation required).
```

Rollback execution requires operator to type `DESTROY` at the interactive prompt — mandatory confirmation gate per charter.

## §6 Charter Compliance

**CTRL-CRED-001 (No secrets in repo):**
- `CLOUDFLARE_API_TOKEN` value is never present in any committed file.
- Both scripts load credentials at runtime from `.env.local` via a line-by-line parser (no `source .env.local` that could export unintended vars).
- Token value is redacted in all log lines (only first 8 chars of account ID shown).
- This audit doc shows API response data only, never token values.
- STATUS: COMPLIANT.

**INV-DATA-RESIDENCY (Schrems II per-region bucket pinning):**
- Each AC R2 bucket is created with a `locationHint` parameter sent to the CF API.
- CF API confirms location in the individual bucket GET response.
- Verified locations: IAD=ENAM, LHR=WEUR, NRT=APAC, SYD=OC.
- **SAM region note:** CF R2 API as of 2026-05-26 does not expose a WLAM (Western Latin America / São Paulo) location code. The valid codes are: `wnam, enam, weur, eeur, apac, oc, auto`. `corelink-ac-sam` was provisioned with `locationHint=enam` (Eastern North America — closest CF-managed region with acceptable latency to São Paulo). The bucket name `-sam` is canonical per schema; the CF region limitation is documented here. Owner decision required at PRR whether to accept ENAM as SAM proxy or wait for CF WLAM region GA.
- The provision script enforces region validation at creation time (HALT trigger #3 fires if CF silently ignores the locationHint — verified working).
- STATUS: COMPLIANT with noted SAM/ENAM limitation (platform constraint, not a gambiarra).

**Token scope (Pages: Edit missing):**
- Current token has `Account: Workers Scripts/KV/R2/D1 Edit` + `Zone: DNS/Workers Routes Edit + Zone Read`.
- `Pages: Edit` scope is NOT present — per spec §2 pre-flight, this is known and expected. Phase F (Pages deploy) requires token bump; this is documented in the wave-32 spec as a Phase F responsibility, not Phase C.
- STATUS: COMPLIANT for Phase C scope. Phase F agent must handle token bump.

**Idempotency:**
- Second run (after all resources existed) produced zero net changes. All 12 resources showed `EXISTS` state with correct IDs/locations.
- STATUS: PROVEN.

## §7 Hard Pause Triggers

| Trigger | Status | Notes |
|---------|--------|-------|
| 1. CF API token lacks KV/R2/D1 Edit scope | CLEAR | Token verified active; all API calls succeeded |
| 2. CF account billing/quota exhausted | CLEAR | All 12 resources created without quota errors |
| 3. R2 region pinning fails | CLEAR | locationHint accepted by API; locations confirmed via individual GET |
| 4. Name collision with different ID | CLEAR | No pre-existing corelink-* resources before Phase C |
| 5. wrangler.toml diff touches unintended lines | CLEAR | Diff shows exactly 4 placeholder substitutions in [env.prod.*] only |

**SAM/ENAM platform limitation:** not a hard pause trigger (CF platform constraint, not an API error). Documented in §6. Owner awareness required.

## §8 Sign-off

Phase C SEALed at commit on branch `wt/r-prep-w32-phaseC-cf-provision`.

All acceptance criteria from spec §3 Phase C verified:
- [x] D1 `corelink-prod-d1` listable via API
- [x] All 5 KV namespaces listable via API
- [x] All 6 R2 buckets listable with location pinning via API
- [x] `wrangler.toml` diff shows no placeholder strings in [env.prod.*] remaining
- [x] Re-running provision script is a no-op (idempotency proven)
- [x] `teardown-cf-corelink-prod.sh --dry-run` prints exact DELETE plan without executing
- [x] CTRL-CRED-001 compliant (no token values in committed files)
- [x] DCO + Co-Authored-By on commit

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
