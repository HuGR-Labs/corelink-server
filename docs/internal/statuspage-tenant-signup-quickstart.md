# Statuspage Tenant Signup — Owner Quickstart

> **Audience:** the Owner provisioning the real Atlassian Statuspage
> tenant during GA cutover (Option A — CNAME path).
>
> **Time budget:** ~5 minutes of clicks + ~30 seconds of DNS edit.
> The bootstrap script does everything else in ~2 minutes.
>
> **Companion docs:**
> - `specs/_runbooks/STATUSPAGE-INIT.md` — full operator runbook
>   (Option A vs Option B, GA-cutover gate, sign-off rules).
> - `config/statuspage/components.yml` — declarative component config.
> - `config/statuspage/incident-templates.yml` — pre-drafted SEV templates.
> - `config/statuspage/dns-cname-record.txt` — exact DNS record to add.
> - `scripts/admin/statuspage-bootstrap.sh` — idempotent post-signup CLI.
> - `scripts/statuspage-init-verify.py` — wave-25 dress-run verifier.

This quickstart compresses the wave-24 operator runbook §2 into the
minimum set of actions the Owner needs to take **in the browser** plus
the bootstrap command that handles the rest.

## What this quickstart automates

Of the wave-24 §2 steps, only **two require a human in a browser**:

1. Signing up at Atlassian Statuspage (tenant creation — Atlassian
   does not expose tenant-creation via API).
2. Saving a DNS CNAME in your DNS provider's console (DNS provider
   login is operator-bound).

Everything else — component groups, components, incident templates,
branding, notification defaults — is driven by
`scripts/admin/statuspage-bootstrap.sh` from a signed-off declarative
config (`config/statuspage/components.yml` +
`config/statuspage/incident-templates.yml`).

## Step 1 — Browser: create the Atlassian Statuspage tenant

Open https://statuspage.atlassian.com in a browser and click
**Sign up** (or **Log in** if you already have an Atlassian account
tied to the CoreLink org).

- Use the org-shared email `ops@humangr.com` so the tenant survives
  individual offboarding.
- Choose the **Business** tier minimum (required for SSO + metrics
  overlay + per-region component status — see runbook §2.1).

## Step 2 — Choose plan + tenant URL

In the new-page setup wizard:

- **Page name:** `CoreLink Status`
- **Page URL (Atlassian-side):** `corelink.statuspage.io`
  (this is the `<statuspage-tenant>` that the DNS CNAME will
  target; pick a name you can live with — it is rarely seen by
  customers once the CNAME is in place).
- **Support URL:** `https://humangr.com/corelink/` (marketing home is the
  separate `hugr-site` Pages project on the apex; `corelink.humangr.com` is
  not a live host)
- **Time zone:** UTC (matches the trust-corpus cadence math).
- **Visibility:** hidden from search until GA (the bootstrap script
  enforces this, but starting hidden avoids a public preview window).

Submit. The tenant is now live at
`https://corelink.statuspage.io` (or whatever subdomain you chose).

## Step 3 — Copy the API key + page ID

In the new tenant's admin UI:

1. **API key.** Top-right user menu → "User profile" → "API keys"
   → "Create key" (label: `wave-28-bootstrap`). Copy the key.
2. **Page ID.** Open the admin URL; the path is
   `https://manage.statuspage.io/pages/<PAGE_ID>/...`. Copy the
   `<PAGE_ID>` segment.

Export both to your shell:

```bash
export STATUSPAGE_API_KEY="osp_..."
export STATUSPAGE_PAGE_ID="abcd1234"
```

> If you want to validate the configs first without making any API
> calls, you can skip this step and run the bootstrap with
> `--dry-run`. The script prints the full plan and exits 0 without
> needing the key.

## Step 4 — Run the bootstrap script

From the repo root:

```bash
# Optional pre-flight: validate configs + see the plan.
bash scripts/admin/statuspage-bootstrap.sh --dry-run

# Real run.
bash scripts/admin/statuspage-bootstrap.sh
```

The script will (idempotently — safe to re-run on any failure):

- Create the 4 component groups (Customer-Facing API · Compliance &
  Audit · Identity & Auth · Infrastructure).
- Create the 5 customer-facing components (CAS API · Audit Export ·
  DSR Pipeline · Auth (Clerk JWT) · BYOK Provider Matrix).
- Pre-publish the 6 incident templates (SEV-0 audit-chain · SEV-1
  failover · SEV-1 DSR · SEV-2 audit-latency · SEV-2 shadow-lag ·
  maintenance BYOK rotation) as idle drafts.
