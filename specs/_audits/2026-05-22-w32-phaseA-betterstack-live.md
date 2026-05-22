# Wave 32 — Phase A — BetterStack status page live (2026-05-22)

> **Doc kind:** wave-phase audit (evidence; _audits/ excluded from canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Parent:** `specs/_audits/2026-05-22-wave32-prod-deploy-spec.md` §4 Phase A.

## 1. Scope (delivered)

- PATCH BetterStack page `247652` ("Human Guardrail / CoreLink") with: timezone=UTC, custom_domain=`status.corelink.humangr.com`, company_url=`https://corelink.humangr.com`, contact_url=`https://corelink.humangr.com/contact`, history=90 days, theme=light, layout=vertical, design=v2.
- DNS: `status.corelink.humangr.com → hugrl.betteruptime.com` (CNAME, proxy OFF/DNS-only, TTL auto). Cloudflare record id `42493307d92c0a0f463d2984c4d5941b`.
- DNS propagation confirmed via `dig +short status.corelink.humangr.com CNAME` → `hugrl.betteruptime.com.`
- BetterStack confirms `custom_domain` registered in API response.

## 2. Scope (deferred to Phase H)

**Component dashboards (5 components) + section groups (4 groups):** BetterStack's `resource` (component) API requires `resource_type` to point at an existing entity (Monitor / Heartbeat / Incident). No URLs exist yet for monitors to check — CoreLink Worker + Container are deployed in Phases B-E. Creating placeholder monitors against `humangr.com` would produce **fake-green health signals** which is worse than no health signal for a B2B trust corpus.

**Phase H absorption plan:**
1. After Phase E (Container deploy) lands, create 5 BetterStack Monitors pointing at the deployed CoreLink endpoints (`/health` per surface).
2. Create 4 BetterStack Sections (component groups): "Customer-Facing API", "Compliance & Audit", "Identity & Auth", "Infrastructure".
3. Create 5 Resources binding the Monitors to the Sections.
4. Optionally create 6 Heartbeat resources for cron / scheduled-job health (DSR pipeline, audit chain anchor, GC sweep, etc.) per `marketing/launch/STATUS-PAGE-SPEC.md`.

**Subscribable (email subscriptions):** requires BetterStack paid tier upgrade. Free tier does NOT support email-subscribe on status pages despite the 2026-05-16 research suggesting otherwise. Documented as gap; Owner decides post-deploy whether to upgrade (~US$25-30/mo for Uptime+Status bundle).

**Incident templates:** can be created without dependencies but provide no value until monitors are firing (templates are draft-from-shorthand for live incidents). Created at Phase H alongside resources.

## 3. Gates (Phase A SEAL — all green)

- `curl -sI https://hugrl.betteruptime.com` → HTTP 301 (BetterStack auto-redirect to custom domain) ✅
- `dig +short status.corelink.humangr.com CNAME` → `hugrl.betteruptime.com.` ✅
- `curl https://uptime.betterstack.com/api/v2/status-pages/247652` returns `custom_domain: status.corelink.humangr.com` ✅
- `validate_specs.py` 449+9 OK ✅
- `validate_references.py` 0 dangling ✅

## 4. Outstanding (TLS issuance window)

- BetterStack-side TLS cert provisioning for `status.corelink.humangr.com` is in flight as of audit timestamp. Typical window: 5-15 min after DNS propagation.
- Verification command (re-run T+15min): `curl -sI https://status.corelink.humangr.com` → expect HTTP 301 or 200.
- If TLS fails after 30 min, troubleshoot per `docs/internal/statuspage-tenant-signup-quickstart.md` Troubleshooting table row "ERR_SSL_PROTOCOL_ERROR" — BetterStack auto-retries ACME issuance.

## 5. Rollback

```bash
# Remove the CoreLink DNS CNAME
source .env.local
curl -X DELETE "https://api.cloudflare.com/client/v4/zones/${CLOUDFLARE_ZONE_ID_HUMANGR}/dns_records/42493307d92c0a0f463d2984c4d5941b" \
  -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}"

# Clear custom_domain on BetterStack
curl -X PATCH "https://uptime.betterstack.com/api/v2/status-pages/${BETTERSTACK_PAGE_ID}" \
  -H "Authorization: Bearer ${BETTERSTACK_API_TOKEN}" -H "Content-Type: application/json" \
  -d '{"custom_domain": null}'
```

Status page itself stays accessible at `https://hugrl.betteruptime.com`.

## 6. Decision Gate A → B

Phase A SEALed. Owner approval required to proceed with Phase B (Worker shim + Durable Object — 16-24h focused effort + cost commitment to CF Containers beta).

## 7. Cross-references

- Parent: `specs/_audits/2026-05-22-wave32-prod-deploy-spec.md`
- Bucket-2 credentials: `.env.local` (gitignored)
- Statuspage runbook: `specs/_runbooks/STATUSPAGE-INIT.md`
- Statuspage spec: `marketing/launch/STATUS-PAGE-SPEC.md`
- DEBT-016 (Statuspage deploy): currently OPEN → uplift to `partial-live (settings + DNS; components deferred to Phase H)` after this audit lands

## 8. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of Wave 32 — Phase A audit.**
