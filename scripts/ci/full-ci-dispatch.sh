#!/usr/bin/env bash
# full-ci-dispatch.sh — dispatch the FULL CI surface on main, on demand.
#
# Why: the heavy gates (coverage / CodeQL / TLA+ / ffi-matrix /
# reproducible-build / cas-foundation / ship-gates) moved OFF per-PR
# (2026-06-02) to nightly+on-main+on-demand. This script is the "full CI"
# button: it fires every workflow_dispatch-able workflow against main.
#
# Scheduled by the owner via user crontab at 04:40 America/Bahia
# (`crontab -l | grep full-ci-dispatch`), so the whole matrix grinds on the
# idle Mac overnight instead of competing with daytime work.
#
# Requirements: `gh` authenticated as the owner (workflow scope). Logs to
# ~/Library/Logs/corelink-full-ci.log via the crontab redirection.
set -euo pipefail

REPO="humangr-labs/corelink-server"
echo "=== full-ci dispatch @ $(date -u +%FT%TZ) on main ==="

# Every workflow that declares workflow_dispatch. Disabled workflows and
# dispatch failures are reported but never abort the sweep.
dispatched=0 skipped=0
while IFS= read -r wf; do
    name=$(basename "$wf")
    if gh workflow run "$name" --ref main --repo "$REPO" 2>/dev/null; then
        dispatched=$((dispatched + 1))
        echo "dispatched: $name"
    else
        skipped=$((skipped + 1))
        echo "skipped (no dispatch trigger / disabled): $name"
    fi
    # Gentle stagger so 60+ dispatches don't slam the queue at one instant.
    sleep 5
done < <(ls "$(git -C "$(dirname "$0")/../.." rev-parse --show-toplevel 2>/dev/null || echo "$HOME/Documents/HuGR/corelink-server")"/.github/workflows/*.yml)

echo "=== done: $dispatched dispatched, $skipped skipped @ $(date -u +%FT%TZ) ==="
