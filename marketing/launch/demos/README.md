---
id: "DEMO-BUNDLE-INDEX"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPMkt"
final_approver: "Gustavo Schneiter"
reviewers: ["CTO", "VPSec", "Design"]
supersedes: null
superseded_by: null
parent: "marketing/launch/LAUNCH-CHECKLIST-V2.md §2 L0 (T-7d demos recorded)"
tags:
  - "marketing"
  - "launch"
  - "demo"
  - "index"
  - "r8"
---

# CoreLink GA — demo asset bundle

> **Status:** scripts + storyboards only; videos not yet recorded. Recording is gated by `marketing/launch/LAUNCH-CHECKLIST-V2.md` row L0 at T-7d.
> **Owners:** VPMkt drives recording; CTO is technical accuracy reviewer; VPSec gates BYOK + audit claim language; Design owns brand pass on title cards + end cards.

---

## Contents

| Asset | Length | Audience | File |
|---|---|---|---|
| 60-second elevator | 60 s | Top-of-funnel social / homepage hero | [`60-SEC-ELEVATOR.md`](./60-SEC-ELEVATOR.md) |
| 5-minute deep dive | 5 min | Evaluator clicking "watch demo" | [`5-MIN-DEEPDIVE.md`](./5-MIN-DEEPDIVE.md) |
| 7-minute BYOK deep dive | 7 min | Enterprise security buyer / CISO | [`byok-deep-dive-demo.md`](./byok-deep-dive-demo.md) |
| 3-minute competitive comparison | 3 min | `bazel-remote` user weighing replacement | [`competitive-comparison-demo.md`](./competitive-comparison-demo.md) |
| Asciinema cast (CLI) | ~3 min | Foundation for all video CLI lanes | [`cli-asciinema-script.sh`](./cli-asciinema-script.sh) |
| Admin-UI screenshot guide | 15 stills | Marketing site, blog heroes, social | [`admin-ui-screenshot-guide.md`](./admin-ui-screenshot-guide.md) |

**Total demo runtime across the four scripted demos:** 60 s + 5 min + 7 min + 3 min = **16 minutes**.

---

## Recording order (recommended)

Stage in this order so each step de-risks the next:

1. **Pre-flight: warm both tenants.** `acme-build-cache` (sandbox), `acme-prod` (enterprise). Run `corelink doctor` against each, ensure 8/8 green.
2. **Asciinema first.** `cli-asciinema-script.sh` produces the canonical CLI cast. Every video reuses this cast (or a clipped subset) for its terminal lane.
3. **Screenshots second.** Capture all 15 shots per `admin-ui-screenshot-guide.md`. Apply the redaction checklist on the way out.
4. **5-minute deep dive third.** It composes everything and is the densest QA target.
5. **60-second elevator fourth.** Reuses 5-min beats; you'll know what works.
6. **BYOK 7-minute fifth.** Requires the second admin persona browser profile — rehearse the dual-approval click sequence.
7. **Competitive 3-minute last.** Requires a running `bazel-remote` instance — most likely to hit setup drift; doing it last protects the other recordings.

---

## Quality gates (before publish)

- [ ] **Every CLI command verified** against `crates/corelink-cli/src/main.rs` (canonical subcommands: `ls`, `get`, `put`, `stat`, `bench`, `doctor`, `version`, plus `config` and `runbook-drill`). No invented flags.
- [ ] **Every admin-UI URL verified** against `apps/admin-ui/src/app/[locale]/`. Routes used in the bundle: `/[locale]/onboarding/{tenant,region-plan,billing,pat,dpa,done}`, `/[locale]/admin/{tenants,tenants/[tenant_id],audit,audit/[event_id],ops,ops/[op_id]}`, `/sign-in`.
- [ ] **No real PATs, customer emails, or production KMS ARNs** anywhere in recordings or screenshots. See `admin-ui-screenshot-guide.md` redaction checklist.
- [ ] **Captions burned in** on every video (accessibility + sound-off social autoplay). Whisper output reviewed manually for technical terms (`BLAKE3`, `BYOK`, `Merkle`, `FIPS`).
- [ ] **Title/end cards branded** per `design/brand/` guide (pending Design pass).
- [ ] **Cross-reference back-linked** in `marketing/launch/LAUNCH-CHECKLIST-V2.md` row L0.

---

## Companion narrative (blog posts)

Each demo has a blog post that carries the same narrative in deeper, written form. The demos are paced reads of these posts.

| Blog post | Companion demo |
|---|---|
| [`marketing/launch/BLOG-POSTS/01-introducing-corelink.md`](../BLOG-POSTS/01-introducing-corelink.md) | 60-sec elevator + 5-min deep dive |
| [`marketing/launch/BLOG-POSTS/02-byok-deep-dive.md`](../BLOG-POSTS/02-byok-deep-dive.md) | 7-min BYOK deep dive |
| [`marketing/launch/BLOG-POSTS/03-audit-chain-merkle-proofs.md`](../BLOG-POSTS/03-audit-chain-merkle-proofs.md) | 5-min §4 + competitive §3 |
| [`marketing/launch/BLOG-POSTS/04-multi-region-residency.md`](../BLOG-POSTS/04-multi-region-residency.md) | 5-min §1.3 region-plan beat |
| [`marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md`](../BLOG-POSTS/05-fast-cache-hit-economics.md) | Competitive §5 summary table |

---

## Cross-reference

- `marketing/launch/LAUNCH-CHECKLIST-V2.md` row L0 — execution gate for this bundle.
- `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md` — full launch sequencer (this bundle is referenced from the T-7d arc).
- `marketing/launch/BLOG-POSTS/` — narrative companions (one post per technical pillar).
- `apps/docs/docs/tutorials/quickstart-10min.mdx` — the public-facing tutorial every demo command was cross-checked against.
- `crates/corelink-cli/src/main.rs` — canonical CLI subcommand surface.
- `apps/admin-ui/src/app/[locale]/` — canonical admin-UI route surface.
