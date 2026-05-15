# CoreLink — Engineering Onboarding

> **Status:** ACTIVE — quarterly review (next: 2026-08-15).
> **Owner:** Engineering Manager (rotating).
> **Audience:** New backend / platform / SRE / security engineers joining CoreLink post-GA.
> **Goal:** From offer letter signed to *first PR merged* in **≤ 5 working days**, to *independent productivity* in **≤ 30 days**.

If you are reading this on Day 1 — welcome. CoreLink is a multi-tenant
content-addressable cache implementing REAPI on Cloudflare's edge.
Spec corpus is 262 documents, 21 sealed sprints, 8 TLA+ specifications,
and ~96 Rust crates. None of that is necessary on Day 1. This document
exists so you can *ignore* most of it on purpose.

The path below is a recommendation, not a SLA. Skip what you already
know; ask your buddy about everything else.

---

## Big picture (read this first — 10 min)

CoreLink is a **shared, content-addressable build cache** for Bazel,
Buck2, and any REAPI-conformant client. The wedge is:

1. **Multi-tenant + regulated-industry-ready.** Existing options force a
   choice between managed (BuildBuddy) and self-hosted control
   (BuildBarn, NativeLink). CoreLink ships both.
2. **Tenant isolation is a TLA+ invariant**, not marketing copy
   (`tenant_isolation.tla`, `cas_integrity.tla`,
   `audit_immutability.tla`, `gc_correctness.tla`).
3. **BYOK across AWS KMS, GCP Cloud KMS, Azure Key Vault, Vault Transit**,
   with a customer-held kill switch and signed Ed25519 erasure
   attestations.
4. **Audit chain as primary artifact** — RFC 6962 Merkle, RFC 8785 JCS
   leaves, daily published proofs.

Architecture in one paragraph: gRPC server (`tonic`) on Cloudflare
Containers terminates REAPI. CAS blobs land in R2 keyed by their BLAKE3
digest (SHA-256 fallback). Action Cache, audit chain, and tenant
metadata live in Postgres (Neon today, migrating to D1). Control plane
runs as Cloudflare Workers; event fan-out goes through Durable Objects.
Auth is Clerk. Billing is Stripe meter-based. Everything customer-facing
sits behind tier-gated rate limits.

For the marketing-grade version, read
`marketing/launch/BLOG-POSTS/01-introducing-corelink.md`. For the
roadmap, read `ROADMAP-TO-GA.md`. For everything else, this document is
the index.

---

## Day 0 — Pre-arrival (manager prep)

Your **hiring manager** is responsible for these *before* your first
day. If anything is missing on Day 1, surface it immediately — none of
this is your fault to chase.

### Laptop & local environment

- Apple silicon MacBook Pro (M2 Pro / M3 Pro minimum; 32 GB RAM
  strongly recommended — the workspace does heavy `cargo` builds).
- FileVault enforced; OS up-to-date; corporate MDM enrolled.
- Pre-installed (via `brew bundle` from the onboarding `Brewfile` —
  ask your buddy):
  - `rustup` (toolchain pin lives in `rust-toolchain.toml`).
  - `protobuf` (`protoc`), `cmake`, `pkg-config`.
  - `wrangler` (Cloudflare).
  - `gh` (GitHub CLI).
  - `gcloud`, `aws-cli`, `az` (read-only IAM Day 1, write later).
  - `pre-commit`, `python@3.12`, `node@20`, `pnpm`.
  - `tla-tools` / `tlc` (optional — only if joining the formal-methods
    rotation).
  - `grpcurl`, `jq`, `yq`.

### Access requests (request *before* Day 0 — most take 24–72h)

| System | Role | Who provisions |
|---|---|---|
| GitHub org `humangr-labs` | `engineering` team (write to non-protected branches) | Eng Manager |
| Drata (compliance) | `auditor-read` | Security |
| PagerDuty | `observer` (escalate to responder after Week 2 shadow) | SRE Lead |
| Slack | `#corelink-eng`, `#corelink-oncall`, `#corelink-ops`, `#corelink-launch` | Eng Manager |
| AWS (`corelink-prod`) | `ReadOnly` IAM | Platform |
| GCP (`corelink-prod`) | `roles/viewer` | Platform |
| Azure (`corelink-prod-rg`) | `Reader` | Platform |
| Cloudflare dashboard | `Analytics: Read` + `Workers: Read` | Platform |
| Sentry / Grafana / Honeycomb | `read` on `corelink-*` projects | SRE Lead |
| 1Password | `engineering` vault (read) | Security |
| Notion / docs | `engineering` workspace | Eng Manager |

### Mandatory pre-reads (≤ 3 docs; ≤ 90 min total)

