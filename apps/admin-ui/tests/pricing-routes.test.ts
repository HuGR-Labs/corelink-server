import { describe, expect, it } from "vitest";

import {
  PUBLIC_LLMS_TXT_URL,
  PUBLIC_PRICING_TXT_URL,
  PUBLIC_PRICING_URL,
} from "@/lib/public-url";

function makeGetRequest(url: string): import("next/server").NextRequest {
  return new Request(url) as unknown as import("next/server").NextRequest;
}

describe("pricing public routes", () => {
  it("redirects /pricing to the public pricing page", async () => {
    const { GET } = await import("@/app/pricing/route");
    const res = GET(makeGetRequest("https://corelink-admin.humangr.com/pricing"));

    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(PUBLIC_PRICING_URL);
  });

  it("redirects /pricing.txt to the canonical public text endpoint", async () => {
    const { GET } = await import("@/app/pricing.txt/route");
    const res = GET();

    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(PUBLIC_PRICING_TXT_URL);
  });

  it("redirects /llms.txt to the canonical public LLM endpoint", async () => {
    const { GET } = await import("@/app/llms.txt/route");
    const res = GET();

    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(PUBLIC_LLMS_TXT_URL);
  });

  // The regression this file exists for. The version of these routes on the
  // rejected feature branch redirected to `/corelink/pricing`,
  // `/corelink/pricing.txt` and `/corelink/llms.txt` — all three 404 on the
  // live site, while the apex equivalents return 200. The tests shipped
  // alongside asserted those exact 404 URLs, so they passed while the buyer
  // funnel 307'd into a dead page.
  //
  // Asserting the string is not enough: it is what the previous tests did.
  // What makes this a guard is asserting the SHAPE that was wrong — a
  // basePath prefix on a destination that does not have one.
  it("never prefixes a public destination with the /corelink basePath", () => {
    for (const url of [
      PUBLIC_PRICING_URL,
      PUBLIC_PRICING_TXT_URL,
      PUBLIC_LLMS_TXT_URL,
    ]) {
      expect(new URL(url).pathname.startsWith("/corelink")).toBe(false);
    }
  });
});
