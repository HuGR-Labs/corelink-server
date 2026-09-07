/** Stripe webhook signature verification. */

export function decodeWebhookSecret(secret: string): Uint8Array | null {
    if (!secret.startsWith("whsec_")) return null;
    return new TextEncoder().encode(secret);
}

/** Stripe rejects signatures outside this replay window. */
const MAX_TIMESTAMP_AGE_MS = 5 * 60 * 1000;

/**
 * Verify a Stripe webhook signature header (constant-time compare).
 *
 * Returns true on success and false on malformed, stale, or mismatched input.
 * Reads the raw body rather than re-serialising the parsed JSON to avoid
 * any canonicalization drift.
 */
export async function verifyStripeSignature(
    rawBody: string,
    sigHeader: string | null,
    secret: string,
    nowMs: number = Date.now(),
): Promise<boolean> {
    if (!sigHeader) return false;

    const secretBytes = decodeWebhookSecret(secret);
    if (!secretBytes) return false;

    // Parse `t=<timestamp>,v1=<sig>[,v1=<sig>…]`
    const parts = sigHeader.split(",");
    let timestamp: string | null = null;
    const v1Sigs: string[] = [];

    for (const part of parts) {
        const eq = part.indexOf("=");
        if (eq === -1) continue;
        const k = part.slice(0, eq);
        const v = part.slice(eq + 1);
        if (k === "t") timestamp = v;
        else if (k === "v1") v1Sigs.push(v);
    }

    if (!timestamp || v1Sigs.length === 0) return false;

    // Replay-attack guard: timestamp must be within MAX_TIMESTAMP_AGE_MS.
    const ts = parseInt(timestamp, 10);
    if (isNaN(ts) || Math.abs(nowMs - ts * 1000) > MAX_TIMESTAMP_AGE_MS) return false;

    // Compute expected HMAC-SHA256 over `${timestamp}.${rawBody}`.
    const key = await crypto.subtle.importKey(
        "raw",
        // TS 6's lib.dom BufferSource narrows ArrayBufferView to an
        // ArrayBuffer-backed view, while Workers' TextEncoder returns the
        // runtime-compatible ArrayBufferLike form.
        secretBytes as unknown as BufferSource,
        { name: "HMAC", hash: "SHA-256" },
        false,
        ["sign"],
    );
    const toSign = new TextEncoder().encode(`${timestamp}.${rawBody}`);
    const sigBytes = new Uint8Array(await crypto.subtle.sign("HMAC", key, toSign));

    // Convert to lowercase hex (Stripe uses hex, not base64).
    const expected = Array.from(sigBytes)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");

    // Constant-time compare against each v1 candidate.
    for (const candidate of v1Sigs) {
        if (candidate.length !== expected.length) continue;
        let diff = 0;
        for (let i = 0; i < candidate.length; i++) {
            diff |= candidate.charCodeAt(i) ^ expected.charCodeAt(i);
        }
        if (diff === 0) return true;
    }

    return false;
}

// ---------------------------------------------------------------------------
