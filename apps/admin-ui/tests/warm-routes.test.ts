/**
 * Regression lock for the e2e warm-up's 404 classification.
 *
 * The `admin-ui e2e` gate has twice been red for a whole job because the Next
 * dev server answered 404 for App Router routes that exist on disk (runs
 * 30771910241 and 30781415861 — the same three deepest routes both times,
 * green on the same commit ten minutes later, and present in the production
 * route table of the very same run's `next build`). These tests pin the two
 * behaviours that turn that into an accurate, self-healing signal:
 *
 *   1. every entry in `ROUTES` really does resolve to a `page` file under
 *      `src/app` — so a 404 can never be blamed on a stale warm list without
 *      proof, and a stale warm list is caught here rather than in CI;
 *   2. `runWarmPasses` recovers from "exists on disk but the dev server has no
 *      entry" by forcing a route-tree re-scan, and does NOT recover from
 *      "nothing on disk serves this path" — the branch that must stay fatal.
 */
import { describe, expect, it, vi } from "vitest";
import { existsSync, watch } from "node:fs";
import { dirname } from "node:path";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import type { FullConfig } from "@playwright/test";
import {
  APP_DIR,
  directoryChainFor,
  resolveAppPage,
} from "./e2e/app-route-index";
import warmRoutes, {
  ROUTES,
  runWarmPasses,
  type ProbeResult,
  type WarmDeps,
} from "./e2e/warm-routes";

const ok: ProbeResult = { status: 200, detail: "200" };
const notFound: ProbeResult = { status: 404, detail: "404" };

function deps(overrides: Partial<WarmDeps>): WarmDeps {
  return {
    routes: ROUTES,
    probe: async () => ok,
    resolvePage: (route) => resolveAppPage(route),
    rescan: async () => {},
    log: () => {},
    sleep: async () => {},
    attempts: 2,
    backoffMs: 0,
    recoveries: 1,
    ...overrides,
  };
}

describe("app-route-index — filesystem truth for App Router URLs", () => {
  it("resolves every route in the warm list to a real page file", () => {
    for (const route of ROUTES) {
      const page = resolveAppPage(route);
      expect(page, `no page file resolves ${route}`).not.toBeNull();
      expect(existsSync(page as string)).toBe(true);
    }
  });

  it("resolves the three routes that CI has seen 404 (the whole point)", () => {
    expect(resolveAppPage("/en/customer/audit/visualization")).toContain(
      "customer/audit/visualization/page.tsx",
    );
    // Both op ids are served by the same dynamic segment.
    expect(resolveAppPage("/en/admin/ops/op_byok_001")).toContain(
      "admin/ops/[op_id]/page.tsx",
    );
    expect(resolveAppPage("/en/admin/ops/op_dsr_001")).toContain(
      "admin/ops/[op_id]/page.tsx",
    );
  });

  it("sees through route groups and optional catch-alls", () => {
    // `(authenticated)` contributes no URL segment.
    expect(resolveAppPage("/en/customer")).toContain(
      "(authenticated)/customer/page.tsx",
    );
    // `sign-in/[[...sign-in]]` must match with ZERO trailing segments.
    expect(resolveAppPage("/sign-in")).toContain("sign-in/[[...sign-in]]/page.tsx");
  });

  it("returns null for a path nothing on disk serves", () => {
    expect(resolveAppPage("/en/customer/audit/nope")).toBeNull();
    expect(resolveAppPage("/en/definitely/not/a/route")).toBeNull();
  });

  it("lists the directory chain Next had to read, app dir first", () => {
    const page = resolveAppPage("/en/customer/audit/visualization") as string;
    const chain = directoryChainFor(page);
    expect(chain[0]).toBe(APP_DIR);
    expect(chain[chain.length - 1]).toContain("customer/audit/visualization");
    // The directory whose listing came back short in CI is on the chain.
    expect(chain.some((d) => d.endsWith("customer/audit"))).toBe(true);
  });
});

describe("runWarmPasses — 404 classification", () => {
  it("is a no-op when every route resolves", async () => {
    const rescan = vi.fn(async () => {});
    const unresolved = await runWarmPasses(deps({ rescan }));
    expect(unresolved).toEqual([]);
    expect(rescan).not.toHaveBeenCalled();
  });

  it("recovers a route that exists on disk but is missing from the route tree", async () => {
    const broken = "/en/customer/audit/visualization";
    let rescanned = false;
    const rescan = vi.fn(async () => {
      rescanned = true;
    });

    const unresolved = await runWarmPasses(
      deps({
        rescan,
        probe: async (route) =>
          route === broken && !rescanned ? notFound : ok,
      }),
    );

    expect(rescan).toHaveBeenCalledTimes(1);
    expect(rescan).toHaveBeenCalledWith([resolveAppPage(broken)]);
    expect(unresolved).toEqual([]);
  });

  it("still reports a route that stays 404 through the re-scan", async () => {
    const broken = "/en/admin/ops/op_byok_001";
    const unresolved = await runWarmPasses(
      deps({ probe: async (route) => (route === broken ? notFound : ok) }),
    );
    expect(unresolved).toEqual([{ route: broken, detail: "404" }]);
  });

  /**
   * End-to-end over the REAL globalSetup, the REAL re-scan and a stub HTTP
   * server standing in for the dev server: a route that 404s until Next is
   * nudged is exactly the CI fault, and this proves (a) the nudge produces an
   * event a filesystem watcher actually sees — the mechanism the recovery
   * depends on — and (b) the warm-up then completes instead of failing the job.
   */
  it("recovers against a stub dev server, and the re-scan is watcher-visible", async () => {
    const broken = "/en/customer/audit/visualization";
    const pageFile = resolveAppPage(broken) as string;
    const pageDir = dirname(pageFile);
    const parentDir = dirname(pageDir);

    let rescanSeen = false;
    const watchers = [pageDir, parentDir].map((dir) =>
      watch(dir, () => {
        rescanSeen = true;
      }),
    );

    const server = createServer((req, res) => {
      const path = (req.url ?? "").replace("/corelink", "");
      const missing = path === broken && !rescanSeen;
      res.writeHead(missing ? 404 : 200, { "content-type": "text/html" });
      res.end("");
    });
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const port = (server.address() as AddressInfo).port;

    try {
      await warmRoutes({
        projects: [{ use: { baseURL: `http://127.0.0.1:${port}/corelink` } }],
      } as unknown as FullConfig);
    } finally {
      for (const w of watchers) w.close();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }

    expect(rescanSeen).toBe(true);
  }, 90_000);

  it("fails immediately, without re-scanning, when no page file serves the route", async () => {
    const rescan = vi.fn(async () => {});
    await expect(
      runWarmPasses(
        deps({
          routes: ["/en/customer", "/en/customer/phantom"],
          rescan,
          probe: async (route) => (route === "/en/customer" ? ok : notFound),
        }),
      ),
    ).rejects.toThrow(/no page file under src\/app/);
    expect(rescan).not.toHaveBeenCalled();
  });
});
