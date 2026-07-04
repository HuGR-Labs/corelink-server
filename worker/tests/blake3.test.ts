/**
 * Unit tests for the self-contained BLAKE3-256 helper (`src/lib/blake3.ts`).
 *
 * This helper backs the DORMANT exact-key narrowing of a runner-job PAT
 * (cf-multitenant WP5a): `blake3("clw/ref/runner/v1/" + output_name)`. Because
 * it is hand-rolled (no dependency for a dormant path), it MUST be pinned
 * against the OFFICIAL BLAKE3 reference test vectors so it matches the Rust
 * `blake3` crate the container (WP5b) derives with byte-for-byte.
 *
 * Reference vectors are from the BLAKE3 team's `test_vectors.json` (unkeyed
 * mode, 32-byte output). The input for the numeric-length vectors is the
 * repeating byte sequence 0,1,2,…,250,0,1,… (i mod 251), exactly as the
 * reference harness defines it.
 */

import { describe, it, expect } from "vitest";
import { blake3Hex, blake3HexBytes } from "../src/lib/blake3.js";

/** Reference input: `length` bytes of the repeating (i mod 251) pattern. */
function refInput(length: number): Uint8Array {
  const b = new Uint8Array(length);
  for (let i = 0; i < length; i++) b[i] = i % 251;
  return b;
}

describe("blake3-256 — official reference test vectors (unkeyed, 32-byte out)", () => {
  // (length → expected 64-char hex) straight from the BLAKE3 test_vectors.json.
  const cases: ReadonlyArray<readonly [number, string]> = [
    [0, "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"],
    [1, "2d3adedff11b61f14c886e35afa036736dcd87a74d27b5c1510225d0f592e213"],
    [1023, "10108970eeda3eb932baac1428c7a2163b0e924c9a9e25b35bba72b28f70bd11"],
    [1024, "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7"],
    [1025, "d00278ae47eb27b34faecf67b4fe263f82d5412916c1ffd97c8cb7fb814b8444"],
    [2048, "e776b6028c7cd22a4d0ba182a8bf62205d2ef576467e838ed6f2529b85fba24a"],
    [2049, "5f4d72f40d7a5f82b15ca2b2e44b1de3c2ef86c426c95c1af0b6879522563030"],
    [3072, "b98cb0ff3623be03326b373de6b9095218513e64f1ee2edd2525c7ad1e5cffd2"],
  ];

  for (const [len, expected] of cases) {
    it(`length ${String(len)} → ${expected.slice(0, 12)}…`, async () => {
      expect(await blake3HexBytes(refInput(len))).toBe(expected);
    });
  }
});

describe("blake3-256 — string inputs (the WP5a narrowing derivation)", () => {
  it("hashes a short UTF-8 string (single chunk)", async () => {
    // blake3("abc") — cross-checked against `b3sum`.
    expect(await blake3Hex("abc")).toBe(
      "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
    );
  });

  it("pins the WP5a exact-key value blake3(\"clw/ref/runner/v1/build-out\")", async () => {
    // This is the value handleRunnerMint writes to pat.runner_job_ac_key when a
    // caller supplies ac_output_name="build-out". Pinned here + in
    // worker/tests/runner_mint.test.ts (EXPECTED_AC_KEY_HEX) — they MUST agree.
    expect(await blake3Hex("clw/ref/runner/v1/build-out")).toBe(
      "bbd4ce224ed20e213837318117052855a885949a210ab760e1e075402e826adb",
    );
  });

  it("output is always 64-char lowercase hex", async () => {
    for (const s of ["", "x", "clw/ref/runner/v1/a-longer-workspace-name-here"]) {
      expect(await blake3Hex(s)).toMatch(/^[0-9a-f]{64}$/);
    }
  });
});
