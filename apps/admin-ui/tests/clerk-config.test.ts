import { describe, it, expect, afterEach } from "vitest";
import { getClerkConfig, issuePat } from "@/lib/clerk-config";

const ORIGINAL = { ...process.env };

afterEach(() => {
  for (const k of Object.keys(process.env)) {
    if (!(k in ORIGINAL)) delete process.env[k];
  }
  Object.assign(process.env, ORIGINAL);
});

describe("clerk-config", () => {
  it("returns empty publishable key when env unset", () => {
    delete process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];
    const cfg = getClerkConfig();
    expect(cfg.publishableKey).toBe("");
  });

  it("returns publishable key when env set", () => {
    process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"] = "pk_test_xyz";
    process.env["CLERK_SECRET_KEY"] = "sk_test_xyz";
    const cfg = getClerkConfig();
    expect(cfg.publishableKey).toBe("pk_test_xyz");
    expect(cfg.secretKey).toBe("sk_test_xyz");
  });

  it("issuePat stub throws (WI-S16-002 deferred)", async () => {
    await expect(
      issuePat({ scope: "read-only", expiresIn: "30d", label: "test" }),
    ).rejects.toThrow(/WI-S16-002/);
  });
});
