#!/usr/bin/env bash
# check-container-pin-fresh.sh — FAIL if the wrangler.toml container image pin is STALE.
#
# Root-cause guard for the 2026-07-05 incident: the prod container pin sat at
# `c1337115` (PR #594) for 109 commits while every `cf-deploy-prod` "converged"
# (running image == pinned image, both stale) and reported success — so the
# entire container-side cutover (runner purchase, GDPR anchor, audit fixes) never
# actually shipped. The Worker/signup-worker deploys don't use the container pin,
# so only the container silently froze.
#
# This check makes that failure LOUD at deploy time: if any container-affecting
# path changed between the pinned image SHA and HEAD, the pinned image predates
# current container code → deploying it would ship a container BEHIND main. The
# fix is always: rebuild (container-build-push-prod.yml) + repin wrangler.toml to
# HEAD, THEN deploy.
#
# Exit: 0 = fresh (pin reflects current container code); 1 = STALE; 2 = usage error.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The Rust crates ARE the container binary; the Dockerfile + lockfile pin the
# build. A change in any of these since the pinned SHA means the image is behind.
PATHS=(crates/ Dockerfile Cargo.lock Cargo.toml)

# Extract the pinned image SHA (the `<sha>` in `…corelinkserver-prod:<sha>-r<N>`)
# from the prod container pin. All 5 envs are pinned to the same SHA by
# push-container-multiregion.sh, so the first prod pin is representative.
PIN="$(grep -oE 'corelink-prod-corelinkserver-prod:[0-9a-f]+-r[0-9]+' wrangler.toml \
        | head -1 | sed -E 's/.*:([0-9a-f]+)-r[0-9]+/\1/')"
if [ -z "${PIN:-}" ]; then
    echo "::error::check-container-pin-fresh: could not parse a container image pin from wrangler.toml." >&2
    exit 2
fi

HEAD_SHA="$(git rev-parse --short HEAD)"

if ! git rev-parse -q --verify "${PIN}^{commit}" >/dev/null 2>&1; then
    echo "::error::check-container-pin-fresh: pinned container SHA '${PIN}' is not a commit in this repo (shallow clone? unpushed pin?). Cannot verify freshness." >&2
    exit 2
fi

if git diff --quiet "${PIN}" HEAD -- "${PATHS[@]}"; then
    echo "check-container-pin-fresh: OK — pin ${PIN} reflects current container code (no ${PATHS[*]} change vs HEAD ${HEAD_SHA})."
    exit 0
fi

N="$(git rev-list --count "${PIN}..HEAD" -- "${PATHS[@]}" 2>/dev/null || echo '?')"
{
    echo "::error::STALE CONTAINER PIN. wrangler.toml pins the container at ${PIN}, but container code (${PATHS[*]}) changed in ${N} commit(s) since — deploying now would ship a container BEHIND main (the 2026-07-05 stale-pin failure mode)."
    echo "  FIX: run container-build-push-prod.yml on main, then repin wrangler.toml [[env.*.containers]] to HEAD (the sed push-container-multiregion.sh prints), then deploy."
    echo "  Container-affecting diff ${PIN}..${HEAD_SHA}:"
    git --no-pager diff --stat "${PIN}" HEAD -- "${PATHS[@]}" | tail -25
} >&2
exit 1
