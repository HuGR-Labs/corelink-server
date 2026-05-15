---
id: "LIGHTHOUSE-KIT-01-OUTREACH-EMAIL"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "marketing", "outreach", "email", "oss", "enterprise", "byok"]
---

# 01 — Cold Outreach Email Templates

> **Use:** First-touch outreach to candidates from `specs/_lighthouse/recruitment-shortlist.md`.
> **Send from:** Customer Success (variant A) or Founder (variant B).
> **Send window:** D-60..D-45 per program schedule.
> **CRM tracking:** every send + reply recorded in Sales CRM; this repo does NOT store contact details.

---

## Common subject line

```
Subject: CoreLink Lighthouse Program — 6 months free in exchange for case study
```

Alt subject line for warm intros (already met at conference / mutual contact):

```
Subject: Re: [warm intro topic] — CoreLink Lighthouse Program (6 months free)
```

---

## Variant A — OSS Bazel/Buck2-using team (LH-OSS-01)

**Audience:** OSS maintainer / engineering lead of a Bazel, Buck2, or Pants project from the shortlist (e.g. `bazelbuild/bazel-buildfarm`, `bazelbuild/rules_rust`, Buck2-adjacent projects, `tilt-dev/tilt`, `wix/exodus`).

**Tone:** Engineer-to-engineer; respect maintainer time; no marketing fluff.

```
Hi {first_name},

I'm reaching out because {Project} is one of the build-tooling projects in
the Bazel/Buck2 ecosystem we admire most, and I think we can save your
contributors a meaningful chunk of CI minutes.

CoreLink is a managed, content-addressable remote cache built specifically
for Bazel / Buck2 / Pants. It speaks the bazel-remote-cache + Buck2 CAS
protocols natively, ships across 4 regions with active-active failover,
and we are at the tail end of a 60-day GA gate that requires us to recruit
3 lighthouse customers — one of those slots is reserved for an OSS project
that uses Bazel or Buck2 in real CI traffic.

The offer is:
  • 6 months free on our Team tier
  • A dedicated onboarding engineer for the first 30 days
  • A co-authored, customer-approved case study
  • Direct roadmap-influence quarterly calls for the first year

In return we ask for: a signed 30-day SLA attestation, a public testimonial,
and a 60-min interview for the case study (≤ 8 engineering hours total over
the engagement).

Can I grab 30 minutes on your calendar in the next two weeks to walk you
through the architecture and answer the questions you'll inevitably have
about content-addressing semantics, cache eviction policy, and the
multi-region failover model?

Booking link: {CALENDAR_BOOKING_LINK_PLACEHOLDER}
(or just reply with two or three slots that work for you).

Thanks for the time either way,
{sender_name}
Customer Success, CoreLink
{sender_email}
```

### Variant A — short-form fallback (community Discord / DM)

```
Hey {first_name} — I run customer engagement at CoreLink (managed remote
cache for Bazel/Buck2/Pants). We have a 6-month-free lighthouse slot open
for one OSS project in the Bazel ecosystem in exchange for a 30-day SLA
attestation + case study. Worth a 30-min intro? Calendar: {LINK}
```

---

## Variant B — Enterprise BYOK CISO / Head of Platform

**Audience:** CISO, VP Platform, VP Eng, or Head of DevOps at a Series B/C company in fintech, healthtech, AI/ML platforms, or government contracting (per shortlist §3). Decision-maker has hands-on BYOK / KMS exposure.

**Tone:** Compliance-fluent; respect their evaluation rigor; no over-promising.

```
Hi {first_name},

I'm the founder of CoreLink — a managed content-addressable storage and
build-cache service designed for organizations that cannot use vanilla
SaaS object stores because of customer-key-control, audit-chain, or data
residency obligations.

We are 60 days out from GA and recruiting our one Enterprise BYOK
lighthouse customer. The profile we're optimizing for is a security- and
compliance-led platform team where:

  • BYOK (AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault) is a hard
    requirement, not a feature request.
  • Schrems II + EU data-residency posture matters to your DPA review.
  • Audit-chain provenance (Ed25519-signed, append-only) is something your
    auditors will actually look at.

What we offer the lighthouse:

  • 6 months free on the Enterprise tier (full BYOK + DPA amendment + SOC 2
    Type 1 audit kickoff materials + pentest letter on request)
  • Founder-led onboarding (me) + dedicated Customer Success engineer
  • Direct PagerDuty escalation rights during the 30-day observation window
  • Sanitized, NDA-protected case study you co-approve before any external use

In return:

  • Signed 30-day SLA attestation (template available on request, mirrors
    the SLOs measured in our production observability stack)
  • Co-developed reference architecture (sanitized for publication)
  • Up to two reference calls per quarter for the first 12 months

I'd appreciate 30 minutes to walk you through the architecture, the BYOK
key-handling model (including the ≤ 5-minute kill-switch chaos drill we
run weekly), and the DPA / Schrems II posture before you decide whether
your team should engage further.

Booking link: {CALENDAR_BOOKING_LINK_PLACEHOLDER}
(or reply with availability and I'll send a calendar invite directly).

Best,
{founder_name}
Founder, CoreLink
{founder_email}
```

### Variant B — warm-intro variant (mutual contact)

```
Hi {first_name},

{Mutual} suggested I reach out — they thought CoreLink's BYOK +
audit-chain posture would line up well with what your team is building
on the {compliance posture} side.

We're recruiting one Enterprise BYOK lighthouse customer ahead of GA;
the offer is 6 months free, founder-led onboarding, and a co-developed
reference architecture, in exchange for a signed 30-day SLA attestation
and a sanitized case study.

I'd love 30 minutes whenever it works. Calendar: {CALENDAR_BOOKING_LINK_PLACEHOLDER}.

Best,
{founder_name}
```

---

## Send hygiene checklist (per send)

- [ ] Recipient is on `specs/_lighthouse/recruitment-shortlist.md` and the CRM has a row.
- [ ] `{Project}` / `{first_name}` / `{Mutual}` / `{compliance posture}` substituted; no curly braces left.
- [ ] Calendar link present and active.
- [ ] CRM logged with `outreach_sent_at` and variant (`A` / `B`).
- [ ] If recipient already replied to a prior touchpoint, switch to follow-up template (not this cold one).

---

## Reply triage

| Reply class | Owner | Next step |
|---|---|---|
| Interested + books call | Customer Success | Walk variant of `02-intro-deck.md` |
| Interested, no time this month | Customer Success | Soft-warm, re-touch in 4 weeks |
| Not interested | Customer Success | Mark CRM `Declined`; move to backup candidate |
| Out of office / bounce | Customer Success | Re-send in 14d; try alternate channel (LinkedIn / Discord) |
| Forwarded to procurement | Founder (Enterprise) | Send DPA + pricing band + SOC 2 Type 1 attestation kickoff letter |

---

**Fim 01-OUTREACH-EMAIL.**
