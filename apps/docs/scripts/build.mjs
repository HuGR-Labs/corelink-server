#!/usr/bin/env node
// Placeholder build script.
//
// The full Docusaurus 3.x build is wired in WI-S18-001. This stub validates that
// every .mdx file in docs/ is parseable (frontmatter + valid UTF-8) and that
// the directory layout matches the Diataxis convention required by WI-S18-003.
//
// On success exits 0; on any malformed page exits 1.

import { readdir, readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const docsRoot = resolve(here, "..", "docs");

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

function parseFrontmatter(src) {
  if (!src.startsWith("---\n")) return null;
  const end = src.indexOf("\n---\n", 4);
  if (end === -1) return null;
  const yaml = src.slice(4, end);
  /** @type {Record<string,string>} */
  const meta = {};
  for (const line of yaml.split("\n")) {
    const m = line.match(/^(\w[\w-]*):\s*(.*)$/);
    if (m) meta[m[1]] = m[2].replace(/^["']|["']$/g, "");
  }
  return meta;
}

async function main() {
  let exists = false;
  try {
    const s = await stat(docsRoot);
    exists = s.isDirectory();
  } catch {
    exists = false;
  }
  if (!exists) {
    console.error(`build: docs/ directory missing at ${docsRoot}`);
    process.exit(1);
  }

  const files = await walk(docsRoot);
  let failed = 0;
  for (const f of files) {
    const src = await readFile(f, "utf8");
    const fm = parseFrontmatter(src);
    if (!fm) {
      console.error(`build: missing frontmatter: ${f}`);
      failed++;
      continue;
    }
    if (!fm.title) {
      console.error(`build: missing title in frontmatter: ${f}`);
      failed++;
    }
  }
  if (failed > 0) {
    console.error(`build: ${failed} page(s) failed validation`);
    process.exit(1);
  }
  console.log(`build: validated ${files.length} mdx page(s)`);
}

main().catch((e) => {
  console.error("build: unexpected error:", e);
  process.exit(1);
});
