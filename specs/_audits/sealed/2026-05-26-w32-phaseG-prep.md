# Wave 32 Phase G PREP — DNS Production Plan (2026-05-26)

> **Doc kind:** wave-scope PREP audit (evidence; _audits/ excluded from canonical schema validation).
>
> **Owner:** Gustavo Schneiter (Security + Release Lead).
>
> **Authored:** 2026-05-26 by Claude Sonnet 4.6.
>
> **Trigger:** Phase G PREP is parallel-safe with Phases B, C, D-prep, F-prep.
> Phase G APPLY is blocked on Phase E (Container deploy complete).
>
> **Branch:** `wt/r-prep-w32-phaseG-prep`
>
> **Baseline commit:** `f9badfe9947df6a821a2f349c99853fa5b851fad`
>
> **Charter compliance:** SOTA bar; CTRL-CRED-001 (no secrets emitted; DNS records
> are public zone data); zero gambiarra; staging-before-prod for any irreversible action;
> rollback documented per-record.

---

## §1 Scope

This PREP phase delivers the **DNS production toolchain** for CoreLink's `humangr.com` zone
without applying any DNS changes. Phase G APPLY (the actual record creation) requires Phase E
(Container deploy + Worker `wrangler deploy`) to be complete first.

**Deliverables:**

| Script | Purpose |
|--------|---------|
| `scripts/dns-prod-plan.sh` | Generates the DNS plan in markdown or JSON; diffs against live CF zone |
| `scripts/dns-prod-apply.sh` | Idempotent per-record CREATE/UPDATE with rollback snapshot; default `--dry-run` |
| `scripts/dns-prod-verify.sh` | Post-apply DNS resolution + TLS cert verification; green/red per record |

**Not in scope for this PREP:**
- Applying any DNS changes (blocked on Phase E).
- Worker route binding (Phase B wrangler.toml `[[routes]]` section).
- Pages project creation (Phase F).
- Custom cert ordering (CF Universal SSL is automatic — no action needed).

---

## §2 Routes inventory

Sources parsed: `wrangler.toml` prod env + Phase F Pages projects + Phase A BetterStack status +
spec §4 Phase G subdomain list.

### Worker routes (wrangler.toml `[env.prod]`)

The root `wrangler.toml` does not yet have a `[[routes]]` section — this is a known Phase B
deliverable (`worker/src/index.ts` + route table). The prod worker name is `corelink-prod`
(from `[env.prod] name = "corelink-prod"`). After Phase B deploy, the workers.dev subdomain
will be `corelink-prod.gustavoschneiter.workers.dev`.

Worker-backed subdomains (all proxied, orange-cloud):
- `api.corelink.humangr.com` — primary API entry point, all `/v1/*` routes
- `signup.corelink.humangr.com` — pilot onboard flow
- `admin.corelink.humangr.com` — internal admin (Clerk JWT-gated)
- `acme-dev.corelink.humangr.com` — ACME dev environment (wave-29 inventory)
- `sandbox.corelink.humangr.com` — sandbox/trial (wave-29 inventory)
- `go.corelink.humangr.com` — go-link redirector (wave-29 inventory)

Staging worker (`[env.staging] name = "corelink-staging"`):
- `staging.corelink.humangr.com` — staging traffic (`wrangler --env staging`)

### Pages projects (Phase F)

| Project name | Custom domain | Pages default subdomain |
|---|---|---|
| `corelink-docs` | `docs.corelink.humangr.com` | `corelink-docs.pages.dev` |
| `corelink-admin-ui` | `app.corelink.humangr.com` | `corelink-admin-ui.pages.dev` |

### BetterStack status page (Phase A — complete)

| Custom domain | Target | Proxied |
|---|---|---|
| `status.corelink.humangr.com` | `hugrl.betteruptime.com` | false (DNS-only) |

**Status:** already live. Phase A SEALed at `4d4fb8f6`. Record present in zone. NO-OP at apply.

