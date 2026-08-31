### Added

- **B-121 — a gate that compares the DOCUMENTED API surface against the SERVED
  one.** `scripts/validate_api_surface.py` (stdlib only) checks
  `openapi/corelink-v1.yaml` against the routes the code actually registers,
  run by `.github/workflows/api-surface-parity.yml` on `pull_request`. Nothing
  had ever compared the two, and they had drifted into eight endpoint
  families: `/v1/admin/ops` (+ `{op_id}`, approve, reject),
  `/v1/enterprise/inquire`, `/v1/admin/audit/events`, `/v1/admin/tenants`,
  `/v1/data-categories` and `/v1/pats` are documented and unregistered, while
  `/v1/dpa/accept` and `/v1/audit/export` are documented at a path the code
  does not serve — the worse form, since the doc looks right and the call 404s
  after a successful auth.

  The comparator targets the spec, not the 45 generated MDX pages under
  `apps/docs`: those are propagation (`scripts/gen-api-reference.py` emits
  them), and `api-reference-sync.yml` already keeps them faithful to a spec
  that is itself wrong.

  It is bidirectional, so one instrument closes both families — `MISSING_ROUTE`
  is documented-without-a-route, `MISSING_DOC` is a public `/v1` route absent
  from the spec (B-117, which named two such routes; the sweep found 28).
  Known divergences live in an in-file ledger pinned to their backlog ids, and
  a ledger entry whose divergence has been fixed fails the gate
  (`STALE_LEDGER`), so a repair cannot land leaving its own excuse behind.

  The extractor is expression-oriented rather than line-oriented, because a
  line-oriented `grep '.route("'` is what produced a phantom 32-absence count
  on the first attempt at this sweep. It resolves four registration forms — a
  literal on the `.route(` line, a literal on the line after it, a path
  registered via a `const` (`AUDIT_EXPORT_ROUTE`, `TURBO_GET_ROUTE`,
  `ROUTE_EVENT_COUNT`), and a path the Worker terminates in `matchRoute`. A
  `--self-test` positive control proves it can see one known instance of each
  form, and runs as a precondition of every comparison: a blind extractor
  reports a clean surface, and a clean report from a blind instrument is the
  failure this gate exists to prevent.

  Routes are not probed over HTTP — the Worker rejects an uncredentialed
  request before it routes, so `401` cannot distinguish "exists" from "does
  not exist".

  Two divergences beyond the eight catalogued surfaced and were confirmed
  against an independent search: `/v1/dpa/re-accept` (documented, registered
  nowhere — only a proptest names it) and `/api/csp-report` (served by
  `apps/admin-ui`, not by this API).

  This adds the instrument, not the repairs: B-116, B-117, B-119 and B-120
  remain open.
