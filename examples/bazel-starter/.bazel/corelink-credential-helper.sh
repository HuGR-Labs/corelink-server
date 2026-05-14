#!/usr/bin/env bash
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

printf '{"headers":{"Authorization":["Bearer %s"]}}\n' "${CORELINK_PAT}"
