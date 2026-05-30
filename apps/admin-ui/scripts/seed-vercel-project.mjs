#!/usr/bin/env node
/*
 * Seed a minimal `.vercel/project.json` so `vercel build` runs locally
 * without prompting for `--yes` (which triggers a token check against
 * Vercel's API, even though we never deploy to Vercel — we only need
 * the build output to feed @cloudflare/next-on-pages).
 *
 * When `.vercel/project.json` exists, `vercel build` treats the project
 * as already linked and skips the auth check entirely.
 */

import fs from "node:fs";
import path from "node:path";

const projectDir = path.resolve(".vercel");
const projectFile = path.join(projectDir, "project.json");

if (!fs.existsSync(projectDir)) fs.mkdirSync(projectDir, { recursive: true });

// IDs are arbitrary — they're only used for Vercel's own deploy targeting,
// which we never reach. The build pipeline only reads them as presence.
const seed = {
  projectId: "corelink-admin-ui-local",
  orgId: "corelink-local",
  settings: {
    framework: "nextjs",
  },
};

fs.writeFileSync(projectFile, JSON.stringify(seed, null, 2) + "\n");
console.log(`[seed-vercel-project] wrote ${projectFile}`);
