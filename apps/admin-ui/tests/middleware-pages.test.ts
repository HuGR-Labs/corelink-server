/**
 * Contract tests for apps/admin-ui/functions/_middleware.ts (E1, 2026-05-28).
 *
 * Asserts:
 *   (a) Host ending in .pages.dev returns 404 (BLOCK mode — admin-ui has
 *       live Stripe / Clerk / Resend secrets in Functions; never serve any
 *       body or run any downstream handler).
 *   (b) Canonical custom-domain host falls through to ctx.next().
 *
 * Renamed from middleware.test.ts to middleware-pages.test.ts to avoid
 * collision with the existing root-level Next.js edge middleware tests.
 */

import { describe, expect, it, vi } from "vitest";
import { onRequest } from "../functions/_middleware";

function makeCtx(url: string) {
  const next = vi.fn(async () =>
    new Response("downstream", { status: 200 }),
  );
  const ctx = {
    request: new Request(url),
    next,
  } as unknown as Parameters<typeof onRequest>[0];
  return { ctx, next };
}

describe("admin-ui pages.dev _middleware (BLOCK mode)", () => {
  it("returns 404 for *.pages.dev hosts and never invokes ctx.next()", async () => {
    const { ctx, next } = makeCtx(
      "https://corelink-admin-ui.pages.dev/admin/tenants",
    );
    const res = (await onRequest(ctx)) as Response;
    expect(res.status).toBe(404);
    expect(await res.text()).toBe("Not Found");
    expect(next).not.toHaveBeenCalled();
  });

  it("returns 404 for preview branch *.<branch>.pages.dev hosts", async () => {
    const { ctx, next } = makeCtx(
      "https://abc123.corelink-admin-ui.pages.dev/admin",
    );
    const res = (await onRequest(ctx)) as Response;
    expect(res.status).toBe(404);
    expect(next).not.toHaveBeenCalled();
  });

  it("falls through to ctx.next() for the canonical custom domain", async () => {
    const { ctx, next } = makeCtx(
      "https://app.corelink.humangr.com/admin/tenants",
    );
    const res = (await onRequest(ctx)) as Response;
    expect(next).toHaveBeenCalledTimes(1);
    expect(res.status).toBe(200);
    expect(await res.text()).toBe("downstream");
  });
});
