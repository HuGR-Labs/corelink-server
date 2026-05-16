# Product Hunt — Launch Day Plan

> **DRAFT — pending Marketing + Hunter coordination + Maker comment review.**
> Trace: WI-S20-008 §2.1.4 · spec contract S-20 §5.2 R-S20-8.
> Launch window: **D+25 .. D+30** (Maker outreach 5-day pre-launch window per Product Hunt convention).

---

## 0. Launch principle

Launch on Product Hunt is a marketing artifact, not the GA decision. The GA decision is the engineering gate. Product Hunt launch can shift its date without affecting engineering readiness. CEO/Founder enforces the gate.

## 1. Launch day timeline (Pacific Time)

| Time | Action | Owner |
|---|---|---|
| **T-1d 18:00 PT** | Final go/no-go check against engineering gate. If engineering gate not APPROVED → defer PH launch. | CEO/Founder |
| **T-1d 20:00 PT** | Hunter receives final asset bundle (hero image, tagline, description, demo video link, maker comment, FAQ). | Marketing |
| **T-1d 22:00 PT** | Hunter confirms scheduled post for 00:01 PT. | Hunter + Marketing |
| **T-0 00:01 PT** | **GO LIVE.** Hunter posts CoreLink to Product Hunt. | Hunter |
| T-0 00:05 PT | Maker leaves opening Maker comment (see `PH-MAKER-COMMENT.md`). | CEO/Founder (Maker) |
| T-0 00:15 PT | Internal ambassadors (HuGR team) upvote + comment. No fake engagement. Genuine team commentary only. | Marketing |
| T-0 01:00 PT | First-hour rank check. Adjust comment-tree response cadence. | Marketing |
| T-0 06:00 PT | Press release wire (BusinessWire) — embargoed asset goes live. Blog post 01 ("Introducing CoreLink") publishes on `corelink.humangr.com/blog`. | PR + Marketing |
| T-0 09:00 PT | CEO LinkedIn announcement (see `SOCIAL/LINKEDIN-POST.md`). Twitter launch thread (`SOCIAL/TWITTER-THREAD.md`). | CEO + Marketing |
| T-0 10:00 PT | Hacker News Show HN post (see `SOCIAL/HACKERNEWS-SHOW-HN.md`). Single submission, no resubmission, no vote manipulation. | CEO/Founder |
| T-0 12:00 PT | Mid-day rank check + incident-response posture review. SEV-1 in CoreLink production → CTO escalation; PH activity continues but Maker comment flags the issue transparently. | SRE + Marketing |
| T-0 15:00 PT | Comment-tree response sweep #2. Maker responds to every substantive top-level comment. | CEO/Founder |
| T-0 21:00 PT | Daily rank close. Capture final rank, comment count, upvote count. | Marketing |
| **T+1d** | Media response coordination — handle inbound press inquiries from PH visibility. | PR |
| **T+1d** | Maker writes a "thank you" follow-up comment with day-1 numbers and answers stragglers. | CEO/Founder |
| **T+7d** | Retrospective: PH rank trajectory, comment quality, signup conversion attributed to PH, SEV-X incidents during launch window. | Marketing + SRE |

## 2. Hunter coordination

Product Hunt convention recommends a Hunter with a track record of credible technical launches. Selection criteria:

- Demonstrated history of launching developer-infrastructure products.
- Genuine interest / engagement with the product (no transactional hunters).
- Capacity to post at the target launch time without timezone friction.

Hunter outreach scheduled D+25..D+30 per WI-S20-008 §2.1.4. Candidate hunter list maintained in `PRODUCT-HUNT/hunter-shortlist.md` (private, not for repo distribution).

## 3. Ambassador / Maker outreach

The Maker outreach list (50 makers + 30 reviewers + 20 ecosystem) is maintained separately for sales / privacy reasons and is not committed to the repo. The outreach contract is:

- 5-day pre-launch window for warm outreach.
- No "please upvote my launch" asks. The ask is: "we are launching on D+30, here is the launch page, here is the demo, we would value your honest feedback in the comment thread."
- Maker outreach to Bazel / Buck2 / RBE ecosystem maintainers (rules_rust, buf, tilt, build-without-the-bytes contributors, etc.) prioritized for technical credibility.

## 4. Comment-tree response plan

Every substantive top-level comment in the first 24 hours gets a Maker response. Response cadence:

- **First hour:** Maker present, responding live, ≤ 15-minute reply latency.
- **Hours 2–6:** Maker present, ≤ 30-minute reply latency.
- **Hours 6–24:** Maker swept every 2 hours.

Response posture:

- Technical questions → direct, sourced answer (link to docs / spec / trust center).
- Comparison questions ("how does this differ from `[competitor]`?") → facts about CoreLink, no competitor disparagement.
- Compliance / trust questions → trust center pointer + one-line plain-language answer.
- Hostile / bad-faith comments → polite, factual, single response. No engagement spiral.

## 5. Failure modes

- **Low first-hour rank.** Acceptable; PH ranking is noisy. Plan does not change.
- **Hunter unavailable T-0.** Fallback hunter list in `hunter-shortlist.md`. If no hunter available → CEO/Founder self-launches (lower ceiling but acceptable).
- **CoreLink production SEV-1 during launch window.** PH activity continues with a transparent Maker comment acknowledging the incident; incident response per `runbooks/`. Engineering gate is binary — we do not retract GA over a SEV-1 unless the incident scope warrants it.
- **PR firm not contracted.** Fallback: Owner + Final Approver dual-hat self-PR per WI-S20-008 §6.2 NP6.
- **Embargo break by journalist.** PR firm coordinates response. Wire goes early if necessary. Adjust blog post 01 publish time accordingly.

## 6. Metrics

See `METRICS-DASHBOARD.md` for the full instrumentation plan. PH-specific metrics:

- Hour-by-hour rank trajectory.
- Upvote count.
- Comment count (Maker + non-Maker).
- Click-through to `corelink.humangr.com` from PH.
- Signup conversion attributed to PH referrer.

## 7. Anti-patterns we will not do

- No paid upvote services.
- No coordinated team upvoting that masquerades as organic engagement.
- No competitor disparagement in comment threads.
- No false claims about features or compliance posture.
- No retraction of substantiated claims under bad-faith pressure.

---

## Internal notes

- This plan is launch-day-specific. Pre-launch warm-up plan handled inline in the runbook (`COORDINATION/LAUNCH-RUNBOOK.md`).
- All times Pacific to match Product Hunt's launch-day reset convention (00:01 PT).
