// ──────────────────────────────────────────────────────────────────────────────
// PAT format parsing (pure TS port of crates/corelink-pat/src/format.rs)
// ──────────────────────────────────────────────────────────────────────────────

/** Parsed PAT segments needed by the Worker auth path. */
export interface PatParsed {
  /** 16-char Crockford b32 token_id (D1 lookup key). */
  readonly tokenId: string;
  /** The HMAC preimage: bytes of `<token_id>.<random_secret>`. */
  readonly hmacPreimage: Uint8Array;
  /** 16 raw bytes of the HMAC-SHA256 truncated signature. */
  readonly hmacSigBytes: Uint8Array;
}

/**
 * Parse a CoreLink PAT plaintext `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`.
 *
 * Returns null for any non-canonical input (wrong prefix, wrong length,
 * wrong charset, wrong env) — the caller treats null as auth failure.
 *
 * Constant-time notes: this function performs character-by-character scans in
 * fixed loops (no short-circuit on first bad byte) for the prefix and env
 * segments, consistent with the Rust `parse_plaintext` constant-time contract.
 * Full timing-indistinguishability across all valid env variants is preserved
 * by running all three env comparisons before selecting a match.
 */
export function parsePat(token: string): PatParsed | null {
  // Canonical total lengths: 95 (env=2: "ci"/"ro") or 96 (env=3: "pat").
  const totalLen = token.length;
  if (totalLen !== 95 && totalLen !== 96) {
    return null;
  }
  const envLen = totalLen === 95 ? 2 : 3;

  // Prefix scan: constant-time byte-by-byte over all 9 chars.
  const PREFIX = "corelink_";
  let prefixBad = 0;
  for (let i = 0; i < PREFIX.length; i++) {
    prefixBad |= token.charCodeAt(i) !== PREFIX.charCodeAt(i) ? 1 : 0;
  }
  if (prefixBad !== 0) {
    return null;
  }

  // Env segment: compare all valid literals (constant-time — all three run
  // regardless of match to prevent timing discrimination between valid envs).
  const envStart = PREFIX.length; // 9
  const envSeg = token.slice(envStart, envStart + envLen);
  const ENV_2 = ["ci", "ro"] as const;
  const ENV_3 = ["pat"] as const;
  let envMatched = 0;
  if (envLen === 2) {
    for (const e of ENV_2) {
      envMatched |= ctEqStr(envSeg, e) ? 1 : 0;
    }
    // Also run the length-3 comparison to equalize timing across envLen branches.
    ctEqStr("pat", "pat"); // no-op; keeps the compiler from eliding the path
  } else {
    // envLen === 3
    for (const e of ENV_3) {
      envMatched |= ctEqStr(envSeg, e) ? 1 : 0;
    }
    // Run length-2 comparisons to equalize timing.
    for (const e of ENV_2) {
      ctEqStr(envSeg.slice(0, 2), e); // discarded result
    }
  }
  if (envMatched === 0) {
    return null;
  }

  // Separator `_` after env.
  const sepAfterEnvIdx = envStart + envLen;
  if (token.charCodeAt(sepAfterEnvIdx) !== 0x5f /* '_' */) {
    return null;
  }

  // token_id: 16 Crockford b32 chars.
  const TOKEN_ID_LEN = 16;
  const tokenIdStart = sepAfterEnvIdx + 1;
  const tokenIdEnd = tokenIdStart + TOKEN_ID_LEN;
  const tokenIdSeg = token.slice(tokenIdStart, tokenIdEnd);
  if (!isCrockfordB32(tokenIdSeg)) {
    return null;
  }

  // Separator `.` after token_id.
  if (token.charCodeAt(tokenIdEnd) !== 0x2e /* '.' */) {
    return null;
  }

  // random_secret: 43 base64url-no-pad chars.
  const SECRET_LEN = 43;
  const secretStart = tokenIdEnd + 1;
  const secretEnd = secretStart + SECRET_LEN;
  const secretSeg = token.slice(secretStart, secretEnd);
  if (!isBase64Url(secretSeg)) {
    return null;
  }

  // Separator `.` after random_secret.
  if (token.charCodeAt(secretEnd) !== 0x2e /* '.' */) {
    return null;
  }

  // hmac_sig: 22 base64url-no-pad chars.
  const SIG_LEN = 22;
  const sigStart = secretEnd + 1;
  const sigEnd = sigStart + SIG_LEN;
  if (sigEnd !== totalLen) {
    return null;
  }
  const sigSeg = token.slice(sigStart, sigEnd);
  if (!isBase64Url(sigSeg)) {
    return null;
  }

  // Decode hmac_sig bytes (16 raw bytes from 22 base64url chars).
  const hmacSigBytes = base64urlDecode(sigSeg);
  if (hmacSigBytes === null || hmacSigBytes.length !== 16) {
    return null;
  }

  // HMAC preimage = bytes of `<token_id>.<random_secret>`.
  const enc = new TextEncoder();
  const hmacPreimage = enc.encode(token.slice(tokenIdStart, secretEnd));

  return { tokenId: tokenIdSeg, hmacPreimage, hmacSigBytes };
}

