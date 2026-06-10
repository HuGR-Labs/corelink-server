import { describe, it, expect, vi } from "vitest";
import { apiPost, apiGet, redactTokens, ApiClientError } from "@/lib/api-client";

describe("api-client", () => {
  it("attaches Authorization header but never logs the token", async () => {
    const logs: unknown[] = [];
    const spy = vi.spyOn(console, "log").mockImplementation((...a) => {
      logs.push(a);
    });
    const spyWarn = vi.spyOn(console, "warn").mockImplementation((...a) => {
      logs.push(a);
    });
    const spyErr = vi.spyOn(console, "error").mockImplementation((...a) => {
      logs.push(a);
    });

    const fetchImpl = vi.fn(async (_url: string, init?: RequestInit) => {
      const headers = new Headers(init?.headers);
      // sanity check: header is set
      expect(headers.get("Authorization")).toBe("Bearer sekrit-token-xyz");
      return new Response(JSON.stringify({ ok: true }), { status: 200 });
    });

    await apiPost(
      "/v1/tenants",
      { name: "acme" },
      {
        token: "sekrit-token-xyz",
        baseUrl: "https://api.test",
        fetchImpl: fetchImpl as unknown as typeof fetch,
      },
    );

    // No log statement anywhere contains the token.
    const joined = JSON.stringify(logs);
    expect(joined).not.toContain("sekrit-token-xyz");

    spy.mockRestore();
    spyWarn.mockRestore();
    spyErr.mockRestore();
  });

  it("redacts corelink_ tokens from error bodies", () => {
    expect(
      redactTokens("token=corelink_prod_abc123xyz failed"),
    ).toBe("token=corelink_***_REDACTED failed");
  });

  it("redacts JWT-shaped substrings (e.g. Clerk session tokens)", () => {
    const jwt =
      "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyXzEyMyJ9.sig-part_ABC123";
    expect(redactTokens(`Bearer ${jwt} rejected`)).toBe(
      "Bearer jwt_***_REDACTED rejected",
    );
    // Both token shapes in one body are scrubbed.
    expect(
      redactTokens(`pat=corelink_test_abc jwt=${jwt}`),
    ).toBe("pat=corelink_***_REDACTED jwt=jwt_***_REDACTED");
    // Non-JWT text containing "eyJ" without the 3-segment shape is untouched.
    expect(redactTokens("prefix eyJonly-one-segment suffix")).toBe(
      "prefix eyJonly-one-segment suffix",
    );
  });

  it("throws ApiClientError on non-2xx with redacted body", async () => {
    const fetchImpl = async (): Promise<Response> =>
      new Response("denied: corelink_prod_leaked_value", { status: 403 });
    await expect(
      apiGet("/v1/oops", {
        baseUrl: "https://api.test",
        fetchImpl: fetchImpl as unknown as typeof fetch,
      }),
    ).rejects.toMatchObject({ status: 403 });
    try {
      await apiGet("/v1/oops", {
        baseUrl: "https://api.test",
        fetchImpl: fetchImpl as unknown as typeof fetch,
      });
    } catch (e) {
      expect(e).toBeInstanceOf(ApiClientError);
      expect((e as ApiClientError).body).not.toContain("leaked_value");
      expect((e as ApiClientError).body).toContain("REDACTED");
    }
  });
});
