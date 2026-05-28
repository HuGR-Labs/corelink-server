---
id: "FOLLOWUP-2026-05-27-COSMETIC"
type: "followup"
doc_status: "ACTIVE"
followup_status: "OPEN"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["followup", "cosmetic", "manual-action", "launch-polish"]
---

# 2026-05-27 — Cosmetic followups (Gustavo-manual)

> **None of these block product or Phase 2.** Listed here for closeout
> before Show HN launch (Phase 3). All are 5-30min manual UI actions or
> external-account signups; no agent path exists for any of them.

## §1 GitHub UI actions

| Item | Where | Why | Effort |
|---|---|---|---|
| 1.1 Pin 4 repos on `humangr-labs` profile | https://github.com/humangr-labs → "Customize your profile" → "Edit pinned repositories" | Show HN traffic sees clean profile; pinning order: `corelink-server`, `corelink-cli`, `humangr-labs`, `.github` | 1 min |
| 1.2 Upload OG image to `corelink-server` | https://github.com/humangr-labs/corelink-server/settings → "Social preview" → Edit | Link previews on Twitter/HN/Slack | 30 sec |
| 1.3 Upload OG image to `corelink-cli` | https://github.com/humangr-labs/corelink-cli/settings → "Social preview" → Edit | Same as 1.2 | 30 sec |
| 1.4 Upload OG image to `corelink-bazel-example` | https://github.com/humangr-labs/corelink-bazel-example/settings → "Social preview" → Edit | Same as 1.2 | 30 sec |

OG image sources at `marketing/og/{corelink-server,corelink-cli,corelink-bazel-example}.png` (1280×640).

## §2 External-vendor signups + secret provisioning

| Item | Vendor | What | Effort | Failure mode if skipped |
|---|---|---|---|---|
| 2.1 Sentry account | https://sentry.io free tier | Get DSN → `wrangler pages secret put SENTRY_DSN` for admin-ui + 3 Workers | 5 min | SDK runs as no-op (by design); zero errors captured remotely until provisioned. App functional. |
| 2.2 Resend Audience | https://resend.com/audiences | Create Audience → copy ID → `wrangler pages secret put RESEND_NEWSLETTER_AUDIENCE_ID` | 2 min | `/api/newsletter/subscribe` returns 500. Newsletter widget visible but non-functional. |
| 2.3 BetterStack badge URL | https://uptime.betterstack.com | Confirm `badge.json` URL is set to canonical status; verify StatusPill in docs reads it | 1 min | StatusPill defaults to "operational" green (fail-quiet). |
| 2.4 Plausible account | https://plausible.io $9/mo | Confirm `corelink-docs.humangr.com` site is registered + script is allowed in CSP (already done) | 5 min | No analytics until registered. Site functional. |
| 2.5 PostHog account | https://posthog.com free | Currently NOT wired (per CSP audit). Decision: keep deferred OR wire up. | 0 / 15 min | N/A — currently no PostHog code paths. |
| 2.6 Cal.com event type | https://cal.com free | Create "Discovery / Sandbox Tour" 30-min event; link from docs footer + landing CTA | 10 min | Interviews go via raw email/calendar instead of self-serve booking link. |
| 2.7 HubSpot Free CRM | https://hubspot.com free | Import 47 ICP accounts from `2026-05-27-icp-target-list.md` | 30 min | Outreach tracking done in spreadsheet instead of CRM. |
| 2.8 Twitter `@corelinkdev` | https://twitter.com signup | Create account; set bio + 20-account follow list from `2026-05-27-twitter-starter-pack.md` | 15 min | No Twitter handle to drive HN traffic to. |
| 2.9 GHA secret `CORELINK_TEST_TOKEN_CI` | https://github.com/humangr-labs/corelink-bazel-example/settings/secrets | Production PAT for the bazel-example smoke workflow | 1 min | bazel-example CI exits 0 in `disk-cache` mode (no remote auth); skipped remote-cache assertion. |

## §3 Legal/Ops (NOT cosmetic — bloqueia receber dinheiro)

> Listed here for proximity; these ARE blockers for first revenue, NOT for
> Phase 2 outreach/interviews. Detached from cosmetic register; see
> `ROADMAP-TO-LAUNCH.md` §6 Phase 1.7-1.9.

| Item | What | Effort | Cost |
|---|---|---|---|
| 3.1 Stripe Atlas DE C-Corp filing | https://stripe.com/atlas | 1 h once + 2-3 weeks turnaround | $500 |
| 3.2 83(b) election | IRS Form 83(b), must file ≤30d after incorporation | 30 min | $0 (USPS certified) |
| 3.3 Termageddon + DPA publication | https://termageddon.com $119/yr | 1 h to publish privacy/terms/cookie via Termageddon iframe | $119/yr |

## §4 Closeout policy

- Items in §1-§2 are revisited at the **start of Phase 3** (week of Show HN).
  If not done by then, Show HN is postponed 1 week.
- §1 + 2.2 (Resend) + 2.8 (Twitter) are MUST-HAVE before Show HN.
- §2.1, 2.4, 2.6, 2.7 are NICE-TO-HAVE; the launch survives without them.
- §3 is independent track; revisit weekly until all CLOSED.

## §5 DCO

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
