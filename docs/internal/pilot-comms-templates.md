# CoreLink Pilot Communications Templates

> **Audience:** CS team operating the wave-23+ pilot cohort.
>
> **Status:** living. Edit freely; PRs welcome. Keep the voice
> human — these are not marketing emails. Each template should
> read like a senior engineer writing to a peer.
>
> **Companion:** `docs/internal/customer-success-playbook.md`
> — defines when each template fires.
>
> **Voice rules:**
>
> 1. Plain text only (no HTML wrappers). Slack-friendly.
> 2. First-person singular from the CS owner — not "the team."
> 3. Subject line ≤ 60 chars; body ≤ 200 words where possible.
> 4. Always include a concrete next action with a date.
> 5. Never use the word "delighted." Never use the word "journey."

---

## §1 Pilot welcome (week 0)

**When:** within 1 business hour of contract execution.
**Channel:** email + Slack Connect pin.
**Owner:** CS owner assigned to the tenant.

**Subject:** Welcome to the CoreLink pilot — your activation link

**Body:**

Hi {first_name},

Welcome aboard. Your CoreLink pilot is live. Three things to get
you to your first cache write today:

1. **Activation link** (single-use, expires in 72 hours):
   {activation_url}
2. **Quickstart guide** (15-minute read; the S3-compatible client
   section is the one most pilots use):
   {quickstart_url}
3. **Shared Slack channel** for fast questions:
   {slack_connect_url}

Your tenant ID is `{tenant_id}` and your residency region is
`{region}`. Both are pinned in the Slack channel.

Goal for this week: land your first CAS write within 24 hours and
your first audit export within 7 days. I will check in on day 3
and day 7 regardless — but feel free to pull me in earlier.

Kickoff call is on {kickoff_date} at {kickoff_time}. Calendar
invite incoming.

— {cs_owner_name}, Customer Success at HuGR
{cs_owner_email} · {cs_owner_pager}

---

## §2 Onboarding nudge (day 3 if no first blob)

**When:** day 3 after activation, if `cas.write.success` count
for this tenant is still 0.
**Channel:** Slack first (low-friction). If no Slack reply in 4h,
follow up via email.
**Owner:** CS owner.

**Subject:** Quick check — anything blocking your first CoreLink write?

**Body:**

Hi {first_name},

Three days in and I don't see a CAS write from `{tenant_id}` yet —
which is totally normal at this stage, but I want to make sure
we're not stuck on something I could unblock fast.

Common day-3 friction points:

- **Endpoint URL mismatch** (residency-specific URLs are not
  interchangeable — your URL is `{region_endpoint_url}`).
- **Credential rotation pending** (the initial admin credentials
  need a rotate-on-first-use; let me know if you'd like me to walk
  through it).
- **CI/CD wiring waiting on an internal approval** (no rush — just
  helpful for me to know).

Want to grab 20 minutes today or tomorrow for a screen-share? I
can usually unblock CI wiring inside that window.

— {cs_owner_name}

---

## §3 First-week success check (day 7)

**When:** day 7 from activation. Send regardless of status — the
metric we care about here is *engagement*, not just throughput.
**Channel:** email.
**Owner:** CS owner.

**Subject:** Week-1 check-in — how is CoreLink fitting in?

**Body:**

Hi {first_name},

Quick week-1 recap from my side:

- First CAS write: {first_write_status} ({first_write_timestamp})
- First audit export: {audit_export_status}
- Tickets opened this week: {ticket_count} ({ticket_summary})
- p95 latency (last 24h, your tenant): {p95_latency_ms} ms
- Availability (last 7d): {availability_pct}%

Three questions, no wrong answers:

1. What surprised you (good or bad) about the integration?
2. Is there any workload pattern we should be testing this week
   that we aren't?
