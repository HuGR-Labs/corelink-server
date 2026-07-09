# Phase 0.H — `get.corelink.io` DNS + CF Custom-Domain Setup Runbook

**Date.** 2026-05-27
**Owner.** Gustavo (Cloudflare account admin)
**Agent.** `CORELINK-CLI-INSTALL-SCRIPT` (Phase 0.H)
**Status.** **PAUSED — awaiting Gustavo's CF dashboard click.**
**Estimated Gustavo effort.** 10 minutes.

---

## §1 Why this runbook exists

Phase 0.H §4 ships the `get-corelink-worker` (this repo) which serves the
CoreLink CLI install one-liner at `https://get.corelink.io`. The Worker
is built, typechecked, and tested today (24 unit tests green, wrangler
dry-run validates dev + prod env configs), but it cannot be **deployed
to production** until two CF dashboard actions happen — both of which
require **Cloudflare account admin** permissions that the agent does
NOT hold.

Per Phase 0.H §6 hard-pause trigger 2: this runbook is emitted to
Gustavo. The agent's commit on the get-corelink-worker remains valid
and shippable; nothing in this repo blocks Gustavo from completing the
runbook at his convenience.

---

## §2 What the agent has already done

Shipped in commit `feat(get-corelink-worker): install-script serving Worker on get.corelink.io`:

- `apps/get-corelink-worker/wrangler.toml` — including `[env.prod.routes]`
  with `pattern = "get.corelink.io/*"` + `zone_name = "corelink.io"`.
- `apps/get-corelink-worker/src/index.ts` — Worker entry serving
  `text/x-shellscript` install template on `GET /`.
- `apps/get-corelink-worker/src/install.ts` — the install script
  template (placeholders for release origin + API endpoint).
- 24 unit tests, all passing.

The Worker is deployable to a `wrangler dev --remote` preview URL TODAY
(no DNS needed; preview URLs are auth-gated, not bot-scannable). But
the customer-facing `https://get.corelink.io` URL won't resolve until
Gustavo completes §3.

---

## §3 Gustavo's checklist (10 minutes)

### §3.1 Confirm domain ownership (1 minute)

The `wrangler.toml` `[env.prod.routes]` block assumes `corelink.io` is
either:

- (a) already owned by HumanGuardrail and delegated to CF nameservers, OR
- (b) ownable today (not held by a third party).

Verify with:

```bash
whois corelink.io | grep -iE 'registrant|status|nameserver' | head -10
```

- If (a) — proceed to §3.2.
- If (b) — register `corelink.io` at the same registrar used for
  `humangr.com` (Cloudflare Registrar preferred for at-cost pricing
  + automatic CF delegation). Then proceed to §3.2.
- If the domain is **already taken by a third party**, hard-pause and
  decide between (i) buying it from the holder, (ii) switching the
  install one-liner to `get.humangr.com`, or (iii) using
  `get.corelink-cdn.workers.dev` (NOT recommended — breaks the
  `workers_dev = false` security posture).

### §3.2 Add `corelink.io` as a CF zone (3 minutes)

In https://dash.cloudflare.com → Websites → Add a site:

1. Enter `corelink.io`.
2. Plan: **Free** (the Worker route binding works on Free tier; no
   need for Pro+ unless we want WAF rules, image optimization, etc).
3. CF will scan existing DNS records. If `corelink.io` was just
   registered through CF Registrar, no DNS records exist yet —
   that's fine.
4. **Update nameservers.** If `corelink.io` is on Cloudflare Registrar,
   nameservers are already CF — skip. If at another registrar,
   point the registrar's NS records at the two CF nameservers
   shown in the dashboard. Wait for propagation (5-30 min, refresh
   the dashboard to confirm "Active").

### §3.3 Add the `get.corelink.io` Worker route (2 minutes)

Once the zone is Active in CF:

1. Deploy the Worker to prod env first so the route can bind to it:
   ```bash
   cd apps/get-corelink-worker
   pnpm install
   pnpm deploy:prod
   ```
   (This requires `wrangler login` having been run once for the
   HumanGuardrail CF account.)
