---
id: "AUDIT-2026-05-27-INTERVIEW-SCRIPT-READY"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "customer-development", "interview-script", "mom-test", "jtbd", "launch-content"]
---

# Customer Interview Script — Ready-to-Run (20-min Mom-Test + JTBD)

> **Purpose.** Document-ready (not embedded-in-playbook) version of the 20-minute customer-development call. Open this doc on a second monitor while running the call; print or duplicate the script section per interviewee for note-taking.
>
> **Source.** Verbatim conversion of `specs/_audits/2026-05-27-customer-development-playbook.md` §3 (commit `fd1fc6d1` in `main`). For underlying frameworks, Mom-Test rules (§2), and post-cycle analysis instructions, refer back to the source playbook.
>
> **How to use with Granola / Otter.**
> - Start the recorder before the opening line. Granola/Otter both transcribe in real time; the numbered questions below show up as clear timeline anchors in the transcript.
> - For each question, leave the cursor in the blank space below to type 1-3 keywords ("date: last Tue", "tool: sccache + S3", "dollar: ~$1.4k/mo") while the interviewee talks. Do not write full sentences — the auto-transcript handles that.
> - After the call, the timestamped Granola/Otter export pairs with this doc for the 60-minute post-call recap (last section).

---

## §1 — Pre-call checklist (one page, run T-15 minutes)

Do all 12 in order. Skipping any one degrades the data quality of the call.

- [ ] **T-24h.** Confirmation email sent with Cal.com link, expected length (20 min), and the line "no demo, no slides — just your workflow."
- [ ] **T-60min.** Recorder app open (Granola or Otter); consent line drafted ("Mind if I record for my own notes? I won't share it.").
- [ ] **T-30min.** Their GitHub org open in a tab (do NOT screenshare). Skim the last 30 commits, the `.bazelrc` / `.github/workflows/` / `Dockerfile`s, any public benchmark or CI postmortem.
- [ ] **T-30min.** Their LinkedIn open in a second tab — tenure at the company, prior roles, last 3 posts. Used only for natural reference, never quoted.
- [ ] **T-15min.** This script open on a second monitor.
- [ ] **T-15min.** Paper notepad on the desk (optics — "I'm taking notes" feels less surveilling than typing).
- [ ] **T-10min.** Water within reach. You will talk less than 30% of 20 min; dehydration shows up as filler words you cannot afford.
- [ ] **T-10min.** Phone on Do Not Disturb. Slack, email, all notifications muted on the laptop.
- [ ] **T-10min.** Camera on, light source in front of you (not behind), background neutral.
- [ ] **T-5min.** Re-read the 5 Mom-Test rules below. Out loud is better.
- [ ] **T-2min.** Join the call link, mute mic, wait for them.
- [ ] **T-0.** Open with the script line in §3.

**The 5 rules to re-read at T-5 (Fitzpatrick + Moesta synthesis):**

1. Talk about THEIR life, not your idea. The word "CoreLink" does not appear before minute 18.
2. Past behavior, not hypotheticals. "Walk me through the last time..." beats "Would you...".
3. Listen more than you talk. Hard cap: founder talks <= 30% of the 20 minutes.
4. Anchor every pain in time + money + frequency. "Annoying" is not data.
5. A compliment is a red flag. Only signals: calendar opened, colleague intro'd mid-call, "how do I install it right now?".

---

## §2 — Hard rules during the call (do not break)

- Never say "CoreLink" before minute 18.
- Never demo, never slide, never screenshare your product.
- If they compliment your idea, route it to a separate "fluff" column in your notes — never count as validation.
- If you finish the call and you talked > 7 minutes, the data is contaminated — flag the call in the post-call recap and weight it 0.5x in your scoring.
- Anchor every pain mention with one of: "How often?" / "How long did it cost you?" / "What did you do instead?" / "Who else got pulled in?"
- If they mention a competing tool (BuildBuddy, EngFlow, sccache, Bazel Remote Cache, Nx Cloud, Turborepo, Depot), immediately ask Moesta's killer question: **"Why was today the day?"**

---

## §3 — The script (verbatim, 20 minutes)