- Apply org-level branding (logo, favicon, primary + secondary
  colours) per `config/statuspage/components.yml`.
- Set email-notification defaults (`from = status@humangr.com`,
  ops alert = `ops@humangr.com`).

Expected runtime: ~2 minutes (mostly network round-trips).

## Step 5 — Browser: configure custom domain on Statuspage

In the tenant admin UI:

1. **Settings → Customise → Custom domain.**
2. Enter `status.corelink.humangr.com`.
3. Statuspage will display the Atlassian-side CNAME target — this is
   your `<statuspage-tenant>.statuspage.io` value. Copy it (or just
   use the value you already chose in step 2).

Statuspage will then say *"Add this CNAME in your DNS provider, then
return to verify"*. Leave this tab open.

## Step 6 — DNS provider: add the CNAME

In **Cloudflare DNS** (zone `humangr.com` — CoreLink hostnames are flat
records within this zone, e.g. `corelink-api.humangr.com`; there is no
separate `corelink.humangr.com` zone), add the record exactly as
specified in `config/statuspage/dns-cname-record.txt`:

| Field | Value |
|-------|-------|
| Type | CNAME |
| Name | `status` |
| Target | `<statuspage-tenant>.statuspage.io` |
| Proxy | **OFF** (DNS-only, grey cloud) |
| TTL | Auto (Cloudflare) or 300 (other providers) |

> **Critical:** Proxy MUST be OFF. Atlassian's automated TLS-cert
> issuance fails through Cloudflare's reverse proxy. Confirm the
> cloud icon is grey before saving.

Save the record. Return to the Statuspage tab from step 5 and click
**Verify**. Atlassian will:

1. Detect the CNAME (typically ≤ 5 min).
2. Issue a TLS cert (typically ≤ 15 min).

## Step 7 — Verify

Run the wave-25 verify script:

```bash
python3 scripts/statuspage-init-verify.py
```

Expected: HTTP 200 on `https://status.corelink.humangr.com`, summary.json
reports the 5 customer-facing components, RSS + Atom feeds resolve.

Cross-check the 8 internal-subsystem mappings in
`config/statuspage/components.yml` against the bootstrap report
emitted at the end of step 4 — every subsystem must map to exactly
one customer-facing component.

## Sign-off

Once verify is green, file the sign-off line in
`specs/_compliance/GA-GATE-CRITERIA.md` under the row
"Statuspage provisioned (Option A — CNAME)" per runbook §2.5.

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| Bootstrap exits with code 2 | `STATUSPAGE_API_KEY` not in env | Re-run step 3 in the same shell that runs step 4. |
| Bootstrap exits with code 3 | API rate-limit or transient 5xx | Re-run; the script is idempotent and resumes from the last successful item. |
| Bootstrap exits with code 4 | YAML parse error in components.yml or incident-templates.yml | `python3 -c 'import yaml; yaml.safe_load(open("config/statuspage/components.yml"))'` to surface the offending line. |
| `status.corelink.humangr.com` returns ERR_SSL_PROTOCOL_ERROR after step 6 | TLS cert still issuing | Wait 15 min; Atlassian's ACME flow needs a clean retry window. |
| Statuspage tenant shows duplicate components after a re-run | Reconcile-by-name match failed (e.g. you renamed a component in YAML) | Delete the duplicate by hand in admin UI; the script will not invent duplicates on subsequent runs because it patches by name. |
| Proxy was accidentally left ON in Cloudflare | Atlassian custom-domain verification hangs forever | Flip the cloud icon to grey, then re-click Verify in the Statuspage tab. No bootstrap re-run needed. |
| You picked Option B (different domain) by mistake | This quickstart only covers Option A | See `specs/_runbooks/STATUSPAGE-INIT.md` §3 for the docs-rebuild flow. |

## Cross-references

- Runbook: `specs/_runbooks/STATUSPAGE-INIT.md`
- Components config: `config/statuspage/components.yml`
- Incident templates: `config/statuspage/incident-templates.yml`
- DNS record: `config/statuspage/dns-cname-record.txt`
- Bootstrap script: `scripts/admin/statuspage-bootstrap.sh`
- Verify script (wave-25): `scripts/statuspage-init-verify.py`
- GA gate: `specs/_compliance/GA-GATE-CRITERIA.md`
- Cutover dependency map: `specs/_audits/sealed/2026-05-16-cutover-dependency-map.md`