2. The `wrangler.toml` `[env.prod.routes]` block configures the
   `get.corelink.io/*` route automatically on deploy.
3. Verify in https://dash.cloudflare.com/?to=/:account/workers/services/view/corelink-get-cli-prod/production
   → **Triggers → Routes** that `get.corelink.io/*` is listed.

### §3.4 Add the `get` CNAME / A record (2 minutes)

In CF dashboard → `corelink.io` zone → **DNS → Records → Add record**:

- **Type.** `AAAA` or `A`. (CF Workers don't care; use `AAAA` with
  target `100::` — the "black-hole" address — which CF documents as
  the canonical Workers-route DNS placeholder.)
- **Name.** `get`.
- **Content.** `100::` (or `192.0.2.1` for A record).
- **Proxy status.** **Proxied** (orange cloud). Critical — without
  proxy, CF Edge won't intercept the request and the Worker route
  never fires.
- **TTL.** Auto.

The CF documentation calls this the "DNS-only-as-placeholder for
Workers Routes" pattern. The actual response body comes from the
Worker, not from the DNS target — but a DNS record MUST exist for
the hostname to resolve, even with proxy on.

### §3.5 End-to-end smoke (2 minutes)

```bash
# Should return the install script body:
curl -fsSL https://get.corelink.io | head -5

# Expected output (first 5 lines):
#   #!/bin/sh
#   # CoreLink CLI installer — served from https://get.corelink.io.
#   # Source: github.com/HumanGuardrail/corelink-server :: apps/get-corelink-worker.
#   # Re-run is safe: writes to /usr/local/bin/corelink and ~/.corelink/config.toml.
#   set -eu

# Should return 200:
curl -I https://get.corelink.io | head -1

# Healthz endpoint:
curl -fsSL https://get.corelink.io/healthz
# → ok

# Bot scans should NOT find the Worker on workers.dev (security posture check):
curl -I https://corelink-get-cli-prod.HumanGuardrail.workers.dev/ 2>&1 | head -1
# → expected: connection error / 404 (workers_dev = false)
```

If all four checks pass, the runbook is closed and Phase 0.H
acceptance item 4 (the curl-pipe end-to-end smoke) becomes
satisfiable — pending §3.6 below.

### §3.6 Coordinate with the `corelink-cli` repo (depends on sibling runbook)

The end-to-end `curl -fsSL https://get.corelink.io | sh -s -- --token=$T`
flow downloads
`https://github.com/HumanGuardrail/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}`
in step 4 of the install script. That URL only resolves once the
`HumanGuardrail/corelink-cli` repo exists AND has cut a release with
binaries for all 5 target triples.

See sibling runbook
`specs/_audits/2026-05-27-phase-0-corelink-cli-repo-bootstrap.md`
for the corelink-cli repo creation steps. Both runbooks must be closed
before Phase 0.H acceptance item 4 actually succeeds end-to-end with
a real customer install.

---

## §4 Hard-pause exit criteria

This runbook is **closed** when:

1. `nslookup get.corelink.io` resolves (DNS propagated, CF proxy on).
2. `curl -fsSL https://get.corelink.io` returns the install script
   body with HTTP 200 and `Content-Type: text/x-shellscript`.
3. `curl -fsSL https://get.corelink.io/healthz` returns `ok`.
4. The bot-scan-resistance check (workers.dev URL returns
   error/404) passes.

Until all four are true, Phase 0.H ships in a "Worker built, deploy
pending" state — F's `/welcome` SSE pane cannot flip from "Waiting" to
"Connected" for any real customer (the install URL is unreachable).

---

## §5 Cross-references

- Phase 0 plan: `specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.H.
- Sibling runbook (corelink-cli repo creation):
  `specs/_audits/2026-05-27-phase-0-corelink-cli-repo-bootstrap.md`.
- Security posture rationale (`workers_dev = false`): root
  `wrangler.toml` lines 11-26 (Wave 32 Phase E pre-deploy
  hardening — $36 surprise CF bill post-mortem).
