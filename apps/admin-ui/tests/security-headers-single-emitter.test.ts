/**
 * Regression: `STATIC_SECURITY_HEADERS` must have EXACTLY ONE emitter per
 * response (fix/reporting-endpoints-emitted-twice).
 *
 * Defect, observed live on `/corelink/sign-in` minutes after #981 shipped:
 *
 *   reporting-endpoints: csp-endpoint="/corelink/api/csp-report", csp-endpoint="/corelink/api/csp-report"
 *
 * One header line, the same RFC 8941 dictionary key twice — and identically on
 * `x-frame-options`, `referrer-policy` and `permissions-policy`. (HSTS and
 * `x-content-type-options` are masked: the Cloudflare zone's `security_header`
 * setting overwrites those two at the edge.)
 *
 * Cause: the static set had two emitters whose path coverage OVERLAPPED —
 * `next.config.ts`'s `headers()` on `source: "/:path*"`, and the
 * `res.headers.set()` loop in `src/middleware.ts`. `set()` replaces, so
 * middleware alone can never double a header; but `@opennextjs/cloudflare`
 * merges the two emitters as PLAIN OBJECT KEYS before the Worker constructs its
 * `Headers`, and the two disagree on case (`Reporting-Endpoints` from our
 * `h.key`, verbatim, vs `reporting-endpoints` from the middleware `Response`).
 * Both keys survive the merge, and `new Headers({...})` APPENDS each entry:
 *
 *   new Headers({ "x-frame-options": "DENY", "X-Frame-Options": "DENY" })
 *     .get("x-frame-options")  ===  "DENY, DENY"
 *
 * Neither emitter could simply be deleted — the middleware matcher skips static
 * assets, and OpenNext's routing layer discards the `headers()` set whenever
 * middleware short-circuits (e.g. the 307 to /sign-in). So the two are
 * partitioned by path, and this test is the thing that keeps them partitioned.
 *
 * The load-bearing assertion is `exactly one emitter per path`: restoring
 * `source: "/:path*"` on either side makes `/sign-in` doubly-covered and fails.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { config } from "@/middleware";
import {
  MIDDLEWARE_EXCLUDED_ASSET_PATHS,
  SECURITY_HEADER_ROUTE_SOURCES,
  STATIC_SECURITY_HEADERS,
  securityHeaderRoutes,
} from "@/lib/csp";

const here = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(here, "..");

/**
 * Minimal path-to-regexp stand-in — `SECURITY_HEADER_ROUTE_SOURCES` only ever
 * uses literal segments and a trailing `/:path*`, which is all this handles.
 * Deliberately NOT the real matcher: a test that reimplements the production
 * matcher proves nothing about it.
 */
function sourceToRegExp(source: string): RegExp {
  const wildcard = "/:path*";
  const hasWildcard = source.endsWith(wildcard);
  const literal = hasWildcard ? source.slice(0, -wildcard.length) : source;
  const escaped = literal.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`^${escaped}${hasWildcard ? "(?:/.*)?" : ""}$`);
}

/** `config.matcher` entries are already regex sources; anchor them. */
function matcherToRegExp(entry: string): RegExp {
  return new RegExp(`^${entry}$`);
}

const matcherEntries = config.matcher as readonly string[];

/** Does the root middleware run on this (basePath-stripped) path? */
function middlewareCovers(pathname: string): boolean {
  return matcherEntries.some((m) => matcherToRegExp(m).test(pathname));
}

/** Does `next.config.ts`'s `headers()` cover this path? */
function nextConfigCovers(pathname: string): boolean {
  return SECURITY_HEADER_ROUTE_SOURCES.some((s) => sourceToRegExp(s).test(pathname));
}

/**
 * Paths are basePath-STRIPPED: Next prefixes `/corelink` onto both the matcher
 * and the `headers()` sources at build time, so both live in the same space.
 */
