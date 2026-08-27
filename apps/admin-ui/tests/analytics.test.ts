import { afterEach, describe, expect, it, vi } from "vitest";

import { track, newId, sessionId } from "@/lib/analytics";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("newId", () => {
  it("returns a 32-char hex string when crypto.randomUUID is available", () => {
    vi.stubGlobal("crypto", {
      randomUUID: () => "00000000-0000-4000-8000-000000000000",
    });
    expect(newId()).toBe("00000000000040008000000000000000");
  });
});

describe("sessionId", () => {
  it("returns 'ssr' on server side (no window)", () => {
    const origWindow = globalThis.window;
    // @ts-expect-error – simulate SSR
    delete globalThis.window;
    expect(sessionId()).toBe("ssr");
    globalThis.window = origWindow;
  });
});

describe("track", () => {
  it("calls fetch with the production endpoint and correct shape", () => {
    const fetchImpl = vi.fn(() =>
      Promise.resolve(new Response(null, { status: 204 })),
    );
    vi.stubGlobal("fetch", fetchImpl);
    vi.stubGlobal("crypto", {
      randomUUID: () => "00000000-0000-4000-8000-000000000000",
    });

    track("signup_started");

    expect(fetchImpl).toHaveBeenCalledOnce();
    const [url, init] = fetchImpl.mock.calls[0] as [string, RequestInit];
    expect(url).toContain("/v1/event");
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("omit");
    expect(init.keepalive).toBe(true);
    const body = JSON.parse(init.body as string);
    expect(body.event_name).toBe("signup_started");
    expect(body.tenant_id).toBeNull();
    expect(body.user_id).toBeNull();
  });

  it("swallows fetch errors without throwing", () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new Error("network"))));
    expect(() => track("signup_started")).not.toThrow();
  });

  it("no-ops when fetch is not defined", () => {
    // @ts-expect-error – simulate no fetch
    vi.stubGlobal("fetch", undefined);
    expect(() => track("signup_started")).not.toThrow();
  });
});
