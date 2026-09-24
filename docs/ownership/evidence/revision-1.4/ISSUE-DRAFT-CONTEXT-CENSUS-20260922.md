# Issue-draft context census — 2026-09-22

The 105 issue drafts were read through the publication ledger's `body_path`
references. All 105 paths exist and match their package/manifest identities.
The first marker-only probe was too narrow: it looked only for English labels
and undercounted Portuguese/context-specific sections. It is superseded by the
section-aware census below.

- Draft paths present: **105/105**.
- Identity readback: **105/105** (see `ISSUE-DRAFT-IDENTITY-READBACK-20260922.md`).
- Structured manifest-context section (`Contexto extraído do manifesto`):
  **24/105**.
- Explicit dependency-key section (`Chaves de dependência inspecionadas`):
  **20/105**.
- Remaining drafts: **81/105** retain the identity/seed shape and still need
  deeper package-specific hydration.
- Interpretation: the 24/105 figure is a preparation-context measure, not a
  completeness or approval claim; the four package artifacts may contain richer
  facts than their issue seed.

The context gate remains `PENDING`; each seed still requires a complete,
package-specific set of entrypoints, targets/features, direct dependencies,
known consumers/re-exports, OKF links, risk surfaces, commands and explicit
unknowns before publication.
