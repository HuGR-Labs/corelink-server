### Fixed

- **Scope B-245 synthetic raw-curl environment data.** `validate_secrets_matrix.py` now recognizes the three raw-curl fixture values
(`CORELINK_HTTP_PORT_FILE`, `CORELINK_HTTP_REQUEST_FILE`, and
`CORELINK_HTTP_STATUS`) only at their two exact documentation-test paths. A
fixture move, a new name, or any production-path use remains code-only drift and
fails the gate.

- **Scope B-245 performance credentials.** The exact-scope scanner excludes the
isolated-lane unit test that injects a dummy `CORELINK_PERF_PAT` value; the
owner-provisioned credential remains limited to its workflow and measurement
collectors.

The legacy deploy shell verifier (`scripts/secrets-checklist-verify.sh`) still reports
these three names as code-only because its scanner has no path-scoped synthetic-environment
manifest. This is a known legacy-scope limitation, not a clean result; the Python validator
is the path-scoped check for these fixtures until the shell verifier is upgraded.
