# Cold review, wave 2: shared ownership standard — 2026-09-23

**Verdict: FIX_FIRST.** The contract is coherent in its separation of structural
checks, independent review and publication, but two fail-closed claims have
counterexamples. Do not freeze these bytes as the approved standard yet.

## Scope and independent checks

Read the current `STANDARD.md`, package-record schema, `ownership_gate.py`,
`generate_registry.py`, the relevant ownership tests and
`STANDARD-CONTRACT-REPAIR-20260923.md`. Inspected `check_docs.py` and
`publication_gate.py` only where the reviewed tools delegate validation.
This verdict does not inherit an earlier review.

Reviewed SHA-256: standard `12c4e3a4f9c4`, schema `2a43de660cad`,
ownership gate `abbb67e6be7b`, registry generator `e70097f9945a`.

- `python3 -m unittest discover -s docs/ownership/tests -q`: 143 tests, OK.
- `git diff --check` on the reviewed standard, schema, tools and tests: pass.
- Read-only `generate_registry.build` on this checkout: 105 packages; all
  `UNVERIFIED`, all `NOT_PUBLISHED`, `calibration=PASS`, no calibration errors.
- The calibration checker recomputes all four line/word/UTF-8-byte triples and
  compares the reference profile for each of five pilots. It checks that
  population, overflow and capacity-decision cells are present; their semantic
  accuracy remains a reviewer responsibility. No real package record exists in
  this checkout, so the positive record path has only fixture coverage.

## Required fixes

1. **Metadata-only comparison can discard an unpermitted change.**
   `ownership_gate.py:111-116` removes *every* indented allowed-key line from
   skill frontmatter, regardless of its parent mapping. Two otherwise valid
   synthetic skill documents can change both `metadata.package` and
   `extra.package`; `check_docs.validate` returns no errors for either, while
   `metadata_only_delta` returns `{'package'}`. The second change is outside
   the four permitted metadata fields, contrary to `STANDARD.md` §9.3's
   remaining-byte equality rule. Restrict removal to the exact four entries
   under `metadata`, or reject unknown frontmatter mappings before applying
   this exception. Add a regression that changes an identically named key
   outside `metadata` and requires rejection.

2. **A contradictory issue identity can be labeled published.**
   `generate_registry.py:128-138` accepts `record_readback`'s identity and
   compares its returned number and URL separately to the ledger.
   `publication_gate.py:108-112` checks the issue URL's form but does not
   require its `/issues/<number>` suffix to equal `issue.number`. An in-memory
   readback with `number=7` and URL ending `/issues/8` is accepted; a ledger
   repeating both values then satisfies the generator and can yield
   `PUBLISHED`. Enforce number/URL agreement before promotion and cover the
   mismatch in a registry test.

The ordinary exact-hash path, explicit `semantic_review`/`metadata_readback`
schema fields, reviewed snapshot hash checks, missing-record `UNVERIFIED`,
missing-readback `READBACK_REQUIRED`, and current-byte calibration comparison
are sound within their stated evidence-consistency scope. The two findings
above prevent a fail-closed standard freeze.
