#!/usr/bin/env bash
# CoreLink credential helper — Bazel 6+ credential helper protocol.
#
# CTRL-CRED-001 (Lote 10.15 codex P0 canonical fix):
#   The PAT is read exclusively from the CORELINK_PAT environment variable.
#   It is NEVER passed via argv (which would appear in `ps aux` output).
#
# Protocol: Bazel calls this script with a single JSON line on stdin:
#   { "uri": "https://corelink.dev/v1/cache" }
# The helper writes a single JSON line to stdout:
#   { "headers": { "Authorization": ["Bearer <token>"] } }
#
# See https://bazel.build/docs/credential-helper for the full specification.

set -euo pipefail

# Validate that CORELINK_PAT is set.
if [[ -z "${CORELINK_PAT:-}" ]]; then
    echo "ERROR: CORELINK_PAT environment variable is not set." >&2
    echo "  Set it via: export CORELINK_PAT=corelink_prod_..." >&2
    echo "  See https://docs.corelink.dev/auth for PAT generation." >&2
    exit 1
fi

# Emit the Bazel credential helper JSON response.
# The helper ignores the stdin URI (single-endpoint helper); extend here if
# multi-endpoint support is needed.
printf '{"headers":{"Authorization":["Bearer %s"]}}\n' "${CORELINK_PAT}"
