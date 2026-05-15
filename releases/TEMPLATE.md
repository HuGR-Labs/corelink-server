---
id: "RELEASE-TEMPLATE"
type: "release_notes_template"
doc_status: "TEMPLATE"
audit_status: "ACTIVE"
version: "0.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
parent: "CHANGELOG.md"
tags:
  - "release-notes"
  - "template"
  - "customer-facing"
source_refs:
  - "scripts/generate-release-notes.py"
  - "marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md"
---

<!--
  CoreLink release-notes template — handcrafted polish layer on top of the
  auto-generator output (`scripts/generate-release-notes.py`).

  Workflow (see marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md):
    1. CI runs on `v*` tag push → produces releases/RELEASE-<version>.md
       and opens an editorial PR.
    2. Operator copies this TEMPLATE structure on top of the auto-output,
       moving auto-bullets into the right narrative bucket.
    3. Operator hand-writes prose only in the marked NEEDS-MANUAL sections.
    4. Operator merges the PR; CI re-runs nothing; the draft GitHub Release
       body is updated by hand from the polished markdown.

  Do NOT add content that is not derivable from:
    - git log between two refs,
    - CHANGELOG.md,
    - specs/_audits/*debt-register*.md,
    - openapi/** diff,
    - docs/customer/MIGRATION-<version>.md (if present).

  Marketing flourish goes in the launch blog post, not here.
-->

# CoreLink {{VERSION}} — Release Notes

> **Status:** PUBLISHED · **Date:** {{ISO-DATE}} · **Engineering gate:** {{tag-name}}
> Authoritative changelog: [`CHANGELOG.md`](../CHANGELOG.md). This document is
> the customer-facing prose layer; engineering detail lives in the changelog.

## TL;DR

<!-- NEEDS-MANUAL — 3 sentences. Customer-visible "what & why", no jargon.
     Pulled by auto-gen from CHANGELOG <version> heading if present; otherwise
     hand-write here. Aim: a developer-tools reader can decide in 15 seconds
     whether to upgrade. -->

1.
2.
3.

## What's new

<!-- AUTO-OK (from feat:* commits + customer-visible-labeled PRs). Edit only
     to consolidate bullets into capability-level sentences. Keep the
     parenthetical sha for traceability. -->

-

## Performance

<!-- AUTO-OK (from perf:* commits with measured deltas). Add a one-line
     summary at the top if there is a headline gain. Surface numbers exactly
     as measured; never round upward. -->

-

## Security

<!-- AUTO-OK (from sec/security:* commits + fix(sec*) + CVE refs). NEEDS-MANUAL
     for any pentest finding closures: cite the report ID + remediation date.
     If no security changes, leave the "_No security fixes_" auto sentence.
     -->

-

## Compliance

<!-- AUTO-OK (from debt-register CLOSED rows in window). NEEDS-MANUAL if any
     SOC2 / ISO27001 / GDPR control passed an external review in this
     release window — name the framework, the control ID, and the auditor
     ack (no marketing fluff). -->

-

## Breaking changes

<!-- AUTO-OK (BREAKING CHANGE: footers + OpenAPI deprecations). NEEDS-MANUAL
     to write a "Why this change?" sentence per breaking item — customers
     budget against this section first. -->

-

## Migration guide

<!-- AUTO link to docs/customer/MIGRATION-<version>.md if present. NEEDS-MANUAL
     when there is no migration doc but a breaking change exists — write the
     steps here inline. -->

See [`docs/customer/MIGRATION-{{VERSION}}.md`](../docs/customer/MIGRATION-{{VERSION}}.md).

## Acknowledgments

<!-- AUTO-OK (external author emails from git log). NEEDS-MANUAL to verify
     names + add lighthouse-customer thanks where consented (per Marketing
     opt-in list). -->

-

---

## Cross-references

- [`CHANGELOG.md`](../CHANGELOG.md) — authoritative engineering changelog.
- [`ROADMAP-TO-GA.md`](../ROADMAP-TO-GA.md) §8 — R-8 launch wave.
- [`marketing/launch/LAUNCH-CHECKLIST-V2.md`](../marketing/launch/LAUNCH-CHECKLIST-V2.md) — T-24h .. T+72h launch checklist.
- [`marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md`](../marketing/launch/RELEASE-NOTES-EDITORIAL-GUIDE.md) — editorial workflow.

<!-- End of TEMPLATE. Auto-generator preserves this footer block. -->
