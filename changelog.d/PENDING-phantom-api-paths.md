### Fixed

- **13 of the 40 paths in the published API contract were served by nothing, and
  the gate that measures exactly this was green.** `scripts/validate_api_surface.py`
  reported 14 `MISSING_ROUTE` + 33 `MISSING_DOC` = 47 raw divergences, and all 47
  sat in its ledger under B-116/117/119/120/121 — so the check passed while
  `scripts/gen-api-reference.py` published 15 EN reference pages (mirrored as MT
  stubs in pt-BR / es-419 / de: **60 pages total**) with copy-pasteable curl,
  Rust, Python, Go and JS snippets aimed at endpoints that 404. Ten paths were
  deleted; three were repointed at what is actually served — `/v1/audit/export`
  → `/v1/audit/{tenant}/export`, `/v1/dpa/accept` → `/v1/onboarding/dpa-accept`,
  and the `/v1/pats` family → `/v1/customer/keys` — each with its schema
  rewritten from the handler rather than kept as invented. The contract is now
  30 paths, 0 `MISSING_ROUTE`.
- **The `MISSING_ROUTE` direction is no longer ledgerable.** A published path
  nothing serves is a customer-facing lie, not a tracked pause: any occurrence
  now fails the gate, and re-populating `LEDGER_MISSING_ROUTE` fails it too, so
  the exception cannot be reopened with one line. `MISSING_DOC` keeps its ledger
  (B-117, 29 entries).
- **A live docs page fetched one of the phantoms.** `apps/docs/src/pages/customer/audit-chain.tsx`
  called `/v1/audit/export?from=&to=` for both the chain-head probe and the
  NDJSON download; the tenant is a path segment, not a query param, so both
  404'd after authenticating.
- **Six paths lied in the opposite direction** — marked `x-status: planned` with
  a note that the mount was "deferred to the CF Worker entry crate" while the
  route was in fact registered (`/v1/signup`, `/api/health`,
  `/v1/onboarding/tier-select`, both `/v1/privacy/dsr` paths, and
  `/v1/billing/stripe-webhook`, which carried no status at all). Four published
  pages likewise told customers that self-service PAT creation, listing and
  revocation "returns 404" and to email support — all three are live behind the
  dashboard's Keys page. Fixing only the over-promises would have left the docs
  equally wrong.