const PATHS = [
  // Middleware-covered (the render + redirect surface).
  "/",
  "/sign-in",
  "/dashboard",
  "/welcome",
  "/api/csp-report",
  "/upgrade",
  "/mock-gallery.html",
  // Middleware-excluded (the static-asset surface `headers()` owns).
  "/_next/static/chunks/4328qzuhdjhtk.js",
  "/_next/static/css/a.css",
  "/_next/image",
  "/favicon.ico",
  "/robots.txt",
  "/sitemap.xml",
] as const;

describe("static security headers: exactly one emitter per response", () => {
  it.each(PATHS)("%s is covered by exactly one emitter", (pathname) => {
    const emitters = [
      middlewareCovers(pathname) ? "middleware" : null,
      nextConfigCovers(pathname) ? "next.config headers()" : null,
    ].filter(Boolean);

    expect(
      emitters,
      `${pathname} must have exactly one emitter of STATIC_SECURITY_HEADERS — ` +
        `zero means the response ships bare, two means every header is emitted ` +
        `twice and folded onto one line (got: ${emitters.join(" + ") || "none"})`,
    ).toHaveLength(1);
  });

  it("Reporting-Endpoints specifically has exactly one emitter everywhere", () => {
    const reporting = STATIC_SECURITY_HEADERS.filter(
      (h) => h.name.toLowerCase() === "reporting-endpoints",
    );
    expect(reporting).toHaveLength(1);

    for (const pathname of PATHS) {
      const routes = securityHeaderRoutes().filter((r) =>
        sourceToRegExp(r.source).test(pathname),
      );
      const fromConfig = routes.flatMap((r) =>
        r.headers.filter((h) => h.key.toLowerCase() === "reporting-endpoints"),
      );
      const fromMiddleware = middlewareCovers(pathname) ? reporting : [];
      expect(
        [...fromConfig, ...fromMiddleware],
        `Reporting-Endpoints emitter count on ${pathname}`,
      ).toHaveLength(1);
    }
  });
});

describe("the partition is wired to the real config, not just to itself", () => {
  it("next.config.ts delegates to securityHeaderRoutes() and declares no other source", () => {
    const src = readFileSync(resolve(appRoot, "next.config.ts"), "utf8");
    expect(src).toContain("securityHeaderRoutes()");
    // A blanket source here is the exact shape of the defect: it overlaps the
    // middleware matcher on every rendered path.
    expect(src, "next.config.ts must not re-introduce a blanket headers() source").not.toMatch(
      /source:\s*["'`]\/:path\*/,
    );
    expect(src, "headers() must not hand-roll a source list").not.toMatch(/\bsource:/);
  });

  it("the middleware matcher excludes exactly MIDDLEWARE_EXCLUDED_ASSET_PATHS", () => {
    // Next statically analyses `config.matcher`, so the matcher literal cannot
    // import the constant. Re-read it and compare.
    const src = readFileSync(resolve(appRoot, "src/middleware.ts"), "utf8");
    const alternation = /\(\?!([^)]+)\)/.exec(src)?.[1];
    expect(alternation, "middleware matcher negative lookahead").toBeTypeOf("string");
    expect(alternation?.split("|")).toEqual([...MIDDLEWARE_EXCLUDED_ASSET_PATHS]);
  });

  it("every excluded path has a matching headers() source (no bare assets)", () => {
    expect(SECURITY_HEADER_ROUTE_SOURCES).toHaveLength(
      MIDDLEWARE_EXCLUDED_ASSET_PATHS.length,
    );
    for (const excluded of MIDDLEWARE_EXCLUDED_ASSET_PATHS) {
      const covered = SECURITY_HEADER_ROUTE_SOURCES.some(
        (s) => s === `/${excluded}` || s === `/${excluded}/:path*`,
      );
      expect(covered, `${excluded} is skipped by middleware and needs a headers() source`).toBe(
        true,
      );
    }
  });

  it("every static header (including the CSP report channel) is declared once", () => {
    const names = STATIC_SECURITY_HEADERS.map((h) => h.name.toLowerCase());
    expect(new Set(names).size).toBe(names.length);
  });
});