1. `marketing/launch/BLOG-POSTS/01-introducing-corelink.md` — what we
   sell and to whom.
2. `README.md` — repo layout, stack, local `cargo build`.
3. `ROADMAP-TO-GA.md` §0 + §1 — where we are post-spec-corpus.

If you are stress-reading anything beyond these three before Day 1,
you are over-preparing. Stop.

---

## Day 1 — Welcome + context (≈ 4 hours)

| Block | Duration | Activity |
|---|---|---|
| 09:00 | 15 min | Welcome from CTO (calendar invite sent Day-3). |
| 09:15 | 30 min | Re-read this doc's *Big picture* section. Ask your buddy 3 questions. |
| 09:45 | 15 min | Read `01-introducing-corelink.md` (skim if already done). |
| 10:00 | 60 min | **Buddy 1:1**. Codebase nav, internal jargon, "who owns what". |
| 11:00 | 30 min | Break / coffee. |
| 11:30 | 60 min | **First build**: `git clone`, `cargo build --workspace`, `cargo test -p tenant-path` smoke. See *First build* below. |
| 12:30 | — | Lunch. |

### First build (Day 1 PM, 60 min target — 90 min ceiling)

```bash
# 1. Clone (SSH).
git clone git@github.com:humangr-labs/corelink-server.git
cd corelink-server

# 2. Pin toolchain (reads rust-toolchain.toml automatically).
rustup show

# 3. Install pre-commit hooks.
pre-commit install

# 4. Full workspace build (first build is ~12–18 min cold).
cargo build --workspace

# 5. Smoke test — tenant path (small crate, fast).
cargo test -p corelink-tenant-prefix

# 6. Spec validator (no Rust required).
python3 -m pip install -r requirements-ci.txt
python3 scripts/validate_specs.py
```

**Expected output:** 0 failures from `validate_specs.py`,
green `cargo test`. If anything is red, screenshot the error and ping
your buddy — there is exactly one "first-day build failed" Slack
thread per quarter and yours might be it.

---

## Day 2 — Architecture deep-dive (≈ 6 hours)

Walk the **four canonical architecture documents** in this order. Each
is 90 minutes: 60 min read, 10 min notes, 20 min self-quiz.

| Order | Doc | Why this order |
|---|---|---|
| 1 | `specs/03_architecture/security_model.md` | Tenant isolation is the foundation invariant; everything else assumes it. |
| 2 | `specs/03_architecture/data_model.md` | CAS, AC, audit chain, tenant prefix — the nouns you will touch every day. |
| 3 | `specs/03_architecture/privacy_model.md` | DSR, consent, residency, BYOK — the regulated-industry wedge. |
| 4 | `specs/03_architecture/observability_model.md` | Metrics, traces, SLOs, audit log — how we know production is healthy. |

If you are time-constrained, drop `observability_model.md` and read it
during Day 3 hands-on instead — observability is best learned by
*looking at the dashboards while doing*.

### Self-quiz (each doc, 10 questions, answered in writing)

After each doc, write answers in your onboarding doc (a private Notion
page works). Your buddy reviews them at the start of Day 3. Sample
questions, in case the doc author has not added a quiz yet:

- *Security:* Why is tenant-prefix derivation in `corelink-tenant-prefix`
  rather than in `corelink-cas`? What invariant breaks if we move it?
- *Data:* What is the canonical-form input to the action-key hash, and
  why must it be canonical?
- *Privacy:* What is the upper bound on DEK-cache TTL and where is it
  enforced (file + line range)?
- *Observability:* Which SLO degrades first when R2 latency spikes by
  2× — `cas_get_p95` or `ac_get_p95`? Why?

### Supporting reads (skim, do not deep-read)

- `specs/03_architecture/invariant_registry.md` — index of the 136 INVs.
  Read the table of contents; do *not* try to read all 136 today.
- `specs/03_architecture/failure_modes.md` — pair this with
  `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` to see how an FM-XXX maps to
  an on-call runbook.
- `specs/03_architecture/slo_catalog.md` — SLO names you will hear in
  standup all the time (`cas_get_p95`, `ac_write_success_rate`, etc.).

---

## Day 3 — Hands-on (≈ 6 hours)

Today you stop reading and start *doing*. The objective is to make the
codebase feel less abstract.

### Block 1 — 10-min quickstart (≈ 60 min including writeup)

Run the published 10-minute quickstart against the staging sandbox.
Steps live in `docs/customer/QUICKSTART.md` (if not yet published,
your buddy has the staging Bazel `.bazelrc.staging` snippet). At
minimum:

1. `cargo install --path crates/corelink-cli` (or grab a release binary).
2. `corelink auth login --env staging`.
3. Configure Bazel with the `--remote_cache=https://staging.corelink.dev`
   block.
