import { describe, it, expect, beforeEach } from "vitest";
import {
  checkRateLimit,
  _resetRateLimiter,
  RATE_LIMIT_CONFIG,
} from "@/lib/rate-limit";

describe("csp-report rate-limit", () => {
  beforeEach(() => {
    _resetRateLimiter();
  });

  it("allows up to maxPerWindow then rejects with retry-after", () => {
    const ip = "203.0.113.5";
    const allowedCount = RATE_LIMIT_CONFIG.maxPerWindow;
    for (let i = 0; i < allowedCount; i++) {
      const d = checkRateLimit(ip, 1_000_000);
      expect(d.allowed).toBe(true);
    }
    const blocked = checkRateLimit(ip, 1_000_000);
    expect(blocked.allowed).toBe(false);
    expect(blocked.retryAfterSec).toBeGreaterThan(0);
  });

  it("isolates buckets across IPs", () => {
    const a = checkRateLimit("198.51.100.1", 1_000_000);
    const b = checkRateLimit("198.51.100.2", 1_000_000);
    expect(a.allowed).toBe(true);
    expect(b.allowed).toBe(true);
    expect(a.remaining).toBe(b.remaining);
  });

  it("rolls the window after windowMs", () => {
    const ip = "192.0.2.7";
    for (let i = 0; i < RATE_LIMIT_CONFIG.maxPerWindow; i++) {
      checkRateLimit(ip, 1_000_000);
    }
    const blocked = checkRateLimit(ip, 1_000_000);
    expect(blocked.allowed).toBe(false);
    // Advance past window
    const later = checkRateLimit(ip, 1_000_000 + RATE_LIMIT_CONFIG.windowMs + 1);
    expect(later.allowed).toBe(true);
  });
});
