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
    cp "${ROOT}/.github/workflows/cosign-sign.yml" "${dest}/.github/workflows/cosign-sign.yml"
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

expect_repro_red() {
    local id="$1" root="$2"
    if python3 "${CHECKER}" --root "${root}" >/dev/null 2>&1; then
        echo "B134-${id}: adversarial reproduction survived" >&2
        return 1
    fi
    echo "B134-${id}: PASS (reproduction red)"
}

expect_green baseline "${ROOT}"

copy_fixture "${TMP}/smoke-route"
sed -i.bak 's/runs-on: corelink/runs-on: ubuntu-latest/' "${TMP}/smoke-route/.github/workflows/smoke-install.yml"
expect_red smoke-route "${TMP}/smoke-route"

copy_fixture "${TMP}/smoke-backend"
sed -i.bak '/if command -v docker.*docker info.*then/d' "${TMP}/smoke-backend/.github/workflows/smoke-install.yml"
expect_red smoke-backend "${TMP}/smoke-backend"

copy_fixture "${TMP}/smoke-backend-comment"
sed -i.bak 's/^          if command -v docker.*then$/          # if command -v docker \/dev\/null 2>\&1 \&\& docker info \/dev\/null 2>\&1; then\n          if true; then/' "${TMP}/smoke-backend-comment/.github/workflows/smoke-install.yml"
expect_red smoke-backend-comment "${TMP}/smoke-backend-comment"

copy_fixture "${TMP}/smoke-pat"
# shellcheck disable=SC2016  # Fixture regex must keep the GitHub expression literal.
sed -i.bak '/if \[ -n "\${CORELINK_CANARY_PAT:-}" \]; then/d' "${TMP}/smoke-pat/.github/workflows/smoke-install.yml"
expect_red smoke-pat "${TMP}/smoke-pat"

copy_fixture "${TMP}/smoke-run-echo"
sed -i.bak 's/^          docker run --rm/          echo docker run --rm/g' "${TMP}/smoke-run-echo/.github/workflows/smoke-install.yml"
expect_red smoke-run-echo "${TMP}/smoke-run-echo"

copy_fixture "${TMP}/cosign-verify"
sed -i.bak '/cosign verify \\/d' "${TMP}/cosign-verify/.github/workflows/cosign-sign.yml"
expect_red cosign-verify "${TMP}/cosign-verify"

copy_fixture "${TMP}/cosign-backend-comment"
sed -i.bak 's/^          if ! docker info/          # if ! docker info/' "${TMP}/cosign-backend-comment/.github/workflows/cosign-sign.yml"
expect_red cosign-backend-comment "${TMP}/cosign-backend-comment"

copy_fixture "${TMP}/cosign-backend"
sed -i.bak '/^          if ! docker info.*then$/d' "${TMP}/cosign-backend/.github/workflows/cosign-sign.yml"
expect_red cosign-backend "${TMP}/cosign-backend"

copy_fixture "${TMP}/cosign-sign-echo"
sed -i.bak 's/^          cosign sign --yes/          echo cosign sign --yes/' "${TMP}/cosign-sign-echo/.github/workflows/cosign-sign.yml"
expect_red cosign-sign-echo "${TMP}/cosign-sign-echo"

copy_fixture "${TMP}/placeholder"
# shellcheck disable=SC2016  # Fixture replacement must keep the GitHub expression literal.
sed -i.bak 's/cosign sign --yes.*/cosign sign --yes "${IMAGE}" # placeholder mutation/' "${TMP}/placeholder/.github/workflows/cosign-sign.yml"
expect_red placeholder "${TMP}/placeholder"

copy_fixture "${TMP}/ledger"
sed -i.bak 's/| UNMEASURED |/| PASS |/g' "${TMP}/ledger/docs/campaigns/remediation/B-134-docker-shim-experiment.md"
expect_red ledger "${TMP}/ledger"

copy_fixture "${TMP}/synthetic-token"
# shellcheck disable=SC2016  # Fixture replacement must keep the GitHub expression literal.
sed -i.bak 's/-e CORELINK_TEST_TOKEN="\$CORELINK_CANARY_PAT"/-e CORELINK_INSTALL_PROBE_TOKEN="synthetic"/' "${TMP}/synthetic-token/.github/workflows/smoke-install.yml"
expect_red synthetic-token "${TMP}/synthetic-token"

