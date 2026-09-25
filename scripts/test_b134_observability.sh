#!/usr/bin/env bash
# B-134 static contract and mutation suite. No network calls or dispatches.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKER="${ROOT}/scripts/check_b134_observability.py"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/b134-observability.XXXXXX")"
trap 'rm -rf "${TMP}"' EXIT

copy_fixture() {
    local dest="$1"
    mkdir -p "${dest}/.github/workflows" "${dest}/docs/campaigns/remediation" "${dest}/apps/get-corelink-worker/test"
    cp "${ROOT}/.github/workflows/smoke-install.yml" "${dest}/.github/workflows/smoke-install.yml"
    cp "${ROOT}/docs/campaigns/remediation/B-134-docker-shim-experiment.md" "${dest}/docs/campaigns/remediation/B-134-docker-shim-experiment.md"
    cp "${ROOT}/apps/get-corelink-worker/test/smoke-install.Dockerfile" "${dest}/apps/get-corelink-worker/test/smoke-install.Dockerfile"
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

copy_fixture "${TMP}/route"
sed -i.bak 's/runs-on: ubuntu-24.04/runs-on: corelink/' "${TMP}/route/.github/workflows/smoke-install.yml"
expect_red route "${TMP}/route"

copy_fixture "${TMP}/provenance"
sed -i.bak 's/I1672_FLEET_LABEL: github-hosted/I1672_FLEET_LABEL: corelink/' "${TMP}/provenance/.github/workflows/smoke-install.yml"
expect_red provenance "${TMP}/provenance"

copy_fixture "${TMP}/trigger"
sed -i.bak 's/workflow_dispatch:/workflow_dispatch_removed:/' "${TMP}/trigger/.github/workflows/smoke-install.yml"
expect_red trigger "${TMP}/trigger"

copy_fixture "${TMP}/backend"
sed -i.bak 's/if docker info > /if docker status > /' "${TMP}/backend/.github/workflows/smoke-install.yml"
expect_red backend "${TMP}/backend"

copy_fixture "${TMP}/fail-closed"
sed -i.bak 's/set -euo pipefail/set -uo pipefail/' "${TMP}/fail-closed/.github/workflows/smoke-install.yml"
expect_red fail-closed "${TMP}/fail-closed"

copy_fixture "${TMP}/helper"
sed -i.bak 's#python3 scripts/smoke_install_observe.py#python3 scripts/missing_observer.py#' "${TMP}/helper/.github/workflows/smoke-install.yml"
expect_red helper "${TMP}/helper"

copy_fixture "${TMP}/helper-always"
sed -i.bak '/      - name: Observe process/{n;s/if: \${{ always() }}/if: \${{ success() }}/;}' "${TMP}/helper-always/.github/workflows/smoke-install.yml"
expect_red helper-always "${TMP}/helper-always"

copy_fixture "${TMP}/upload-always"
sed -i.bak '/      - name: Upload structured observation/{n;s/if: \${{ always() }}/if: \${{ success() }}/;}' "${TMP}/upload-always/.github/workflows/smoke-install.yml"
expect_red upload-always "${TMP}/upload-always"

copy_fixture "${TMP}/upload-action"
sed -i.bak 's#uses: actions/upload-artifact@#uses: actions/download-artifact@#' "${TMP}/upload-action/.github/workflows/smoke-install.yml"
expect_red upload-action "${TMP}/upload-action"

copy_fixture "${TMP}/upload-path"
sed -i.bak 's#path: artifacts/i1672/#path: artifacts/missing/#' "${TMP}/upload-path/.github/workflows/smoke-install.yml"
expect_red upload-path "${TMP}/upload-path"

copy_fixture "${TMP}/receipt"
sed -i.bak 's#--receipt artifacts/i1672/smoke-install-receipt.json#--receipt artifacts/i1672/receipt.json#' "${TMP}/receipt/.github/workflows/smoke-install.yml"
expect_red receipt "${TMP}/receipt"

copy_fixture "${TMP}/ledger"
sed -i.bak 's/| UNMEASURED |/| PASS |/g' "${TMP}/ledger/docs/campaigns/remediation/B-134-docker-shim-experiment.md"
expect_red ledger "${TMP}/ledger"

copy_fixture "${TMP}/ledger-status"
sed -i.bak 's/\*\*Status:\*\* open/**Status:** closed/' "${TMP}/ledger-status/docs/campaigns/remediation/B-134-docker-shim-experiment.md"
expect_red ledger-status "${TMP}/ledger-status"

copy_fixture "${TMP}/cosign-reintroduced"
cp "${ROOT}/.github/workflows/smoke-install.yml" "${TMP}/cosign-reintroduced/.github/workflows/cosign-sign.yml"
expect_red cosign-reintroduced "${TMP}/cosign-reintroduced"

copy_fixture "${TMP}/dockerfile-token"
echo 'ENV CORELINK_TEST_TOKEN=synthetic' >> "${TMP}/dockerfile-token/apps/get-corelink-worker/test/smoke-install.Dockerfile"
expect_red dockerfile-token "${TMP}/dockerfile-token"

echo "B-134 observability mutations: all controls passed"
