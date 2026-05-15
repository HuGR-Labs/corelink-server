---
id: "DEMO-ADMIN-UI-SCREENSHOTS"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPMkt"
final_approver: "Gustavo Schneiter"
reviewers: ["CTO", "Design"]
supersedes: null
superseded_by: null
parent: "marketing/launch/LAUNCH-CHECKLIST-V2.md §2 (T-7d demos recorded)"
tags:
  - "marketing"
  - "launch"
  - "demo"
  - "screenshots"
  - "r8"
---

# Admin UI screenshot guide — 15 marketing-grade shots

> **Purpose:** numbered, reproducible screenshot list for marketing site, blog hero images, social posts, and embedded use inside `60-SEC-ELEVATOR.md` (shots #06, #07) and `5-MIN-DEEPDIVE.md` (shots #01–#15 spanning all sections).
>
> **Tooling:** Playwright (`apps/admin-ui/playwright/`) is the canonical capture tool — gives us deterministic viewport + auth state. Manual screenshots only as fallback.
>
> **Viewport:** **1440×900** (browser inner), Chrome on macOS, 2× DPR. Resize / crop downstream as needed.
>
> **Theme:** light theme for marketing (better photographic re-encode). Capture dark variants too for the "dark mode" badge on the homepage.
>
> **Tenant context:** all shots use `acme-build-cache` (sandbox) or `acme-prod` (enterprise, for BYOK shots). Both pre-provisioned per `5-MIN-DEEPDIVE.md` pre-flight.
>
> **PII / secret hygiene:** PATs, customer emails, real KMS ARNs, real IP addresses **must** be replaced with realistic-but-fake placeholders before publish. See §"Redaction checklist" at the bottom of this doc.

---

## Numbering convention

`Shot #NN — <route> — <one-line purpose>`

- Routes are written in their canonical Next.js form using `[locale]` (we capture under `en/`).
- Each shot has: page URL, what to highlight, optional annotation overlay (use a 2px ring + matching call-out arrow; brand color from `tailwind.config.ts`).
- Order is roughly the order a new evaluator encounters the UI — useful as a "guided tour" sequence on the marketing site.

---

## Shot #01 — Sign-in page

- **Route:** `/sign-in`
- **What to capture:** the full Clerk sign-in card centered on the page; "Sign in with Google" + "Sign in with GitHub" buttons visible.
- **Highlight:** subtle ring around the SSO buttons + small call-out: "SSO via Clerk — no separate password".
- **Annotation overlay:** "Sign in once. Your SSO, your audit trail."
- **Used in:** `5-MIN-DEEPDIVE.md` §1.1; marketing homepage "How it works" rail.

## Shot #02 — Onboarding step 1 (tenant creation)

- **Route:** `/en/onboarding/tenant`
- **What to capture:** form with display name `Acme Build Cache`, slug auto-filled, region dropdown showing `us-west-2`.
- **Highlight:** ring around the region dropdown.
- **Annotation overlay:** "Residency enforced at the routing layer."
- **Used in:** `5-MIN-DEEPDIVE.md` §1.2; blog post `04-multi-region-residency.md` hero.

## Shot #03 — Onboarding step 2 (region plan)

- **Route:** `/en/onboarding/region-plan`
- **What to capture:** primary region selector + optional secondary regions matrix (us-west-2 primary; EU-Frankfurt + AP-Sydney as opt-in secondaries).
- **Highlight:** primary region card.
- **Annotation overlay:** "Add replication regions any time — never silently."
- **Used in:** `5-MIN-DEEPDIVE.md` §1.3; pricing page residency section.

## Shot #04 — Onboarding step 3 (skip-billing CTA, sandbox tier)

- **Route:** `/en/onboarding/billing`
- **What to capture:** "Sandbox tier — no credit card" banner at top; sandbox CTA highlighted; paid-tier comparison table below (lightly faded so eye lands on sandbox).
- **Highlight:** ring around the "Start with sandbox" button.
- **Annotation overlay:** "Free 24h sandbox. Upgrade in one click."
- **Used in:** `5-MIN-DEEPDIVE.md` §1.4; pricing-page sandbox callout.

## Shot #05 — PAT issued (one-time view)

- **Route:** `/en/onboarding/pat`
- **What to capture:** the one-time PAT display card. The token must be **fake** (`corelink_sandbox_t_xxx.xxx.xxx` literal) with the rest blurred for the recording.
- **Highlight:** ring around the "Copy" button + the "I have saved this token" checkbox.
- **Annotation overlay:** "Shown once. Never logged. Never re-displayable."
- **Used in:** `5-MIN-DEEPDIVE.md` §1.5; quickstart hero (`apps/docs/docs/tutorials/quickstart-10min.mdx` step 1).
- **Redaction note:** the token characters in the real PAT card must be blurred or replaced before publish — see redaction checklist.

## Shot #06 — Audit page (filtered to last 5 minutes)

- **Route:** `/en/admin/audit`
- **What to capture:** the audit table with `put` + `get` rows for digest `af1c3e9b…` produced by the CLI demo. Filter chip "last 5 minutes" visible.
- **Highlight:** ring around the digest column.
- **Annotation overlay:** "Every put + get. Actor, IP, digest, timestamp."
- **Used in:** `60-SEC-ELEVATOR.md` Beat 4; `5-MIN-DEEPDIVE.md` §4.1; marketing homepage hero.

## Shot #07 — Audit event detail (Merkle proof block)

- **Route:** `/en/admin/audit/[event_id]` (use the event_id for the digest `af1c3e9b…` put)
- **What to capture:** the event metadata card + the "Merkle proof" panel showing leaf hash, daily-root hash, and inclusion-proof bytes (truncated).
- **Highlight:** ring around the Merkle proof panel.
- **Annotation overlay:** "Replay against the published signed root. Don't trust — verify."
- **Used in:** `60-SEC-ELEVATOR.md` Beat 4 zoom; `5-MIN-DEEPDIVE.md` §4.2; blog post `03-audit-chain-merkle-proofs.md` hero.

## Shot #08 — Tenant overview / detail

- **Route:** `/en/admin/tenants/acme-build-cache`
- **What to capture:** tenant summary card — total CAS bytes, request rate (last 24h), region pin, plan, member count, last activity.
- **Highlight:** the "Plan: Sandbox · TTL 23h 45m" badge.
- **Annotation overlay:** "Every tenant has a hard residency pin."
- **Used in:** `5-MIN-DEEPDIVE.md` §5.1 (transition shot); marketing site "Multi-tenant boundary" section.

## Shot #09 — Tenant list (multi-tenant boundary visible)

- **Route:** `/en/admin/tenants`
- **What to capture:** list of tenants the current admin has access to — only `acme-build-cache` + `acme-prod` visible. A third "phantom" row showing `?…` for unauthorized tenants (proves the boundary).
- **Highlight:** the entire visible list.
- **Annotation overlay:** "RBAC at the row level. Cross-tenant reads are not just hidden — they're refused."
- **Used in:** `competitive-comparison-demo.md` "multi-tenant boundary" beat; blog post `01-introducing-corelink.md`.

## Shot #10 — Encryption tab (BYOK provider matrix)

- **Route:** `/en/admin/tenants/acme-prod` → "Encryption" tab
- **What to capture:** the four-provider grid (AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault) with "Vendor-managed" currently active and "AWS KMS" highlighted as the selected target.
- **Highlight:** the four provider cards.
- **Annotation overlay:** "Four providers. Envelope encryption. Real cryptographic boundary."
- **Used in:** `5-MIN-DEEPDIVE.md` §5.2; `byok-deep-dive-demo.md` §2; blog post `02-byok-deep-dive.md` hero.

## Shot #11 — BYOK configure form (KMS key ARN + IAM role + test access)

- **Route:** `/en/admin/tenants/acme-prod` → "Encryption" → "Configure AWS KMS"
- **What to capture:** form with example KMS key ARN (`arn:aws:kms:us-west-2:111122223333:key/abcd...`) and IAM role to assume; "Test access" button with a green check next to it.
- **Highlight:** ring around the green check (proves the round-trip worked).
- **Annotation overlay:** "Encrypt-then-decrypt round-trip verified against your KMS."
- **Used in:** `5-MIN-DEEPDIVE.md` §5.3; `byok-deep-dive-demo.md` §3.
- **Redaction note:** ARN account ID must be the AWS docs-canonical `111122223333`, never a real account.

## Shot #12 — Sensitive ops queue (dual-approval workflow)

- **Route:** `/en/admin/ops`
- **What to capture:** queue of sensitive operations with the new "Activate BYOK on acme-prod" row in "pending — 1/2 approvers" state.
- **Highlight:** ring around the new pending row.
- **Annotation overlay:** "Two distinct approvers. Not one admin clicking twice."
- **Used in:** `5-MIN-DEEPDIVE.md` §5.4; `byok-deep-dive-demo.md` §4.

## Shot #13 — Sensitive op detail (approve / reject)

- **Route:** `/en/admin/ops/[op_id]`
- **What to capture:** the op detail page showing requestor, op type (`byok_activate`), requested-at, justification field, approval log (with first approver listed), and "Approve" / "Reject" buttons greyed out for the *requestor* (they cannot self-approve).
- **Highlight:** the greyed-out approve button + the "requestor cannot self-approve" hint.
- **Annotation overlay:** "Separation of duties. Cryptographically enforced."
- **Used in:** `byok-deep-dive-demo.md` §4; blog post `02-byok-deep-dive.md` §"Kill switch".

## Shot #14 — Audit page filtered for `byok.*` events (rotation evidence)

- **Route:** `/en/admin/audit?event_type=byok.rotate`
- **What to capture:** rows for a BYOK rotation showing the old key fingerprint → new key fingerprint, both approvers' actor IDs, and the timestamps for the four phases (request → first approval → second approval → rotation complete).
- **Highlight:** the timestamps column.
- **Annotation overlay:** "Rotation is auditable end-to-end."
- **Used in:** `byok-deep-dive-demo.md` §5; blog post `02-byok-deep-dive.md` §"Rotation that survives an audit".

## Shot #15 — Tenant overview with BYOK active + FIPS-mode attestation

- **Route:** `/en/admin/tenants/acme-prod` (post-activation)
- **What to capture:** tenant overview now shows "Encryption: AWS KMS (FIPS 140-2 Level 3 attested)" badge + last-rotation timestamp + next-scheduled-rotation timestamp + kill-switch toggle (state: armed).
- **Highlight:** the FIPS attestation badge + the kill-switch toggle.
- **Annotation overlay:** "FIPS attested. Kill switch armed. You're in control."
- **Used in:** `byok-deep-dive-demo.md` §6 + §7; marketing site "Enterprise readiness" section.

---

## Capture recipe (Playwright)

The `apps/admin-ui/playwright/` directory contains the auth-stub helpers. Recommended capture stub (place under `apps/admin-ui/playwright/marketing-shots.spec.ts`, not committed by this task — owned by Design):

```ts
import { test, expect } from "@playwright/test";

test.use({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });

test("shot-06-audit", async ({ page, context }) => {
  await context.storageState({ path: "./playwright/auth-states/admin.json" });
  await page.goto("/en/admin/audit?range=5m");
  await page.waitForSelector("table[data-testid='audit-table']");
  await page.screenshot({
    path: "marketing/launch/demos/screenshots/06-audit-recent.png",
    fullPage: false,
  });
});

// ... one test per shot.
```

Run with: `pnpm --filter admin-ui exec playwright test marketing-shots.spec.ts`

---

## Redaction checklist (before any public publish)

- [ ] PATs replaced with literal `corelink_sandbox_t_xxx.xxx.xxx` or blurred.
- [ ] Customer emails (sign-in card, audit actor field) replaced with `demo-eval@example.com`.
- [ ] AWS account IDs replaced with `111122223333` (docs-canonical).
- [ ] GCP project IDs replaced with `corelink-demo-project`.
- [ ] Azure subscription IDs replaced with the all-zeros canonical form.
- [ ] Vault addresses replaced with `https://vault.example.com:8200`.
- [ ] Source IP columns in audit screenshots — IPs swapped for `198.51.100.0/24` (TEST-NET-2) or `203.0.113.0/24` (TEST-NET-3).
- [ ] PII (real names, real emails, real org names) scrubbed from actor fields.
- [ ] Tenant IDs allowed: `acme-build-cache`, `acme-prod` (both clearly fake). No real customer slugs.
- [ ] Tokens / IP / ARN / project IDs cross-checked **after** PNG export — sometimes redaction overlays drop during re-encoding.

---

## Naming convention

`marketing/launch/demos/screenshots/<NN>-<short-slug>.png`

Examples:

- `01-signin.png`
- `06-audit-recent.png`
- `10-encryption-matrix.png`
- `15-byok-active-fips.png`

Plus a `screenshots/dark/` parallel directory for dark-theme variants of the same shots (only for those used on the homepage).

---

## Cross-reference

- `60-SEC-ELEVATOR.md` — uses shots #06, #07.
- `5-MIN-DEEPDIVE.md` — uses shots #01, #02, #03, #04, #05, #06, #07, #08, #10, #11, #12.
- `byok-deep-dive-demo.md` — uses shots #08, #10, #11, #12, #13, #14, #15.
- `competitive-comparison-demo.md` — uses shots #06, #07, #09.
- `apps/admin-ui/playwright/` — canonical capture tooling.
- `marketing/launch/LAUNCH-CHECKLIST-V2.md` row T-7d ("demos recorded") — gating dependency.
