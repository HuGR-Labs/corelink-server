/**
 * Unit tests for the install-script template renderer.
 *
 * Asserts the script invariants documented in `src/install.ts`:
 *   1. POSIX-sh shebang + `set -eu` is the first executable line.
 *   2. `--token=…` parsing branch exists and exits non-zero when absent.
 *   3. OS / ARCH derived from `uname -s` (lowercased) and `uname -m`.
 *   4. Final shell command is `corelink whoami` — AND that verb is checked
 *      against the CLI's real subcommand list, not merely string-matched.
 *   5. Placeholder substitution actually substitutes — no `__RELEASE_ORIGIN__`
 *      or `__DEFAULT_API_ENDPOINT__` markers leak through to the rendered
 *      output.
 *   6. End-of-file "Next:" hint is present (this is the developer's
 *      next-action signpost from the Phase 0 plan).
 */

import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { renderInstallScript } from "../src/install.ts";

/**
 * The CLI's REAL top-level subcommands, parsed from the clap enum.
 *
 * This exists because the previous version of invariant 4 asserted the script
 * contained the literal `corelink ping` — and `ping` has never been a
 * subcommand. The test therefore PINNED the defect: the suite stayed green
 * precisely because it demanded the broken call be present, while the public
 * one-liner at corelink-get.humangr.com ended in a clap "unrecognized
 * subcommand" error and a non-zero exit for every customer.
 *
 * String-matching a verb can only prove the script says what we expect. It
 * cannot prove the verb EXISTS. So read the enum and check membership.
 *
 * Bound the parse by brace depth: `enum Commands` is followed by further
 * subcommand enums (`Export`, `VerifyExport`, …) that are NOT top-level verbs,
 * and a naive line scan silently swallows them.
 */
function cliSubcommands(): string[] {
  const main = readFileSync(
    fileURLToPath(new URL("../../../tools/cli/src/main.rs", import.meta.url)),
    "utf8",
  ).split("\n");
  const start = main.findIndex((l) => l.trim() === "enum Commands {");
  if (start < 0) throw new Error("clap `enum Commands` not found in tools/cli/src/main.rs");
  const verbs: string[] = [];
  let depth = 0;
  for (const line of main.slice(start)) {
    depth += (line.match(/{/g) ?? []).length - (line.match(/}/g) ?? []).length;
    if (depth === 0 && line.trim().startsWith("}")) break;
    const m = /^ {4}([A-Z][A-Za-z0-9]*)\s*[,{]/.exec(line);
    // clap renames variants to kebab-case by default (BazelInit -> bazel-init).
    if (m) verbs.push(m[1].replace(/(?<!^)(?=[A-Z])/g, "-").toLowerCase());
  }
  if (verbs.length === 0) throw new Error("parsed zero subcommands — the parser is broken");
  return verbs;
}

const FIXTURE = {
  releaseOrigin: "https://github.com/HumanGuardrail/corelink-cli/releases/latest/download",
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

  it("invariant 4 :: the final command is `corelink whoami`", () => {
    const script = renderInstallScript(FIXTURE);
    // `whoami` is not just a liveness check: it caches `tenant_id` into
    // ~/.corelink/config.toml, which the installer does NOT write and every
    // later tenant-scoped command needs. `doctor` would be wrong here — it
    // performs a real prod CAS write, so it fails for a read-only PAT.
    expect(script).toContain("corelink whoami\n");
    const verbIdx = script.indexOf("corelink whoami\n");
    const nextIdx = script.indexOf('echo "Next:');
    expect(nextIdx).toBeGreaterThan(verbIdx);
  });

  it("invariant 4b :: EVERY `corelink <verb>` the script runs actually exists in the CLI", () => {
    // The regression that makes 4b matter more than 4: the old test asserted
    // the literal string `corelink ping`, and `ping` has never existed. Green
    // suite, broken installer, for every customer. Membership beats matching.
    const script = renderInstallScript(FIXTURE);
    const real = cliSubcommands();
    expect(real).toContain("whoami"); // the parser found a plausible enum

    const invoked = [...script.matchAll(/(?:^|[\s;&|(])corelink\s+([a-z][a-z0-9-]*)/gm)]
      .map((m) => m[1]);
    expect(invoked.length).toBeGreaterThan(0); // never vacuously pass

    const phantom = invoked.filter((v) => !real.includes(v));
    expect(phantom, `install script invokes non-existent subcommand(s): ${phantom.join(", ")}`)
      .toEqual([]);
  });

  // ── Architecture mapping (2026-08-02) ──────────────────────────────────────
  // `uname -m` says `arm64` on Apple Silicon; the published release asset is
  // `corelink-darwin-aarch64`. Passing uname through unmapped built a URL that
  // 404s, so EVERY Apple Silicon Mac got "FATAL: failed to download" — verified
  // against the real release: the fixed script fetches corelink-darwin-aarch64
  // and reports "Checksum OK.", the old one 404s. Linux was unaffected (uname
  // already says aarch64/x86_64 there), which is why this survived so long — it
  // failed on laptops, not in CI.
  //
  // These EXECUTE the mapping with a stubbed `uname` rather than string-matching
  // it, so they measure behaviour and not spelling.
  const archFor = (unameM: string): { arch: string; code: number } => {
    const script = renderInstallScript(FIXTURE);
    const re = new RegExp(String.raw`ARCH=\$\(uname -m\)\ncase "\$ARCH" in[\s\S]*?\nesac`);
    const block = re.exec(script);
    if (!block) throw new Error("ARCH mapping block not found in the rendered script");
    const dir = mkdtempSync(join(tmpdir(), "corelink-arch-"));
    writeFileSync(join(dir, "uname"), `#!/bin/sh\necho "${unameM}"\n`, { mode: 0o755 });
    const r = spawnSync("sh", ["-c", `${block[0]}\necho "$ARCH"`], {
      env: { PATH: `${dir}:/usr/bin:/bin` },
      encoding: "utf8",
    });
    return { arch: r.stdout.trim(), code: r.status ?? -1 };
  };

  it("arch :: Apple Silicon `arm64` maps to the published `aarch64` asset name", () => {
    expect(archFor("arm64")).toEqual({ arch: "aarch64", code: 0 });
  });

  it("arch :: the platforms that already worked still map to themselves", () => {
    expect(archFor("aarch64").arch).toBe("aarch64");
    expect(archFor("x86_64").arch).toBe("x86_64");
    expect(archFor("amd64").arch).toBe("x86_64");
  });

  it("arch :: an unsupported architecture FAILS instead of building a 404 URL", () => {
    expect(archFor("riscv64").code).not.toBe(0);
  });

  it("integrity :: the download is checksum-verified BEFORE it is made executable", () => {
    const script = renderInstallScript(FIXTURE);
    expect(script).toContain('curl -fsSL "$URL.sha256"');
    expect(script).toMatch(/sha256sum|shasum -a 256/);
    // Order matters: a binary chmod'd before the check is already a usable
    // artefact on disk.
    expect(script.indexOf("checksum mismatch")).toBeLessThan(
      script.indexOf("chmod +x /tmp/corelink"),
    );
    // A missing checksum must REFUSE, not fall back to trusting the transport.
    expect(script).toContain("FATAL: no checksum published");
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
      'URL="https://github.com/HumanGuardrail/corelink-cli/releases/latest/download/corelink-${OS}-${ARCH}"',
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