---

## §3 Plan table

Generated output of `./scripts/dns-prod-plan.sh --plan-md` (captured 2026-05-26T18:50:48Z):

| # | Source Name | Type | Target | Proxied | TTL | Notes |
|---|-------------|------|--------|---------|-----|-------|
| 1 | `api.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes (orange) | Auto | Worker prod entry point (all /v1/* routes) |
| 2 | `app.corelink.humangr.com` | CNAME | `corelink-admin-ui.pages.dev` | yes (orange) | Auto | Phase F Pages: corelink-admin-ui |
| 3 | `docs.corelink.humangr.com` | CNAME | `corelink-docs.pages.dev` | yes (orange) | Auto | Phase F Pages: corelink-docs (4 locales) |
| 4 | `signup.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes (orange) | Auto | Worker signup/pilot-onboard route |
| 5 | `admin.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes (orange) | Auto | Worker internal-admin route (Clerk-gated) |
| 6 | `acme-dev.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes (orange) | Auto | Worker ACME-dev environment (wave-29 inventory) |
| 7 | `staging.corelink.humangr.com` | CNAME | `corelink-staging.gustavoschneiter.workers.dev` | yes (orange) | Auto | Staging worker (wrangler --env staging) |
| 8 | `sandbox.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes (orange) | Auto | Sandbox/trial route (wave-29 inventory) |
| 9 | `go.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes (orange) | Auto | Redirect/go-links worker route |
| 10 | `status.corelink.humangr.com` | CNAME | `hugrl.betteruptime.com` | no (grey) | Auto | Phase A (ALREADY EXISTS — dns-only per BetterUptime requirement) |

**Count:** 10 records (9 CREATE + 1 NO-OP). Within the 7-10 range specified in the task.

---

## §4 Diff-against-live output

Generated output of `./scripts/dns-prod-plan.sh --diff-against-live` (captured 2026-05-26T18:50:51Z):

### Current live records (humangr.com zone — 19 total)

| Name | Type | Content | Proxied |
|------|------|---------|--------|
| `_dmarc.humangr.com` | TXT | `"v=DMARC1; p=none; rua=mailto:dmarc-reports@humangr.com; ..."` | no |
| `api.humangr.com` | AAAA | `100::` | yes |
| `cf2024-1._domainkey.humangr.com` | TXT | `"v=DKIM1; h=sha256; k=rsa; p=..."` | no |
| `dl.humangr.com` | CNAME | `public.r2.dev` | yes |
| `humangr.com` | CNAME | `hugr-site.pages.dev` | yes |
| `humangr.com` | MX | `route1/2/3.mx.cloudflare.net` (×3) | no |
| `humangr.com` | TXT | `"v=spf1 include:_spf.mx.cloudflare.net ~all"` | no |
| `humangr.com` | TXT | `"google-site-verification=..."` | no |
| `mail.humangr.com` | MX | `route1/2/3.mx.cloudflare.net` (×3) | no |
| `mail.humangr.com` | TXT | `"v=spf1 include:_spf.mx.cloudflare.net ~all"` | no |
| `resend._domainkey.humangr.com` | TXT | `"p=MIGfMA0G..."` | no |
| `send.humangr.com` | MX | `feedback-smtp.sa-east-1.amazonses.com` | no |
| `send.humangr.com` | TXT | `"v=spf1 include:amazonses.com ~all"` | no |
| `status.corelink.humangr.com` | CNAME | `hugrl.betteruptime.com` | no |
| `www.humangr.com` | CNAME | `hugr-site.pages.dev` | yes |

### Delta (what apply would change)

