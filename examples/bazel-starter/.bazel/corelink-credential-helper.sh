#!/usr/bin/env bash
<<<<<<< HEAD
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
=======
# CoreLink Bazel credential helper (Bazel 6+ protocol).
# Reads CORELINK_PAT from environment; writes JSON to stdout.
#
# CTRL-CRED-001: PAT is NEVER echoed to stderr, NEVER passed via argv,
# NEVER logged. The helper emits the minimal Bazel credential JSON response.
#
# Reference: https://bazel.build/docs/credential-helper
# WI-S15-005, CAP-SDK-004.

set -euo pipefail

if [[ -z "${CORELINK_PAT:-}" ]]; then
    # No PAT configured — return empty headers (unauthenticated; cache read may still work).
    printf '{"headers":{}}\n'
    exit 0
fi

>>>>>>> wt/wi-s15-005
printf '{"headers":{"Authorization":["Bearer %s"]}}\n' "${CORELINK_PAT}"