copy_fixture "${TMP}/dockerfile-token"
echo 'ENV CORELINK_TEST_TOKEN=synthetic' >> "${TMP}/dockerfile-token/apps/get-corelink-worker/test/smoke-install.Dockerfile"
expect_red dockerfile-token "${TMP}/dockerfile-token"

copy_fixture "${TMP}/dockerfile-arg-token"
echo 'ARG CORELINK_TEST_TOKEN=synthetic' >> "${TMP}/dockerfile-arg-token/apps/get-corelink-worker/test/smoke-install.Dockerfile"
expect_red dockerfile-arg-token "${TMP}/dockerfile-arg-token"

copy_fixture "${TMP}/cosign-zone"
# shellcheck disable=SC2016  # Fixture regex must keep the GitHub expression literal.
sed -i.bak '/if ! \[\[ "\${CF_DEPLOY_ZONE_ID:-}" =~/d' "${TMP}/cosign-zone/.github/workflows/cosign-sign.yml"
expect_red cosign-zone "${TMP}/cosign-zone"

copy_fixture "${TMP}/cosign-manual-tag"
echo '      tag:' >> "${TMP}/cosign-manual-tag/.github/workflows/cosign-sign.yml"
expect_red cosign-manual-tag "${TMP}/cosign-manual-tag"

copy_fixture "${TMP}/cosign-tag-guard"
# shellcheck disable=SC2016  # Fixture regex must keep the GitHub expression literal.
sed -i.bak '/if ! \[\[ "\${GITHUB_REF:-}" =~/d' "${TMP}/cosign-tag-guard/.github/workflows/cosign-sign.yml"
expect_red cosign-tag-guard "${TMP}/cosign-tag-guard"

copy_fixture "${TMP}/ledger-status"
sed -i.bak 's/^\*\*Status:\*\* open/**Status:** closed/' "${TMP}/ledger-status/docs/campaigns/remediation/B-134-docker-shim-experiment.md"
expect_red ledger-status "${TMP}/ledger-status"

# Adversarial regressions: required commands must be reachable, not merely
# present in comments or dead shell branches. These focused reproductions are
# additional to the 18 named contract mutations above.
copy_fixture "${TMP}/repro-if-false"
sed -i.bak '/^          cosign verify \\/i\
          if false; then' "${TMP}/repro-if-false/.github/workflows/cosign-sign.yml"
sed -i.bak '/^          echo "Signature verification passed\."/i\
          fi' "${TMP}/repro-if-false/.github/workflows/cosign-sign.yml"
expect_repro_red repro-if-false "${TMP}/repro-if-false"

copy_fixture "${TMP}/repro-false-and"
python3 - "${TMP}/repro-false-and/.github/workflows/cosign-sign.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
text = text.replace("          cosign verify \\\n", "          false && \\\n          cosign verify \\\n", 1)
path.write_text(text, encoding="utf-8")
PY
expect_repro_red repro-false-and "${TMP}/repro-false-and"

copy_fixture "${TMP}/repro-exit"
sed -i.bak '/^          cosign verify \\/i\
          exit 0' "${TMP}/repro-exit/.github/workflows/cosign-sign.yml"
expect_repro_red repro-exit "${TMP}/repro-exit"

copy_fixture "${TMP}/repro-return"
sed -i.bak '/^          cosign verify \\/i\
          return 0' "${TMP}/repro-return/.github/workflows/cosign-sign.yml"
expect_repro_red repro-return "${TMP}/repro-return"

copy_fixture "${TMP}/repro-arg-token"
echo 'ARG UNRELATED_TOKEN=synthetic' >> "${TMP}/repro-arg-token/apps/get-corelink-worker/test/smoke-install.Dockerfile"
expect_repro_red repro-arg-token "${TMP}/repro-arg-token"

copy_fixture "${TMP}/repro-env-pat"
echo 'ENV PAT=changeme' >> "${TMP}/repro-env-pat/apps/get-corelink-worker/test/smoke-install.Dockerfile"
expect_repro_red repro-env-pat "${TMP}/repro-env-pat"

echo "B-134 observability mutations: all controls passed"