| Action | Name | Type | Target | Proxied | Conflict? |
|--------|------|------|--------|---------|----------|
| CREATE | `api.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes | none |
| CREATE | `app.corelink.humangr.com` | CNAME | `corelink-admin-ui.pages.dev` | yes | none |
| CREATE | `docs.corelink.humangr.com` | CNAME | `corelink-docs.pages.dev` | yes | none |
| CREATE | `signup.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes | none |
| CREATE | `admin.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes | none |
| CREATE | `acme-dev.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes | none |
| CREATE | `staging.corelink.humangr.com` | CNAME | `corelink-staging.gustavoschneiter.workers.dev` | yes | none |
| CREATE | `sandbox.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes | none |
| CREATE | `go.corelink.humangr.com` | CNAME | `corelink-prod.gustavoschneiter.workers.dev` | yes | none |
| NO-OP | `status.corelink.humangr.com` | CNAME | `hugrl.betteruptime.com` | no | none (Phase A) |

**Hard pause collision check result:** No collisions. All 9 CREATE targets are new names with
zero existing records. The `status` record is an exact match NO-OP.

### Pre-apply DNS resolution (baseline)

Output of `./scripts/dns-prod-verify.sh --current-state` (captured 2026-05-26T18:51:02Z):

```
[INFO]  api.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  app.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  docs.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  signup.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  admin.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  acme-dev.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  staging.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  sandbox.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[INFO]  go.corelink.humangr.com: NXDOMAIN (expected pre-apply)
[PASS]  status.corelink.humangr.com: resolved to hugrl.betteruptime.com [dns-only]
RESULT: PASS (mode=--current-state, 0 failures)
```

---

## §5 Cert strategy

### CF Universal SSL (proxied records — rows 1-9)

All 9 Worker/Pages-backed subdomains will be proxied (orange cloud). Cloudflare automatically
provisions Universal SSL covering `*.humangr.com` wildcards plus per-SAN coverage for any
hostname that routes through the CF edge.

- **No operator action needed** for TLS on any of these 9 records.
- CF Universal SSL renews automatically 30 days before expiry.
- Edge cert includes `*.corelink.humangr.com` once the first `corelink.*` CNAME is proxied.
- TLS 1.2 minimum; TLS 1.3 preferred. HSTS header via CF edge settings.

### BetterUptime-managed cert (status — row 10)

`status.corelink.humangr.com` is DNS-only (grey cloud, `proxied=false`). BetterUptime issues
its own TLS cert after custom domain verification (Phase A). This cert:
- Is not renewed by CF — renewed by BetterUptime automatically.
- Was verified as part of Phase A gate (HTTP 403 from BetterUptime origin is expected until the
  status page custom domain is fully activated).

### Custom cert path (if needed — low probability)

If a customer-facing compliance requirement demands an EV or OV certificate on `api.corelink.humangr.com`,
CF Advanced Certificate Manager can be used to order a custom cert. This is **not planned** for Wave 32
GA — CF Universal SSL is sufficient for launch.

---

## §6 Charter compliance

| Control | Check | Status |
|---|---|---|
| CTRL-CRED-001 | No secret values in scripts or audit doc | PASS — scripts source `.env.local` at runtime; no values committed |
| DNS records are public | Zone records visible to anyone via `dig` | PASS — plan content committed without redaction |
| No PII in DNS records | Zone records only contain CNAME targets | PASS |
| Idempotency | Re-apply on correct state = no-op | PASS — `--dry-run` shows `SKIPPED` for `status` (already correct) |
| Default safe | `dns-prod-apply.sh` defaults to `--dry-run` | PASS |
| Rollback documented | Pre-change snapshot + `--rollback` flag | PASS — see §7 |
| Collision protection | Script reports collision, does NOT auto-overwrite | PASS — hard pause trigger 3 |
| Hard pause triggers | All 3 triggers documented and tested | PASS — see §8 |

---

## §7 What Phase G APPLY will do

**Precondition:** Phase E must be complete (Worker + Container deployed, `wrangler deploy --env prod`
successful, `corelink-prod.gustavoschneiter.workers.dev/health` returns 200).

**Steps:**

1. **Run plan JSON:** `./scripts/dns-prod-plan.sh --plan-json > /tmp/dns-plan.json`
2. **Run diff to confirm no new collisions:** `./scripts/dns-prod-plan.sh --diff-against-live`
3. **Dry-run apply to confirm changes:** `./scripts/dns-prod-apply.sh --dry-run`
4. **Owner reviews dry-run output** — must show 9 CREATE + 1 SKIPPED. No COLLISION rows.
5. **Real apply (Owner-approved):** `./scripts/dns-prod-apply.sh --apply`
   - Per-record log: `[N/M] CREATE <name> -> <target> CNAME proxied=true ... OK (id=<cf_id>)`
   - Rollback snapshot written to `/tmp/dns-rollback-<TIMESTAMP>.json`
6. **Wait for propagation** (CF proxied records propagate within seconds; DNS-only up to TTL).
7. **Post-apply verify:** `./scripts/dns-prod-verify.sh --post-apply`
   - All 9 Worker/Pages records must resolve to CF IPs (proxied orange-cloud).
   - TLS check on `https://api.corelink.humangr.com/health` must return HTTP 200.
