# Launch Day Runbook

> **DRAFT — pending Marketing + CEO + SRE + PR sign-off.**
> Trace: WI-S20-008 §2.1.6 · spec contract S-20 §13 (D+30 GA gate decision) · CAP-LAUNCH-001.
> Launch principle: Engineering gate is binary. Launch orchestration runs alongside and does not unblock the engineering decision. CEO/Founder enforces the gate.

---

## T-7d — Engineering gate precondition checks

| Time | Action | Owner | Pass criterion |
|---|---|---|---|
| T-7d | **Pentest letter received** (zero HIGH/CRITICAL pending, retest passed) — `EVT-040`. | Security Lead | Letter on file. |
| T-7d | **SOC 2 readiness confirmed** — gap analysis delivered, GAP-XX list with fix timeline — `EVT-031`. | Compliance Officer | Document on file. |
| T-7d | **Lighthouse SLA attestations on file** — 3 customers attesting 30d SLA met — `EVT-018`. | Customer Success | 3 attestations signed. |
| T-7d | **PRR globally approved** — 13 canonical sign-offs (or 5/8 minimum per ADR-0034 Option C). | Owner | PRR-S20-GA approved. |
| T-7d | **30d sustained staging** confirmed — zero SEV-1, < 3 SEV-2 not resolved — `EVT-021`. | SRE Lead | Dashboard verified. |
| T-7d | **TLA+ all 4 specs green in CI** — `EVT-022`. | Engineer | CI dashboard verified. |
| T-7d | **SBOM CycloneDX 1.5+ signed, published** — `EVT-010`. | Engineer | SBOM published. |
| T-7d | **24/7 on-call PagerDuty schedule live** — synthetic page < 5 min sustained — `EVT-026`. | SRE Lead | Schedule verified. |
| T-7d | **Engineering gate decision: APPROVED or DEFER.** Binary. CEO/Founder enforces. | CEO/Founder | APPROVED → proceed to T-3d. |

**If any precondition fails:** defer launch. Engineering gate is unappealable. Marketing reschedules; engineering does not retreat.

## T-3d — Press kit distribution under embargo

| Time | Action | Owner |
|---|---|---|
| T-3d | **Press kit packaged**: PRESS-RELEASE.md (final, Legal-cleared), executive bios, hi-res logos, hero image, lighthouse-customer permission-cleared quotes, trust-center pointer pack. | PR + Marketing |
| T-3d | **Embargoed distribution** to journalist list (tech press: TechCrunch, The Register, Ars Technica; ecosystem: Bazel community newsletter, Buck2 community, RBE community channels). | PR firm (or Owner dual-hat per WI-S20-008 §6.2 NP6) |
| T-3d | **Embargo language**: tied to Engineering Gate D-day + 24h. NO distribution until embargo lift signal. | PR |
| T-3d | **Hunter confirmation** for Product Hunt — final asset bundle delivery scheduled. | Marketing |

## T-1d — Dry run

| Time | Action | Owner |
|---|---|---|
| T-1d 09:00 PT | **System load dry-run**: synthetic load against staging mirroring expected launch-day signup spike + cache traffic surge. | SRE |
| T-1d 11:00 PT | **Dry-run review** — identify scaling adjustments or oncall posture changes. | SRE + Engineer |
| T-1d 14:00 PT | **Incident response readiness check**: PagerDuty on-call confirmed, escalation matrix verified, SEV-1 runbooks sweep. | SRE Lead |
| T-1d 16:00 PT | **Final asset review** — press release, blog 01, LinkedIn, Twitter thread, Show HN body, PH maker comment cross-checked against canonical sources. | Marketing + Owner |
| T-1d 18:00 PT | **Final go/no-go check.** Engineering gate still APPROVED? On-call still staffed? No new SEV-1 in staging? **GO** signal or **DEFER**. | CEO/Founder |
| T-1d 20:00 PT | **Hunter receives final bundle.** | Marketing |
| T-1d 22:00 PT | **Hunter confirms scheduled PH post for 00:01 PT.** | Hunter + Marketing |

## T-0 — Launch day

### Pre-dawn (Pacific)

| Time | Action | Owner |
|---|---|---|
| T-0 00:01 PT | **Product Hunt: GO LIVE.** Hunter posts CoreLink. | Hunter |
| T-0 00:05 PT | **Maker opening comment** posted (per `PRODUCT-HUNT/PH-MAKER-COMMENT.md`). | CEO/Founder |
| T-0 00:15 PT | Internal ambassadors (HuGR team) genuine engagement only — no coordinated upvoting. | Marketing |
| T-0 01:00 PT | First-hour PH rank check. | Marketing |

### Morning (Pacific)

| Time | Action | Owner |
|---|---|---|
| T-0 06:00 PT | **Press release wire: BusinessWire** (primary) — embargo lifts. PR Newswire (secondary) goes at T+2h. | PR firm |
| T-0 06:00 PT | **Blog post 01 publishes** on `corelink-docs.humangr.com/blog`. | Marketing |
| T-0 06:00 PT | **Trust center embargoed assets** unlocked: TLA+ specs, SBOM, pentest summary letter (NDA-gated), DPA package. | Marketing + Trust Engineering |
| T-0 06:30 PT | **Inbound press monitoring** begins. PR firm fields journalist follow-ups. | PR firm |

### Mid-morning (Pacific)

