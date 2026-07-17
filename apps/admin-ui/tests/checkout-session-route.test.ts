/**
 * /api/checkout/session — route handler tests
 * (Phase 0.C — see `specs/_audits/2026-05-27-phase-0-execution-plan.md`
 * §2.C).
 *
 * Verifies the contract with `<UpgradeButton />` and the backend
 * `POST /v1/onboarding/tier-select`:
 *   - 401 when no Clerk session.
 *   - 400 on invalid tier.
 *   - 200 + JSON when Accept: application/json.
 *   - 303 redirect otherwise.
 *   - Backend errors mapped through (status preserved).
 *   - HTTPS-only Checkout URLs accepted.
 *   - Origin derived from x-forwarded-host (never user-supplied).
 */

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Hoisted mocks — must be set up BEFORE the route module is imported
// so the lazy `import("@clerk/nextjs/server")` inside the handler
// resolves to our stub.
const { mockGetToken, mockApiPost } = vi.hoisted(() => ({
  mockGetToken: vi.fn<() => Promise<string | null>>(),
  mockApiPost: vi.fn(),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: async () => ({
    getToken: mockGetToken,
  }),
}));

vi.mock("@/lib/api-client", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api-client")>(
    "@/lib/api-client",
  );
  return {
    ...actual,
    apiPost: mockApiPost,
  };
});

// Import the route AFTER the mocks are registered.
const { POST } = await import("@/app/api/checkout/session/route");

function makeRequest(
  body: unknown,
  headers: Record<string, string> = {},
): import("next/server").NextRequest {
  const init: RequestInit = {
    method: "POST",
    body: JSON.stringify(body),
    headers: { "content-type": "application/json", ...headers },
  };
  // NextRequest accepts a standard Request — cast at boundary.
  return new Request("https://app.corelink.humangr.com/api/checkout/session", init) as unknown as import("next/server").NextRequest;
}

