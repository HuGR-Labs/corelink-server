---
id: "RB-STATUSPAGE-INIT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "R-prep"
parent_wi: "DEBT-016"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-DSR-STATUSPAGE-PUBLISH-FAILED"
  - "ONCALL-ESCALATION-MATRIX"
tags: ["runbook", "statuspage", "operator", "ga-cutover", "provisioning", "debt-016", "wave-24"]
escalation: "SRE Lead → Incident Commander → CTO"
review_cadence: "annual + post-incident"
related:
  - "specs/_audits/2026-05-16-debt-016-statuspage-urls.md"
  - "marketing/launch/STATUS-PAGE-SPEC.md"
  - "specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md"
  - "apps/docs/docs/trust/incident-response.mdx"
  - "apps/docs/docs/trust/index.mdx"
debt_links:
  - "DEBT-016"
---

# RB-STATUSPAGE-INIT — Statuspage operator provisioning playbook

**Trigger:** GA cutover preparation — operator must provision the real
Atlassian Statuspage tenant and bind it to the canonical hostname
`status.corelink.dev` (or override via the `STATUSPAGE_URL` env-var path)
**before T-7d pre-launch**.

**Owner:** SRE Lead (operator-side). Engineering side closed per
`specs/_audits/2026-05-16-debt-016-statuspage-urls.md` (DEBT-016 engineering
closure, R-prep wave-24).

**Severity:** GA-blocker if neither provisioning path is complete by T-7d.

---

## 1. Background

The CoreLink customer-facing trust corpus references the statuspage URL
from **5 MDX pages × 4 locales** (en-US / pt-BR / es-419 / de) plus **20+
internal runbooks and compliance docs**:

- `apps/docs/docs/trust/index.mdx`
- `apps/docs/docs/trust/incident-response.mdx`
- `apps/docs/docs/trust/subprocessors.mdx`
- `apps/docs/docs/explanation/security/incident-history.mdx`
- `apps/docs/docs/how-to/billing/manage-subscription.mdx`
- (and i18n mirrors under `apps/docs/i18n/{pt-BR,de,es-419}/...`)

The canonical default is `https://status.corelink.dev`. The engineering
side exposes a single source of truth at
`apps/docs/src/statuspage-url.ts` (`DEFAULT_STATUSPAGE_URL` +
`getStatuspageUrl()` helper) and wires it through
`docusaurus.config.ts customFields.statuspageUrl` so future MDX components
can consume the operator-provided value via
`useDocusaurusContext()`.

There are **two operator provisioning paths**. Choose **Option A** unless
constraints below force Option B.

---

## 2. Option A — CNAME the canonical hostname (preferred, zero docs rebuild)

**Use when:** the operator owns `corelink.dev` DNS (the default
GA-cutover assumption) and is happy for the public statuspage to be
served under `status.corelink.dev`.

### Steps

1. Provision Atlassian Statuspage tenant
   - Subscribe to Atlassian Statuspage "Business" tier (minimum for SSO,
     metrics overlay, and per-region component status).
   - Create page with name `CoreLink Status`, support URL
     `https://corelink.dev`, hidden from search until GA.
2. Define the **8 tracked components** per
   `apps/docs/docs/trust/incident-response.mdx §Status page`:
   - API ingress
   - CAS read path
   - CAS write path
   - Action cache
   - Audit chain
   - Admin plane
   - Identity (Clerk)
   - BYOK envelope
3. Configure custom domain `status.corelink.dev` in Statuspage settings;
   Statuspage will issue a `*.statuspage.io` target.
4. Add CNAME `status.corelink.dev → <tenant>.statuspage.io` in
   Cloudflare DNS (zone `corelink.dev`).
5. Wait for Atlassian's automated TLS-cert provisioning (typically
   ≤15 min) — confirm `https://status.corelink.dev` resolves and serves
   the new tenant.
6. **No docs rebuild required.** All 5 trust MDX pages × 4 locales + 20+
   internal docs now resolve correctly because the canonical hostname is
   pointing at the real tenant.
