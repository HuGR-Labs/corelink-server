#!/usr/bin/env bash
# provision-storage-monitor.sh
#
# RETIRED by B-082. The anonymous /_health/container route intentionally
# redacts storage, and BetterStack's public monitor cannot safely carry the
# dedicated admin credential required by /_health/container/authenticated.
# Keep this guard so an old operator command cannot recreate an ineffective
# public keyword monitor. Use the secret-bearing operator probe documented in
# docs/operator/storage-backing-alert-2026-05-30.md instead.

set -euo pipefail

echo "ERROR: historical storage monitor retired by B-082; no public monitor can carry CORELINK_ADMIN_AUTH_KEY" >&2
echo "Use GET /_health/container/authenticated with X-Corelink-Internal-Auth from a protected probe." >&2
exit 2
