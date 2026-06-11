#!/usr/bin/env node
// strip-client-sourcemaps.mjs — remove browser-served .js.map files from the
// OpenNext deployable assets before deploy.
//
// Why: the OpenNext/Next build emits 138 `*.js.map` files under
// `.open-next/assets/_next/static/`, which the Worker serves publicly. A
// 2026-06-11 black-box pentest confirmed they were live in prod
// (HTTP 200 + valid `{"version":3,...}` exposing exact dep versions and
// original module paths — eases reverse-engineering + CVE targeting).
//
// We strip ONLY the client-served maps under `.open-next/assets/`. Server-side
// maps elsewhere in `.open-next/` are not edge-served and are left intact.
//
// Wired into `cf:build` so every deploy is clean. Idempotent.
import { readdirSync, statSync, rmSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const assetsRoot = join(
  fileURLToPath(new URL(".", import.meta.url)),
  "..",
  ".open-next",
  "assets",
);

let removed = 0;
function walk(dir) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return; // assets dir absent (e.g. build skipped) — nothing to strip
  }
  for (const name of entries) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      walk(p);
    } else if (name.endsWith(".js.map") || name.endsWith(".css.map")) {
      rmSync(p);
      removed += 1;
    }
  }
}

walk(assetsRoot);
console.log(`[strip-client-sourcemaps] removed ${removed} source map(s) from .open-next/assets`);
