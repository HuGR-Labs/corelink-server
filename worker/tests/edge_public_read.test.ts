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
 * prefix `PyVXQ4S5KVrIUwJf` requires the TDK secret, so it is NOT asserted here
 * (a test must not depend on a local secret, and referencing it trips the
 * secrets-matrix scanner). The `derive_prefix` algorithm is proven against a
 * checked-in dummy-TDK vector below; its byte-for-byte match to the REAL prod
 * prefix was proven out-of-band (Python against R2_TDK_HEX_PROD, captured in
 * the F0 evidence) and is re-proven live in the F1 shadow run.
 */

import { describe, it, expect, beforeEach } from "vitest";
import {
  canonicalBottlePath,
  brewUrlHash,
  pipUrlHash,
  derivePublicPrefix,
  publicR2Key,
  lookupPublicContentHash,
  lookupPublicContentHashCached,
  pubmapKvKey,
  publicBlobCacheKey,
  parseByteRange,
  readPublicHit,
  shadowCompareEdgePublicRead,
  __resetPublicMapCacheForTests,
  PUBLIC_NAMESPACE,
  publicBlocklistKvKey,
  writePublicBlocklistKv,
  isPublicRevokedAtEdge,
} from "../src/lib/edge_public_read.js";

// The `_public` map cache is a module-level (per-isolate) singleton; reset it
// between cases so an L1 entry from one test never leaks into the next.
beforeEach(() => {
  __resetPublicMapCacheForTests();
});

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