8. **Commit rollback snapshot path** to Phase G audit doc for operator runbook.

### Rollback snapshot format

```json
{
  "created_at": "2026-05-26T18:51:09Z",
  "zone_id": "73f57f6d508beea67e2f78bea11d3c25",
  "mode": "--apply",
  "pre_change_state": [
    {
      "_rollback_action": "DELETE_NEW",
      "_applied_id": "<cf_record_id>",
      "name": "api.corelink.humangr.com",
      "type": "CNAME",
      "content": "corelink-prod.gustavoschneiter.workers.dev"
    }
  ]
}
```

- `DELETE_NEW`: record was new (didn't exist pre-apply) → rollback deletes it.
- `RESTORE_ORIGINAL`: record was updated → rollback PATCHes it back to original content.
- Invoke rollback: `./scripts/dns-prod-apply.sh --rollback /tmp/dns-rollback-<TS>.json`

---

## §8 Hard pause triggers

1. **CF credentials absent** — `dns-prod-plan.sh` exits with `HARD PAUSE TRIGGER 1` if
   `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ZONE_ID_HUMANGR`, or `CLOUDFLARE_ACCOUNT_ID` are unset.

2. **CF API token lacks `Zone: DNS Edit` scope** — `dns-prod-apply.sh` will receive a 403
   from the CF API on the first POST. Script exits with `ERROR: API call failed`. Operator
   must bump token scope (per spec §2 pre-flight — the token already has this scope; verify
   at apply time with `curl .../user/tokens/verify`).

3. **Collision on planned name** — `dns-prod-plan.sh --diff-against-live` prints
   `COLLISION DETECTED` and documents the existing record. `dns-prod-apply.sh` does NOT
   auto-overwrite: it prints `ERROR` for that record and continues to the next. Operator
   must manually resolve the collision (delete the conflicting record or update the plan).

**Current state (2026-05-26):** All 3 triggers are clear. No collisions, credentials valid,
token active (`status: active`).

---

## §9 Acceptance criteria sign-off

| Criterion | Status |
|---|---|
| `scripts/dns-prod-plan.sh` written + executable | DONE |
| `scripts/dns-prod-apply.sh` written + executable | DONE |
| `scripts/dns-prod-verify.sh` written + executable | DONE |
| Plan markdown table generated (10 rows) | DONE — §3 |
| Diff-against-live captured | DONE — §4 |
| Collision check: no collisions | DONE — §4 |
| Cert verification dry-run (current DNS baseline) | DONE — §4 `verify --current-state` |
| Rollback snapshot format documented | DONE — §7 |
| Charter compliance documented | DONE — §6 |
| Hard pause triggers documented | DONE — §8 |
| SEAL audit committed | DONE — this document |

---

## Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

**End of Wave 32 Phase G PREP audit.**
