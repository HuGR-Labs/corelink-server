# Issue and backlog deduplication preflight — 2026-09-23

Read-only publication preflight for the 105 prepared package identities. No
GitHub issue or repository data was created, updated, or deleted.

## Source anchors and retrieval

| Evidence | Observed value |
|---|---|
| Campaign checkout | `HEAD=3df52eb71acdd4e00084d42f0b64191674bf42eb` |
| Fetched `origin/main` | `e0f231110524fe81ed0b8d3451903879daf9d519` |
| GitHub repository returned by `gh repo view` | `HuGR-dev/corelink-server`, repository ID `R_kgDOSW9xYw`, default branch `main` |
| Repository visibility returned by GitHub | Public |
| All-state issue query | 307 issues: 92 open, 215 closed; pull requests excluded |

The queried `HuGR-Labs/corelink-server` name resolves to the canonical
`HuGR-dev/corelink-server` repository. This is a current readback, not an
assumption from earlier permissions or snapshots. The paginated read-only query
was `gh api --paginate --slurp -X GET repos/HuGR-Labs/corelink-server/issues -F state=all -F per_page=100`;
`gh issue list -R HuGR-Labs/corelink-server --state all --limit 1000` returned
the same 307 issue count.

## Prepared identity and draft reconciliation

| Check | Result |
|---|---:|
| Registry package records | 105 |
| Publication-ledger records | 105 |
| One-to-one issue drafts | 105/105 |
| Package name, skill slug, manifest marker, and canonical title agree | 105/105 |
| Draft identity mismatches | 0 |
| Ledger records with issue number or URL | 0 |
| Ledger state | 105 `BLOCKED` |

For each registry row, the corresponding ledger record and draft were compared.
Each draft's first line is the exact stable marker
`<!-- corelink-ownership:v1:manifest=<manifest> -->`; its second line uses
`[ownership] <package>: skill, referência, blast radius e manutenção`. Every
package, slug, manifest, marker, and title matched. The prepared identity set
therefore has no mechanical package/draft mismatch.

The ledger's `former_manifests` alias field is empty for all 105 records. The
structured manifest-context file covers 24/105 packages; the remaining 81
drafts still say to complete aliases/targets/features. A separate census lists
631 Cargo targets, but target inventory is not itself a reviewed alias list or
proof that each alias was searched for an existing issue. In particular,
`corelink-cli` has the binary alias `corelink`, and `corelink-server` is the
package in `crates/corelink-container/Cargo.toml`. Alias search is therefore
not certified for all 105 identities.

## GitHub duplicate search

The complete current open/closed issue readback contained:

- exact ownership markers: **0**;
- exact standardized ownership titles: **0**;
- exact package/manifest text-hit packages: **23/105**, same count but not the
  same membership as the earlier R2 lexical census;
- issue records in the existing dedup registry's cited snapshot: **267**,
  versus **307** returned now.

No existing issue is an exact published ownership issue by marker or title.
The additional issue records and selected alias/manifest text hits were
inspected for scope. They are distinct work, not substitutes for the four
ownership deliverables:

| Candidate | Scope read back | Decision for ownership issue |
|---|---|---|
| #2218 | WP-150 workflow-owner/status inventory | `DISTINCT`; workflow ownership is not package ownership documentation |
| #2206, #2139 | Repository-wide OKF manifest/citation coverage | `DISTINCT`; system-wide knowledge maintenance is not a package issue |
| #2153, #2152 | Hosted CI capacity and CLI/SBOM release diagnostics | `DISTINCT`; CI/release work does not deliver the four package artifacts |
| #2237, #1653 | Tenant-bound object-lock archive and BYOK/KMS evidence | `DISTINCT`; separate product/security boundaries |
| #1702 | Repository organization migration and its migration skill | `DISTINCT`; migration work is not a per-package ownership set |
| #1691 | Repository-wide backlog-zero initiative | `DISTINCT`; umbrella tracker, not a package deliverable issue |
| #1724, #1804 | CLI signed provenance and hosted-CI migration; `corelink` alias search | `DISTINCT`; release/runner issues are not package ownership artifacts |

The current 23-package literal-hit set differs from the earlier R2 set by four
entries on each side. New current matches are `corelink-client-verify`,
`corelink-hash`, `corelink-meta`, and `corelink-reapi`; each comes from #2153's
hosted-CI workflow inventory and is `DISTINCT` from the ownership deliverable.
Earlier direct-hit rows `corelink-audit`, `corelink-billing`,
`corelink-billing-stripe`, and `corelink-signup` have no current exact
package/manifest text hit. The earlier semantic decisions remain evidence of
their inspected issues, but their lexical match set is not current. This
membership drift is another reason to refresh all 105 decisions against the
current snapshot. Literal-hit decisions do not establish that every package
alias has been searched.

## Backlog and existing decision records

The current canonical `origin/main:BACKLOG.md` blob is
`2cdabd6fc6972df13b768d2d4f284d16180e75d3`; the campaign checkout blob is
`0da98e05aa60ca236ce57880ad360ad3ae537a31`. Compared with the earlier
deduplication anchor `3445b217` / blob
`1d2dc27c523164b85c07a71aa9c4e1be3adffb04`, the canonical backlog diff is 60
changed lines: B-012's hosted bot-PR evidence/status and B-171's verification
date. Neither change introduces a package-ownership issue. A fresh exact
package/slug/manifest/directory scan against the canonical blob still finds
37/105 packages with literal hits and 68/105 without one. A no-hit remains
unresolved evidence, not a `DISTINCT` decision.

The existing `DEDUPE-DECISION-REGISTRY.json` records **92 `DISTINCT`, 4
`REUSE`, 9 `EXPAND`, and 0 `UNRESOLVED`** against a 267-issue snapshot. Its
23 literal-hit membership is stale relative to the current issue readback. The
four reuse and nine expand entries point to pre-existing functional/backlog
issues; none is an existing ownership-marker issue. Treat these as candidate
backlog reconciliation decisions, not as confirmation that an existing issue
already satisfies the ownership campaign. The registry explicitly says reuse
and expand remain subject to publication-ledger confirmation.

The four recorded `REUSE` rows are `corelink-dpa-acceptance`,
`corelink-dt-webhook`, `corelink-tenant-path`, and `corelink-tier-selection`.
The nine `EXPAND` rows are `corelink-billing-emit`,
`corelink-billing-stripe-traits`, `corelink-chaos-scheduler`, `corelink-dsr`,
`corelink-reapi`, `corelink-rotation-adapters`, `corelink-telemetry`,
`corelink-terraform-drift-consumer`, and `migrate-single-to-multi-region`.

## Decision and blockers

**No duplicate published ownership issue was found** in the current 307-issue
snapshot. No issue reuse is authorized by this preflight. The semantic backlog
registry has 105 recorded decisions, but its GitHub snapshot is stale by 40
issues and its alias coverage is not complete. The publication ledger remains
105/105 `BLOCKED`, with issue numbers empty and publication count zero.

Before an issue can pass preflight, the owner must refresh/ratify the 105
semantic decisions against the 307-issue snapshot, reconcile all actual
package aliases (not just manifest paths or literal package names), bind the
four `REUSE` and nine `EXPAND` candidates to explicit package-level outcomes,
and update the publication ledger only after the shared standard and other
publication gates are valid. This report does not authorize issue creation.
