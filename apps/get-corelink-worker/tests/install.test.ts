/**
 * Unit tests for the install-script template renderer.
 *
 * Asserts the script invariants documented in `src/install.ts`:
 *   1. POSIX-sh shebang + `set -eu` is the first executable line.
 *   2. `--token=…` parsing branch exists and exits non-zero when absent.
 *   3. OS / ARCH derived from `uname -s` (lowercased) and `uname -m`.
 *   4. Final shell command is `corelink ping`.
 *   5. Placeholder substitution actually substitutes — no `__RELEASE_ORIGIN__`
 *      or `__DEFAULT_API_ENDPOINT__` markers leak through to the rendered
 *      output.
 *   6. End-of-file "Next:" hint is present (this is the developer's
 *      next-action signpost from the Phase 0 plan).
 */

import { describe, expect, it } from "vitest";
import { renderInstallScript } from "../src/install.ts";

const FIXTURE = {
  releaseOrigin: "https://github.com/humangr-labs/corelink-cli/releases/latest/download",
  defaultApiEndpoint: "https://corelink-api.humangr.com",
} as const;

describe("renderInstallScript", () => {
  it("starts with a POSIX-sh shebang", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script.startsWith("#!/bin/sh\n")).toBe(true);
  });

  it("invariant 1 :: set -eu appears before any executable command", () => {
    const script = renderInstallScript(FIXTURE);
    // Strip the shebang + comment lines, then the first non-comment line must
    // be `set -eu`.
    const firstExecLine = script
      .split("\n")
      .slice(1)
      .find((l) => l.trim().length > 0 && !l.trim().startsWith("#"));
    expect(firstExecLine).toBe("set -eu");
  });

  it("invariant 2 :: requires --token, exits 2 with FATAL message when absent", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain('if [ -z "$TOKEN" ]; then');
    expect(script).toContain('echo "FATAL: --token required" >&2');
    expect(script).toContain("exit 2");
  });

  it("invariant 3 :: derives OS from `uname -s` (lowercased) and ARCH from `uname -m`", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain("OS=$(uname -s | tr '[:upper:]' '[:lower:]')");
    expect(script).toContain("ARCH=$(uname -m)");
  });

  it("invariant 4 :: final non-comment line invokes `corelink ping`", () => {
    const script = renderInstallScript(FIXTURE);
    // The very last executable command before the "Next:" hint must be
    // `corelink ping` — this is what flips the /welcome SSE pane.
    expect(script).toContain("corelink ping\n");
    // The Next-hint comes AFTER the ping invocation.
    const pingIdx = script.indexOf("corelink ping\n");
    const nextIdx = script.indexOf('echo "Next:');
    expect(nextIdx).toBeGreaterThan(pingIdx);
  });

  it("invariant 5 :: no placeholder markers leak through to rendered output", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).not.toContain("__RELEASE_ORIGIN__");
    expect(script).not.toContain("__DEFAULT_API_ENDPOINT__");
  });

  it("invariant 6 :: includes the next-action signpost (Bazel init)", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain('echo "Next: cd into your Bazel repo, run: corelink bazel-init"');
  });

  it("substitutes RELEASE_ORIGIN into the download URL", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain(
      'URL="https://github.com/humangr-labs/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}"',
    );
  });

  it("substitutes DEFAULT_API_ENDPOINT into the config.toml heredoc", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain('endpoint = "https://corelink-api.humangr.com"');
  });

  it("hardens config.toml file permissions to 0600 (token is secret-equivalent)", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain('chmod 600 "$HOME/.corelink/config.toml"');
  });

  it("falls back to non-sudo move when running as root (CI runners)", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain('if [ "$(id -u)" -eq 0 ]; then');
    expect(script).toContain("mv /tmp/corelink /usr/local/bin/corelink");
    expect(script).toContain("sudo mv /tmp/corelink /usr/local/bin/corelink");
  });

  it("supports an alternative release origin (staging override)", () => {
    const staging = renderInstallScript({
      releaseOrigin: "https://staging.example.com/corelink-cli/releases/latest/download",
      defaultApiEndpoint: "https://corelink-api.staging.humangr.com",
    });
    expect(staging).toContain(
      'URL="https://staging.example.com/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}"',
    );
    expect(staging).toContain('endpoint = "https://corelink-api.staging.humangr.com"');
  });

  it("is deterministic — same config yields byte-identical output", () => {
    const a = renderInstallScript(FIXTURE);
    const b = renderInstallScript(FIXTURE);
    expect(a).toBe(b);
  });
});
