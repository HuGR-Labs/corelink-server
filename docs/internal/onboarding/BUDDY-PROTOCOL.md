# Buddy Mentor Protocol

> **Audience:** the senior or staff engineer assigned as a new hire's
> "buddy" for Weeks 1–4.
> **Owner:** Engineering Manager (assigns buddies).
> **Recognition:** buddy effort is logged and surfaced in the next
> perf review cycle as IC4+ glue contribution. The Engineering Manager
> handles this — buddies don't self-report.

## Why this exists

CoreLink has a lot of internal jargon (262 spec docs, ~96 crates, 21
sealed sprints). A new engineer can read forever and still not know
*who to ask about X*. The buddy's job is to compress that gap.

The buddy is **not** the new hire's manager, code reviewer of record,
or domain mentor. Those are separate roles. The buddy is the person
the new hire can ask "this is probably a stupid question, but…" without
hesitation.

## Time commitment

| Phase | Cadence | Duration |
|---|---|---|
| Week 1 | Daily | 30 min/day |
| Week 2 | Twice/week | 30 min/session |
| Weeks 3–4 | Once/week | 30 min/session |
| Week 5+ | Async; on-demand. New hire pings buddy first when stuck before escalating to manager. |

Total ~10–12 hours over the four weeks. Block it on your calendar
before the new hire starts.

## What the buddy covers

### Week 1

- **Day 1 (60-min 1:1):** introduce yourself, walk the repo top-level,
  draw the architecture on a whiteboard once (you'll redraw it many
  times — that's fine). Hand off the GLOSSARY-CHEATSHEET. Confirm
  laptop / access / Slack channels work.
- **Day 2 (30 min):** review the new hire's self-quiz answers from
  the architecture deep-dive. Correct misconceptions. Note which
  topics they're confident on — those are *not* areas to deep-dive
  later.
- **Day 3 (30 min):** review the request-trace sketch from Day 3
  hands-on. Add the crates they missed. Point at one runbook for
  Week 2 mentor pairing.
- **Day 4–5 (pair on first PR):** sit with them while they open the
  PR. Don't write code for them — answer questions and unblock
  tooling friction (slow CI, wrong env var, missing IAM permission).

### Week 2

- One session at start of week: review domain choice; introduce
  domain mentors.
- One session at end of week: review domain reading list + 3
  exercises. Walk through what surprised them.

### Weeks 3–4

- Weekly check-in. New hire drives the agenda (typically: "things I'm
  confused about" + "what's worth my time next").
- You attend their on-call shadow shift in observer mode for at least
  one page (helps you spot gaps in their runbook fluency).

### Week 5+

- Async. Be reachable on Slack DM. Ping them once at Day 60 to ask
  how they're doing (the manager will ask formally; you ask
  informally — different signal).

## Translation duties (always-on)

The codebase is full of internal shorthand. Translate when you hear
the new hire stumble on:

- **`R-charter`** → "the workspace-wide engineering constraints; see
  the Reference section of ENGINEERING-ONBOARDING.md."
- **`SEAL APPROVED`** → "the sprint spec is locked; treat the doc as
  authoritative."
- **`InMemoryFake`** → "this crate's real impl isn't wired yet; read
  the trait, ignore the body."
- **`/techlead`** → "the PR review protocol; orchestrator runs it
  before merge."
- **`P0 / P1`** → "P0 = drop everything; P1 = ship within the sprint."

If the new hire hears a term they don't recognize twice in 24h, add
it to GLOSSARY-CHEATSHEET. That's a great first PR for *you* on their
behalf (or coach them to do it).

## What the buddy does *not* do

- **Code review of record.** The CODEOWNERS file handles this; you
  may *also* review but it's not your job.
- **Performance feedback.** That's the manager's lane.
- **Domain teaching.** That's the domain mentor's lane (see the
  domain docs).
- **On-call coverage.** You're not their backup; the rotation
  captain is.

## Buddy rotation

Each new hire gets a **different** buddy from the previous hire. This
intentionally builds network breadth — by the time you've hired 5
engineers, 5 different seniors have onboarded someone, and the org
has 5 newly-strengthened cross-team bonds. The Engineering Manager
maintains the rotation list.

A senior should not buddy more than once per quarter. If you've just
finished, you're off the next cycle.

## Buddy → Engineering Manager handoff

At Day 30 (the productivity checkpoint), the buddy sends the manager
a 5-bullet async note:

- What the new hire picked up fast.
- What they still struggle with.
- What in the onboarding doc / glossary was wrong or missing.
- Who else they should be exposed to next (domain, region, function).
- Recommended Days 30–90 focus.

The manager uses this to drive the Day-30 1:1. The new hire never
sees the note directly — its purpose is to give the manager prep
material, not to grade the new hire.

## Recognition

- Logged automatically by the manager in the buddy's perf-cycle
  inputs as "glue work — engineering onboarding for <name>".
- Surfaced in promotion packets as IC4+ contribution evidence.
- Pizza / coffee / equivalent at the end of each rotation, charged
  to the engineering team budget.

If you've been asked to buddy and weren't expecting recognition for
it — flag it to the manager. We don't run this on goodwill alone.
