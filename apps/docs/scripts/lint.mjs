#!/usr/bin/env node
// PII/PAT lint check (CTRL-CRED-001 + CTRL-PRIV-001 reflection per WI-S18-003 §13).
//
// Walks all docs/**/*.mdx files and grep-rejects:
//   - PAT-like tokens that look real (regex tightened to avoid false positives
//     on placeholders `corelink_dev_t_xxx.xxx.xxx`).
//   - `--pat` CLI flag in examples (CTRL-CRED-001 forbids; env var only).
//
// Exit 0 on clean; 1 on any hit.

import { readdir, readFile } from "node:fs/promises";
import { join, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const docsRoot = resolve(here, "..", "docs");

// PAT format: corelink_<env>_<token_id>.<random_secret>.<hmac_sig>
// Placeholder canonical: corelink_dev_t_xxx.xxx.xxx
// A "real" PAT has more than 6 chars in each of the 3 dot-segments after the env_,
// or uses an env other than dev/test/local AND non-placeholder bytes.
const PAT_REAL = /corelink_(?:prod|production|live|staging|stg)_[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}/;
// Forbidden literal flag usage in examples.
const PAT_CLI_FLAG = /\bcorelink\b[^\n`]*--pat[=\s]/;

/** @param {string} root */
async function walk(root) {
  /** @type {string[]} */
  const out = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const full = join(root, entry.name);
    if (entry.isDirectory()) out.push(...(await walk(full)));
    else if (entry.isFile() && full.endsWith(".mdx")) out.push(full);
  }
  return out;
}

/**
 * Decide whether a `--pat`-bearing line is a legitimate anti-example.
 *
 * Anti-examples are tolerated when the surrounding fenced code block opens
 * with (or is immediately preceded by) one of the canonical "do not do this"
 * markers. Anything else is a real violation.
 */
function isAntiExample(lines, idx) {
  // Look back at most 6 lines for an anti-example marker.
  const start = Math.max(0, idx - 6);
  const window = lines.slice(start, idx).join("\n").toLowerCase();
  return (
    window.includes("do not do this") ||
    window.includes("forbidden") ||
    window.includes("wrong —") ||
    window.includes("wrong --")
  );
}

async function main() {
  const files = await walk(docsRoot);
  let violations = 0;
  for (const f of files) {
    const src = await readFile(f, "utf8");
    const lines = src.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (PAT_REAL.test(line)) {
        console.error(`lint: real-PAT-like token detected (CTRL-CRED-001): ${f}:${i + 1}`);
        violations++;
      }
      if (PAT_CLI_FLAG.test(line) && !isAntiExample(lines, i)) {
        console.error(`lint: forbidden --pat CLI flag in example (CTRL-CRED-001): ${f}:${i + 1}`);
        violations++;
      }
    }
  }
  if (violations > 0) {
    console.error(`lint: ${violations} violation(s) found`);
    process.exit(1);
  }
  console.log(`lint: clean (${files.length} files scanned)`);
}

main().catch((e) => {
  console.error("lint: unexpected error:", e);
  process.exit(1);
});
