# B-069 authenticated admin UI E2E

- The nine legacy authenticated UI journeys now execute as real Playwright
  tests with a serialized, warmed Next route tree. DSR access and category
  erasure exercise the fresh-MFA gate, submit against the test-only same-origin
  fixture, render a signed receipt, and verify the request in the status list.
- Consent capture/withdraw remain deliberately retired: the tests require the
  compiled route-local 404 marker and assert no receipt or mutation, preserving
  the GDPR fail-closed boundary until a real ledger is deployed.
- Admin audit opens the fixture proof and surfaces a Merkle mismatch; dual
  approval rejects the requestor and accepts only an independent approver with
  a required reason header. `scripts/verify-b069-e2e.sh` rejects deferred,
  skipped, exclusive, or hollow journeys, with Vitest mutation tests proving
  those checks fail closed.
- Consent retirement probes require route-local markers, and the shared warm-up
  now covers B-069's non-retired routes. A generic/uncompiled 404 cannot satisfy
  the intentional consent 404 checks.
