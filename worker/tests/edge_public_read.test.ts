/**
 * Parity + unit tests for the Worker-native `_public` cache-HIT read
 * (`src/lib/edge_public_read.ts`, F3.3).
 *
 * The load-bearing assertions are PROD-GROUND-TRUTH parity vectors: the exact
 * `url_hash` and `_public` R2 prefix that the live container wrote for a real
 * Homebrew bottle (`tree` 2.3.2 arm64_linux). If the TS port ever diverges from
 * the Rust container, these fail — the drift firewall. The vectors were captured
 * from prod on 2026-08-16:
 *   path      = /brew/<tenant>/v2/homebrew/core/tree/blobs/sha256:cb6d74ec…
 *   url_hash  = 5cbfb8cef2461d1de64a5c14892c13aeb1d822926995f8b1a7bbc9c0434df23f
 *   content   = bd095a1b8a99bfcc9834c0b4515279c216023ff2a89ea295b401a09b3ce7b2a1 (77998 B)
 *   R2 key    = iad/PyVXQ4S5KVrIUwJf/bd095a1b… (prefix derived from R2_TDK_HEX_PROD)
 *
 * The url_hash vector is a pure function of a PUBLIC path (no secret). The real
 * prefix `PyVXQ4S5KVrIUwJf` needs the TDK, so it is asserted ONLY when the
 * secret is present in the env (local run with .env.local sourced); CI without
 * the secret still verifies the algorithm against a checked-in dummy-TDK vector.
 */

import { describe, it, expect } from "vitest";
import {
  canonicalBottlePath,
  brewUrlHash,
  pipUrlHash,
  derivePublicPrefix,
  publicR2Key,
  lookupPublicContentHash,
  readPublicHit,
  shadowCompareEdgePublicRead,
  PUBLIC_NAMESPACE,
} from "../src/lib/edge_public_read.js";

const TREE_PATH =
  "/brew/d863fafb-17c3-4ec3-92f6-b5a85c27d7bd/v2/homebrew/core/tree/blobs/" +
  "sha256:cb6d74ecafcc8b6d736e538b46be79249457e6c417e18b83de65b405b8495557";
const TREE_URL_HASH =
  "5cbfb8cef2461d1de64a5c14892c13aeb1d822926995f8b1a7bbc9c0434df23f";
const TREE_CONTENT_HASH =
  "bd095a1b8a99bfcc9834c0b4515279c216023ff2a89ea295b401a09b3ce7b2a1";
const REAL_PUBLIC_PREFIX = "PyVXQ4S5KVrIUwJf";

describe("brew url_hash — prod parity", () => {
  it("canonicalizes the tree bottle path exactly like the container", () => {
    expect(canonicalBottlePath(TREE_PATH)).toBe(
      "v2/homebrew/core/tree/blobs/" +
        "sha256:cb6d74ecafcc8b6d736e538b46be79249457e6c417e18b83de65b405b8495557",
    );
  });

  it("reproduces the exact url_hash prod stored (5cbfb8ce…)", async () => {
    expect(await brewUrlHash(TREE_PATH)).toBe(TREE_URL_HASH);
  });

  it("strips the query string (matches code, which does NOT percent-decode)", async () => {
    expect(await brewUrlHash(TREE_PATH + "?bust=1")).toBe(TREE_URL_HASH);
  });

  it("is tenant-independent (key excludes the tenant segment)", async () => {
    const other = TREE_PATH.replace("d863fafb-17c3-4ec3-92f6-b5a85c27d7bd", "some-other-tenant");
    expect(await brewUrlHash(other)).toBe(TREE_URL_HASH);
  });
});

describe("pip url_hash", () => {
  const SHA = "a".repeat(64);
  it("uses the sha256 path segment, lowercased", () => {
    expect(pipUrlHash(`/pip/t1/pkg/${SHA.toUpperCase()}/foo-1.0-py3.whl`)).toBe(SHA);
  });
  it("returns null when there is no valid 64-hex sha256 segment", () => {
    expect(pipUrlHash("/pip/t1/simple/foo/")).toBeNull();
    expect(pipUrlHash("/pip/t1/pkg/notahash/foo.whl")).toBeNull();
  });
});

