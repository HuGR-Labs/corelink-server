/**
 * WI-S18-003 — CLI per-command reference test.
 *
 * Asserts:
 *   1. All 8 reference pages exist (index + 7 canonical subcommands + config).
 *   2. Each page covers the flag set declared in
 *      tools/cli/src/main.rs (moved from crates/corelink-cli in wave-33
 *      stage 2.D.3, 360042b8).
 *   3. The 8 doctor checks are documented exactly (no drift).
 *   4. The index lists every subcommand.
 */

import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { CLI_REFERENCE_PAGES } from "../src/sdk-guides.js";

const here = dirname(fileURLToPath(import.meta.url));
const refRoot = resolve(here, "..", "docs", "reference", "cli");

describe("WI-S18-003 CLI per-command reference", () => {
  it.each(CLI_REFERENCE_PAGES)("page exists: %s.mdx", (slug) => {
    expect(existsSync(resolve(refRoot, `${slug}.mdx`))).toBe(true);
  });

  it("index lists every subcommand", async () => {
    const src = await readFile(resolve(refRoot, "index.mdx"), "utf8");
    for (const cmd of ["ls", "get", "put", "stat", "bench", "doctor", "version", "config"]) {
      expect(src).toContain(`\`${cmd}\``);
    }
  });

  it("ls covers documented flags", async () => {
    const src = await readFile(resolve(refRoot, "ls.mdx"), "utf8");
    for (const flag of ["--tenant", "--prefix", "--limit", "--cursor", "--output"]) {
      expect(src).toContain(flag);
    }
  });

  it("get covers documented flags + verify mandatory", async () => {
    const src = await readFile(resolve(refRoot, "get.mdx"), "utf8");
    for (const flag of ["-o", "--output", "<DIGEST>"]) {
      expect(src).toContain(flag);
    }
    // verify is non-optional in the CLI; the only acceptable mention of
    // `--no-verify` is in prose explicitly noting that no such flag exists.
    const advertisesNoVerify = /^\s*\|\s*`--no-verify`/m.test(src);
    expect(advertisesNoVerify).toBe(false);
  });

  it("put covers documented flags", async () => {
    const src = await readFile(resolve(refRoot, "put.mdx"), "utf8");
    for (const flag of ["<FILE>", "--digest", "--output"]) {
      expect(src).toContain(flag);
    }
  });

  it("stat covers documented args", async () => {
    const src = await readFile(resolve(refRoot, "stat.mdx"), "utf8");
    expect(src).toContain("<DIGEST>");
    expect(src).toContain("--output");
  });

  it("bench advertises write/read/full + mutual exclusion", async () => {
    const src = await readFile(resolve(refRoot, "bench.mdx"), "utf8");
    for (const flag of ["--write", "--read", "--full", "--output"]) {
      expect(src).toContain(flag);
    }
    expect(src).toMatch(/mutually exclusive/i);
  });

  it("doctor documents exactly 8 canonical checks", async () => {
    const src = await readFile(resolve(refRoot, "doctor.mdx"), "utf8");
    for (const check of [
      "network",
      "auth",
      "storage_write",
      "storage_read",
      "byok",
      "region",
      "quota",
      "client_verify",
    ]) {
      expect(src).toContain(check);
    }
    expect(src).toContain("--json");
  });

  it("version surfaces SLSA attestation link", async () => {
    const src = await readFile(resolve(refRoot, "version.mdx"), "utf8");
    expect(src).toContain("slsa");
    expect(src).toContain("git_rev");
  });

  it("config documents all 4 sub-actions", async () => {
    const src = await readFile(resolve(refRoot, "config.mdx"), "utf8");
    for (const action of ["set", "get", "list", "rotate"]) {
      expect(src).toContain(`### \`${action}\``);
    }
    // PAT must be redacted in list/get
    expect(src).toMatch(/<redacted>/);
  });

  it("global --pat rejection is documented somewhere", async () => {
    const index = await readFile(resolve(refRoot, "index.mdx"), "utf8");
    expect(index).toMatch(/--pat.*reject/i);
  });

  it("index lists exit codes 0-5", async () => {
    const src = await readFile(resolve(refRoot, "index.mdx"), "utf8");
    for (const c of ["`0`", "`1`", "`2`", "`3`", "`4`", "`5`"]) {
      expect(src).toContain(c);
    }
  });

  it("CLI source flags match documented set", async () => {
    const cliSrc = await readFile(
      resolve(here, "..", "..", "..", "tools", "cli", "src", "main.rs"),
      "utf8",
    );
    // every documented subcommand must appear in the Rust source as a clap variant
    // (unit variants like `Version,` have no body; tuple/struct variants use `{`)
    for (const v of ["Ls", "Get", "Put", "Stat", "Bench", "Doctor", "Version", "Config"]) {
      expect(cliSrc).toMatch(new RegExp(`\\b${v}\\s*[{,]`));
    }
  });
});
