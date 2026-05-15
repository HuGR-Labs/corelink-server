# GitHub Security Advisory — template

Use this template when promoting a private GHSA draft to public, or when
filing a CVE with MITRE if we have not yet been assigned as a CNA.

> **Where this template lives:** `.github/SECURITY/advisory-template.md`.
> **Companion runbook:** `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md` §C.2.
> **Policy:** `specs/_security/vulnerability-disclosure-policy.md`.

---

## Title

`[CVE-YYYY-NNNNN] <Component>: <One-line description of the issue>`

Keep the title under 80 characters. Front-load the component name so the
advisory list is easy to scan.

## Ecosystem

Pick the appropriate ecosystem(s) so dependency scanners pick it up:

- `rust` / `cargo` — for Rust crates
- `go` — for Go SDK
- `npm` — for TypeScript SDK
- `docker` — for container images
- (mixed) — list each affected ecosystem separately if needed

## Severity

CVSS v3.1 Base Score + vector string. Use
<https://www.first.org/cvss/calculator/3.1>. Map to one of:

- CRITICAL (9.0 – 10.0)
- HIGH (7.0 – 8.9)
- MEDIUM (4.0 – 6.9)
- LOW (0.1 – 3.9)

## Affected versions

```
>= X.Y.Z, < A.B.C
```

List ALL affected version ranges. Be precise — overly broad ranges
trigger false-positive scanner alerts for unaffected users.

## Patched versions

```
A.B.C
```

The first version containing the fix. Backports to older supported
minor lines (per `SECURITY.md` §"Supported Versions") get their own entry.

## Workarounds

A user-runnable mitigation that does not require upgrading. Examples:

- Set environment variable `XYZ=safe`.
- Block path `/admin/legacy` at the WAF until the upgrade.
- Disable feature flag `experimental.foo`.

If no workaround exists, say so explicitly.

## Description

```markdown
## Summary
One paragraph: what is the vulnerability, what is the impact, who is
affected. Plain language — assume a security-aware engineer, not a
specialist in this codebase.

## Details
Technical detail: root cause, attack vector, prerequisites. Include code
references with permalinks to the **pre-fix** commit.

Be specific enough that downstream maintainers can verify the patch but
do not publish a working exploit until the embargo lifts entirely.

## Impact
Who is impacted, under what configuration, what is the worst-case
outcome. Map to the CIA triad explicitly.

## Patches
What changed. Link to the PR / commit that fixed it.

## References
- Discoverer: <name or "Anonymous">
- Original report: CL-VULN-YYYY-NNN (internal)
- Related: link to relevant SECURITY-MODEL CTRL-* if any
- External advisories: e.g., upstream library advisory if relevant

## Acknowledgements
Thanks to <researcher> for the responsible disclosure. Hall of Fame:
https://corelink.dev/security/hall-of-fame
```

## CWE

Pick at least one CWE ID from <https://cwe.mitre.org/>. Common picks for
our space:

- CWE-200 — Information Exposure
- CWE-285 — Improper Authorization
- CWE-287 — Improper Authentication
- CWE-352 — CSRF
- CWE-78 — OS Command Injection
- CWE-94 — Code Injection
- CWE-918 — SSRF
- CWE-79 — XSS
- CWE-89 — SQL Injection
- CWE-639 — Authorization Bypass Through User-Controlled Key
- CWE-345 — Insufficient Verification of Data Authenticity

## Reviewer checklist (before publishing)

- [ ] Title is descriptive and under 80 chars.
- [ ] CVSS vector is correct and severity bucket matches.
- [ ] Affected + patched version ranges have been **manually verified**
      (don't trust GitHub's autosuggested ranges).
- [ ] At least one workaround is documented (or absence is explicit).
- [ ] CWE is set.
- [ ] All linked commits are public at the time of publish.
- [ ] Hall of Fame entry queued (or reporter declined credit).
- [ ] `CHANGELOG.md` entry includes the GHSA link.
- [ ] Customer Success has confirmed pre-notification window expired
      (for CRITICAL hosted impact).
- [ ] Legal has reviewed the text (for HIGH/CRITICAL).
- [ ] Marketing has blog draft ready (for HIGH/CRITICAL).