7. Sanity-check:
   ```bash
   curl -sI https://status.corelink.dev | head -3
   curl -s https://status.corelink.dev/api/v2/summary.json | jq '.page.url'
   curl -s https://status.corelink.dev/history.rss | head -3
   ```
   Expected: HTTP 200, JSON-API `page.url == "https://status.corelink.dev"`,
   RSS valid XML.

### Sign-off

SRE Lead records completion in `specs/_compliance/GA-GATE-CRITERIA.md`
checklist row for "Statuspage provisioned (Option A — CNAME)".

---

## 3. Option B — `STATUSPAGE_URL` env-var override (rebuild required)

**Use when:** the operator wants to host the statuspage under a different
domain (e.g. `https://status.example-corp.com`) because of an existing
status-portal contract, brand policy, or DNS-zone separation.

### Steps

1. Provision Atlassian Statuspage tenant as in §2 steps 1–2 above.
2. Provision the operator's chosen domain in Statuspage (e.g.
   `status.example-corp.com`); CNAME from the operator's DNS zone.
3. Set the build-time env var in the docs deploy pipeline:
   ```bash
   export STATUSPAGE_URL="https://status.example-corp.com"
   ```
4. Rebuild docs:
   ```bash
   pnpm --filter @corelink/docs build
   ```
   This propagates the operator value into
   `siteConfig.customFields.statuspageUrl` for any future MDX consumer
   that uses the `getStatuspageUrl()` helper.
5. **MDX literal URLs do not auto-rewrite.** The 5 trust pages × 4
   locales currently contain literal `https://status.corelink.dev`. If
   Option B is chosen, the operator must additionally run a one-shot
   text substitution under `apps/docs/docs/` and `apps/docs/i18n/` (and
   re-build) — recommended workflow:
   ```bash
   # From repo root, on a deploy-time branch:
   rg -l "status\.corelink\.dev" apps/docs/docs apps/docs/i18n |
     xargs sed -i.bak "s#https://status.corelink.dev#${STATUSPAGE_URL}#g; s#status.corelink.dev#${STATUSPAGE_URL#https://}#g"
   find apps/docs -name "*.bak" -delete
   pnpm --filter @corelink/docs build
   ```
   Commit the substitution as a deploy-time artifact (do NOT merge to
   main — main retains the canonical default).

### Sign-off

SRE Lead records completion in `specs/_compliance/GA-GATE-CRITERIA.md`
checklist row for "Statuspage provisioned (Option B — env-var override)".
Include the chosen domain + commit SHA of the substitution branch.

---

## 4. GA-cutover gate (T-7d)

By **T-7d pre-launch**, one of the following MUST be true (verified by
SRE Lead + Release Captain):

- [ ] **Option A:** `curl -sI https://status.corelink.dev` returns HTTP 200
      and `summary.json` reports the 8 tracked components, **OR**
- [ ] **Option B:** `STATUSPAGE_URL` env var is set in the docs deploy
      pipeline, deploy-time text-substitution branch merged to the
      release tag, and the operator-chosen domain resolves to the
      tenant.

If **neither** is true at T-7d, the launch readiness review **blocks GA
cutover** (see `specs/_compliance/GA-GATE-CRITERIA.md`). Manual fallback
(static HTML notice on `corelink.dev/status`) is **NOT** acceptable
because the customer-facing trust corpus cites the live JSON-API /
RSS / Atom / email-subscribe channels.

---

## 5. Post-incident review

After every SEV1 incident, SRE Lead validates that the statuspage was
updated within the cadence promised in
`apps/docs/docs/trust/incident-response.mdx §Communicating during a SEV1`
(5 min initial post + 30 min update + 1 hr follow-up). Any cadence
violation feeds into the next quarterly `RB-COMPLIANCE-WEEKLY-REVIEW.md`
review.

---

## 6. Cross-references

- Engineering-side closure audit: `specs/_audits/2026-05-16-debt-016-statuspage-urls.md`
- Build-time accessor: `apps/docs/src/statuspage-url.ts`
- Build config wiring: `apps/docs/docusaurus.config.ts customFields.statuspageUrl`
- DSR-channel runbook: `specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md`
- Launch spec: `marketing/launch/STATUS-PAGE-SPEC.md`
- Debt-register row: `specs/_audits/2026-05-15-debt-register.md` (DEBT-016)
