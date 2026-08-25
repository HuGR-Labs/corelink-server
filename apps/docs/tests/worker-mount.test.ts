import { describe, expect, it } from "vitest";

import worker from "../worker/index.js";

/**
 * The docs Worker strips `/corelink/docs` before delegating to the ASSETS
 * binding, so the assets layer answers relative to the build root and has no
 * idea the mount exists. Every redirect it issues therefore points OUTSIDE the
 * docs Worker unless the shim puts the prefix back.
 *
 * Measured against prod on 2026-08-24, before the fix:
 *   /corelink/docs/reference/api/            -> 307 -> /reference/api  -> the
 *                                               marketing homepage, HTTP 200
 *   /corelink/docs/goproxy/<mod>/@v/list     -> 307 -> /goproxy/... -> HTML,
 *                                               which broke every `go get`
 * Neither surfaced as an error. That is the point of these cells.
 */

type AssetsReply = { status: number; location?: string };

function workerWith(reply: AssetsReply, seen: { url?: string } = {}) {
  return {
    ASSETS: {
      async fetch(request: Request): Promise<Response> {
        seen.url = request.url;
        const headers = new Headers();
        if (reply.location) headers.set("location", reply.location);
        return new Response(reply.status === 200 ? "ok" : null, {
          status: reply.status,
          headers,
        });
      },
    },
  };
}

async function get(path: string, reply: AssetsReply) {
  const seen: { url?: string } = {};
  const res = await worker.fetch(
    new Request(`https://humangr.com${path}`),
    workerWith(reply, seen),
  );
  return { res, assetsSawPath: seen.url ? new URL(seen.url).pathname : undefined };
}

describe("docs Worker mount prefix", () => {
  it("strips the mount before the assets lookup", async () => {
    const { assetsSawPath } = await get("/corelink/docs/reference/api", { status: 200 });
    expect(assetsSawPath).toBe("/reference/api");
  });

  it("re-attaches the mount to a trailing-slash redirect", async () => {
    const { res } = await get("/corelink/docs/reference/api/", {
      status: 307,
      location: "/reference/api",
    });
    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe("/corelink/docs/reference/api");
  });

  it("re-attaches the mount to the goproxy `@` normalisation redirect", async () => {
    const { res } = await get("/corelink/docs/goproxy/m/@v/list", {
      status: 307,
      location: "/goproxy/m/%40v/list",
    });
    expect(res.headers.get("location")).toBe("/corelink/docs/goproxy/m/%40v/list");
  });

  it("leaves a location that already carries the mount alone", async () => {
    const { res } = await get("/corelink/docs/x", {
      status: 301,
      location: "/corelink/docs/y",
    });
    expect(res.headers.get("location")).toBe("/corelink/docs/y");
  });

  it("leaves a cross-origin location alone", async () => {
    const { res } = await get("/corelink/docs/x", {
      status: 302,
      location: "https://example.com/z",
    });
    expect(res.headers.get("location")).toBe("https://example.com/z");
  });

  it("leaves a protocol-relative location alone", async () => {
    const { res } = await get("/corelink/docs/x", {
      status: 302,
      location: "//example.com/z",
    });
    expect(res.headers.get("location")).toBe("//example.com/z");
  });

  it("does not touch a non-redirect response", async () => {
    const { res } = await get("/corelink/docs/x", { status: 200 });
    expect(res.status).toBe(200);
    expect(await res.text()).toBe("ok");
  });
});