4. Build any open-source Bazel target (e.g. `bazelbuild/rules_rust`).
5. Confirm cache hits on the second build.
6. Note your *time-to-first-hit* in your onboarding doc.

### Block 2 — Dry-run a runbook (≈ 90 min)

Pick *one* runbook from `specs/_runbooks/`. Recommended for first-time:

- `RB-WEBHOOK-DLQ-REPLAY.md` — short, isolated, doesn't need real auth.
- `RB-SYNTHETIC-PAGE-DRILL.md` — pager mechanics, no destructive ops.
- `RB-D1-MIGRATION-APPLY.md` — read-only first pass (don't apply!).

Dry-run *as if* you were the on-call. Note every command you would
hesitate on. Bring those to your buddy at end of day.

### Block 3 — Trace a request end-to-end (≈ 3 hours)

Pick one request path and walk it crate-by-crate. Recommended starter
path:

> **Signup → tier-select → Stripe webhook → audit-chain leaf**

Read each crate's `lib.rs` and the top-level test module:

1. `crates/corelink-clerk` — Clerk JWT validation entrypoint.
2. `crates/corelink-tier-selector` — tier mapping logic.
3. `crates/corelink-billing-stripe` — Stripe webhook handler.
4. `crates/corelink-billing-emit` — billing event emission.
5. `crates/corelink-audit-chain` — Merkle leaf append.

You will not understand every line. Goal: build a mental graph of
*which crate calls which*. Sketch it on paper. Show your buddy.

---

## Day 4–5 — First PR (≈ 12 hours)

Pair with a senior engineer to ship one item from
`docs/internal/onboarding/FIRST-PR-BACKLOG.md`. The list is curated
monthly; every entry is XS-sized (≤ 4 hours from `git checkout` to PR
open) and chosen because it teaches *one specific thing* about the
codebase.

### Standard first-PR flow

1. **Pick** an item with your buddy. Ideally something in the domain
   you'll specialize in (see Week 2).
2. **Branch**: `git checkout -b <handle>/<short-desc>` (no `wt/` prefix
   — that's reserved for orchestrator worktrees).
3. **Implement + test**. Every PR runs:
   - `cargo build --workspace`
   - `cargo test --workspace`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo deny check`
   - `python3 scripts/validate_specs.py`
   - `pre-commit run --all-files`
4. **Self-review** before opening: read your own diff once, run
   `git diff --stat origin/main...HEAD` and ask "would I approve this?".
5. **Open PR** with the template. Reference the backlog item by ID.
6. **Tag** your buddy as reviewer + the domain CODEOWNERS.
7. **/techlead protocol**: orchestrator (or human tech lead) runs
   `/techlead` on your branch; you address comments; merge once
   approval lands and CI is green.

### Mandatory observations (Day 4–5, async OK)

- Attend **one daily standup** (read-only — listen, don't commit to
  work yet).
- Attend **one sprint-close review** if timing aligns. If not, read the
  most recent `specs/04_sprints/SXX/audit_*.md` to see what one looks
  like.

By end of Day 5, you should have **one PR merged to `main`**.

---

## Week 2 — Domain depth

Choose one **primary domain**. You will own work items in this domain
for at least the first quarter; you can rotate later.

| Domain | Path | Mentor (named per quarter — ask your manager) |
|---|---|---|
| **BYOK** | `docs/internal/onboarding/domains/byok.md` | 2 mentors, 1 backup |
| **Audit chain** | `docs/internal/onboarding/domains/audit-chain.md` | 2 mentors, 1 backup |
| **Billing** | `docs/internal/onboarding/domains/billing.md` | 2 mentors, 1 backup |
| **Privacy** | `docs/internal/onboarding/domains/privacy.md` | 3 mentors (DPO + 2 eng) |
| **Ops / SRE** | `docs/internal/onboarding/domains/ops.md` | 2 mentors + on-call rotation lead |

Each domain doc has: 5–10 must-read docs, 3 hands-on exercises, and
2–3 mentors named. Spend Week 2 working through it. Expect ~25 hours.

### Week 2 deliverables

- Domain reading list: completed, with quiz answers committed to your
  onboarding doc.
- 3 domain exercises: done, walked through with mentor.
- Second PR merged — this one *in your domain*, slightly larger than
  Day 4–5 (S-sized, ≤ 1 day effort).
- On-call shadow shift scheduled (begins Week 3).

---

## Days 6–30 — Ramp

**Goals by Day 30:**

- **Shipping:** 1–2 PRs/week independently, in your domain.
- **On-call:** completed one full *shadow* week (paired with primary on-call).
- **Sprint planning:** contributed at least one work-item proposal to
  the next sprint cycle.
- **Reviews:** reviewed ≥ 5 PRs from other engineers.
- **Runbooks:** dry-ran 3 runbooks; updated ≥ 1 with a clarification
  PR.

### Day-30 productivity checkpoint (90 min, manager 1:1)

- **Self-assessment** (you fill out 24 h in advance):
  - Top 3 things you understand well.
  - Top 3 things still fuzzy.
  - What in the onboarding doc was wrong / out of date / missing? (This
    feedback drives the quarterly revision — see below.)
- **Manager 1:1**: discuss the above, agree on Days 30–90 focus
  (deeper domain work, secondary domain exposure, on-call primary
  rotation, etc.).

### What "independent productivity" means

- You can pick a backlog item, scope it, implement it, get it merged,
  and have it not break production — **without** needing a senior to
  unblock you more than once per PR.
- You can take a runbook page and execute it under time pressure
  without freezing.
- You can describe CoreLink to a peer engineer in 5 minutes such that
  *they* could re-explain it after.

---

## Reference — the meta of this codebase

You will encounter all of these eventually. Skim once now, return when
you hit them.

### Repo conventions

- **Trait-abstraction-defer** (`R-charter`): every crate's public API
  starts as a `trait` with an `InMemoryFake` impl; real HTTP/CF/KMS
  wiring lands later. Most crates are still in the fake phase — this
  is intentional, not a bug. Read the trait, not the placeholder body.
- **`#[non_exhaustive]`** on every public enum and `struct` with
  optional fields. If you remove it, CI fails the charter audit.
- **Audit-fail-CLOSED.** Audit-emit happens *before* user-visible
  success. Ordering is enforced by clippy lint + crate-level pattern.
- **PROPTEST_CASES is a runtime function**, not a const. See
  `crates/corelink-test-utils`.
- **No `prop_assert!(matches!(..., Variant { .. }))`** — use struct
  destructuring with explicit field assertions instead.

### Sprint contracts

Every spec sprint (S-00 .. S-20) has a `sprint.md` plus a
`_spec_contract.md`. The contract pins frontmatter conventions,
INV-XXX nomenclature, and SEAL criteria. Read
`specs/_templates/sprint_contract.md` once to understand the shape;
you will not author one in your first quarter.

### `/techlead` protocol

Every PR runs through `/techlead` (an orchestration script + human
review) before merge. The checklist lives at
`docs/internal/TECHLEAD-CHECKLIST.md`. As a new engineer you are *not*
expected to run `/techlead` on your own PRs — your buddy or the
orchestrator does. You *are* expected to read the checklist so you
understand what is being checked.

### TLA+

Eight specifications live in `specs/tla/`. If you are not on the
formal-methods rotation, you only need to know:

- They run in CI on every PR that touches a state machine.
- If yours fails, ping `#corelink-eng` with the failing trace — there
  are 2–3 people in the org who debug these regularly.

### Glossary

50 most-used terms: `docs/internal/onboarding/GLOSSARY-CHEATSHEET.md`.
Bookmark it. You will need it during standup in Week 1.

---

## Feedback loop

This document is regenerated **quarterly** based on the Day-30
checkpoint feedback. Last revision: 2026-05-15. Next revision:
2026-08-15. The owner rotates each quarter — see the frontmatter.

If something here is wrong, out of date, or missing: open a PR. First
PRs *into this document* are an excellent first PR.

---

## Cross-links

- `docs/internal/onboarding/BUDDY-PROTOCOL.md` — what your buddy is
  signed up to do.
- `docs/internal/onboarding/FIRST-PR-BACKLOG.md` — curated first-PR
  list (refreshed quarterly).
- `docs/internal/onboarding/GLOSSARY-CHEATSHEET.md` — 50 terms.
- `docs/internal/onboarding/domains/*.md` — 5 domain learning paths.
- `docs/internal/AUTHOR-PRE-PR-CHECKLIST.md` — **run before opening
  your first PR** (30 rows, ~15 min). Mirrors the gates `/techlead`
  will check.
- `docs/internal/CODE-REVIEW-CHECKLIST.md` — what you walk through
  when reviewing other engineers' PRs (Week-2 deliverable: review ≥ 5
  PRs). Reviewer-side L0-L7.
- `docs/internal/TECHLEAD-CHECKLIST.md` — orchestrator-side L0-L10 PR
  merge protocol.
- `.github/PULL_REQUEST_TEMPLATE.md` — the template auto-loaded when
  you `gh pr create`; pre-populates the author checklist.
- `.github/CODEOWNERS` — review-routing; tells you which team must
  approve a PR touching a given path.
- `ROADMAP-TO-GA.md` — where the product is in its lifecycle.
- `specs/_runbooks/INDEX.md`-equivalent: just `ls specs/_runbooks/`.
