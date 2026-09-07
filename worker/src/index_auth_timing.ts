/** Timing padding for unauthenticated 404 responses. */

export async function applyTimingPad(
  requestStart: number,
  requestId: string,
  requestCounter: number,
  serverNonce: number,
): Promise<void> {
  const enc = new TextEncoder();
  const idHash = await crypto.subtle.digest("SHA-256", enc.encode(requestId));
  const idView = new DataView(idHash);
  const idSeed = idView.getUint32(0, true);

  // Triple-source seed mix (mirrors padding.rs:74-93 splitmix_u64 logic)
  const seed = splitmix32(serverNonce ^ (requestCounter & 0xffffffff) ^ idSeed);
  // seed is 0..2^32; map to [-jitter, +jitter] pct
  const jitter = ((seed / 0xffffffff) * 2 - 1) * TIMING_PAD_JITTER_PCT;
  const padMs = Math.max(
    TIMING_PAD_MIN_MS,
    TIMING_PAD_TARGET_MS * (1 + jitter / 100),
  );

  const deadline = requestStart + padMs;
  const remaining = deadline - Date.now();
  if (remaining > 0) {
    await new Promise<void>((resolve) => setTimeout(resolve, remaining));
  }
}

/** splitmix32 finalizer — single round. */
function splitmix32(x: number): number {
  let z = (x + 0x9e3779b9) | 0;
  z = Math.imul(z ^ (z >>> 16), 0x85ebca6b);
  z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35);
  return (z ^ (z >>> 16)) >>> 0;
}
