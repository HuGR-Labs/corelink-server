import { afterEach, describe, expect, it, vi } from "vitest";

import { track } from "@/lib/analytics";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe("track", () => {
  it("does not post to the production analytics endpoint from localhost by default", () => {
    const fetchImpl = vi.fn();
    vi.stubGlobal("fetch", fetchImpl);
    vi.stubGlobal("crypto", { randomUUID: () => "00000000-0000-4000-8000-000000000000" });
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { hostname: "localhost" },
    });

    track("signup_started");

    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it("uses an explicit analytics endpoint even on localhost", () => {
    const fetchImpl = vi.fn(() => Promise.resolve(new Response(null, { status: 204 })));
    vi.stubGlobal("fetch", fetchImpl);
    vi.stubGlobal("crypto", { randomUUID: () => "00000000-0000-4000-8000-000000000000" });
    vi.stubEnv("NEXT_PUBLIC_ANALYTICS_ENDPOINT", "http://localhost:8788/v1/event");
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { hostname: "localhost" },
    });

    track("signup_started");

    expect(fetchImpl).toHaveBeenCalledWith(
      "http://localhost:8788/v1/event",
      expect.objectContaining({ method: "POST", credentials: "omit" }),
    );
  });
});
