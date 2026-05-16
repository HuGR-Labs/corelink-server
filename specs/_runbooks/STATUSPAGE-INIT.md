---
id: "RB-STATUSPAGE-INIT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.2.0"
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
tags: ["runbook", "statuspage", "operator", "ga-cutover", "provisioning", "debt-016", "wave-24", "wave-27", "wave-28"]
escalation: "SRE Lead → Incident Commander → CTO"
review_cadence: "annual + post-incident"
related:
  - "specs/_audits/2026-05-16-debt-016-statuspage-urls.md"
  - "specs/_audits/2026-05-16-statuspage-init-dressrun.md"
  - "specs/_audits/2026-05-16-cutover-dependency-map.md"
  - "specs/_runbooks/RB-GA-CUTOVER.md"
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
`status.corelink.humangr.com` (or override via the `STATUSPAGE_URL` env-var path)
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

The canonical default is `https://status.corelink.humangr.com`. The engineering
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

**Use when:** the operator owns `corelink.humangr.com` DNS (the default
GA-cutover assumption) and is happy for the public statuspage to be
served under `status.corelink.humangr.com`.

### Steps

1. Provision Atlassian Statuspage tenant
   - Subscribe to Atlassian Statuspage "Business" tier (minimum for SSO,
     metrics overlay, and per-region component status).
   - Create page with name `CoreLink Status`, support URL
     `https://corelink.humangr.com`, hidden from search until GA.
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
3. Configure custom domain `status.corelink.humangr.com` in Statuspage settings;
   Statuspage will issue a `*.statuspage.io` target.
4. Add CNAME `status.corelink.humangr.com → <tenant>.statuspage.io` in
   Cloudflare DNS (zone `corelink.humangr.com`).
5. Wait for Atlassian's automated TLS-cert provisioning (typically
   ≤15 min) — confirm `https://status.corelink.humangr.com` resolves and serves
   the new tenant.
6. **No docs rebuild required.** All 5 trust MDX pages × 4 locales + 20+
   internal docs now resolve correctly because the canonical hostname is
   pointing at the real tenant.
7. Sanity-check:
   ```bash
   curl -sI https://status.corelink.humangr.com | head -3
   curl -s https://status.corelink.humangr.com/api/v2/summary.json | jq '.page.url'
   curl -s https://status.corelink.humangr.com/history.rss | head -3
   ```
   Expected: HTTP 200, JSON-API `page.url == "https://status.corelink.humangr.com"`,
   RSS valid XML.

### Sign-off

SRE Lead records completion in `specs/_compliance/GA-GATE-CRITERIA.md`
checklist row for "Statuspage provisioned (Option A — CNAME)".

### 2.5 Timing relative to GA-cutover sequence

The wave-24 cut of this runbook stated only "before T-7d" without
propagation analysis. Wave-27 hardens the timing per the canonical
dependency map at
`specs/_audits/2026-05-16-cutover-dependency-map.md` (nodes N-S-1 +
N-S-2).

**Recommended Option A schedule:**

| Anchor | Action | Owner |
|---|---|---|
| T-10d | Begin §2 steps 1–6 (tenant provisioning + 8 components + CNAME) | SRE Lead |
| T-10d → T-7d | **3d buffer** for Atlassian TLS-cert issuance + CF zone update propagation + 8-component setup + statuspage banner staging (`RB-GA-CUTOVER.md` §0.5) | passive |
| T-7d | §4 go-live gate verified (§2 step 7 curl block) and sign-off filed in `GA-GATE-CRITERIA.md` | SRE Lead + Release Captain |

The CNAME TTL at provisioning is the standard Cloudflare default
(**300s**), not 7d. The wave-24 framing of a "7-day window" referred to
the operator's *decision deadline* between Option A and Option B, not
the DNS TTL: re-pointing the `status.corelink.humangr.com` CNAME after GA is
not in scope (Atlassian manages the underlying tenant target).

If `T-10d` start slips, the gate at `T-7d` flips RED and triggers the
`RB-GA-LAUNCH-ROLLBACK.md` §7 RA-3 sustained-staging clause — GA T-0
defers by **≥ 7 days minimum** because the gate is binary and the
window does not auto-extend. See dependency map §9 for the slip-window
matrix.

### 2.6 Wave-28 automation (Option A only)

Wave-28 R-prep step-5 lands AS-IF-AUTOMATED Option A artifacts that
collapse the §2 steps 1–4 + 6 into **2 browser clicks + 1 shell
command**. The artifacts are:

| Artifact | Purpose |
|---|---|
| `docs/internal/statuspage-tenant-signup-quickstart.md` | 7-step Owner quickstart (browser signup + bootstrap CLI + DNS click) |
| `config/statuspage/components.yml` | Declarative 4-group / 5-component config (drives bootstrap) |
| `config/statuspage/incident-templates.yml` | 6 pre-drafted SEV templates (SEV-0 audit-chain · SEV-1 failover · SEV-1 DSR · SEV-2 audit-latency · SEV-2 shadow-lag · maintenance BYOK rotation) |
| `config/statuspage/dns-cname-record.txt` | Exact Cloudflare CNAME record (proxy=OFF, TTL=Auto) |
| `scripts/admin/statuspage-bootstrap.sh` | Idempotent post-signup CLI (component groups + components + templates + branding + notifications) |

The two operator-bound surfaces that remain manual are:

1. **Browser signup at `statuspage.atlassian.com`** — Atlassian does
   not expose tenant-creation via API.
