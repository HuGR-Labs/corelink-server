#!/usr/bin/env bash
# CoreLink credential helper — Bazel 6+ credential helper protocol.
#
# CTRL-CRED-001 (Lote 10.15 codex P0 canonical fix):
#   The PAT is read exclusively from the CORELINK_PAT environment variable.
#   It is NEVER passed via argv (which would appear in `ps aux` output),
#   NEVER echoed to stderr, NEVER logged.
#
# Protocol: Bazel calls this script with a single JSON line on stdin:
#   { "uri": "https://corelink.humangr.com/v1/cache" }
# The helper writes a single JSON line to stdout:
#   { "headers": { "Authorization": ["Bearer <token>"] } }
#
# Reference: https://bazel.build/docs/credential-helper
# WI-S15-002 / WI-S15-005, CAP-SDK-004.

set -euo pipefail

# Validate that CORELINK_PAT is set.  If unset, return empty headers
# (unauthenticated; cache read may still work for public tenants).
if [[ -z "${CORELINK_PAT:-}" ]]; then
    printf '{"headers":{}}\n'
    exit 0
fi

# Emit the Bazel credential helper JSON response.
# The helper ignores the stdin URI (single-endpoint helper); extend here if
# multi-endpoint support is needed.
printf '{"headers":{"Authorization":["Bearer %s"]}}\n' "${CORELINK_PAT}"
