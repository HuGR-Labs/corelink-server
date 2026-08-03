/**
 * basePath-blindness regression lock for CLIENT-SIDE absolute paths.
 *
 * The app is path-mounted at `humangr.com/corelink` (`basePath` in
 * next.config.ts). Next re-attaches that prefix to framework-generated URLs
 * only — `next/link`, `router.push`, the Sentry `tunnelRoute`, the static
 * `headers()` source. It NEVER touches a URL a component builds by hand and
 * hands to `fetch()`, `EventSource`, `new URL()`, `window.location.assign()`
 * or a raw `<a href>`.
 *
 * The apex `humangr.com` is a DIFFERENT application (the `hugr-site` marketing
 * Pages project), so a bare client-side `/api/...` does not merely 404 — it is
 * answered by another product:
 *
 *   POST humangr.com/api/v1/customer/billing/portal-session  -> 405
 *   GET  humangr.com/api/welcome/stream                      -> 200 text/html
 *
 * The second one is the dangerous shape: `EventSource` opens happily against
 * HTML and then never delivers an event, so the activation pane waits forever
 * and merely looks empty.
 *
 * This class has now recurred four times (#804 checkout, #894 Clerk widgets,
 * the CSP `report-uri`, and these). The last `describe` in this file is the
 * class-closing guard: a bare `"/api…"` string literal anywhere under `src/`
 * fails the suite unless it is basePath-wrapped on the same line or carries a
 * reviewed, verified-SAFE exemption.
 */

import * as React from "react";
import { render, cleanup } from "@testing-library/react";
import { describe, it, expect, vi, afterEach } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { WelcomeStream } from "@/app/[locale]/(authenticated)/welcome/WelcomeStream";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(HERE, "..");

afterEach(() => cleanup());

// ---------------------------------------------------------------------------
// WelcomeStream — the activation SSE reader
// ---------------------------------------------------------------------------

describe("WelcomeStream basePath", () => {
  function stubEventSource(): string[] {
    const seen: string[] = [];
    class FakeEventSource {
      onmessage: ((e: MessageEvent) => void) | null = null;
      onerror: (() => void) | null = null;
      constructor(url: string) {
        seen.push(url);
      }
      addEventListener(): void {}
      removeEventListener(): void {}
      close(): void {}
    }
    vi.stubGlobal("EventSource", FakeEventSource);
    return seen;
  }

  afterEach(() => vi.unstubAllGlobals());

  it("opens the SSE stream at the basePath-absolute URL, not the apex", () => {
    const seen = stubEventSource();
    render(<WelcomeStream />);
    // A bare `/api/welcome/stream` connects to the apex marketing app, which
    // answers 200 text/html — the stream opens and then never fires.
    expect(seen).toEqual(["/corelink/api/welcome/stream"]);
  });

  it("still honours an explicitly injected streamUrl (test seam intact)", () => {
    const seen = stubEventSource();
    render(<WelcomeStream streamUrl="https://stream.test/sse" />);
    expect(seen).toEqual(["https://stream.test/sse"]);
  });
});

// ---------------------------------------------------------------------------
// Same-origin API-client fallbacks
// ---------------------------------------------------------------------------

function captureFetch(captured: string[], body: unknown): typeof fetch {
  return vi.fn(async (url: string) => {
    captured.push(url);
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }) as unknown as typeof fetch;
}

async function withoutApiUrlEnv<T>(fn: () => Promise<T>): Promise<T> {
  const prev = process.env["NEXT_PUBLIC_CORELINK_API_URL"];
  delete process.env["NEXT_PUBLIC_CORELINK_API_URL"];
  try {
    return await fn();
  } finally {
    if (prev !== undefined) process.env["NEXT_PUBLIC_CORELINK_API_URL"] = prev;
  }
}

describe("API-client same-origin fallback basePath", () => {
  it("AdminClient falls back to /corelink/api, not the apex /api", async () => {
    await withoutApiUrlEnv(async () => {
      vi.resetModules();
      const { AdminClient } = await import("@/lib/admin-client");
      const captured: string[] = [];
      const client = new AdminClient({ fetchImpl: captureFetch(captured, { ops: [] }) });
      await client.listOps();
      expect(captured).toEqual(["/corelink/api/v1/admin/ops"]);
    });
  });

  it("CustomerClient falls back to /corelink/api, not the apex /api", async () => {
    await withoutApiUrlEnv(async () => {
      vi.resetModules();
      const { CustomerClient } = await import("@/lib/customer-client");
      const captured: string[] = [];
      const client = new CustomerClient({ fetchImpl: captureFetch(captured, {}) });
      await client.getBilling();
      expect(captured).toEqual(["/corelink/api/v1/customer/billing"]);
    });
  });

  it("leaves an absolute NEXT_PUBLIC_CORELINK_API_URL untouched (no /corelink graft)", async () => {
    const prev = process.env["NEXT_PUBLIC_CORELINK_API_URL"];
    process.env["NEXT_PUBLIC_CORELINK_API_URL"] = "https://corelink-api.humangr.com";
    try {
      vi.resetModules();
      const { CustomerClient } = await import("@/lib/customer-client");
      const captured: string[] = [];
      const client = new CustomerClient({ fetchImpl: captureFetch(captured, {}) });
      await client.getBilling();
      expect(captured).toEqual(["https://corelink-api.humangr.com/v1/customer/billing"]);
    } finally {
      if (prev === undefined) delete process.env["NEXT_PUBLIC_CORELINK_API_URL"];
      else process.env["NEXT_PUBLIC_CORELINK_API_URL"] = prev;
    }
  });
});

