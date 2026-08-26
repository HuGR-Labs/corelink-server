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
import { isValidPatSigningKeyHex } from "../src/lib/pat_signing_key.js";

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
    expect(isValidPatSigningKeyHex("not-a-real-key-but-long-enough-xyz!!").length >= 64 &&
      isValidPatSigningKeyHex("not-a-real-key-but-long-enough-xyz!!")).toBe(false);
  });

  it("rejects empty string", () => {
    expect(isValidPatSigningKeyHex("")).toBe(false);
  });
});