### Opening (60 sec) — set the frame

> "Thanks for the 20 minutes. Quick context: I'm not selling anything today and I'm not going to demo anything. I'm researching how teams like yours handle [build / CI / Docker / ML — pick the one matching their stack]. The most useful thing you can do is talk about your current workflow, not react to my ideas. Cool?"

Wait for a verbal "cool" or "yeah". This single line raises data quality 2x by giving them permission to be candid.

---

### Block A — Problem validation (5 questions, ~8 min)

**Goal:** confirm the pain exists in their past behavior.

---

**Q1.** "Walk me through the last time you waited on a build, a CI run, or a Docker rebuild that felt way too slow. What were you doing? When was it?"

*Listen for: date, frequency, who else was blocked, what they were trying to ship.*

Notes:




---

**Q2.** "How do you handle build / CI / image caching today? What's set up?"

*Listen for: tool names (sccache, Bazel RBE, Nx Cloud, Turborepo, GitHub Actions cache, S3 hack, nothing). Tool names = real adoption.*

Notes:




---

**Q3.** "When that setup breaks or under-performs, what happens? Walk me through the most recent time."

*Listen for: workaround behavior. Workarounds = unmet need.*

Notes:




---

**Q4.** "Who on your team complains about this most? Has anyone tried to fix it?"

*Listen for: internal champions and failed projects. Failed internal projects = market signal.*

Notes:




---

**Q5.** "If you could wave a wand and have one thing change about your build / CI / image pipeline, what would it be — and why that specifically?"

*The "why that specifically" is mandatory; it converts a wish into a job-to-be-done.*

Notes:




---

### Block B — Pain quantification (5 questions, ~8 min)

**Goal:** dollarize the pain. JTBD Switch Forces (Moesta) woven in.

---

**Q6 (PUSH).** "On a typical week, how many engineering-hours get burned waiting on builds / CI / image rebuilds across your team? How did you arrive at that number?"

*Push to math: team size x wait minutes x frequency. If they refuse to estimate, the pain is not top-of-mind.*

Notes:




---

**Q7 (PULL).** "Last time your team evaluated a cache or build-acceleration tool — what triggered the evaluation, and what did you end up doing?"

*Mom-Test gold: past evaluation behavior = real intent. "We never evaluated" = no pull yet; deprioritize.*

*If they name a competitor adopted recently, follow up immediately:* **"Why was today the day?"**

Notes:




---

**Q8 (ANXIETY).** "If you imagined swapping your current cache setup for something new tomorrow, what would your team be nervous about?"

*Listen for: security / SOC2, lock-in, migration cost, "who maintains it." These are objection inputs for the cold-outreach pack.*

Notes:




---

**Q9 (HABIT).** "What would have to be true for you to stop using [their current tool / homegrown hack]?"

*Habit = strongest force blocking switch per Moesta. If they can't articulate a trigger, they won't switch.*

Notes:




---

**Q10 (COST).** "What's the rough monthly spend on CI minutes + cache infra today? And what's the unspoken cost — slow PR reviews, devs context-switching, on-call ML retrains?"

*Aim for two numbers: line-item infra $ AND fully-loaded engineering opportunity cost.*

Notes:

Line-item $/mo: ____________

Opportunity $/mo: ____________




---

### Block C — Closing (3 minutes)

**Goal:** extract referral + ONE willingness-to-pay signal. NO pitch.

---

**Q11 (REFERRAL — ALWAYS ASK, NON-NEGOTIABLE).** "Who else on your team — or at another company — runs into this same problem and would be worth me talking to? Mind doing a quick intro?"

*Best validation metric: do they volunteer 2+ names unprompted?*

Notes:

Name 1: ____________  intro promised? Y / N

Name 2: ____________  intro promised? Y / N




---

**Q12 (WILLINGNESS-TO-PAY, framed for past behavior).** "Has your team ever paid for a build / CI / cache tool? What was the approval process — who signed, what was the budget line?"

*NOT "would you pay $X for CoreLink." Past purchase behavior + budget owner identity is the only real signal.*

Notes:

Tool paid for: ____________

Approver: ____________

Budget line: ____________




