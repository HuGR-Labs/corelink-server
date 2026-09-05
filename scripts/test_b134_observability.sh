#!/usr/bin/env bash
# B-134 static contract and mutation suite. No network calls or dispatches.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKER="${ROOT}/scripts/check_b134_observability.py"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/b134-observability.XXXXXX")"
trap 'rm -rf "${TMP}"' EXIT

copy_fixture() {
    local dest="$1"
    mkdir -p "${dest}/.github/workflows" "${dest}/docs/campaigns/remediation"
    cp "${ROOT}/.github/workflows/smoke-install.yml" "${dest}/.github/workflows/smoke-install.yml"
    cp "${ROOT}/.github/workflows/cosign-sign.yml" "${dest}/.github/workflows/cosign-sign.yml"
    cp "${ROOT}/docs/campaigns/remediation/B-134-docker-shim-experiment.md" "${dest}/docs/campaigns/remediation/B-134-docker-shim-experiment.md"
}

expect_green() {
    local id="$1" root="$2"
    if ! python3 "${CHECKER}" --root "${root}" >/dev/null; then
        echo "B134-${id}: baseline unexpectedly red" >&2
        return 1
    fi
    echo "B134-${id}: PASS"
}

expect_red() {
    local id="$1" root="$2"
    if python3 "${CHECKER}" --root "${root}" >/dev/null 2>&1; then
        echo "B134-${id}: mutation survived" >&2
        return 1
    fi
    echo "B134-${id}: PASS (mutation red)"
}

expect_green baseline "${ROOT}"

copy_fixture "${TMP}/smoke-route"
sed -i.bak 's/runs-on: corelink/runs-on: ubuntu-latest/' "${TMP}/smoke-route/.github/workflows/smoke-install.yml"
expect_red smoke-route "${TMP}/smoke-route"

copy_fixture "${TMP}/cosign-verify"
sed -i.bak '/cosign verify \\/d' "${TMP}/cosign-verify/.github/workflows/cosign-sign.yml"
expect_red cosign-verify "${TMP}/cosign-verify"

copy_fixture "${TMP}/placeholder"
echo '# placeholder mutation' >> "${TMP}/placeholder/.github/workflows/cosign-sign.yml"
expect_red placeholder "${TMP}/placeholder"

copy_fixture "${TMP}/ledger"
sed -i.bak 's/| UNMEASURED |/| PASS |/g' "${TMP}/ledger/docs/campaigns/remediation/B-134-docker-shim-experiment.md"
expect_red ledger "${TMP}/ledger"

copy_fixture "${TMP}/synthetic-token"
sed -i.bak 's/-e CORELINK_TEST_TOKEN="\$CORELINK_CANARY_PAT"/-e CORELINK_INSTALL_PROBE_TOKEN="synthetic"/' "${TMP}/synthetic-token/.github/workflows/smoke-install.yml"
expect_red synthetic-token "${TMP}/synthetic-token"

echo "B-134 observability mutations: all controls passed"
