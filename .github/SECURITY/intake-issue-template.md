---
name: "Security finding — internal tracking"
about: "Private internal tracking issue for an inbound security report. DO NOT use for public issues."
title: "[CL-VULN-YYYY-NNN] <component> — <one-line summary>"
labels: ["security-finding"]
assignees: []
---

> **PRIVATE.** This template is for **private GitHub Security Advisory
> drafts only** (Repository → Security → Advisories → New draft). It MUST
> NOT be used to open a regular public issue.
>
> Companion runbook: `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md`.

## Tracking ID

`CL-VULN-YYYY-NNN`

## Reporter

- Name / handle:
- Organisation:
- Contact:
- Wants public credit? (yes / no / anonymous)

## Receipt timestamp

`YYYY-MM-DDTHH:MM:SSZ` (UTC) — used to start the SLA clock.

## Channel of receipt

- [ ] `security@corelink.dev` email
- [ ] `/.well-known/security-report` web form
- [ ] CERT/CC ticket
- [ ] GHSA from another vendor
- [ ] Other: _____________

## Original report

Encrypted archive S3 URL: `s3://corelink-security-archive/CL-VULN-YYYY-NNN/`

Do **not** paste the raw report into the issue body — keep PII / exploit
details in the encrypted archive.

## Triage

### Severity

- CVSS v3.1 vector: `CVSS:3.1/AV:?/AC:?/PR:?/UI:?/S:?/C:?/I:?/A:?`
- CVSS Base score: `?.?`
- Severity bucket: CRITICAL / HIGH / MEDIUM / LOW / INFO
- Auto-escalation triggered?
  - [ ] Tenant-isolation violation (`INV-TenantIsolation`) — forces CRITICAL
  - [ ] Exploited in the wild — forces CRITICAL + §5 path

### Scope

- [ ] In scope per VDP-001 §2.1
- [ ] NOT in §7 out-of-scope list

### Affected versions

```
>= ?, < ?
```

### Affected components

- [ ] Data plane
- [ ] Control plane
- [ ] Admin UI
- [ ] Rust crates (specify): _____________
- [ ] Go SDK
- [ ] TS SDK
- [ ] Helm chart
- [ ] Terraform
- [ ] Container images (specify): _____________
- [ ] Docs site
- [ ] Other: _____________

## SLA clocks

| Clock                  | Target           | Actual           |
| ---------------------- | ---------------- | ---------------- |
| Acknowledge receipt    | per VDP-001 §4   | `YYYY-MM-DD HH:MM` |
| Full triage complete   | per VDP-001 §4   | `YYYY-MM-DD HH:MM` |
| Fix released           | per VDP-001 §4   | `YYYY-MM-DD HH:MM` |
| Public advisory        | fix + window     | `YYYY-MM-DD HH:MM` |

## Owners

- Security Lead:
- Eng lead (component):
- AppSec advisor:
- Legal (HIGH/CRITICAL):

## Linked artefacts

- Email thread (S3): `s3://corelink-security-archive/CL-VULN-YYYY-NNN/`
- Slack channel: `#sec-CL-VULN-YYYY-NNN`
- Fix branch: `security/CL-VULN-YYYY-NNN`
- Fix PR: `#____` (private fork or branch-protected security/* branch)
- GHSA draft: `GHSA-____-____-____`
- CVE: `CVE-YYYY-NNNNN`
- Hall of Fame entry: `apps/docs/docs/explanation/security/hall-of-fame.mdx`
- `CHANGELOG.md` entry: line `____`

## Post-disclosure follow-ups

- [ ] Hall of Fame updated
- [ ] Reporter notified of disclosure (template `T-DISCLOSE`)
- [ ] Reward processed (swag / bounty)
- [ ] SAST rule added (if applicable): `tools/sast/rules/_____.yml`
- [ ] Regression test added: `tests/____.{rs,ts,go}`
- [ ] SECURITY-MODEL updated if a new CTRL-* was needed
- [ ] Post-mortem filed (CRITICAL only): `specs/_postmortems/____.md`