---

**Q13 (COMMITMENT ESCALATOR — only if Q11 and Q12 both went well).** "If I had something to show you in 3 weeks that addressed [exact pain they described in Q3], would you give me 30 minutes to try it against your real workload?"

*A "yes + calendar invite right now" is the commitment. A "sure, ping me" is a polite no — log as such.*

Result (circle): YES-with-calendar / yes-vague / no /  N/A (Q11-Q12 didn't warrant ask)

---

**Close.** "This was incredibly useful. I'll email you a 5-bullet summary of what I heard so you can correct me if I misunderstood. Thanks for the time."

---

## §4 — Post-call recap template (60-minute hygiene)

**Deadline.** Complete all 5 sub-tasks within 60 minutes of hang-up. After 60 minutes, recall accuracy drops > 50% (Fitzpatrick / standard interview-research finding).

### §4.1 — Quantitative score (1-5 each, /25 total)

| Dimension | Score (1-5) | One-line evidence |
|---|---|---|
| Problem severity | __ | |
| Problem frequency | __ | |
| Dollar pain (quantified Q6 + Q10) | __ | |
| Budget authority (Q12) | __ | |
| Switch willingness (Q9 + Q13) | __ | |
| **Total** | **__ / 25** | |

Interpretation: **>= 18 / 25** = strong P1 fit; pursue design-partner offer. **12-17** = warm but not urgent; nurture via the cold-outreach pack 3-touch sequence. **< 12** = deprioritize, do not re-engage for 90 days.

### §4.2 — JTBD Forces grid (one sentence each, quote verbatim where possible)

- **Push** (what's wrong with their current life — from Q1, Q3, Q6):




- **Pull** (what's attracting them — from Q5, Q7):




- **Anxiety** (what they're afraid of about switching — from Q8):




- **Habit** (what's keeping them where they are — from Q9):




### §4.3 — Compliment column (the "fluff" bucket — never validation)

Log compliments here so they do not contaminate the scoring above:

- 
- 
- 

### §4.4 — Action queue (do within today)

- [ ] **Send recap email within 60 min** — use the Post-Interview Recap template in `specs/_audits/2026-05-27-cold-outreach-pack.md` §6.
- [ ] **If Q11 yielded a name** — send the intro-request email TODAY (latency kills referrals). Template in `2026-05-27-cold-outreach-pack.md` §2.
- [ ] **If Q13 = YES-with-calendar** — confirm the 3-week follow-up invite is on the calendar before EOD.
- [ ] **Log to HubSpot Free / CRM** — fields: company, name, score /25, JTBD grid, next-action-date, referral name(s).
- [ ] **Flag contaminated calls.** If founder talked > 7 min, weight this call 0.5x in the aggregate scoring spreadsheet and note why.

### §4.5 — Sanity check before closing the doc

- [ ] Did I avoid saying "CoreLink" before minute 18? (If no, flag the call — frame was broken.)
- [ ] Did I anchor every pain mention with a time/money/frequency follow-up?
- [ ] Did I ask Q11 (referral)? (Non-negotiable — if missed, send a follow-up email today asking it directly.)
- [ ] Did I avoid demo / slides / screenshare? (If no, flag the call — data is contaminated.)

---

## §5 — Cycle-level aggregation (run after every 6 interviews)

This belongs in the source playbook (`2026-05-27-customer-development-playbook.md` §6 Week 3-4 mid-cycle decision gate at Day 22), but the per-call data feeds it from here. Once 6 calls have been scored:

- Distribution of /25 scores. If < 3 of 6 score 18+, pause outreach and re-scope ICP.
- Top 3 recurring pain themes across JTBD Forces grids (verbatim quotes preferred).
- Top 3 recurring anxieties (Q8) — these become objection-handling content for the cold-outreach pack and landing-page FAQ.
- Top 3 referral names received but not yet contacted — clear that queue before booking more first-touches.

---

**END OF SCRIPT DOC.** For the underlying frameworks, the full 60-day plan, hard rules, and tooling stack, see the source playbook at `specs/_audits/2026-05-27-customer-development-playbook.md` (commit `fd1fc6d1`).
