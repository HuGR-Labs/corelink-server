import { describe, expect, it } from "vitest";

function makeGetRequest(url: string): import("next/server").NextRequest {
  return new Request(url) as unknown as import("next/server").NextRequest;
}

describe("pricing public routes", () => {
  it("redirects /pricing to the default-locale pricing page", async () => {
    const { GET } = await import("@/app/pricing/route");
    const res = GET(makeGetRequest("https://humangr.com/corelink/pricing"));

    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(
      "https://humangr.com/corelink/pricing",
    );
  });

  it("redirects /pricing.txt to the canonical public text endpoint", async () => {
    const { GET } = await import("@/app/pricing.txt/route");
    const res = GET();

    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(
      "https://humangr.com/corelink/pricing.txt",
    );
  });

  it("redirects /llms.txt to the canonical public LLM endpoint", async () => {
    const { GET } = await import("@/app/llms.txt/route");
    const res = GET();

    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(
      "https://humangr.com/corelink/llms.txt",
    );
  });
});