| Time | Action | Owner |
|---|---|---|
| T-0 09:00 PT | **CEO LinkedIn post** publishes (`SOCIAL/LINKEDIN-POST.md`). | CEO |
| T-0 09:00 PT | **Twitter / X launch thread** posts (`SOCIAL/TWITTER-THREAD.md`). | Marketing |
| T-0 10:00 PT | **Hacker News Show HN** submission (`SOCIAL/HACKERNEWS-SHOW-HN.md`). Single submission, no resubmission. | CEO/Founder |
| T-0 10:00..12:00 PT | **HN thread response window** — Founder + Engineer primary responders. | CEO/Founder + Engineer |

### Mid-day (Pacific)

| Time | Action | Owner |
|---|---|---|
| T-0 12:00 PT | **Mid-day posture review** — PH rank, HN rank, signup conversion, system load, incident posture. | Marketing + SRE |
| T-0 12:00 PT | **Incident-response escalation check**: any SEV-1 in CoreLink production? On-call manager confirms posture. SEV-1 → escalation to CTO + Owner; PH activity continues with transparent Maker comment acknowledging the incident if appropriate. | SRE Lead |
| T-0 15:00 PT | **Comment-tree sweep #2** on Product Hunt + Hacker News. Maker / Founder responds to substantive comments. | CEO/Founder |
| T-0 17:00 PT | **Press response sweep** — handle inbound journalist follow-ups. | PR firm |

### Evening (Pacific)

| Time | Action | Owner |
|---|---|---|
| T-0 21:00 PT | **Day-1 close metrics capture**: PH final rank, HN final rank, press pickups, signup count, Lighthouse score, social engagement. | Marketing |
| T-0 22:00 PT | **Day-1 retrospective stand-up** (15 min, async-friendly). | Marketing + Owner |

## T+1d — Post-launch

| Time | Action | Owner |
|---|---|---|
| T+1d 09:00 PT | **Media response coordination** — PR firm fields ongoing journalist inquiries. Embargo formally over. | PR firm |
| T+1d 12:00 PT | **Maker "thank you" comment** on PH with day-1 numbers; answers stragglers in thread. | CEO/Founder |
| T+1d 14:00 PT | **Inbound enterprise sales triage** — Sales picks up the BYOK enterprise inbound from press + lighthouse-case-study visibility. | Sales |
| T+1d 17:00 PT | **Twitter follow-up thread** with day-1 numbers (if PH / HN rank warrants amplification). | Marketing |

## T+7d — Retrospective + metrics

| Time | Action | Owner |
|---|---|---|
| T+7d 10:00 PT | **Launch retrospective** — what worked, what didn't, what we change for the next launch (Phase 2). | Marketing + Owner |
| T+7d 12:00 PT | **Metrics dashboard finalization** (`METRICS-DASHBOARD.md`) — capture: PR pickups, blog UVs, PH rank trajectory, HN rank trajectory, Show HN comment quality, signup conversion, Lighthouse score, social share velocity, inbound enterprise pipeline. | Marketing |
| T+7d 14:00 PT | **Engineering / launch separation post-mortem** — confirm the engineering gate held, marketing pressure did not contaminate the engineering decision, and the two-gate model remained intact through the launch day. Document for future launches. | Owner |

## Failure-mode matrix (launch-day)

| Failure mode | Detection | Mitigation |
|---|---|---|
| Engineering gate not APPROVED at T-7d | Precondition check fails. | Defer launch. Engineering gate is unappealable. Reschedule per CEO/Founder decision. |
| Embargo break by journalist | PR firm monitors wire + tech press feeds. | Wire goes early. Blog 01 publishes early. Adjust LinkedIn / Twitter / PH timing accordingly. Maker comment notes the early publication. |
| PH hunter unavailable T-0 | Hunter no-confirm at T-1d 22:00 PT. | Fallback hunter from shortlist. If no fallback → CEO/Founder self-launches. |
| SEV-1 in CoreLink production during launch window | PagerDuty page + dashboard alarm. | On-call manager runs incident response. CTO + Owner notified. PH / HN activity continues with transparent Maker / Founder acknowledgment. Engineering gate is binary — we do not retract GA over a SEV-1 unless scope warrants it (Owner decision). |
| Coordinated bad-faith comment campaign on PH or HN | Comment-tree sweep. | Single polite, factual Maker response per bad-faith thread. No engagement spiral. Report to PH / HN moderators if guideline-violating. |
| Press release Legal review reopens at T-3d | Legal team flags. | Pause distribution. Iterate Legal language. If un-resolvable in 48h → defer launch per engineering-gate-first principle. |
| Wire service distribution failure | PR firm monitor. | Failover to secondary wire (PR Newswire). |
| Lighthouse customer revokes quote at T-3d | Customer Success notifies. | Pull quote from press release. Use placeholder framing. Blog series unchanged (case studies are separate DRAFT artifacts). |

## Roles

| Role | Owner | Backup |
|---|---|---|
| Launch-day commander | CEO/Founder (Gustavo) | Engineer (S-20 lead) |
| Engineering gate enforcer | CEO/Founder | Owner / Final Approver (dual-hat per ADR-0034 Option A) |
| Incident response | SRE Lead | On-call manager |
| Press inbound | PR firm | Owner (dual-hat fallback per WI-S20-008 §6.2 NP6) |
| Maker / Hunter coordination | Marketing | CEO/Founder |
| Comment-tree response | CEO/Founder | Engineer (technical questions) |
| Compliance posture | Compliance Officer | Privacy Officer |
| Sales inbound (post-PH visibility) | Sales | Customer Success |

---

## Internal notes

- Times in Pacific (PT) to anchor against Product Hunt's 00:01 PT launch-day reset.
- Engineering gate enforcement is the single non-negotiable principle; everything else flexes.
- No competitor-specific contingencies; we do not plan around competitor activity.
