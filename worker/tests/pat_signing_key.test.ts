/**
 * WP-F2 (MED-1) — the shared PAT-signing-key validity predicate.
 *
 * Defect being closed: the PRIMARY `PAT_SIGNING_KEY` was validated only by
 * LENGTH (`>= 64`), while its rotation siblings got the full predicate
 * (even-length + all-hex). A 64+ char NON-hex main key slipped through,
 * `hexDecode` returned null every request, HMAC verification failed
 * silently and EVERY legitimate client took 401s with zero operational
 * signal. Both sites now share ONE predicate so they cannot diverge again.
 */
import { describe, expect, it } from "vitest";
import { isValidPatSigningKeyHex, normalizePatSigningKeyEnv } from "../src/lib/pat_signing_key.js";

describe("isValidPatSigningKeyHex (WP-F2)", () => {
  it("accepts a canonical 64-char all-hex key", () => {
    expect(isValidPatSigningKeyHex("ab".repeat(32))).toBe(true);
  });

  it("accepts longer even-length hex keys", () => {
    expect(isValidPatSigningKeyHex("cd".repeat(64))).toBe(true);
  });

  it("accepts uppercase hex", () => {
    expect(isValidPatSigningKeyHex("AB".repeat(32))).toBe(true);
  });

  it("rejects < 64 chars", () => {
    expect(isValidPatSigningKeyHex("ab".repeat(31))).toBe(false);
  });

  it("rejects odd length", () => {
    expect(isValidPatSigningKeyHex("ab".repeat(31) + "a")).toBe(false);
  });

  it("rejects 64+ chars that are NOT hex (the silent-401 shape)", () => {
    // THE defect shape: long enough to pass the old length-only gate,
    // undecodable by hexDecode.
    expect(isValidPatSigningKeyHex("z".repeat(64))).toBe(false);
    // This case used to read
    //   expect(fn(s).length >= 64 && fn(s)).toBe(false)
    // which asserted NOTHING: `fn` returns a boolean, `.length` on a boolean
    // is `undefined`, `undefined >= 64` is `false`, and `false && …`
    // short-circuits before `fn` is ever called a second time — so the whole
    // expression was the literal `false` and `expect(false).toBe(false)`
    // passed no matter what the function did. The string it used was also
    // only 36 chars, so even a live assertion would have been rejected on
    // LENGTH and never exercised the hex predicate it was written for.
    const longNonHex = "not-a-real-key-but-long-enough-xyz!!".repeat(2);
    expect(longNonHex.length).toBeGreaterThanOrEqual(64);
    expect(longNonHex.length % 2).toBe(0);
    expect(isValidPatSigningKeyHex(longNonHex)).toBe(false);
  });

  it("rejects empty string", () => {
    expect(isValidPatSigningKeyHex("")).toBe(false);
    expect(isValidPatSigningKeyHex("   \t")).toBe(false);
  });

  it("normalizes empty values to EMPTY and whitespace to a malformed marker", () => {
    expect(normalizePatSigningKeyEnv(undefined)).toBe("");
    expect(normalizePatSigningKeyEnv("")).toBe("");
    expect(normalizePatSigningKeyEnv("   \t")).toBe("__invalid_blank_pat_signing_key__");
    expect(normalizePatSigningKeyEnv("  abcd  ")).toBe("  abcd  ");
  });
});
