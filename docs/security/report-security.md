# Report a security vulnerability

> **Doc kind:** customer-facing security disclosure policy (source-of-truth).
> The Docusaurus mirror is published at `/security/report-security` via
> `apps/docs/docs/explanation/security/report-security.mdx`. The
> legally-binding text is at
> `specs/_security/vulnerability-disclosure-policy.md`.
>
> **Owner:** Security Lead (`security@corelink.dev`).
> **Last updated:** 2026-05-16 (wave-29 stream-8, Trust Center publish prep).

## TL;DR

| Item | Value |
|---|---|
| **Where to send** | `security@corelink.dev` |
| **PGP key fingerprint** | `5C2C 8E2A 6B17 9F4D 1A0B  3E9C 6F8B 2D7E 4A91 0C15` (published at `/.well-known/security.txt`) |
| **Response SLA — acknowledgement** | 1 business day |
| **Response SLA — triage decision** | 5 business days |
| **Coordinated disclosure window** | 90 days from triage acceptance (extendable by mutual agreement) |
| **Safe-harbor scope** | All `*.corelink.dev` domains + open-source CoreLink crates and CLIs |
| **Bounty program** | Not yet active — recognition only pre-GA; monetary rewards roll out at GA + 30 days |

## How to report

1. **Send an email to `security@corelink.dev`** with subject prefix
   `[VULN-REPORT]`. Encrypt the body with our PGP key (fingerprint above;
   key material at `/.well-known/security.txt` and on `keys.openpgp.org`).
2. **Include enough detail to reproduce**: affected endpoint or component,
   request payload, expected vs observed behaviour, screenshots / video if
   relevant. Proof-of-concept code is welcome but not required.
3. **Tell us your preferred attribution**: handle for the Hall of Thanks
   (post-GA), or "anonymous". We will never publish a researcher's identity
   without consent.
4. **Tell us your geography**: so we can route to the on-call rotation that
   covers your timezone.

## What you can expect from us

- **Acknowledgement within 1 business day** confirming receipt and assigning
  a tracking ID (`VULN-YYYY-NNNN`).
- **Triage decision within 5 business days**: we will tell you whether we
  consider the report in-scope and what severity we have provisionally
  assigned (CVSS v3.1, with rationale).
- **Fix-or-mitigate cadence aligned to severity**:
  - CVSS 9.0+ (Critical): hotfix within 24 hours.
  - CVSS 7.0-8.9 (High): hotfix within 7 days.
  - CVSS 4.0-6.9 (Medium): patch within 30 days.
  - CVSS < 4.0 (Low): patch within 90 days or next scheduled release.
- **Public disclosure** at the end of the 90-day window (or earlier if both
  parties agree), coordinated with a CVE assignment where applicable.
- **No legal action** against good-faith researchers who comply with this
  policy — see Safe Harbor below.

## What we ask from you

1. **Do not disclose publicly** until the coordinated-disclosure window
   closes or we have agreed an early-release date.
2. **Do not exfiltrate customer data**. If your proof-of-concept happens to
   surface another tenant's data, stop immediately and notify us.
3. **Do not run automated scans against `*.corelink.dev` production**
   without our written authorisation. We monitor those endpoints; aggressive
   scanning may cause real customer impact.
4. **Do not pivot from a low-severity finding into a higher-severity one**
   without checking in with us first.
5. **Do not test on customer tenants you do not own or have explicit
   written permission to test.**

## Safe harbor

If you make a good-faith effort to comply with this policy:

- We will not pursue or support legal action against you (DMCA, CFAA, or
  equivalent foreign statutes).
- We will treat your research as authorised under the Computer Fraud and
  Abuse Act (18 U.S.C. §1030) and any analogous laws.
- We will work with you to resolve any inadvertent violations rather than
  pursuing enforcement.

This safe-harbor language is grounded in the legally-binding VDP at
`specs/_security/vulnerability-disclosure-policy.md` §3. If you operate in
a jurisdiction not covered by the laws above, please flag it in your
initial report so we can confirm coverage with our counsel.

## Scope

### In-scope

- All `*.corelink.dev` subdomains (`app.`, `api.`, `docs.`, `status.`,
  `cdn.`).
- All open-source crates published from
  `github.com/humangr-labs/corelink-server` (CLI, SDKs, REAPI shim).
- The `corelink-cli` distributable.
- The CoreLink Cloudflare Worker entrypoints (Bazel / Buck2 / Pants
  remote-cache REAPI).

### Out-of-scope

- Third-party sub-processors (report directly to vendor — list at
  `/trust/sub-processor-register`).
- Social engineering of CoreLink staff.
- Physical attacks against CoreLink infrastructure or staff.
- Denial-of-service attacks (we will not reward proof of capability).
- Findings only reachable with an out-of-date browser or pre-EOL OS.
- Vulnerabilities in third-party dependencies that are already publicly
  disclosed — file an issue against the upstream project and let us know
  separately if you want credit for the supply-chain notice.

## Bounty program

Pre-GA (today): recognition only. Researchers who submit valid reports
will be acknowledged in the public Hall of Thanks at GA launch +30 days.

GA + 30 days (planned): monetary rewards on the following scale, subject
to final Finance sign-off and Owner+Security-Lead 2-key authorisation
(ADR-0034b):

| Severity | Reward range |
|---|---|
| Critical (CVSS 9.0+) | USD 2,500 - USD 10,000 |
| High (CVSS 7.0-8.9) | USD 1,000 - USD 2,500 |
| Medium (CVSS 4.0-6.9) | USD 250 - USD 1,000 |
| Low (CVSS < 4.0) | Recognition + swag |

Rewards are USD-denominated, paid via wire transfer or to a charity of the
researcher's choice. Sanctions screening applies (OFAC / EU / UK lists).

## Hall of thanks

The Hall of Thanks roster will be published at GA launch +30 days at
`/security/hall-of-thanks` (post-GA route). Pre-GA reports are queued and
will be honoured at that page's launch.

## Contact

- **Email**: `security@corelink.dev`
- **PGP**: fingerprint above, key at `/.well-known/security.txt`
- **Tor mirror**: `corelink-vdp.onion` (post-GA)
- **Bug-bounty intake form**: `corelink.dev/security/report` (post-GA — pre-GA, use email)

## References

- `specs/_security/vulnerability-disclosure-policy.md` — legally-binding VDP.
- `SECURITY.md` (repo root) — RFC 9116 alignment + maintainer-facing notes.
- `/.well-known/security.txt` — RFC 9116 machine-readable contact card.
- `/security/policy` — customer-facing security policy (CVSS matrix +
  out-of-scope items).
- `/trust` — consolidated Trust Center landing.

---

**Disclosure window**: 90 days from triage acceptance. **Last updated**:
2026-05-16. **Owner**: Security Lead (`security@corelink.dev`).