// ---------------------------------------------------------------------------
// The class-closing guard
// ---------------------------------------------------------------------------

/**
 * Strip `//`, block and JSX (`{/* … *\/}`) comments while leaving string
 * literals intact, so prose that merely MENTIONS `/api/...` is not flagged.
 *
 * Regex literals can be mis-read as the start of a comment; the failure mode
 * is deliberately one-sided (the scanner strips more than it should and MISSES
 * a hit) so the guard can never fire on something it did not really see.
 */
function stripComments(src: string): string {
  let out = "";
  let i = 0;
  type S = "code" | "line" | "block" | "sq" | "dq" | "tpl";
  let state: S = "code";
  while (i < src.length) {
    const c = src[i]!;
    const n = src[i + 1];
    if (state === "code") {
      if (c === "/" && n === "/") {
        state = "line";
        i += 2;
        continue;
      }
      if (c === "/" && n === "*") {
        state = "block";
        i += 2;
        continue;
      }
      if (c === '"') state = "dq";
      else if (c === "'") state = "sq";
      else if (c === "`") state = "tpl";
      out += c;
      i += 1;
      continue;
    }
    if (state === "line") {
      if (c === "\n") {
        state = "code";
        out += c;
      }
      i += 1;
      continue;
    }
    if (state === "block") {
      if (c === "*" && n === "/") {
        state = "code";
        i += 2;
      } else {
        if (c === "\n") out += c;
        i += 1;
      }
      continue;
    }
    // inside a string literal
    if (c === "\\") {
      out += c + (src[i + 1] ?? "");
      i += 2;
      continue;
    }
    if ((state === "dq" && c === '"') || (state === "sq" && c === "'") || (state === "tpl" && c === "`")) {
      state = "code";
    }
    out += c;
    i += 1;
  }
  return out;
}

/** A string literal that STARTS with `/api` — i.e. a hand-built URL. */
const BARE_API_LITERAL = /(["'`])\/api(?=[/"'`?])/;

/** Same-line markers that prove the basePath is re-attached. */
const BASEPATH_MARKERS = ["withAppBasePath(", "requestBasePath(", "APP_BASE_PATH"];

/**
 * Verified-SAFE exemptions. Each entry is a real audit result, NOT a silenced
 * bug — re-verify before adding one.
 */
const EXEMPT: ReadonlyArray<{ file: string; why: string }> = [
  {
    // Middleware matchers. Next STRIPS basePath before middleware runs, so the
    // pathname these are compared against is basePath-LESS by construction.
    // Prefixing them here would break every public-route exemption.
    file: "src/lib/route-matcher.ts",
    why: "middleware match prefixes — compared against a basePath-stripped pathname",
  },
  {
    // Consumed exclusively by the kit `<Button href>` anchor, which applies
    // withAppBasePath itself (src/components/ui/linear/Button.tsx). Verified
    // 2026-08-03: RunnersClient has no other consumer of INSTALL_HREF.
    file: "src/components/customer/RunnersClient.tsx",
    why: "flows only into <Button href>, which basePath-prefixes it",
  },
];

function walkSources(dir: string, acc: string[] = []): string[] {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkSources(p, acc);
    } else if (/\.(ts|tsx)$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name)) {
      acc.push(p);
    }
  }
  return acc;
}

describe("guard: no basePath-blind /api literal under src/", () => {
  it("every hand-built /api URL is basePath-prefixed or reviewed-exempt", () => {
    const srcRoot = path.join(APP_ROOT, "src");
    const offenders: string[] = [];

    for (const file of walkSources(srcRoot)) {
      const rel = path.relative(APP_ROOT, file).split(path.sep).join("/");
      if (rel.includes("/__tests__/")) continue;
      if (EXEMPT.some((e) => e.file === rel)) continue;

      stripComments(fs.readFileSync(file, "utf8"))
        .split("\n")
        .forEach((line, idx) => {
          if (!BARE_API_LITERAL.test(line)) return;
          if (BASEPATH_MARKERS.some((m) => line.includes(m))) return;
          offenders.push(`${rel}:${idx + 1}: ${line.trim()}`);
        });
    }

    expect(
      offenders,
      [
        "Bare client-side `/api…` literal(s) found. On the path-mounted",
        "surface these resolve against the apex humangr.com — a DIFFERENT",
        "app (hugr-site marketing) — not this one. Wrap with",
        "`withAppBasePath()` from @/lib/route-matcher, or add a reviewed",
        "entry to EXEMPT above if the value is provably prefixed downstream.",
      ].join(" "),
    ).toEqual([]);
  });

  it("the guard's own scanner still detects an unwrapped literal", () => {
    // Self-check: a guard that cannot see the defect it exists to catch is the
    // failure mode this repo has already been bitten by. Prove the matcher.
    const sample = stripComments(
      ['const dead = "/api/welcome/stream";', '// prose about /api/welcome/stream'].join("\n"),
    )
      .split("\n")
      .filter((l) => BARE_API_LITERAL.test(l));
    expect(sample).toHaveLength(1);
  });
});