describe("derive_prefix — algorithm + prod parity", () => {
  const DUMMY_TDK =
    "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

  it("matches the checked-in dummy-TDK vector (HMAC→base64url-no-pad→[..16])", async () => {
    expect(await derivePublicPrefix(DUMMY_TDK)).toBe("q6dAqWQwyZC_67Lm");
  });

  it("is 16 base64url chars", async () => {
    const p = await derivePublicPrefix(DUMMY_TDK);
    expect(p).toHaveLength(16);
    expect(p).toMatch(/^[A-Za-z0-9_-]{16}$/);
  });

  it("rejects a non-32-byte TDK", async () => {
    await expect(derivePublicPrefix("00")).rejects.toThrow();
  });

  const realTdk = process.env.R2_TDK_HEX_PROD;
  it.runIf(!!realTdk)(
    "reproduces the REAL prod _public prefix PyVXQ4S5KVrIUwJf",
    async () => {
      expect(await derivePublicPrefix(realTdk as string)).toBe(REAL_PUBLIC_PREFIX);
    },
  );

  it("builds the exact prod R2 key shape", () => {
    expect(publicR2Key("iad", REAL_PUBLIC_PREFIX, TREE_CONTENT_HASH)).toBe(
      `iad/${REAL_PUBLIC_PREFIX}/${TREE_CONTENT_HASH}`,
    );
  });
});

// --- map read + full read, with fakes ---

function fakeDb(row: { content_hash: string } | null, capture?: (sql: string, binds: unknown[]) => void) {
  return {
    prepare(sql: string) {
      const binds: unknown[] = [];
      return {
        bind(...args: unknown[]) {
          binds.push(...args);
          capture?.(sql, binds);
          return this;
        },
        async first<T>() {
          return row as T | null;
        },
      };
    },
  } as unknown as D1Database;
}

function fakeBucket(bytes: Uint8Array | null) {
  return {
    async get() {
      if (!bytes) return null;
      return { async arrayBuffer() { return bytes.buffer.slice(0); } } as unknown as R2ObjectBody;
    },
  } as unknown as R2Bucket;
}

describe("lookupPublicContentHash", () => {
  it("queries _public with the mandatory public_blocklist NOT EXISTS join", async () => {
    let seenSql = "";
    let seenBinds: unknown[] = [];
    const db = fakeDb({ content_hash: TREE_CONTENT_HASH }, (sql, binds) => {
      seenSql = sql;
      seenBinds = binds;
    });
    const got = await lookupPublicContentHash(db, TREE_URL_HASH);
    expect(got).toBe(TREE_CONTENT_HASH);
    expect(seenSql).toContain("NOT EXISTS");
    expect(seenSql).toContain("public_blocklist");
    expect(seenBinds).toEqual([PUBLIC_NAMESPACE, TREE_URL_HASH]);
  });

  it("returns null on a map miss / revoked content_hash", async () => {
    expect(await lookupPublicContentHash(fakeDb(null), TREE_URL_HASH)).toBeNull();
  });
});