2. **DNS CNAME save in Cloudflare** — DNS-provider login is
   operator-bound; the record content is the canonical
   `dns-cname-record.txt` above.

Everything else (component tree, branding, templates, notifications)
is driven by the bootstrap script from signed-off YAML, runs in ~2
minutes, and is safe to re-run on failure.

For the §2 8-internal-component view, the components.yml file
preserves the wave-24 mapping via the `internal_subsystems:` block on
each customer-facing component — see the file header for the
5-vs-8 reconciliation note.

---

## 3. Option B — `STATUSPAGE_URL` env-var override (rebuild required)

> **Wave-28 automation cross-ref.** The wave-28 R-prep step-5
> artifacts (see §2.6) target Option A only. Option B operators
> can still consume `config/statuspage/components.yml`,
> `config/statuspage/incident-templates.yml`, and
> `scripts/admin/statuspage-bootstrap.sh` against their own
> tenant + page-id — only the DNS-record file
> (`config/statuspage/dns-cname-record.txt`) is Option A specific
> (Cloudflare + `status.corelink.humangr.com`). Substitute your own
> domain + DNS provider before applying.

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
   locales currently contain literal `https://status.corelink.humangr.com`. If
   Option B is chosen, the operator must additionally run a one-shot
   text substitution under `apps/docs/docs/` and `apps/docs/i18n/` (and
   re-build) — recommended workflow:
   ```bash
   # From repo root, on a deploy-time branch:
   rg -l "status\.corelink\.dev" apps/docs/docs apps/docs/i18n |
     xargs sed -i.bak "s#https://status.corelink.humangr.com#${STATUSPAGE_URL}#g; s#status.corelink.humangr.com#${STATUSPAGE_URL#https://}#g"
   find apps/docs -name "*.bak" -delete
   pnpm --filter @corelink/docs build
   ```
   Commit the substitution as a deploy-time artifact (do NOT merge to
   main — main retains the canonical default).

### Sign-off

SRE Lead records completion in `specs/_compliance/GA-GATE-CRITERIA.md`
checklist row for "Statuspage provisioned (Option B — env-var override)".
Include the chosen domain + commit SHA of the substitution branch.

### 3.6 Timing relative to GA-cutover sequence

Per the wave-27 dependency map
(`specs/_audits/2026-05-16-cutover-dependency-map.md` node N-S-1b):

| Anchor | Action | Owner |
|---|---|---|
| T-14d ± 3d | Operator decides Option B (post-dress-rehearsal, see dep-map §6.2) | SRE Lead + Owner |
| T-10d | Begin §3 steps 1–4 (tenant + operator-chosen domain + env-var set + docs rebuild on deploy-time branch) | SRE Lead + Engineering Lead |
| T-9d → T-8d | Substitution sweep (§3 step 5) committed on deploy-time branch | Engineering Lead |
| T-8d → T-7d | **1d verify window**: operator-domain CNAME propagation across the operator's zone + docs-build re-run validation | SRE Lead |
| T-7d | §4 go-live gate verified; sign-off filed in `GA-GATE-CRITERIA.md` | SRE Lead + Release Captain |

The 1d verify window is tighter than Option A's 3d buffer because the
wave-25 dress-run already proved the substitution mechanism is
regression-free (S3 PASS, see
`specs/_audits/2026-05-16-statuspage-init-dressrun.md` §3). The verify
window therefore covers operator-domain DNS resolution + docs-build
re-run only, not the substitution mechanism itself.

A slip past T-8d on Option B forfeits the verify window and triggers
the same §7 RA-3 ≥ 7d defer as Option A.

---

## 4. GA-cutover gate (T-7d)

By **T-7d pre-launch**, one of the following MUST be true (verified by
SRE Lead + Release Captain):

- [ ] **Option A:** `curl -sI https://status.corelink.humangr.com` returns HTTP 200
      and `summary.json` reports the 8 tracked components, **OR**
- [ ] **Option B:** `STATUSPAGE_URL` env var is set in the docs deploy
      pipeline, deploy-time text-substitution branch merged to the
      release tag, and the operator-chosen domain resolves to the
      tenant.

If **neither** is true at T-7d, the launch readiness review **blocks GA
cutover** (see `specs/_compliance/GA-GATE-CRITERIA.md`). Manual fallback
(static HTML notice on `corelink.humangr.com/status`) is **NOT** acceptable
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
- Dress-run audit (wave-25): `specs/_audits/2026-05-16-statuspage-init-dressrun.md`
- Cutover dependency map (wave-27): `specs/_audits/2026-05-16-cutover-dependency-map.md`
- Wave-28 Option A automation bundle (§2.6):
  - Quickstart: `docs/internal/statuspage-tenant-signup-quickstart.md`
  - Components config: `config/statuspage/components.yml`
  - Incident templates: `config/statuspage/incident-templates.yml`
  - DNS record: `config/statuspage/dns-cname-record.txt`
  - Bootstrap CLI: `scripts/admin/statuspage-bootstrap.sh`
- GA cutover runbook: `specs/_runbooks/RB-GA-CUTOVER.md` §0 + §3 + §9
- Reverse runbook (slip-window): `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` §7 RA-3
- Build-time accessor: `apps/docs/src/statuspage-url.ts`
- Build config wiring: `apps/docs/docusaurus.config.ts customFields.statuspageUrl`
- DSR-channel runbook: `specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md`
- Launch spec: `marketing/launch/STATUS-PAGE-SPEC.md`
- Debt-register row: `specs/_audits/2026-05-15-debt-register.md` (DEBT-016)
