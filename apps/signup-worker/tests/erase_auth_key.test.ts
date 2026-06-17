/**
 * Unit tests for resolveEraseAuthKey (rt-nuclear #23).
 *
 * Pins the erase-first, shared-fallback resolution that the signup-worker uses
 * for the DSR erase/verify internal calls — mirroring the container's
 * `erase_auth_key_from_env()` and the main Worker's
 * `resolveConsumerKey(env, "erase")` so the keys agree in the full-split config.
 */
import { describe, it, expect } from "vitest";
import { resolveEraseAuthKey } from "../src/lib/erase-auth-key.js";

describe("resolveEraseAuthKey", () => {
  it("prefers the dedicated erase key when set (full-split config)", () => {
    expect(
      resolveEraseAuthKey({
        CORELINK_ERASE_AUTH_KEY: "erase-key",
        CORELINK_INTERNAL_AUTH_KEY: "shared-key",
      }),
    ).toBe("erase-key");
  });

  it("falls back to the shared key when the erase key is unset", () => {
    expect(
      resolveEraseAuthKey({ CORELINK_INTERNAL_AUTH_KEY: "shared-key" }),
    ).toBe("shared-key");
  });

  it("falls back to the shared key when the erase key is empty", () => {
    expect(
      resolveEraseAuthKey({
        CORELINK_ERASE_AUTH_KEY: "",
        CORELINK_INTERNAL_AUTH_KEY: "shared-key",
      }),
    ).toBe("shared-key");
  });

  it("returns null when neither key is bound (caller must retry/skip, never drop)", () => {
    expect(resolveEraseAuthKey({})).toBeNull();
    expect(
      resolveEraseAuthKey({ CORELINK_ERASE_AUTH_KEY: "", CORELINK_INTERNAL_AUTH_KEY: "" }),
    ).toBeNull();
  });
});