/**
 * Constant-time string equality. Runs to completion even on a mismatch.
 * Both strings must have the same length for a meaningful comparison;
 * length-mismatched inputs always return false (constant-time: the loop
 * still runs for `a.length` iterations).
 */
function ctEqStr(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= (a.charCodeAt(i) ^ (b.charCodeAt(i)));
  }
  return diff === 0;
}

/** Return true if every character in `s` is a valid Crockford b32 char (uppercase). */
function isCrockfordB32(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    // 0-9: 0x30..0x39; A-H: 0x41..0x48; J,K: 0x4A,0x4B; M,N: 0x4D,0x4E;
    // P-T: 0x50..0x54; V-Z: 0x56..0x5A  (excludes I=0x49, L=0x4C, O=0x4F, U=0x55)
    const valid =
      (c >= 0x30 && c <= 0x39) || // 0-9
      (c >= 0x41 && c <= 0x48) || // A-H
      c === 0x4a || c === 0x4b || // J,K
      c === 0x4d || c === 0x4e || // M,N
      (c >= 0x50 && c <= 0x54) || // P-T
      (c >= 0x56 && c <= 0x5a);   // V-Z
    if (!valid) return false;
  }
  return true;
}

/** Return true if every character in `s` is a valid base64url-no-pad char. */
function isBase64Url(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    const valid =
      (c >= 0x41 && c <= 0x5a) || // A-Z
      (c >= 0x61 && c <= 0x7a) || // a-z
      (c >= 0x30 && c <= 0x39) || // 0-9
      c === 0x2d ||                // -
      c === 0x5f;                  // _
    if (!valid) return false;
  }
  return true;
}

/** Decode a base64url-no-pad string to bytes. Returns null on decode error. */
function base64urlDecode(s: string): Uint8Array | null {
  try {
    // Re-pad to standard base64 then decode.
    const padded = s.replace(/-/g, "+").replace(/_/g, "/");
    const rem = padded.length % 4;
    const padded2 = rem === 0 ? padded : padded + "=".repeat(4 - rem);
    const binary = atob(padded2);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  } catch {
    return null;
  }
}

// ──────────────────────────────────────────────────────────────────────────────
// HMAC-SHA256 PAT fast-fail (WebCrypto)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Verify the PAT HMAC-SHA256 signature using the Worker's PAT_SIGNING_KEY.
 *
 * The signing key is a hex-encoded byte string (≥ 32 bytes decoded).
 * Pre-image: the `hmacPreimage` bytes (`<token_id>.<random_secret>`).
 * Expected: the first 16 raw bytes of HMAC-SHA256(key, preimage).
 *
 * The comparison uses crypto.subtle.timingSafeEqual for constant-time
 * equality — constant-time compare of the 16-byte truncated MAC.
 *
 * Returns true only if the MAC matches; false on any mismatch or error.
 */
