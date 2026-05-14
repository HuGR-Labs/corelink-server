import { describe, it, expect } from "vitest";
import {
  isPublicPath,
  isProtectedPath,
  PUBLIC_PATH_PREFIXES,
} from "@/lib/route-matcher";

describe("route matcher", () => {
  it("treats canonical public paths as public", () => {
    expect(isPublicPath("/sign-in")).toBe(true);
    expect(isPublicPath("/sign-up")).toBe(true);
    expect(isPublicPath("/api/health")).toBe(true);
    expect(isPublicPath("/api/csp-report")).toBe(true);
    expect(isPublicPath("/_next/static/chunks/main.js")).toBe(true);
    expect(isPublicPath("/locales/en.json")).toBe(true);
  });

  it("treats tenant routes as protected", () => {
    expect(isProtectedPath("/dashboard")).toBe(true);
    expect(isProtectedPath("/settings/security")).toBe(true);
    expect(isProtectedPath("/admin/audit")).toBe(true);
    expect(isProtectedPath("/billing")).toBe(true);
    expect(isProtectedPath("/")).toBe(true);
  });

  it("public prefix list is non-empty and frozen-shape", () => {
    expect(PUBLIC_PATH_PREFIXES.length).toBeGreaterThan(0);
    expect(PUBLIC_PATH_PREFIXES).toContain("/sign-in");
  });
});
