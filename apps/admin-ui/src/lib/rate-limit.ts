/**
 * In-memory IP-based rate limiter for /api/csp-report (WI-S16-001).
 *
 * Bounded sliding window. Suitable for single-instance Edge runtime; replaced
 * by durable KV/D1 backed limiter in WI-S16-005+.
 */

interface Bucket {
  count: number;
  windowStartMs: number;
}

const WINDOW_MS = 60_000;
const MAX_PER_WINDOW = 100;
const MAX_TRACKED_IPS = 10_000;

const buckets = new Map<string, Bucket>();

export interface RateLimitDecision {
  allowed: boolean;
  remaining: number;
  retryAfterSec: number;
}

export function checkRateLimit(
  ip: string,
  now: number = Date.now(),
): RateLimitDecision {
  // Soft cap on memory footprint; evict oldest entry when at limit.
  if (!buckets.has(ip) && buckets.size >= MAX_TRACKED_IPS) {
    const firstKey = buckets.keys().next().value;
    if (firstKey !== undefined) buckets.delete(firstKey);
  }
  const bucket = buckets.get(ip);
  if (!bucket || now - bucket.windowStartMs >= WINDOW_MS) {
    buckets.set(ip, { count: 1, windowStartMs: now });
    return { allowed: true, remaining: MAX_PER_WINDOW - 1, retryAfterSec: 0 };
  }
  if (bucket.count >= MAX_PER_WINDOW) {
    const retry = Math.ceil((WINDOW_MS - (now - bucket.windowStartMs)) / 1000);
    return { allowed: false, remaining: 0, retryAfterSec: Math.max(retry, 1) };
  }
  bucket.count += 1;
  return {
    allowed: true,
    remaining: MAX_PER_WINDOW - bucket.count,
    retryAfterSec: 0,
  };
}

/** For tests: clear state between cases. */
export function _resetRateLimiter(): void {
  buckets.clear();
}

export const RATE_LIMIT_CONFIG = Object.freeze({
  windowMs: WINDOW_MS,
  maxPerWindow: MAX_PER_WINDOW,
  maxTrackedIps: MAX_TRACKED_IPS,
});
