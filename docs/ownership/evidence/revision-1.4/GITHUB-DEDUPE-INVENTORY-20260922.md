# GitHub deduplication inventory — 2026-09-22

This is a read-only decision inventory derived from the authenticated issue
snapshot in [`GITHUB-ISSUE-SNAPSHOT-20260922.md`](GITHUB-ISSUE-SNAPSHOT-20260922.md).
It does not assign `reuse`, `expand` or `distinct`, and it does not authorize
publication. Each row remains `DECISION_REQUIRED` until the canonical backlog,
aliases and issue intent are checked by package.

## Method

The query scanned all 250 issues returned by:

```text
gh issue list --repo HuGR-dev/corelink-server --state all --limit 1000 --json number,url,title,body,state
```

For each provisional package, the issue title and body were matched against
the exact package name or manifest path from `docs/ownership/registry.json`.
This is a discovery signal, not a duplicate proof. A match can be a historical
implementation issue, a broad tracker, a consumer issue or an unrelated text
mention. Ownership markers remain the authoritative duplicate signal for
already-published campaign issues; the snapshot found zero markers.

## Package-level matches

| Package | Matching issues | Decision |
|---|---:|---|
| corelink-adapter-host | #485 | DECISION_REQUIRED |
| corelink-audit | #1647, #1646 | DECISION_REQUIRED |
| corelink-audit-chain | #1647 | DECISION_REQUIRED |
| corelink-billing | #2003, #1639, #1638, #1629 | DECISION_REQUIRED |
| corelink-billing-stripe | #2003, #1639, #1638, #1629 | DECISION_REQUIRED |
| corelink-billing-stripe-materializer | #2003, #1639, #1638, #1629 | DECISION_REQUIRED |
| corelink-byok | #1653 | DECISION_REQUIRED |
| corelink-cli | #1924, #1702 | DECISION_REQUIRED |
| corelink-core | #1626 | DECISION_REQUIRED |
| corelink-gc | #1651 | DECISION_REQUIRED |
| corelink-handler-cas | #1663 | DECISION_REQUIRED |
| corelink-openapi | #1952 | DECISION_REQUIRED |
| corelink-ops | #1644 | DECISION_REQUIRED |
| corelink-pat | #1650 | DECISION_REQUIRED |
| corelink-ratelimit | #1659 | DECISION_REQUIRED |
| corelink-runbook-tracker | #1657 | DECISION_REQUIRED |
| corelink-runner-aggregate | #1630 | DECISION_REQUIRED |
| corelink-runner-overage | #1630 | DECISION_REQUIRED |
| corelink-server | 208 issue hits; high-collision set | DECISION_REQUIRED |
| corelink-signup | #1727 | DECISION_REQUIRED |
| corelink-stripe-real | #1632, #1629 | DECISION_REQUIRED |
| corelink-worker | #1702 | DECISION_REQUIRED |
| e2e-user-journeys | #485 | DECISION_REQUIRED |

The `corelink-server` row is intentionally summarized because its composition
root name appears in a large body of repository-wide migration, audit, CI and
operational issues. Treating those 208 textual hits as ownership duplicates
would be false. The exact result is reproducible from the method above and the
immutable issue snapshot capture.

## Reconciliation boundary

- 23 of 105 provisional packages have at least one package/manifest text hit.
- 82 of 105 have no direct text hit in this issue snapshot, but still require
  the canonical backlog and alias search.
- 0 ownership titles and 0 `corelink-ownership:v1:` markers were observed.
- No row is promoted to `reuse`, `expand` or `distinct` by this inventory.
- The publication ledger therefore remains blocked at the deduplication gate.