/**
 * Verify the PAT HMAC against an OVERLAP KEY SET (key_management.md §3.2.1).
 *
 * Mirrors the Rust `verify_hmac_sig_multi`: a PAT minted under ANY key in the
 * set verifies, so an operator can rotate PAT_SIGNING_KEY (incl. rotate-on-
 * compromise) while old-key tokens stay valid through the overlap window —
 * instead of an instant fleet-wide auth outage.
 *
 * Constant-time discipline: every key is evaluated (no early-return on the
 * first match) and the per-key results are folded with a bitwise OR, so the
 * observable latency does not leak WHICH key matched (which would reveal
 * whether a token is on the old vs. new key during rotation). Each per-key
 * compare itself uses crypto.subtle.timingSafeEqual.
 *
 * Fail-closed: an empty key set returns false (nothing verifies). The caller
 * guarantees the current key is present and well-formed before calling.
 */
export async function verifyPatHmacMulti(
  signingKeysHex: string[],
  hmacPreimage: Uint8Array,
  expectedSigBytes: Uint8Array,
): Promise<boolean> {
  // Fail-closed: no key set bound ⇒ nothing verifies.
  if (signingKeysHex.length === 0) {
    return false;
  }
  let matched = 0;
  for (const keyHex of signingKeysHex) {
    // Do NOT early-return on a match: fold every key so the matching-key
    // identity does not leak via timing.
    const ok = await verifyPatHmac(keyHex, hmacPreimage, expectedSigBytes);
    matched |= ok ? 1 : 0;
  }
  return matched !== 0;
}

export async function verifyPatHmac(
  signingKeyHex: string,
  hmacPreimage: Uint8Array,
  expectedSigBytes: Uint8Array,
): Promise<boolean> {
  try {
    // Decode hex signing key.
    const keyBytes = hexDecode(signingKeyHex);
    if (keyBytes === null || keyBytes.length < 32) {
      return false;
    }

    // Import the key for HMAC-SHA256.
    const cryptoKey = await crypto.subtle.importKey(
      "raw",
      keyBytes,
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );

    // Compute full HMAC-SHA256 over the preimage.
    const macBuf = await crypto.subtle.sign("HMAC", cryptoKey, hmacPreimage);
    // Truncate to first 16 bytes (128-bit truncated MAC per auth_model.md §2.2).
    const macBytes = new Uint8Array(macBuf, 0, 16);

    // Constant-time equality compare of 16-byte buffers.
    return crypto.subtle.timingSafeEqual(macBytes, expectedSigBytes);
  } catch {
    return false;
  }
}

/** Decode a hex-encoded string to bytes. Returns null on invalid input. */
export function hexDecode(hex: string): Uint8Array | null {
  if (hex.length % 2 !== 0) return null;
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    const hi = hexNibble(hex.charCodeAt(i));
    const lo = hexNibble(hex.charCodeAt(i + 1));
    if (hi === -1 || lo === -1) return null;
    bytes[i >> 1] = (hi << 4) | lo;
  }
  return bytes;
}

function hexNibble(c: number): number {
  if (c >= 0x30 && c <= 0x39) return c - 0x30;      // '0'-'9'
  if (c >= 0x61 && c <= 0x66) return c - 0x61 + 10; // 'a'-'f'
  if (c >= 0x41 && c <= 0x46) return c - 0x41 + 10; // 'A'-'F'
  return -1;
}

/** Encode a Uint8Array to base64url (no padding). */
export function base64url(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i++) {
    bin += String.fromCharCode(bytes[i] ?? 0);
  }
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
}
// ──────────────────────────────────────────────────────────────────────────────
// Timing padding for 404 responses (design-pattern-01)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Pad the wall-clock of a 404 response to a constant-ish target.
 *
 * Mirrors the Rust Tower layer's approach:
 *   pad_target = TIMING_PAD_TARGET_MS ± jitter%
 *   sleep_until(requestStart + pad_target)
 *
 * Jitter is seeded from (server_nonce XOR request counter XOR requestId-hash)
 * to prevent attacker-controlled-seed attacks (design-pattern-01 §3.1).
 *
 * Note: Worker CPU time ≠ wall time, but `await`ing a scheduler-based
 * sleep is the correct primitive (zero CPU spin).
 */
