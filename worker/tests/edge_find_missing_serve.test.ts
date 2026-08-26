import { describe, expect, it } from "vitest";
import {
  EDGE_FIND_MISSING_MAX_DIGESTS,
  findMissingResponseBody,
  serveEdgeFindMissing,
} from "../src/lib/edge_find_missing";

const TDK = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const TENANT = "d863fafb-17c3-4ec3-92f6-b5a85c27d7bd";
const PRINCIPAL = "pat_test";
const D = (n: number) => n.toString(16).padStart(64, "0");

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

function bucket(present: Set<string>) {
  return {
    head: async (key: string) => (present.has(key) ? ({ key } as R2Object) : null),
  } as unknown as R2Bucket;
}

const envWith = (b: R2Bucket, d: D1Database) =>
  ({ CAS_BUCKET: b, CONFIG_DB: d, R2_TDK_HEX: TDK, R2_CAS_REGION: "iad" }) as never;

const reqBody = (hashes: string[]) =>
  JSON.stringify({ blobDigests: hashes.map((h) => ({ hash: h, sizeBytes: 1 })) });

/** Records what the audit seam was asked to emit, and answers as told. */
function auditSpy(answer: boolean | "throw") {
  const calls: { digests: string[]; tenant: string; caller_tenant: string }[] = [];
  const emit = async (b: {
    tenant: string;
    principal: string;
    caller_tenant: string;
    at_unix_ms: number;
    digests: string[];
  }) => {
    calls.push({ digests: b.digests, tenant: b.tenant, caller_tenant: b.caller_tenant });
    if (answer === "throw") throw new Error("container unreachable");
    return answer;
  };
  return { emit, calls };
}

describe("edge findMissingBlobs F2 serve", () => {
  it("serves the absent set once the audit committed", async () => {
    const env = envWith(bucket(new Set()), db(false));
    const spy = auditSpy(true);
    const out = await serveEdgeFindMissing(
      env,
      TENANT,
      PRINCIPAL,
      reqBody([D(1), D(2)]),
      spy.emit,
    );
    expect(out?.missing).toEqual([D(1), D(2)]);
    // Every probed digest is audited, not just the missing ones — `exists_batch`
    // writes a ReadAttempted row per REQUESTED digest.
    expect(spy.calls[0]?.digests).toEqual([D(1), D(2)]);
  });

  // THE point of F2. The probe already succeeded and the answer is in hand; a
  // failed audit must still produce a fallthrough, never a served answer.
  it("REFUSES to serve when the audit did not commit", async () => {
    const env = envWith(bucket(new Set()), db(false));
    for (const answer of [false, "throw"] as const) {
      const spy = auditSpy(answer);
      const out = await serveEdgeFindMissing(
        env,
        TENANT,
        PRINCIPAL,
        reqBody([D(1)]),
        spy.emit,
      );
      expect(out).toBeNull();
      // It really did attempt the audit — a null from skipping the call
      // entirely would pass a weaker assertion.
      expect(spy.calls).toHaveLength(1);
    }
  });

  it("never reaches the audit for a tenant it must not answer for", async () => {
    // BYOK tenant: the container probes an HMAC of the digest through the TCS,
    // which the edge cannot resolve. Probing the plaintext key would report
    // every blob missing, and a cache client reacts to that by uploading
    // everything.
    for (const database of [db(true), db(false, true)]) {
      const spy = auditSpy(true);
      const out = await serveEdgeFindMissing(
        envWith(bucket(new Set()), database),
        TENANT,
        PRINCIPAL,
        reqBody([D(1)]),
        spy.emit,
      );
      expect(out).toBeNull();
      expect(spy.calls).toHaveLength(0);
    }
  });

  it("defers over the cap, and on a body it cannot parse", async () => {
    const env = envWith(bucket(new Set()), db(false));
    const tooMany = Array.from({ length: EDGE_FIND_MISSING_MAX_DIGESTS + 1 }, (_, i) =>
      D(i + 1),
    );
    const spy = auditSpy(true);
    expect(
      await serveEdgeFindMissing(env, TENANT, PRINCIPAL, reqBody(tooMany), spy.emit),
    ).toBeNull();
    expect(
      await serveEdgeFindMissing(env, TENANT, PRINCIPAL, "not json", spy.emit),
    ).toBeNull();
    expect(
      await serveEdgeFindMissing(env, TENANT, PRINCIPAL, JSON.stringify({}), spy.emit),
    ).toBeNull();
    expect(spy.calls).toHaveLength(0);
  });

  it("answers an empty batch without auditing — there is no row to write", async () => {
    const spy = auditSpy(false);
    const out = await serveEdgeFindMissing(
      envWith(bucket(new Set()), db(false)),
      TENANT,
      PRINCIPAL,
      reqBody([]),
      spy.emit,
    );
    // Served despite the audit seam answering false: nothing was probed, so
    // nothing was owed. The container returns early on an empty slice too.
    expect(out).toEqual({ missing: [], edgeMs: 0 });
    expect(spy.calls).toHaveLength(0);
  });

  it("omits blobs that are present, and audits them anyway", async () => {
    // The audit is about what was PROBED, not about what came back missing.
    const env = envWith(bucket(new Set()), db(false));
    const present = await probePresent(env);
    expect(present.out?.missing).toEqual([D(2)]);
    expect(present.calls[0]?.digests).toEqual([D(1), D(2)]);
  });

  it("declines a non-UUID tenant instead of inventing a prefix", async () => {
    const spy = auditSpy(true);
    const out = await serveEdgeFindMissing(
      envWith(bucket(new Set()), db(false)),
      "_public",
      PRINCIPAL,
      reqBody([D(1)]),
      spy.emit,
    );
    expect(out).toBeNull();
    expect(spy.calls).toHaveLength(0);
  });

  it("emits the tenant as its own caller_tenant", async () => {
    const spy = auditSpy(true);
    await serveEdgeFindMissing(
      envWith(bucket(new Set()), db(false)),
      TENANT,
      PRINCIPAL,
      reqBody([D(1)]),
      spy.emit,
    );
    // A cross-tenant batch cannot be constructed here: the edge only ever
    // probes the tenant it resolved. The container refuses a mismatch anyway.
    expect(spy.calls[0]?.tenant).toBe(TENANT);
    expect(spy.calls[0]?.caller_tenant).toBe(TENANT);
  });

  it("renders the REAPI response shape", () => {
    expect(JSON.parse(findMissingResponseBody([D(1)]))).toEqual({
      missingBlobDigests: [{ hash: D(1) }],
    });
    expect(JSON.parse(findMissingResponseBody([]))).toEqual({ missingBlobDigests: [] });
  });
});

/** Helper: D(1) present in R2, D(2) absent. */
async function probePresent(env: never) {
  const { deriveTenantPrefix, casBlobKey } = await import("../src/lib/edge_find_missing");
  const prefix = await deriveTenantPrefix(TDK, TENANT);
  const key = casBlobKey("iad", prefix as string, D(1), "sha256");
  const spy = auditSpy(true);
  const e = {
    ...(env as object),
    CAS_BUCKET: bucket(new Set([key])),
  } as never;
  const out = await serveEdgeFindMissing(e, TENANT, PRINCIPAL, reqBody([D(1), D(2)]), spy.emit);
  return { out, calls: spy.calls };
}
