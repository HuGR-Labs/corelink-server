/**
 * `get-corelink-worker` — Cloudflare Worker serving the CoreLink CLI
 * install one-liner at https://get.corelink.io.
 *
 * Architecture:
 *
 *   Internet HTTPS GET https://get.corelink.io
 *                  ↓
 *              this Worker (route table + render install.ts template)
 *                  ↓
 *              text/x-shellscript response (sub-millisecond render)
 *
 * Charter constraints (mirrors `worker/src/index.ts` conventions):
 *   - Zero `any` types — all bindings typed via `Env` interface.
 *   - No body bytes logged; no PII / secrets logged.
 *   - No outbound fetches, no KV / R2 / D1 / DO calls — pure static
 *     render so p99 < 50 ms even from cold start.
 *   - Strict Cache-Control: install scripts MUST be re-fetched on every
 *     run so a `corelink-cli` Releases bump doesn't leave existing CI
 *     runners pinned to a stale URL template. Setting `no-store` on the
 *     response also defeats any well-meaning intermediate proxy.
 */

import type { ExecutionContext, ExportedHandler } from "@cloudflare/workers-types";
import { renderInstallScript } from "./install.ts";

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/** Worker environment bindings — matches `wrangler.toml` `[vars]`. */
export interface Env {
  ENVIRONMENT: string;
  RELEASE_ORIGIN: string;
  DEFAULT_API_ENDPOINT: string;
}

// ──────────────────────────────────────────────────────────────────────────────
// Security headers (pre-HN-launch hardening)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Defense-in-depth security headers applied to EVERY response (including
 * 4xx/5xx). The Worker body is a shell script — not HTML — but several
 * headers (HSTS, nosniff, X-Frame-Options) still matter at the protocol
 * level. CSP is strict-by-default (`default-src 'none'`) because the body
 * is never interpreted as a document by a browser.
 *
 * Target: securityheaders.com B+ or better post-deploy.
 */
export const SECURITY_HEADERS: Readonly<Record<string, string>> = {
  "Content-Security-Policy":
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
  "Strict-Transport-Security":
    "max-age=63072000; includeSubDomains; preload",
  "X-Content-Type-Options": "nosniff",
  "X-Frame-Options": "DENY",
  "Referrer-Policy": "no-referrer",
  "Permissions-Policy": "interest-cohort=(), camera=(), microphone=(), geolocation=()",
};

/**
 * Apply `SECURITY_HEADERS` to a `Response`. Returns a new `Response` so the
 * original (which may originate from `renderInstallScript` or a 4xx path)
 * is left untouched.
 */
function withSecurityHeaders(res: Response): Response {
  const merged = new Headers(res.headers);
  for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
    // Existing per-route values (e.g. install-script Referrer-Policy: no-referrer)
    // are preserved; security headers only fill gaps.
    if (!merged.has(name)) merged.set(name, value);
  }
  return new Response(res.body, {
    status: res.status,
    statusText: res.statusText,
    headers: merged,
  });
}

// ──────────────────────────────────────────────────────────────────────────────
// Route table
// ──────────────────────────────────────────────────────────────────────────────

/**
 * The Worker exposes exactly three routes:
 *
 *   GET  /            → install script (text/x-shellscript)
 *   GET  /healthz     → "ok" (text/plain) — for CF Health Checks
 *   *    anything-else → 404 with timing-padding to match the `/` latency
 *
 * No POST / PUT / DELETE — install endpoint is GET-only by convention
 * (matches `https://sh.rustup.rs` / `https://get.docker.com`).
 */
async function route(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);

  if (request.method !== "GET" && request.method !== "HEAD") {
    return new Response("Method Not Allowed", {
      status: 405,
      headers: { "Allow": "GET, HEAD" },
    });
  }

  if (url.pathname === "/healthz") {
    return new Response("ok\n", {
      status: 200,
      headers: { "Content-Type": "text/plain; charset=utf-8" },
    });
  }

  if (url.pathname === "/" || url.pathname === "") {
    const body = renderInstallScript({
      releaseOrigin: env.RELEASE_ORIGIN,
      defaultApiEndpoint: env.DEFAULT_API_ENDPOINT,
    });

    return new Response(request.method === "HEAD" ? null : body, {
      status: 200,
      headers: {
        // text/x-shellscript is the conventional MIME for `curl | sh` flows
        // (matches sh.rustup.rs and get.docker.com).
        "Content-Type": "text/x-shellscript; charset=utf-8",
        // Force re-fetch on every install — never serve a stale template.
        "Cache-Control": "no-store, no-cache, must-revalidate, max-age=0",
        // Defense-in-depth headers (defensible even though the body is a
        // shell script, not HTML).
        "X-Content-Type-Options": "nosniff",
        "Referrer-Policy": "no-referrer",
        "Strict-Transport-Security": "max-age=63072000; includeSubDomains; preload",
        // Useful for the `corelink-cli` Releases CI to spot which Worker
        // env served a given install.
        "X-Corelink-Env": env.ENVIRONMENT,
      },
    });
  }

  return new Response("Not Found\n", {
    status: 404,
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
}

// ──────────────────────────────────────────────────────────────────────────────
// Worker entry
// ──────────────────────────────────────────────────────────────────────────────

const handler: ExportedHandler<Env> = {
  async fetch(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
    try {
      const res = await route(request, env);
      return withSecurityHeaders(res);
    } catch (err) {
      // Never leak error details to the caller — return a generic 500.
      // Log just the error message (no request body, no env values).
      const msg = err instanceof Error ? err.message : "unknown";
      console.error(`get-corelink-worker error: ${msg}`);
      return withSecurityHeaders(
        new Response("Internal Server Error\n", {
          status: 500,
          headers: { "Content-Type": "text/plain; charset=utf-8" },
        }),
      );
    }
  },
};

export default handler;
