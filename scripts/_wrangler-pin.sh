# scripts/_wrangler-pin.sh — SINGLE SOURCE OF TRUTH for the wrangler CLI pin.
#
# shellcheck shell=bash
#
# Sourced by deploy scripts AND read by .github/workflows/cf-deploy-prod.yml, so
# the workflow's `npm install -g wrangler@<v>` and the scripts' npx fallback can
# never drift apart again.
#
# WHY THIS FILE EXISTS (2026-08-04)
# ---------------------------------
# cf-deploy-prod.yml installed `wrangler@4.95.0` and set `WRANGLER: wrangler`,
# but scripts/deploy-container-prod.sh resolved that override with a PATH test
# (`[ ! -x "$WRANGLER" ]`), which a bare command name can never satisfy. The
# override was silently discarded and every production deploy ran
# `npx wrangler@latest`, resolved fresh from npm at deploy time. Measured on the
# last two prod deploys (runs 30923390611, 30918887614): CI installed 4.95.0,
# the deploy ran 4.118.0. The pin had never been in force on the prod path.
# The resolver bug is fixed; this file removes the second half of the failure
# mode — two hand-maintained copies of the version string.
#
# WHY EXACT, NOT A RANGE
# ----------------------
#   - Floating tags (`wrangler@4`, `@latest`) violate the version-pin policy
#     (audit 2026-06-11, specs/_audits/2026-06-11-prelaunch-pentest.md → #248).
#   - wrangler v3 cannot parse the `[[env.*.containers]]` arrays in
#     wrangler.toml (verified 2026-06-05), so the floor is a hard v4.
#
# TO BUMP: change the line below, run `bash scripts/deploy-container-prod.sh`
# (dry-run) to confirm the new binary parses wrangler.toml, and add a CHANGELOG
# entry. Nothing else needs editing on the cf-deploy-prod path.
#
# Other workflows (docs-deploy, admin-ui-deploy, container-build-push-prod,
# backup-daily*) still carry their own literal `wrangler@4.95.0`; they are out of
# scope for this change and can adopt this file the same way when touched.

# shellcheck disable=SC2034  # consumed by scripts that source this file
WRANGLER_PINNED_VERSION="4.95.0"
