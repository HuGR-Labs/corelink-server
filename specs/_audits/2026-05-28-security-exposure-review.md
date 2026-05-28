---
id: "AUDIT-2026-05-28-SECURITY-EXPOSURE"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-28"
updated: "2026-05-28"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "security", "exposure", "bot-protection", "cloudflare"]
references:
  - "specs/_audits/2026-05-28-multimodel-prod-readiness-audit.md"
  - "wrangler.toml"
  - "apps/analytics-worker/src/ingest.ts"
---

# Security + Exposure Review (2026-05-28)

> Opus independent review + orchestrator live-CF verification. Question:
> what's exposed that shouldn't be / abusable by bots or malicious traffic?

## §0 Headline
Edge posture on the **custom domains** is genuinely strong. The residual
exposures are off the custom-domain path: `*.pages.dev` aliases (no
rate-limit — the prior $36-bill class), a public R2 bucket, and an
Origin-spoofable analytics ingest (not yet deployed). Two of the
orchestrator's earlier worries were FALSE ALARMS (verified live): `/_do/*`
is NOT edge-reachable, and the rate-limit IS enforced.

## §1 VERIFIED-CLEAN (live)
- SSL=strict, min TLS=1.3, Always-HTTPS=on, Bot Fight Mode (fight_mode +
  enable_js + ai_bots_protection=block).
- **Edge rate-limit LIVE + enforced**: 11 req / 10s / IP on the 5
  `corelink-*.humangr.com` hosts (verified: 429 after 10 rapid reqs).
- `/_do/stop` + `/_do/health` **NOT edge-reachable** — `matchRoute()` has
  no `/_do` prefix → external `/_do/*` → 404, never forwarded to the DO.
  (Orchestrator worry corrected.)
- `workers.dev` disabled + unreachable (HTTP 000); `workers_dev=false`
  effective on all workers (signup-worker gap closed 2026-05-28 `df8c0ecb`).
- All corelink DNS proxied (orange-cloud); no grey-cloud bypass.
- CAS / AC / chunk / manifest / mail R2 buckets = private.
- No secrets in git history or tracked tree; `.env.local` gitignored.
- E2E test-mode double-gated on `NODE_ENV!=production` (inert in prod).
- Security headers strong: HSTS, nosniff, X-Frame-Options DENY,
  frame-ancestors 'none', Permissions-Policy across surfaces.

## §2 EXPOSED / UNDER-PROTECTED (fix list)

| # | Sev | Finding | Evidence | Fix | Owner/Agent |
|---|---|---|---|---|---|
| E1 | HIGH | **`*.pages.dev` aliases live + indexable + NO rate-limit** — `corelink-docs.pages.dev`, `corelink-admin-ui.pages.dev`, `hugr-site.pages.dev` all 200; robots `Allow:/`; rate-limit rule matches only `*.humangr.com` (hammered pages.dev 15x → 0 × 429). THE $36-bill vector. | live curl + CF ratelimit ruleset | Restrict Pages access to custom-domain-only (Pages access policy) OR add a rate-limit rule keyed on the `.pages.dev` hosts. | Owner (CF dash) or orchestrator (rate-limit rule via API, with go) |
| E2 | MED | **Public R2 bucket `hugr-downloads`** at `pub-8babe…r2.dev` — public, un-rate-limited egress. (All CAS/AC/etc private.) | CF R2 managed-domains API | Front with a Worker/custom domain you can rate-limit, OR confirm intended public-download + accept egress risk. | Owner decision |
| E3 | MED | **Analytics ingest Origin-spoofable** — `POST /v1/event` auths via `Origin` header (spoofable by any non-browser) → unbounded D1 writes (up to 100/req). Not deployed yet (HTTP 000). | `apps/analytics-worker/src/ingest.ts:52-57,114-136` | Require `X-Corelink-Ingest-Key`; add `corelink-analytics.humangr.com` to the edge rate-limit host set BEFORE deploy. | new micro-WP |
| E4 | MED | **CSP static nonce** — statically-exported Pages app ships literal `'nonce-STATIC'` → nonce-based XSS protection is void; `<script nonce="STATIC">` would execute. | live `corelink-app.humangr.com` + `corelink-admin-ui.pages.dev` CSP | Hash-based CSP for the static export, OR per-request nonce via a Worker, OR drop nonce + rely on 'self'+SRI. | new micro-WP |
| E5 | LOW | **Container-start-before-auth** — DO calls `ensureContainerRunning` (`durable_object.ts:272`) BEFORE any PAT validation → a format-valid (but invalid) token spins a container (cost). | `durable_object.ts:259-285` | WP-A1/T1 must reject invalid PAT in the DO BEFORE ensureContainerRunning. | WP-A1/T1 (in flight) |
| E6 | LOW | **admin-ui middleware** invokes clerkMiddleware but never `auth().protect()`; honors `result` only for 3xx → if Clerk returns 200 for an unauth request, protected page renders. | `apps/admin-ui/middleware.ts:75-96` | Add explicit `auth.protect()` OR confirm Clerk redirects (not pass-through) unauth users. | new micro-WP |
| E7 | INFO | `CLOUDFLARE_API_TOKEN` in `.env.local` is a superadmin all-scopes token — high blast radius if laptop compromised. | `.env.local` (gitignored) | Scope it down to the zones/resources actually needed. | Owner |

## §3 Top 3 before ANY public traffic
1. **Lock down `*.pages.dev`** (E1) — the prior-$36-bill vector. Pages
   access policy (custom-domain-only) OR a rate-limit rule for the
   `.pages.dev` hosts.
2. **Analytics ingest** (E3) — ingest-key + edge rate-limit before it
   deploys (keep undeployed until then).
3. **Static CSP nonce** (E4) — makes nonce XSS protection a no-op on the
   live Pages apps.

Plus E5 (container-start-before-auth) folds into the in-flight WP-A1/T1.

## §4 DCO
DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
(Opus security review + orchestrator live-CF verification, 2026-05-28.)
