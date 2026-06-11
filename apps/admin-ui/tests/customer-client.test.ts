// WP-4 — CustomerClient auth-token wiring:
//   - `Authorization: Bearer <token>` is attached when `getToken` is provided
//     AND resolves to a non-null string.
//   - The header is ABSENT when `getToken` is omitted (E2E mock mode must keep
//     working byte-identical) or when it resolves to null.

import { describe, it, expect, vi } from "vitest";
import { CustomerClient, CustomerClientError } from "@/lib/customer-client";

interface Captured {
  url: string;
  method: string;
  headers: Record<string, string>;
}

function makeFetch(captured: Captured[], body: unknown = {}) {
  return vi.fn(async (url: string, init?: RequestInit) => {
    const h: Record<string, string> = {};
    new Headers(init?.headers).forEach((v, k) => {
      h[k.toLowerCase()] = v;
    });
    captured.push({ url, method: init?.method ?? "GET", headers: h });
    return new Response(JSON.stringify(body), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  }) as unknown as typeof fetch;
}

describe("CustomerClient auth token", () => {
  it("attaches Authorization: Bearer <token> when getToken resolves non-null", async () => {
    const captured: Captured[] = [];
    const getToken = vi.fn(async () => "sess_jwt_abc123");
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
      getToken,
    });
    await client.getOverview();
    expect(getToken).toHaveBeenCalledTimes(1);
    expect(captured).toHaveLength(1);
    expect(captured[0]!.url).toBe("https://api.test/v1/customer/overview");
    expect(captured[0]!.headers["authorization"]).toBe("Bearer sess_jwt_abc123");
  });

  it("attaches the token on POST mutations too", async () => {
    const captured: Captured[] = [];
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
      getToken: async () => "sess_jwt_post",
    });
    await client.createPat({ name: "ci", scopes: ["cache:r"] });
    expect(captured[0]!.method).toBe("POST");
    expect(captured[0]!.headers["authorization"]).toBe("Bearer sess_jwt_post");
    // content-type still set alongside the auth header
    expect(captured[0]!.headers["content-type"]).toBe("application/json");
  });

  it("fetches a FRESH token per request (no caching across calls)", async () => {
    const captured: Captured[] = [];
    const getToken = vi
      .fn<() => Promise<string | null>>()
      .mockResolvedValueOnce("tok_1")
      .mockResolvedValueOnce("tok_2");
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
      getToken,
    });
    await client.getUsage();
    await client.getBilling();
    expect(getToken).toHaveBeenCalledTimes(2);
    expect(captured[0]!.headers["authorization"]).toBe("Bearer tok_1");
    expect(captured[1]!.headers["authorization"]).toBe("Bearer tok_2");
  });

  it("omits Authorization when getToken is not provided (E2E mock mode untouched)", async () => {
    const captured: Captured[] = [];
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
    });
    await client.getOverview();
    expect(captured[0]!.headers).not.toHaveProperty("authorization");
    // Byte-identical request headers vs. the pre-WP-4 client: content-type only.
    expect(captured[0]!.headers).toEqual({ "content-type": "application/json" });
  });

  it("omits Authorization when getToken resolves null (signed-out)", async () => {
    const captured: Captured[] = [];
    const getToken = vi.fn(async () => null);
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl: makeFetch(captured),
      getToken,
    });
    await client.getOverview();
    expect(getToken).toHaveBeenCalledTimes(1);
    expect(captured[0]!.headers).not.toHaveProperty("authorization");
    expect(captured[0]!.headers).toEqual({ "content-type": "application/json" });
  });

  it("still raises CustomerClientError on non-2xx with the token attached", async () => {
    const fetchImpl = vi.fn(async () => new Response("nope", { status: 401 })) as unknown as typeof fetch;
    const client = new CustomerClient({
      baseUrl: "https://api.test",
      fetchImpl,
      getToken: async () => "expired_tok",
    });
    await expect(client.getOverview()).rejects.toBeInstanceOf(CustomerClientError);
    await expect(client.getOverview()).rejects.toMatchObject({ status: 401 });
  });
});
