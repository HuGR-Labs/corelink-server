/**
 * Contract tests for apps/docs/functions/_middleware.ts (E1, 2026-05-28).
 *
 * Asserts:
 *   (a) Host ending in .pages.dev returns 301 to docs.corelink.humangr.com,
 *       preserving path + query.
 *   (b) Canonical custom-domain host falls through to ctx.next().
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

describe("docs pages.dev _middleware (REDIRECT mode)", () => {
  it("301-redirects *.pages.dev to docs.corelink.humangr.com with path preserved", async () => {
    const { ctx, next } = makeCtx(
      "https://corelink-docs.pages.dev/tutorial/getting-started?ref=hn",
    );
    const res = (await onRequest(ctx)) as Response;
    expect(res.status).toBe(301);
    const location = res.headers.get("location");
    expect(location).toBe(
      "https://docs.corelink.humangr.com/tutorial/getting-started?ref=hn",
    );
    expect(next).not.toHaveBeenCalled();
  });

  it("301-redirects preview branch hosts too (*.<branch>.pages.dev)", async () => {
    const { ctx } = makeCtx(
      "https://feature-x.corelink-docs.pages.dev/reference/",
    );
    const res = (await onRequest(ctx)) as Response;
    expect(res.status).toBe(301);
    expect(res.headers.get("location")).toBe(
      "https://docs.corelink.humangr.com/reference/",
    );
  });

  it("falls through to ctx.next() for the canonical custom domain", async () => {
    const { ctx, next } = makeCtx(
      "https://docs.corelink.humangr.com/tutorial/",
    );
    const res = (await onRequest(ctx)) as Response;
    expect(next).toHaveBeenCalledTimes(1);
    expect(res.status).toBe(200);
    expect(await res.text()).toBe("downstream");
  });
});
