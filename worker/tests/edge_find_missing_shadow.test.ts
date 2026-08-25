import { describe, expect, it, vi } from "vitest";
import {
  EDGE_FIND_MISSING_MAX_DIGESTS,
  parseMissingDigests,
  parseRequestDigests,
  probeMissingAtEdge,
  shadowCompareEdgeFindMissing,
  tenantMayUseEdgePath,
} from "../src/lib/edge_find_missing";

const TDK = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const TENANT = "d863fafb-17c3-4ec3-92f6-b5a85c27d7bd";
const D = (n: number) => n.toString(16).padStart(64, "0");

/** CONFIG_DB stub: `byokRow` true ⇒ the tenant has a tenant_byok_secret row. */
function db(byokRow: boolean, throws = false) {
  return {
    prepare: () => ({
      bind: () => ({
        first: async () => {
          if (throws) throw new Error("d1 down");
          return byokRow ? { present: 1 } : null;
        },
      }),
    }),
  } as unknown as D1Database;
}

/** CAS_BUCKET stub: keys in `present` answer a head object, everything else null. */
function bucket(present: Set<string>, onHead?: (key: string) => void) {
  return {
    head: async (key: string) => {
      onHead?.(key);
      return present.has(key) ? ({ key } as R2Object) : null;
    },
  } as unknown as R2Bucket;
}

const envWith = (b: R2Bucket, d: D1Database) =>
  ({ CAS_BUCKET: b, CONFIG_DB: d, R2_TDK_HEX: TDK, R2_CAS_REGION: "iad" }) as never;

const reqBody = (hashes: string[]) =>
  JSON.stringify({ blobDigests: hashes.map((h) => ({ hash: h, sizeBytes: 1 })) });
const containerResp = (hashes: string[]) =>
  new Response(
    JSON.stringify({ missingBlobDigests: hashes.map((h) => ({ hash: h, sizeBytes: 1 })) }),
  );

describe("edge findMissingBlobs shadow", () => {
  it("agrees with the container when both see the same blobs", async () => {
    // 0x01 present, 0x02 absent.
    const key1 = `iad/QB1S4StvCIsDVFjI/bazel/sha256/${D(1)}`;
    const verdict = await shadowCompareEdgeFindMissing(
      envWith(bucket(new Set([key1])), db(false)),
      TENANT,
      reqBody([D(1), D(2)]),
      containerResp([D(2)]),
      () => 0,
    );
    expect(verdict).toBe("match n=2 edge_ms=0");
  });

  it("reports DIVERGENT with counts only — never the digests", async () => {
    // The edge sees nothing present, the container claims everything present.
    const verdict = await shadowCompareEdgeFindMissing(
      envWith(bucket(new Set()), db(false)),
      TENANT,
      reqBody([D(1), D(2)]),
      containerResp([]),
      () => 0,
    );
    expect(verdict).toContain("DIVERGENT");
    expect(verdict).toContain("only_edge=2");
    expect(verdict).toContain("only_container=0");
    // INV-NO-BODY-IN-LOGS: tenant data must not ride in a log line.
    expect(verdict).not.toContain(D(1));
  });

  it("refuses BYOK tenants — a plaintext key would report every blob missing", async () => {
    const verdict = await shadowCompareEdgeFindMissing(
      envWith(bucket(new Set()), db(true)),
      TENANT,
      reqBody([D(1)]),
      containerResp([]),
      () => 0,
    );
    expect(verdict).toBe("skip:byok-or-unknown");
  });

  it("treats a failed BYOK lookup as disqualifying, not as 'not BYOK'", async () => {
    await expect(tenantMayUseEdgePath(envWith(bucket(new Set()), db(false, true)), TENANT))
      .resolves.toBe(false);
  });

  it("declines over the subrequest cap instead of firing thousands of heads", async () => {
    const many = Array.from({ length: EDGE_FIND_MISSING_MAX_DIGESTS + 1 }, (_, i) => D(i + 1));
    let heads = 0;
    const verdict = await shadowCompareEdgeFindMissing(
      envWith(bucket(new Set(), () => heads++), db(false)),
      TENANT,
      reqBody(many),
      containerResp([]),
      () => 0,
    );
    expect(verdict).toContain("skip:over-cap");
    expect(heads).toBe(0);
  });

  it("skips rather than guesses when the request or the response is not what it expects", async () => {
    const e = envWith(bucket(new Set()), db(false));
    await expect(
      shadowCompareEdgeFindMissing(e, TENANT, "not json", containerResp([]), () => 0),
    ).resolves.toBe("skip:unparseable-request");
    await expect(
      shadowCompareEdgeFindMissing(e, TENANT, reqBody([]), containerResp([]), () => 0),
    ).resolves.toBe("skip:empty");
    await expect(
      shadowCompareEdgeFindMissing(e, TENANT, reqBody([D(1)]), new Response("<html>"), () => 0),
    ).resolves.toBe("skip:unparseable-container-response");
  });

  it("skips when the tenant prefix is not derivable rather than probing a wrong key", async () => {
    const verdict = await shadowCompareEdgeFindMissing(
      envWith(bucket(new Set()), db(false)),
      "_anonymous",
      reqBody([D(1)]),
      containerResp([]),
      () => 0,
    );
    expect(verdict).toBe("skip:underivable");
  });

  it("probes every digest in ONE round of concurrent heads, in request order", async () => {
    const seen: string[] = [];
    const missing = await probeMissingAtEdge(
      envWith(bucket(new Set([`iad/QB1S4StvCIsDVFjI/bazel/sha256/${D(2)}`]), (k) => seen.push(k)),
        db(false)),
      TENANT,
      [D(1), D(2), D(3)],
    );
    expect(missing).toEqual([D(1), D(3)]);
    expect(seen).toHaveLength(3);
    expect(seen[0]).toContain(D(1));
  });

  it("parses only the shapes it claims to parse", () => {
    expect(parseRequestDigests('{"blobDigests":[{"hash":"a"}]}')).toEqual(["a"]);
    expect(parseRequestDigests('{"blobDigests":[{"size":1}]}')).toBeNull();
    expect(parseRequestDigests("{}")).toBeNull();
    expect(parseMissingDigests('{"missingBlobDigests":[]}')).toEqual([]);
    expect(parseMissingDigests('{"other":[]}')).toBeNull();
  });

  it("measures the edge, not the comparison around it", async () => {
    const clock = vi.fn().mockReturnValueOnce(1000).mockReturnValueOnce(1042);
    const verdict = await shadowCompareEdgeFindMissing(
      envWith(bucket(new Set()), db(false)),
      TENANT,
      reqBody([D(1)]),
      containerResp([D(1)]),
      clock,
    );
    expect(verdict).toBe("match n=1 edge_ms=42");
  });
});
