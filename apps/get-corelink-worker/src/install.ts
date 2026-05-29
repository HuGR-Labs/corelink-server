/**
 * Canonical CoreLink CLI install shell-script template.
 *
 * Served verbatim by the `get-corelink-worker` at https://get.corelink.io.
 * Two placeholders are interpolated at render time from `Env`:
 *
 *   - `__RELEASE_ORIGIN__` → `env.RELEASE_ORIGIN`
 *     (e.g. `https://github.com/humangr-labs/corelink-cli/releases/latest/download`)
 *   - `__DEFAULT_API_ENDPOINT__` → `env.DEFAULT_API_ENDPOINT`
 *     (e.g. `https://corelink-api.humangr.com`)
 *
 * Both placeholders are non-credential PUBLIC config (the rendered
 * script is fetched by every developer who runs `curl | sh`).
 *
 * Script invariants enforced by unit tests in `tests/install.test.ts`:
 *
 *   1. `set -eu` is the first non-shebang executable line.
 *   2. `--token=…` is REQUIRED; absence exits non-zero with a clear
 *      FATAL message on stderr.
 *   3. `OS` and `ARCH` are derived from `uname -s` (lowercased) and
 *      `uname -m` — the agent prompt §4 fixes this contract.
 *   4. Final command is `corelink ping` (NOT `corelink --version` or a
 *      no-op) — this is what flips the `/welcome` SSE pane from
 *      "Waiting" to "Connected".
 *
 * Bash conventions:
 *   - `#!/bin/sh` (POSIX sh, NOT bash) so macOS default + Alpine/BusyBox both work.
 *   - `set -eu` (not `set -euo pipefail` — POSIX sh has no `pipefail`).
 *   - Quoting per shellcheck SC2086 / SC2046 — always double-quote variable
 *     expansions to survive filenames with spaces.
 */

/** Two-way config injected at render time from wrangler `[vars]`. */
export interface InstallScriptConfig {
  releaseOrigin: string;
  defaultApiEndpoint: string;
}

/**
 * Returns the install script with placeholders substituted.
 *
 * The body is held as a plain string literal (not a tagged template) so
 * the test-suite can diff it byte-for-byte against the canonical
 * version in the Phase 0 plan `2026-05-27-phase-0-execution-plan.md`
 * §2.H "Files to write (in this repo)" block.
 *
 * Placeholders are simple `String.prototype.replaceAll` substitutions
 * — there is no shell-script-side templating risk because the
 * placeholder values come from wrangler `[vars]` (operator-controlled,
 * not user-controlled). User-controlled `--token=$PAT` is parsed at
 * RUNTIME by the script's own `case "$1"` loop, NOT injected here.
 */
export function renderInstallScript(config: InstallScriptConfig): string {
  return INSTALL_SCRIPT_TEMPLATE.replaceAll("__RELEASE_ORIGIN__", config.releaseOrigin).replaceAll(
    "__DEFAULT_API_ENDPOINT__",
    config.defaultApiEndpoint,
  );
}

/**
 * Canonical shell-script template. Pinned verbatim against Phase 0 plan
 * §2.H "Files to write (in this repo)" body (with the URL line factored
 * out into the `__RELEASE_ORIGIN__` placeholder so the Worker can be
 * deployed to staging with a fake release origin without changing this
 * file).
 *
 * Linter exemption: this is a multi-line shell-script literal embedded
 * in TypeScript for runtime substitution. Do not "tidy" the indentation
 * — the leading whitespace of the heredoc body is significant.
 */
const INSTALL_SCRIPT_TEMPLATE = `#!/bin/sh
# CoreLink CLI installer — served from https://get.corelink.io.
# Source: github.com/humangr-labs/corelink-cli :: apps/get-corelink-worker.
# Re-run is safe: writes to /usr/local/bin/corelink and ~/.corelink/config.toml.
set -eu

TOKEN=""
REGION=""
while [ $# -gt 0 ]; do
  case "$1" in
    --token=*) TOKEN="\${1#--token=}" ;;
    --region=*) REGION="\${1#--region=}" ;;
    *) echo "WARN: ignoring unknown arg: $1" >&2 ;;
  esac
  shift
done

if [ -z "$TOKEN" ]; then
  echo "FATAL: --token required" >&2
  echo "Usage: curl -fsSL https://get.corelink.io | sh -s -- --token=<PAT> [--region=<region>]" >&2
  exit 2
fi

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

URL="__RELEASE_ORIGIN__/corelink-\${OS}-\${ARCH}"

echo "Downloading CoreLink CLI from $URL ..."
if ! curl -fsSL "$URL" -o /tmp/corelink; then
  echo "FATAL: failed to download $URL" >&2
  echo "Check https://github.com/humangr-labs/corelink-cli/releases for available binaries." >&2
  exit 3
fi
chmod +x /tmp/corelink

if [ "$(id -u)" -eq 0 ]; then
  mv /tmp/corelink /usr/local/bin/corelink
else
  sudo mv /tmp/corelink /usr/local/bin/corelink
fi

mkdir -p "$HOME/.corelink"
cat > "$HOME/.corelink/config.toml" <<EOF
token = "$TOKEN"
region = "\${REGION:-auto}"
endpoint = "__DEFAULT_API_ENDPOINT__"
EOF
chmod 600 "$HOME/.corelink/config.toml"

corelink ping
echo "Next: cd into your Bazel repo, run: corelink bazel-init"
`;
