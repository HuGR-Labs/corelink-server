# Cross-functional content review tracker

Per S-18 spec contract §10 anti-scope and waiver policy §19, every public page
under `apps/docs/docs/explanation/compliance/`, `apps/docs/docs/explanation/security/`,
and `apps/docs/docs/pricing/` ships in `draft: true` state with
`cross_functional_review: TBD` frontmatter until each named reviewer has signed
off on the page contents.

The mandatory CI gate that enforces the frontmatter contract is
`apps/docs/tests/cross-functional.test.ts`. Do not remove `draft: true` from a
page until every box for that page is checked here and the corresponding
PR carries an explicit sign-off from each reviewer.

## Reviewer roles

| Role | Scope |
| --- | --- |
| Legal | Every public-facing page; final approver on contractual claims. |
| DPO (Data Protection Officer) | Pages that mention personal data, sub-processors, DPA, or LGPD/GDPR. |
| Security Lead | Pages that describe controls, threat model, audit log, BYOK. |
| Finance | Pages that mention pricing, billing, SLAs that carry credits. |
| Product | Pages that describe feature gating per tier. |
| Procurement | Pages that describe SBOM access, sub-processor change notice flow. |

## Status

Legend: `[ ] pending` · `[x] signed-off` · `[!] blocked`

### Compliance

| Page | Legal | DPO | Security Lead | Notes |
| --- | :---: | :---: | :---: | --- |
| `compliance/index.mdx` | [ ] | [ ] | [ ] | overview — frameworks table |
| `compliance/soc2-timeline.mdx` | [ ] | — | [ ] | dates are TBD |
| `compliance/sbom-access.mdx` | [ ] | — | [ ] | procurement path |
| `compliance/pentest-summary.mdx` | [ ] | — | [ ] | vendor TBD |
| `compliance/dpa.mdx` | [ ] | [ ] | — | DPA download + changelog |
| `compliance/sub-processors.mdx` | [ ] | [ ] | — | mirror of `legal/sub-processors.md` |

### Security

| Page | Legal | Security Lead | Notes |
| --- | :---: | :---: | --- |
| `security/index.mdx` | [ ] | [ ] | overview — controls catalog |
| `security/byok.mdx` | [ ] | [ ] | KMS provider matrix |
| `security/audit-chain.mdx` | [ ] | [ ] | RFC 6962 + 7-year retention |
| `security/responsible-disclosure.mdx` | [ ] | [ ] | reward program $X |
| `security/incident-history.mdx` | [ ] | [ ] | empty grid until first incident |

### Pricing

| Page | Finance | Legal | Product | Notes |
| --- | :---: | :---: | :---: | --- |
| `pricing/index.mdx` | [ ] | [ ] | [ ] | 5 tiers + feature matrix |
| `pricing/calculator.mdx` | [ ] | [ ] | [ ] | interactive widget stub |
| `pricing/comparison.mdx` | [ ] | [ ] | [ ] | facts-only competitor table |

### Sub-processors (cross-role)

| Page | DPO | Procurement | Legal | Notes |
| --- | :---: | :---: | :---: | --- |
| `compliance/sub-processors.mdx` | [ ] | [ ] | [ ] | also tracked under Compliance above |

## How to sign off

1. Read the page in full and confirm every claim is supported by an internal
   source linked in the frontmatter `sources:` block.
2. For Finance: confirm every `$X` placeholder is intentional and the rate
   card has been formally adopted before any placeholder is replaced.
3. For Legal: confirm the page does not over-commit beyond the master
   agreement, DPA, or SLA.
4. For Security Lead: confirm every control claim ties to a control ID in
   `specs/03_architecture/security_model.md`.
5. Open a PR that:
   - Flips the relevant `[ ]` to `[x]` in this file with the reviewer name
     and ISO date in a trailing comment.
   - Removes `draft: true` from the page frontmatter once **all** boxes for
     that page are `[x]`.
6. Merge only after all required reviewers have left an explicit "approve"
   review on the PR.

## Waiver policy

Removing or weakening the cross-functional test
(`apps/docs/tests/cross-functional.test.ts`) requires a formal waiver per the
S-18 spec contract §19 with an expiration of at most 90 days. Drift in this
file (boxes flipped without a corresponding PR sign-off) is treated as an
audit finding.
