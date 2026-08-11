#!/usr/bin/env bash
# scripts/sbom-aggregate.sh — R7-1 supply-quality rollup (c) SBOM consolidated.
#
# Produces a SINGLE CycloneDX 1.6 JSON document spanning the entire CoreLink
# workspace (crates/ + apps/). The existing `tools/sbom-publish` crate emits
# per-binary SBOMs via cargo-cyclonedx; this script wraps it and stitches
# the per-crate components into one canonical workspace bom whose top-level
# `component` is `pkg:cargo/corelink-workspace@<git-sha>`.
#
# Output: target/sbom/corelink-workspace.cdx.json  (CycloneDX 1.6 / JSON)
#
# Dependencies:
#   - cargo-cyclonedx     SBOM generator (per cargo subcommand, SHA-pinned
#                         in tools/sbom-publish dev-dep manifest)
#   - jq                  JSON merge / dedup
#
# Charter: set -euo pipefail mandatory.

set -euo pipefail

usage() {
    cat <<'USAGE'
sbom-aggregate.sh — consolidated workspace SBOM (CycloneDX 1.6 JSON).

Usage:
  scripts/sbom-aggregate.sh                # full workspace consolidation
  scripts/sbom-aggregate.sh --help         # this message

Environment:
  SBOM_OUT   output dir (default: target/sbom)
  SBOM_REF   workspace ref / tag for `bom.metadata.component.version`
             (default: `git describe --always --dirty`)

Output:
  $SBOM_OUT/corelink-workspace.cdx.json
USAGE
}

case "${1:-}" in
    --help|-h)
        usage
        exit 0
        ;;
esac

SBOM_OUT="${SBOM_OUT:-target/sbom}"
mkdir -p "$SBOM_OUT"

if ! command -v cargo-cyclonedx >/dev/null 2>&1; then
    echo "error: cargo-cyclonedx not installed." >&2
    echo "  install: cargo install cargo-cyclonedx --locked" >&2
    exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
    echo "error: jq is required for JSON aggregation." >&2
    exit 1
fi

SBOM_REF="${SBOM_REF:-$(git describe --always --dirty 2>/dev/null || echo unknown)}"

echo "==> cargo cyclonedx --workspace --format json"
# cargo-cyclonedx writes one bom.json per crate next to its Cargo.toml.
cargo cyclonedx --workspace --format json --override-filename bom \
    --spec-version 1.6 >/dev/null

# Collect every per-crate bom file. Excludes the target dir to avoid stale
# outputs from prior runs.
mapfile -t BOMS < <(find . -name 'bom.json' \
    -not -path './target/*' -not -path './.git/*' -print)

if [[ "${#BOMS[@]}" -eq 0 ]]; then
    echo "error: cargo-cyclonedx produced no bom.json files." >&2
    exit 1
fi

echo "==> aggregating ${#BOMS[@]} per-crate SBOMs into consolidated workspace bom"

OUT_FILE="$SBOM_OUT/corelink-workspace.cdx.json"
SERIAL="urn:uuid:$(uuidgen 2>/dev/null \
    || python3 -c 'import uuid; print(uuid.uuid4())')"
TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Merge all per-crate `components[]` arrays and deduplicate by `purl`
# (CycloneDX recommended primary identifier — INV-SUPPLY-PURL-UNIQUE).
# The top-level metadata.component represents the workspace as a whole.
jq -s \
    --arg ts "$TIMESTAMP" \
    --arg ref "$SBOM_REF" \
    --arg serial "$SERIAL" \
    '
    {
        bomFormat: "CycloneDX",
        specVersion: "1.6",
        serialNumber: $serial,
        version: 1,
        metadata: {
            timestamp: $ts,
            tools: [{ vendor: "HumanGuardrail", name: "corelink-sbom-aggregate", version: $ref }],
            component: {
                "bom-ref": ("pkg:cargo/corelink-workspace@" + $ref),
                type: "application",
                name: "corelink-workspace",
                version: $ref,
                purl: ("pkg:cargo/corelink-workspace@" + $ref)
            }
        },
        components: (
            [ .[].components // [] | .[] ]
            | unique_by(.purl // (.name + "@" + (.version // "0.0.0")))
        ),
        dependencies: (
            [ .[].dependencies // [] | .[] ]
            | unique_by(.ref)
        )
    }
    ' "${BOMS[@]}" > "$OUT_FILE"

COUNT=$(jq '.components | length' "$OUT_FILE")
echo "==> consolidated SBOM written: $OUT_FILE"
echo "    components (deduped by purl): $COUNT"
echo "    serial: $SERIAL"
echo "    ref:    $SBOM_REF"