describe("POST /api/checkout/session", () => {
  beforeEach(() => {
    mockGetToken.mockReset();
    mockApiPost.mockReset();
    mockGetToken.mockResolvedValue("clerk_test_jwt_xxx");
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("401 when there is no Clerk session", async () => {
    mockGetToken.mockResolvedValue(null);
    const res = await POST(makeRequest({ tier: "pro" }, { accept: "application/json" }));
    expect(res.status).toBe(401);
    expect(mockApiPost).not.toHaveBeenCalled();
  });

  it("400 on invalid tier", async () => {
    const res = await POST(
      makeRequest({ tier: "bogus" }, { accept: "application/json" }),
    );
    expect(res.status).toBe(400);
    expect(mockApiPost).not.toHaveBeenCalled();
  });

  it("returns 200+JSON when Accept: application/json", async () => {
    mockApiPost.mockResolvedValue({
      checkout_url: "https://checkout.stripe.com/c/pay/cs_test_abc",
      session_id: "cs_test_abc",
    });
    const res = await POST(
      makeRequest({ tier: "pro", locale: "en" }, { accept: "application/json" }),
    );
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toEqual({
      checkout_url: "https://checkout.stripe.com/c/pay/cs_test_abc",
      session_id: "cs_test_abc",
    });
    // Backend was called with tier+URLs (Origin derived from request).
    expect(mockApiPost).toHaveBeenCalledTimes(1);
    const [path, payload, opts] = mockApiPost.mock.calls[0]!;
    expect(path).toBe("/v1/onboarding/tier-select");
    expect((payload as { tier: string }).tier).toBe("pro");
    expect((payload as { success_url: string }).success_url).toMatch(
      /^https:\/\/humangr\.com\/corelink\/en\/upgraded\?session_id=\{CHECKOUT_SESSION_ID\}$/,
    );
    expect((payload as { cancel_url: string }).cancel_url).toMatch(
      /^https:\/\/humangr\.com\/corelink\/en\/pricing$/,
    );
    expect((opts as { token: string }).token).toBe("clerk_test_jwt_xxx");
  });

  it("returns 303 redirect when Accept is not JSON (form-post path)", async () => {
    mockApiPost.mockResolvedValue({
      checkout_url: "https://checkout.stripe.com/c/pay/cs_test_form",
      session_id: "cs_test_form",
    });
    const res = await POST(
      makeRequest({ tier: "pro" }, { accept: "text/html" }),
    );
    expect(res.status).toBe(303);
    expect(res.headers.get("location")).toBe(
      "https://checkout.stripe.com/c/pay/cs_test_form",
    );
  });

  it("rejects non-HTTPS Checkout URLs from the backend (502)", async () => {
    mockApiPost.mockResolvedValue({
      checkout_url: "http://attacker.example/phish",
      session_id: "cs_evil",
    });
    const res = await POST(
      makeRequest({ tier: "pro" }, { accept: "application/json" }),
    );
    expect(res.status).toBe(502);
  });

  it("propagates backend ApiClientError status (e.g. 403 DPA-first lock)", async () => {
    const { ApiClientError } = await import("@/lib/api-client");
    mockApiPost.mockRejectedValue(
      new ApiClientError({
        status: 403,
        body: "dpa_first_lock",
        message: "API 403: dpa_first_lock",
      }),
    );
    const res = await POST(
      makeRequest({ tier: "pro" }, { accept: "application/json" }),
    );
    expect(res.status).toBe(403);
  });

  it("defaults to tier=pro when body is empty", async () => {
    mockApiPost.mockResolvedValue({
      checkout_url: "https://checkout.stripe.com/c/pay/cs_default",
      session_id: "cs_default",
    });
    // No body — handler catches the JSON parse and defaults.
    const init: RequestInit = {
      method: "POST",
      headers: { accept: "application/json", "content-type": "application/json" },
      body: "",
    };
    const req = new Request(
      "https://corelink-admin.humangr.com/api/checkout/session",
      init,
    ) as unknown as import("next/server").NextRequest;
    const res = await POST(req);
    expect(res.status).toBe(200);
    expect((mockApiPost.mock.calls[0]![1] as { tier: string }).tier).toBe("pro");
  });

  it("derives origin from an ALLOW-LISTED x-forwarded-host (never trusts body)", async () => {
    mockApiPost.mockResolvedValue({
      checkout_url: "https://checkout.stripe.com/c/pay/cs_origin",
      session_id: "cs_origin",
    });
    const res = await POST(
      makeRequest(
        { tier: "pro", locale: "de" },
        {
          accept: "application/json",
          "x-forwarded-host": "humangr.com",
          "x-forwarded-proto": "https",
        },
      ),
    );
    expect(res.status).toBe(200);
    const payload = mockApiPost.mock.calls[0]![1] as {
      success_url: string;
      cancel_url: string;
    };
    expect(payload.success_url).toContain(
      "https://humangr.com/corelink/de/upgraded",
    );
    expect(payload.cancel_url).toContain(
      "https://humangr.com/corelink/de/pricing",
    );
  });

  it("FALLS BACK to the canonical host when x-forwarded-host is off-domain (redirect can't be steered)", async () => {
    mockApiPost.mockResolvedValue({
      checkout_url: "https://checkout.stripe.com/c/pay/cs_evil",
      session_id: "cs_evil",
    });
    const res = await POST(
      makeRequest(
        { tier: "pro", locale: "en" },
        {
          accept: "application/json",
          "x-forwarded-host": "evil.com",
          "x-forwarded-proto": "https",
        },
      ),
    );
    expect(res.status).toBe(200);
    const payload = mockApiPost.mock.calls[0]![1] as {
      success_url: string;
      cancel_url: string;
    };
    // Attacker host is dropped; Stripe redirect stays on the canonical host
    // (`humangr.com`, path-mounted at /corelink).
    expect(payload.success_url).toContain("https://humangr.com/corelink/en/upgraded");
    expect(payload.success_url).not.toContain("evil.com");
    expect(payload.cancel_url).toContain("https://humangr.com/corelink/en/pricing");
  });
});
