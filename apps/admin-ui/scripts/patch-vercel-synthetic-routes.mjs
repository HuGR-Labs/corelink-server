#!/usr/bin/env node
/*
 * Replace Next.js's synthetic Node.js-runtime functions
 * (`/_not-found`, `/_not-found.rsc`, `/_error`, `/_error.rsc`) with
 * minimal Edge-runtime handlers, so `@cloudflare/next-on-pages` accepts
 * the build.
 *
 * Background — Next.js 15 always emits a `/_not-found` route as a
 * Node.js function (launcher uses `async_hooks`), regardless of:
 *   - `not-found.tsx` exporting `runtime = "edge"`
 *   - the root layout being `runtime = "edge"`
 *   - `dynamic = "force-static"` at the route level
 *
 * Until Next.js exposes a route-segment knob for the synthetic routes
 * (or we migrate to `@opennextjs/cloudflare`), this script post-processes
 * `.vercel/output/functions/` to swap each Node.js synthetic function
 * for a self-contained Edge handler that returns the equivalent error
 * page. The replacement reuses one sibling Edge function's `.vc-config`
 * environment block so the build IDs / preview-mode keys stay consistent
 * with the rest of the deployment.
 *
 * Invoked between `next build` and `npx @cloudflare/next-on-pages` in
 * the `pages:build` package.json script.
 */

import fs from "node:fs";
import path from "node:path";

const FNS_DIR = path.resolve(".vercel/output/functions");

if (!fs.existsSync(FNS_DIR)) {
  console.error(`[patch-synthetic] not found: ${FNS_DIR}`);
  process.exit(1);
}

// Pull a reference `environment` block from any existing edge function so
// the replacements carry the same Next.js build IDs + preview-mode keys.
function findReferenceEdgeEnv(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const child = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name.endsWith(".func")) {
        const cfgPath = path.join(child, ".vc-config.json");
        if (fs.existsSync(cfgPath)) {
          const cfg = JSON.parse(fs.readFileSync(cfgPath, "utf8"));
          if (cfg.runtime === "edge" && cfg.environment) {
            return cfg.environment;
          }
        }
      } else {
        const nested = findReferenceEdgeEnv(child);
        if (nested) return nested;
      }
    }
  }
  return null;
}

const referenceEnv = findReferenceEdgeEnv(FNS_DIR) ?? {};

// Synthetic routes we replace. Each pair is (function-dir-name, body, status).
const SYNTHETIC = [
  {
    dir: "_not-found.func",
    status: 404,
    body: "<!doctype html><html><body><h1>404 — Page not found</h1></body></html>",
  },
  {
    dir: "_not-found.rsc.func",
    status: 404,
    body: '{"error":"not_found"}',
    contentType: "application/json",
  },
  {
    dir: "_error.func",
    status: 500,
    body: "<!doctype html><html><body><h1>500 — Server error</h1></body></html>",
  },
  {
    dir: "_error.rsc.func",
    status: 500,
    body: '{"error":"server_error"}',
    contentType: "application/json",
  },
];

const HANDLER_TEMPLATE = (status, body, contentType) => `
const BODY = ${JSON.stringify(body)};
const STATUS = ${status};
const HEADERS = { "content-type": ${JSON.stringify(contentType ?? "text/html; charset=utf-8")} };

export default {
  async fetch(_request) {
    return new Response(BODY, { status: STATUS, headers: HEADERS });
  },
};
`.trimStart();

let patched = 0;
for (const route of SYNTHETIC) {
  const dir = path.join(FNS_DIR, route.dir);
  if (!fs.existsSync(dir)) continue;

  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });

  const config = {
    runtime: "edge",
    name: route.dir.replace(/\.func$/, ""),
    deploymentTarget: "v8-worker",
    entrypoint: "index.js",
    assets: [],
    framework: { slug: "nextjs", version: "15.5.18" },
    environment: referenceEnv,
  };
  fs.writeFileSync(
    path.join(dir, ".vc-config.json"),
    JSON.stringify(config, null, 2) + "\n",
  );
  fs.writeFileSync(
    path.join(dir, "index.js"),
    HANDLER_TEMPLATE(route.status, route.body, route.contentType),
  );
  patched += 1;
  console.log(`[patch-synthetic] replaced ${route.dir} → edge runtime`);
}

console.log(`[patch-synthetic] patched ${patched}/${SYNTHETIC.length} routes`);
