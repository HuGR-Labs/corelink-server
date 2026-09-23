# Standard/pilot review handling — 2026-09-22

The mechanical integrity repair changed 63 artifact frontmatters across 37
packages. A diff audit confirms that every change is confined to administrative
frontmatter: package/manifest identity, source pin and evidence-set metadata.
No body, heading, contract, invariant, relation, procedure or command changed.

The previous blanket invalidation treatment was too broad. Per the owner's
direction, substantive cold-review findings remain valid for the unchanged
document bodies. The repaired metadata receives a scoped readback of identity,
source pin and evidence-set consistency; a full cold rereview is required only
when semantic document content changes.

This record is a scope correction and metadata readback requirement, not a cold
review and not an approval.

## Required next review

1. Reconcile package, manifest, source pin and evidence-set metadata.
2. Preserve existing semantic cold-review findings for unchanged bodies.
3. Obtain full independent rereview only for artifacts with semantic changes.
4. Re-review any artifact whose body, contract, relation or procedure changes.

The registry now reports **105/105 artifact-integrity PASS**. The registry's
`UNVERIFIED` value is a publication-ledger state, not a claim that all prior
semantic review work vanished; publication remains zero until the standard and
package-level review ledger are reconciled.
