# B245 — scope synthetic raw-curl environment data

`validate_secrets_matrix.py` now recognizes the three raw-curl fixture values
(`CORELINK_HTTP_PORT_FILE`, `CORELINK_HTTP_REQUEST_FILE`, and
`CORELINK_HTTP_STATUS`) only at their two exact documentation-test paths. A
fixture move, a new name, or any production-path use remains code-only drift and
fails the gate.
