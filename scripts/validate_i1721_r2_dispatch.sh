#!/usr/bin/env bash
set -euo pipefail

[[ "${GITHUB_REPOSITORY:-}" == "HuGR-dev/corelink-server" ]]
[[ "${GITHUB_REF:-}" == "refs/heads/main" && "${REF_PROTECTED:-}" == "true" ]]
[[ "${CONFIRM:-}" == "i1721-r2-lock-proof" ]]
[[ "${EXPECTED_SHA:-}" =~ ^[0-9a-f]{40}$ && "${EXPECTED_SHA:-}" == "${GITHUB_SHA:-}" ]]
