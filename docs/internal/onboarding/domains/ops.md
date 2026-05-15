# Domain — Ops / SRE (chaos + on-call + DR + runbooks)

> **Estimated effort:** ~30 hours over Week 2 (denser; you will also
> *shadow* an on-call shift in Week 3, which adds 5 days of background
> exposure).
> **Prerequisites:** Day 1–5 complete; comfortable with PagerDuty; have
> read at least one published post-mortem (Cloudflare, AWS, anywhere)
> in the last 12 months.
> **Mentors (rotated quarterly):**
> - Primary: TBD (SRE lead).
> - Secondary: TBD (on-call rotation captain — current quarter).
> - Backup: TBD (chaos / DR engineering).

## Why this domain matters

We promise sub-five-minute response sustained for thirty days as a
precondition of GA. Three on-call regions, weekly synthetic page
drills, monthly chaos exercises, quarterly DR drills. The
[runbook catalog](../../../specs/_runbooks/) is 19 documents and
growing. Ops is where engineering meets contract: SLOs are revenue
commitments.

## Must-read (in order)

1. `specs/03_architecture/observability_model.md` — metric / trace /
   log conventions.
2. `specs/03_architecture/slo_catalog.md` — every SLO we promise.
3. `specs/03_architecture/failure_modes.md` — FM-XXX taxonomy. Pair
   each FM with its runbook (next item).
4. `specs/_runbooks/` (whole directory — 19 RB-* documents). Read the
   first paragraph of each; deep-read 5. Recommended starting 5:
   - `RB-ONCALL-POLICY.md`
   - `RB-SYNTHETIC-PAGE-DRILL.md`
   - `RB-CHAOS-CATALOG.md`
   - `RB-GA-LAUNCH-ROLLBACK.md`
   - `RB-POSTMORTEM-PROCESS.md`
5. `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — who gets paged when.
6. `docs/internal/dt-dr-runbook.md` — disaster recovery procedure.
7. `crates/corelink-chaos-scheduler/src/lib.rs` — how chaos is
   scheduled and bounded.
8. `crates/corelink-dr-drill/src/lib.rs` — the DR drill harness.
9. `crates/corelink-canary/src/lib.rs` — synthetic probes.
10. `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` plus the most recent
    post-mortem in `specs/_postmortems/`.

## Hands-on exercises

1. **Shadow a synthetic-page drill.** Mentors schedule one per week.
   Attend in observer mode. Note every system the on-call touches
   (PagerDuty, Slack, Grafana, runbook, Sentry). Identify any step
   where the on-call had to "remember" something — those are the
   gaps where runbooks need work.
2. **Author a runbook PR.** Pick a small gap: an environment variable
   name that's wrong, a missing escalation contact, a step that
   assumes a permission you don't have. Open a PR. This is your
   second-PR territory.
3. **Read one full post-mortem and write a 1-pager.** Pick the most
   recent `specs/_postmortems/POSTMORTEM-*.md`. Write a 1-pager
   summarizing: what failed, what saved us, what we changed, what
   action items remain. Show to mentor.

## What "comfortable in this domain" looks like by Day 30

- You have shadowed at least one full on-call week.
- You can navigate the SLO dashboards without help.
- You can execute one runbook end-to-end (with mentor in the loop)
  during a drill.
- You have shipped at least one ops-touching PR (runbook update,
  alert tuning, chaos catalog entry, etc.).

## Common pitfalls

- **Runbook drift is the #1 ops bug.** If a runbook step is wrong,
  fix it in the same PR as your post-mortem action item. Don't punt.
- **Don't escalate before reading the page.** Half of pages on
  CoreLink are caused by a transient dependency hiccup; the runbook's
  first step usually says "wait 30s, re-check". Read first.
- **Chaos is scoped.** The chaos scheduler bounds blast radius via
  config; never disable the bound to "see what happens". File a
  proposal in `RB-CHAOS-CATALOG.md` instead.
- **Drata sync failures are not optional.** They are a compliance
  obligation. See `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md`.