describe("B1b — content_hash edge revocation accelerator (pubblock KV)", () => {
  const BODY = new TextEncoder().encode("abc");
  const BODY_HASH = "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85";
  const env = {
    R2_TDK_HEX: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
    R2_CAS_REGION: "iad",
  };
  const PATH = "/brew/t/v2/homebrew/core/x/blobs/sha256:00";

  /** In-memory KV supporting get/put + a throwing variant for fail-open. */
  function fakeKv(seed: Record<string, string> = {}, opts: { throwOnGet?: boolean } = {}) {
    const store = new Map<string, string>(Object.entries(seed));
    const puts: Array<{ key: string; value: string; ttl?: number }> = [];
    const kv = {
      async get(key: string) {
        if (opts.throwOnGet) throw new Error("kv down");
        return store.get(key) ?? null;
      },
      async put(key: string, value: string, o?: { expirationTtl?: number }) {
        puts.push({ key, value, ttl: o?.expirationTtl });
        store.set(key, value);
      },
    };
    return { kv, puts };
  }

  it("keys the marker by content_hash under the pubblock: prefix", () => {
    expect(publicBlocklistKvKey(BODY_HASH)).toBe(`pubblock:${BODY_HASH}`);
  });

  it("writePublicBlocklistKv puts a bounded-TTL sentinel at the content_hash key", async () => {
    const { kv, puts } = fakeKv();
    await writePublicBlocklistKv(kv, BODY_HASH);
    expect(puts).toHaveLength(1);
    expect(puts[0]!.key).toBe(`pubblock:${BODY_HASH}`);
    expect(puts[0]!.value.length).toBeGreaterThan(0);
    expect(puts[0]!.ttl).toBe(3600);
  });

  it("isPublicRevokedAtEdge: true when marked, false when absent, false (fail-open) on KV fault", async () => {
    const marked = fakeKv({ [`pubblock:${BODY_HASH}`]: "1" });
    expect(await isPublicRevokedAtEdge(marked.kv, BODY_HASH)).toBe(true);
    const clean = fakeKv();
    expect(await isPublicRevokedAtEdge(clean.kv, BODY_HASH)).toBe(false);
    const broken = fakeKv({}, { throwOnGet: true });
    expect(await isPublicRevokedAtEdge(broken.kv, BODY_HASH)).toBe(false);
  });

  it("readPublicHit returns null when the content_hash is edge-revoked, even on a map+R2 HIT", async () => {
    // Map resolves + R2 has the bytes + they re-hash OK — a HIT by every prior
    // gate. The pubblock marker must still force a MISS (→ container).
    const { kv } = fakeKv({ [`pubblock:${BODY_HASH}`]: "1" });
    const e = {
      ...env,
      CONFIG_DB: fakeDb({ content_hash: BODY_HASH }),
      CAS_BUCKET: fakeBucket(BODY),
      METADATA_KV: kv,
    };
    expect(await readPublicHit(e as never, "brew", PATH)).toBeNull();
  });

  it("readPublicHit still HITs when the marker is absent (accelerator is off-path)", async () => {
    const { kv } = fakeKv();
    const e = {
      ...env,
      CONFIG_DB: fakeDb({ content_hash: BODY_HASH }),
      CAS_BUCKET: fakeBucket(BODY),
      METADATA_KV: kv,
    };
    const hit = await readPublicHit(e as never, "brew", PATH);
    expect(hit).not.toBeNull();
    expect(hit!.contentHash).toBe(BODY_HASH);
  });

  it("readPublicHit serves (fails open) when the pubblock KV read faults", async () => {
    const { kv } = fakeKv({}, { throwOnGet: true });
    const e = {
      ...env,
      CONFIG_DB: fakeDb({ content_hash: BODY_HASH }),
      CAS_BUCKET: fakeBucket(BODY),
      METADATA_KV: kv,
    };
    const hit = await readPublicHit(e as never, "brew", PATH);
    expect(hit).not.toBeNull();
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

// ── WP-C: cached map lookup (L1 dedup, negative cache, KV L2, D1-error-safe) ──
describe("lookupPublicContentHashCached — three-tier cache", () => {
  const UH = "a".repeat(64);
  const CH = "b".repeat(64);
  const db = {} as never; // never touched; the injected `fetch` stands in for D1

  it("L1 dedups: two calls for one url_hash hit D1 once", async () => {
    let n = 0;
    const fetch = async () => {
      n++;
      return CH;
    };
    expect(await lookupPublicContentHashCached(db, UH, { fetch })).toBe(CH);
    expect(await lookupPublicContentHashCached(db, UH, { fetch })).toBe(CH);
    expect(n).toBe(1);
  });

  it("caches the NEGATIVE (miss) verdict too — a miss also hits D1 once", async () => {
    let n = 0;
    const fetch = async () => {
      n++;
      return null;
    };
    expect(await lookupPublicContentHashCached(db, UH, { fetch })).toBeNull();
    expect(await lookupPublicContentHashCached(db, UH, { fetch })).toBeNull();
    expect(n).toBe(1);
  });

  it("a D1 FAULT is never cached — the next call retries", async () => {
    let n = 0;
    const fetch = async () => {
      n++;
      if (n === 1) throw new Error("d1 down");
      return CH;
    };
    await expect(lookupPublicContentHashCached(db, UH, { fetch })).rejects.toThrow();
    // second call must re-hit D1 (nothing pinned by the fault) and succeed
    expect(await lookupPublicContentHashCached(db, UH, { fetch })).toBe(CH);
    expect(n).toBe(2);
  });

  it("KV L2 hit short-circuits D1", async () => {
    let n = 0;
    const fetch = async () => {
      n++;
      return "should-not-be-called";
    };
    const kv = {
      get: async (k: string) => (k === pubmapKvKey(UH) ? JSON.stringify({ h: CH }) : null),
      put: async () => undefined,
    };
    expect(await lookupPublicContentHashCached(db, UH, { fetch, kv: kv as never })).toBe(CH);
    expect(n).toBe(0);
  });

  it("on a D1 miss, writes the verdict back to KV under the pubmap key + TTL", async () => {
    const puts: Array<{ key: string; val: string; ttl?: number }> = [];
    const kv = {
      get: async () => null,
      put: async (key: string, val: string, opts?: { expirationTtl?: number }) => {
        puts.push({ key, val, ttl: opts?.expirationTtl });
      },
    };
    await lookupPublicContentHashCached(db, UH, { fetch: async () => CH, kv: kv as never });
    expect(puts).toHaveLength(1);
    expect(puts[0]!.key).toBe(pubmapKvKey(UH));
    expect(JSON.parse(puts[0]!.val)).toEqual({ h: CH });
    expect(puts[0]!.ttl).toBe(60);
  });

  it("pubmapKvKey / publicBlobCacheKey embed the identity", () => {
    expect(pubmapKvKey(UH)).toBe("pubmap:" + UH);
    expect(publicBlobCacheKey(CH).url).toContain(CH);
  });
});

// ── WP-A: colo Cache API L1 (colo-hit skips R2+re-hash; colo-miss fills) ──────
describe("readPublicHit — colo Cache API L1", () => {
  const BODY = new TextEncoder().encode("abc");
  const BODY_HASH = "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85";
  const env = { R2_TDK_HEX: "00".repeat(32), R2_CAS_REGION: "iad" };
  const PATH = "/brew/t/v2/homebrew/core/x/blobs/sha256:00";

  function withFakeCaches<T>(fake: unknown, fn: () => Promise<T>): Promise<T> {
    const g = globalThis as unknown as { caches?: unknown };
    const prev = g.caches;
    g.caches = fake;
    return fn().finally(() => {
      g.caches = prev;
    });
  }

  it("colo-HIT serves the cached bytes WITHOUT calling R2 or re-hashing", async () => {
    let bucketGets = 0;
    // Cache returns DIFFERENT bytes than R2 would — proving no re-hash on colo-hit
    // and that R2 is never consulted (bucketGets stays 0).
    const cachedBytes = new TextEncoder().encode("cached-trusted");
    const fakeCaches = {
      default: {
        match: async () => new Response(cachedBytes),
        put: async () => undefined,
      },
    };
    const e = {
      ...env,
      CONFIG_DB: fakeDb({ content_hash: BODY_HASH }),
      CAS_BUCKET: {
        get: async () => {
          bucketGets++;
          return null;
        },
      },
    };
    const hit = await withFakeCaches(fakeCaches, () =>
      readPublicHit(e as never, "brew", PATH),
    );
    expect(hit).not.toBeNull();
    expect(new Uint8Array(hit!.bytes)).toEqual(cachedBytes);
    expect(bucketGets).toBe(0);
  });

  it("colo-MISS fills the colo cache from R2 after re-hash validation", async () => {
    const puts: Request[] = [];
    const fakeCaches = {
      default: {
        match: async () => undefined, // colo miss
        put: async (req: Request) => {
          puts.push(req);
        },
      },
    };
    const e = { ...env, CONFIG_DB: fakeDb({ content_hash: BODY_HASH }), CAS_BUCKET: fakeBucket(BODY) };
    const hit = await withFakeCaches(fakeCaches, () =>
      readPublicHit(e as never, "brew", PATH),
    );
    expect(hit).not.toBeNull();
    expect(new Uint8Array(hit!.bytes)).toEqual(BODY);
    expect(puts).toHaveLength(1);
    expect(puts[0]!.url).toContain(BODY_HASH);
  });
});

// ── Range parsing (edge serve 206/416 support) ──────────────────────────────
describe("parseByteRange", () => {
  const N = 1000;
  it("returns null when there is no Range header", () => {
    expect(parseByteRange(null, N)).toBeNull();
  });
  it("parses a closed range bytes=a-b (inclusive)", () => {
    expect(parseByteRange("bytes=0-99", N)).toEqual({ start: 0, end: 99 });
    expect(parseByteRange("bytes=100-199", N)).toEqual({ start: 100, end: 199 });
  });
  it("parses an open-ended range bytes=a-", () => {
    expect(parseByteRange("bytes=500-", N)).toEqual({ start: 500, end: 999 });
  });
  it("parses a suffix range bytes=-N (last N bytes)", () => {
    expect(parseByteRange("bytes=-50", N)).toEqual({ start: 950, end: 999 });
    // suffix larger than the object → whole object
    expect(parseByteRange("bytes=-5000", N)).toEqual({ start: 0, end: 999 });
  });
  it("clamps an end past the last byte", () => {
    expect(parseByteRange("bytes=900-999999", N)).toEqual({ start: 900, end: 999 });
  });
  it("returns 'unsatisfiable' for an out-of-bounds start or bytes=-0", () => {
    expect(parseByteRange("bytes=1000-1100", N)).toBe("unsatisfiable");
    expect(parseByteRange("bytes=5000-", N)).toBe("unsatisfiable");
    expect(parseByteRange("bytes=-0", N)).toBe("unsatisfiable");
  });
  it("returns null (→ full 200) for malformed / multi-range / non-bytes units", () => {
    expect(parseByteRange("bytes=abc", N)).toBeNull();
    expect(parseByteRange("bytes=0-9,20-29", N)).toBeNull();
    expect(parseByteRange("items=0-9", N)).toBeNull();
    expect(parseByteRange("bytes=-", N)).toBeNull();
  });
});