describe("readPublicHit — end-to-end with fakes", () => {
  const BODY = new TextEncoder().encode("abc");
  // blake3("abc") — official reference vector (see blake3.test.ts).
  const BODY_HASH = "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85";
  const env = {
    R2_TDK_HEX: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
    R2_CAS_REGION: "iad",
  };

  it("returns the bytes on a verified HIT (map → R2 → re-hash OK)", async () => {
    const e = { ...env, CONFIG_DB: fakeDb({ content_hash: BODY_HASH }), CAS_BUCKET: fakeBucket(BODY) };
    const hit = await readPublicHit(e as never, "brew", "/brew/t/v2/homebrew/core/x/blobs/sha256:00");
    expect(hit).not.toBeNull();
    expect(new Uint8Array(hit!.bytes)).toEqual(BODY);
    expect(hit!.contentHash).toBe(BODY_HASH);
  });

  it("returns null (falls through) on a map miss", async () => {
    const e = { ...env, CONFIG_DB: fakeDb(null), CAS_BUCKET: fakeBucket(BODY) };
    expect(await readPublicHit(e as never, "brew", "/brew/t/v2/homebrew/core/x/blobs/sha256:00")).toBeNull();
  });

  it("returns null when R2 bytes are absent (revoked/hard-deleted)", async () => {
    const e = { ...env, CONFIG_DB: fakeDb({ content_hash: BODY_HASH }), CAS_BUCKET: fakeBucket(null) };
    expect(await readPublicHit(e as never, "brew", "/brew/t/v2/homebrew/core/x/blobs/sha256:00")).toBeNull();
  });

  it("returns null on a re-hash mismatch (poison self-heal)", async () => {
    const wrong = { content_hash: "de".repeat(32) };
    const e = { ...env, CONFIG_DB: fakeDb(wrong), CAS_BUCKET: fakeBucket(BODY) };
    expect(await readPublicHit(e as never, "brew", "/brew/t/v2/homebrew/core/x/blobs/sha256:00")).toBeNull();
  });

  it("returns null when the region/TDK env is not configured", async () => {
    const e = { CONFIG_DB: fakeDb({ content_hash: BODY_HASH }), CAS_BUCKET: fakeBucket(BODY) };
    expect(await readPublicHit(e as never, "brew", "/brew/t/v2/homebrew/core/x/blobs/sha256:00")).toBeNull();
  });
});

describe("shadowCompareEdgePublicRead — F1 parity verdicts", () => {
  const BODY = new TextEncoder().encode("abc");
  const BODY_HASH = "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85";
  const PATH = "/brew/t/v2/homebrew/core/x/blobs/sha256:00";
  const baseEnv = {
    R2_TDK_HEX: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
    R2_CAS_REGION: "iad",
  };
  const containerResp = (bytes: Uint8Array, xcache: "HIT" | "MISS") =>
    new Response(bytes, { headers: { "x-cache": xcache } });

  it("parity_ok — container HIT, edge HIT, identical bytes", async () => {
    const e = { ...baseEnv, CONFIG_DB: fakeDb({ content_hash: BODY_HASH }), CAS_BUCKET: fakeBucket(BODY) };
    expect(await shadowCompareEdgePublicRead(e as never, "brew", PATH, containerResp(BODY, "HIT"))).toBe("parity_ok");
  });

  it("parity_miss — container HIT but edge missed (drift signal)", async () => {
    const e = { ...baseEnv, CONFIG_DB: fakeDb(null), CAS_BUCKET: fakeBucket(BODY) };
    expect(await shadowCompareEdgePublicRead(e as never, "brew", PATH, containerResp(BODY, "HIT"))).toBe("parity_miss");
  });

  it("parity_bytes_diff — both HIT but bytes differ (SERIOUS)", async () => {
    const e = { ...baseEnv, CONFIG_DB: fakeDb({ content_hash: BODY_HASH }), CAS_BUCKET: fakeBucket(BODY) };
    const different = new TextEncoder().encode("abd"); // same length, 1 byte diff
    expect(await shadowCompareEdgePublicRead(e as never, "brew", PATH, containerResp(different, "HIT"))).toBe("parity_bytes_diff");
  });

  it("container_miss_edge_miss — expected (blob not in _public yet)", async () => {
    const e = { ...baseEnv, CONFIG_DB: fakeDb(null), CAS_BUCKET: fakeBucket(null) };
    expect(await shadowCompareEdgePublicRead(e as never, "brew", PATH, containerResp(BODY, "MISS"))).toBe("container_miss_edge_miss");
  });

  it("container_miss_edge_hit — container filled but edge already had it", async () => {
    const e = { ...baseEnv, CONFIG_DB: fakeDb({ content_hash: BODY_HASH }), CAS_BUCKET: fakeBucket(BODY) };
    expect(await shadowCompareEdgePublicRead(e as never, "brew", PATH, containerResp(BODY, "MISS"))).toBe("container_miss_edge_hit");
  });
});
