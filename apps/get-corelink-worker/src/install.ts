/**
 * Canonical CoreLink CLI install shell-script template.
 *
 * Served verbatim by the `get-corelink-worker` at https://corelink-get.humangr.com.
 * Two placeholders are interpolated at render time from `Env`:
 *
 *   - `__RELEASE_ORIGIN__` → `env.RELEASE_ORIGIN`
 *     (e.g. `https://github.com/HumanGuardrail/corelink-cli/releases/latest/download`)
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
 *   4. Final command is `corelink whoami` (NOT `corelink --version` or a
 *      no-op). It was `corelink ping` until 2026-08-02 — a subcommand that has
 *      NEVER existed, so the public one-liner ended in a clap "unrecognized
 *      subcommand" error and a non-zero exit for every customer, and the
 *      documented `curl … | sh … && corelink doctor` never reached its `&&`.
 *      `whoami` is also more than a check: it caches `tenant_id` into
 *      ~/.corelink/config.toml, which this script does not write and every
 *      later tenant-scoped command needs. `doctor` would be wrong — it does a
 *      real prod CAS write, so it fails for a legitimately read-only PAT.
 *      Invariant 4b in the tests now checks the verb against the CLI's actual
 *      clap enum, because the old test string-matched `corelink ping` and so
 *      PINNED the defect rather than catching it.
 *
 *      NOTE: this line was documented as what flips the `/welcome` SSE pane
 *      from "Waiting" to "Connected". It never did, and this script still is
 *      not what does it. The pane waits on a `first_cli_authed` analytics
 *      event, and as of this commit NOTHING LIVE EMITS IT: the only emitter in
 *      the tree is `apps/cas-worker/src/middleware/analytics.ts`, and that app
 *      has no entrypoint and no wrangler config, so it is not deployable. The
 *      pane therefore waits forever. Wiring a real emitter is tracked
 *      separately; do not describe it here until it ships — a comment in the
 *      file that serves the public installer is an executable surface the
 *      docs-reality gate cannot scan, which is precisely where an unshipped
 *      claim survives longest.
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
# CoreLink CLI installer — served from https://corelink-get.humangr.com.
# Source: github.com/HuGR-Labs/corelink-cli :: apps/get-corelink-worker.
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
  echo "Usage: curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=<PAT> [--region=<region>]" >&2
  exit 2
fi

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
# \`uname -m\` and the published asset names DISAGREE, and the disagreement is
# not cosmetic: macOS on Apple Silicon reports \`arm64\`, while the release asset
# is \`corelink-darwin-aarch64\`. Passing uname's answer through unmapped built a
# URL that 404s, so every Apple Silicon Mac got \`FATAL: failed to download\` —
# the most common developer machine, and the exact platform the tutorial used
# as its worked example ("detected darwin/arm64"). Linux was unaffected, since
# uname already says aarch64/x86_64 there; that is why this survived — it fails
# on laptops, not in CI. Normalise here rather than renaming assets, so releases
# already published keep working.
ARCH=$(uname -m)
case "$ARCH" in
  arm64|aarch64) ARCH="aarch64" ;;
  x86_64|amd64) ARCH="x86_64" ;;
  *)
    echo "FATAL: unsupported architecture: $ARCH" >&2
    echo "Supported: arm64/aarch64, x86_64/amd64." >&2
    exit 3
    ;;
esac

URL="__RELEASE_ORIGIN__/corelink-\${OS}-\${ARCH}"

echo "Downloading CoreLink CLI from $URL ..."
if ! curl -fsSL "$URL" -o /tmp/corelink; then
  echo "FATAL: failed to download $URL" >&2
  echo "Check __RELEASE_ORIGIN__ for available binaries." >&2
  exit 3
fi

# Verify before making it executable. This is a curl-pipe-sh installer, so what
# it fetches runs with the user's privileges; every release already publishes a
# \`.sha256\` beside its asset, and not checking it left a published integrity
# signal unused. A missing checksum REFUSES rather than silently degrading to
# trusting the transport.
if ! curl -fsSL "$URL.sha256" -o /tmp/corelink.sha256; then
  echo "FATAL: no checksum published for $URL" >&2
  rm -f /tmp/corelink
  exit 3
fi
EXPECTED=$(cut -d' ' -f1 < /tmp/corelink.sha256)
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL=$(sha256sum /tmp/corelink | cut -d' ' -f1)
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL=$(shasum -a 256 /tmp/corelink | cut -d' ' -f1)
else
  echo "FATAL: neither sha256sum nor shasum available; cannot verify the download" >&2
  rm -f /tmp/corelink /tmp/corelink.sha256
  exit 3
fi
if [ "$EXPECTED" != "$ACTUAL" ]; then
  echo "FATAL: checksum mismatch for $URL" >&2
  echo "  expected: $EXPECTED" >&2
  echo "  actual:   $ACTUAL" >&2
  rm -f /tmp/corelink /tmp/corelink.sha256
  exit 3
fi
rm -f /tmp/corelink.sha256
echo "Checksum OK."
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

corelink whoami
echo "Next: cd into your Bazel repo, run: corelink bazel-init"
`;
