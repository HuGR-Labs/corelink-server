#!/usr/bin/env bash
set -euo pipefail

# Fail-closed verifier for the eight B-069 legacy journeys still shipped. The
# dual-approval mock journey was retired with the unserved /admin/ops surface
# in B-119; keeping it in this list would make this gate fail on an intentional
# deletion. The directory
# override exists only for the verifier's mutation tests; normal CI always
# uses the checked-in focal directory.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="${B069_E2E_DIR:-$ROOT/apps/admin-ui/playwright/e2e}"

required=(
  00-a11y-sweep.spec.ts
  01-onboarding.spec.ts
  02-consent-capture.spec.ts
  03-consent-withdraw.spec.ts
  04-dsr-access.spec.ts
  05-dsr-erasure.spec.ts
  06-admin-audit-viewer.spec.ts
  08-locale-switch.spec.ts
)

fail() { echo "B-069 verifier: $*" >&2; exit 1; }

[ -d "$DIR" ] || fail "focal directory is missing: $DIR"
for file in "${required[@]}"; do
  path="$DIR/$file"
  [ -f "$path" ] || fail "required journey is missing: $file"
  # Deferred tests and skipped blocks are not coverage. `test.only` is also a
  # gate hazard: it can hide the other seven journeys while appearing green.
  ! rg -n 'test\.(fixme|skip|only)|describe\.(skip|only)' "$path" >/dev/null \
    || fail "$file contains deferred, skipped, or exclusive Playwright coverage"
  rg -n '\btest\s*\(' "$path" >/dev/null || fail "$file has no executable test"
  # Every focal journey must make an observable assertion. Requiring expect()
  # and a matcher catches hollow tests whose body only navigates/clicks.
  rg -n '\bexpect\s*\(' "$path" >/dev/null || fail "$file has no expect() assertion"
  rg -n '\.(toBe|toEqual|toHave|toContain|toMatch|toBeVisible|toBeEnabled|toBeDisabled|toHaveURL)\b' "$path" >/dev/null \
    || fail "$file has no Playwright matcher assertion"
done

echo "B-069 verifier: ${#required[@]} focal journeys are executable and asserted"