3. Anything in the docs that wasted your time? (I file these as
   `docs-unclear` tickets — they're our top priority.)

The day-15 mid-pilot review is on {day15_date}. Same agenda as
this email, just longer.

— {cs_owner_name}

---

## §4 Mid-pilot review (day 15)

**When:** day 15 from activation. Always a scheduled call (30
min). This template is the *prep email* sent 24h ahead.
**Channel:** email + calendar invite.
**Owner:** CS owner; product lead optionally attends.

**Subject:** Mid-pilot review tomorrow — pre-read

**Body:**

Hi {first_name},

Quick pre-read for our 30-minute mid-pilot review tomorrow at
{review_time}. No homework — but if you want to skim, here is the
state of the pilot as of today:

**Usage (last 15 days)**
- CAS storage: {cas_gb} GB
- Audit events: {audit_count}
- p95 latency: {p95_latency_ms} ms (against SLO {p95_slo_ms} ms)
- Availability: {availability_pct}%
- SLO breaches: {slo_breach_count}

**Engagement**
- Tickets opened: {ticket_count} ({ticket_open}/{ticket_closed})
- Support escalations: {escalation_count}

**Conversion-criteria status (preview of day-30):**
{criteria_traffic_light_table}

**Agenda for tomorrow:**
1. Walk through the metrics above (10 min).
2. Pain points / blockers from your side (10 min).
3. Conversion-criteria deltas + extension question if relevant (5 min).
4. One ask from us: a 1-question NPS so we can baseline (5 min).

— {cs_owner_name}

---

## §5 SLO breach alert (incident-triggered)

**When:** within 15 minutes of an SLO breach attributable to
CoreLink-side cause affecting this tenant. Send proactively — do
NOT wait for the tenant to notice.
**Channel:** Slack (immediate) + email (formal record).
**Owner:** on-call SRE drafts; CS owner sends.

**Subject:** Heads-up: CoreLink SLO breach affecting {tenant_id} — we're on it

**Body:**

Hi {first_name},

A note rather than a polished postmortem — we're still in the
incident. Here is what I can share now:

- **What happened:** {one_sentence_summary}
- **Detected at:** {detection_timestamp} UTC
- **Your tenant impact:** {tenant_specific_impact} (e.g., elevated
  p99 latency on CAS reads, partial audit emit lag, etc.)
- **Current status:** {mitigation_status} (e.g., mitigation
  deployed, monitoring; or: investigating)
- **Next update from me:** within {next_update_window_minutes}
  minutes.

The on-call SRE is {sre_name}. The incident ID for your records
is `{incident_id}`. A full postmortem will land in your Slack
channel within 5 business days, regardless of severity.

If you want a sync call right now, reply with a thumbs-up and I'll
spin one up in 5 minutes.

— {cs_owner_name}

---

## §6 Pre-GA conversion offer (day 25)

**When:** day 25 from activation. Send only if the pilot is on
track for conversion (≥ 8 of 10 criteria green at day 25).
**Channel:** email with attached order form.
**Owner:** CS owner; CC product lead.

**Subject:** Your CoreLink GA conversion offer — order form attached

**Body:**

Hi {first_name},

You're tracking for GA conversion on {day30_date}, and I want to
get the paperwork in your hands early so nothing slips on the last
day.

Attached: the GA order form for STANDARD tier at the published
price ({price_url}). Three things worth flagging:

- **Your pilot tenant ID, residency, and BYOK posture all carry
  forward** — no migration, no re-onboarding. The flip happens at
  midnight UTC on {day31_date}.
- **The 100 GB pilot cap is removed at GA.** Your projected steady
  state of {projected_steady_state_gb} GB will incur an estimated
  monthly cost of {projected_monthly_usd} USD at the current price.
- **SLA kicks in at GA** ({sla_url}). Pilot had SLOs but no
  credits; STANDARD gets credits per the MSA.

Day-30 review is on {day30_date} at {day30_time} — we'll walk the
conversion criteria together and either flip the switch or scope
an extension.

If anything in the order form looks off, ping me — happy to
redline.

— {cs_owner_name}

---

## §7 Post-pilot retro request (day 35)

**When:** day 35 (5 days after either GA conversion or
termination). The retro is *always* offered, regardless of
conversion outcome.
**Channel:** email.
**Owner:** CS owner; product lead joins the call.

**Subject:** 30-minute retro on your CoreLink pilot — would you?

**Body:**

Hi {first_name},

Whether the pilot ended in GA conversion or off-boarding, we learn
the most from a 30-minute retro a week after the dust settles.
This is the single highest-leverage thing you can do to make
CoreLink better for the next 10 tenants.

The retro is blameless and unrecorded. We ask four questions:

1. What worked about the pilot?
2. What didn't?
3. Where did we waste your time?
4. Would you recommend CoreLink to a peer — and if not, what
   would have to change?

I'd suggest one of: {three_proposed_slots}. Or send a slot that
works — I'll bend the calendar.

If you'd rather write than talk, I'll happily take a written reply
to those four questions and skip the call.

— {cs_owner_name}

---

## §8 Pilot termination notice (if fails conversion)

**When:** within 1 business day of the termination trigger (see
playbook §7). Always paired with a phone call — do not send this
cold.
**Channel:** email (formal record); phone call must precede.
**Owner:** CS owner; product lead CC'd.

**Subject:** CoreLink pilot — off-boarding plan and next steps

**Body:**

Hi {first_name},

Per our conversation today, we're closing the pilot for
`{tenant_id}` on {offboard_date} (14 calendar days from this
email). This was not the outcome either of us wanted, and I want
to make the off-ramp as clean as possible.

**What happens on {offboard_date}:**

- Tenant flips to read-only at midnight UTC on {readonly_date}
  (7 days before off-boarding) so you can finalize any exports.
- Full CAS bundle export available at no cost. Reply to this email
  to request; I'll have it ready within 24 hours.
- Full audit log export (CSV + JSON, NDJSON) at no cost. Same
  request flow.
- Hard delete of tenant data on {hard_delete_date} ({retention_days}
  days post-offboarding), except audit logs retained per regulatory
  minimums in cold storage.
- BYOK material destroyed on {offboard_date} per our BYOK runbook.

**Retro:** I'd like to do a 30-minute retro within 7 days of
off-boarding. Blameless, unrecorded. Four questions, same as every
pilot — what worked, what didn't, where we wasted your time, would
you recommend us anyway. Optional but I would deeply appreciate it.

Anything in the off-boarding plan you want adjusted, reply or
ping me on Slack. Otherwise this is the plan.

Thank you for trying us. Genuinely.

— {cs_owner_name}
