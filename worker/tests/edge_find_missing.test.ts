import { describe, expect, it } from "vitest";
import vectors from "./vectors/tenant_prefix_vectors.json";
import {
  casBlobKey,
  deriveTenantPrefix,
  findMissingProbeKey,
  uuidToBytes,
} from "../src/lib/edge_find_missing";

// The TypeScript half of the two-sided parity contract. The Rust source of
// truth asserts the SAME file in crates/tenant-path/tests/edge_parity_vectors.rs.
describe("edge findMissingBlobs key derivation", () => {
  it("derives byte-identical prefixes to the Rust container", async () => {
    expect(vectors.cases.length).toBeGreaterThan(0); // an empty file would pass vacuously
    for (const c of vectors.cases) {
      await expect(deriveTenantPrefix(vectors.tdk_hex, c.tenant)).resolves.toBe(c.prefix);
    }
  });

  it("builds the same native and REAPI keys the container writes", () => {
    for (const c of vectors.cases) {
      expect(casBlobKey(c.region, c.prefix, c.blake3_digest, "blake3")).toBe(c.blake3_key);
      expect(casBlobKey(c.region, c.prefix, c.sha256_digest, "sha256")).toBe(c.sha256_key);
    }
  });

  it("keeps REAPI sha256 digests OUT of the native keyspace", () => {
    // The whole route probes sha256 digests. Collapsing the two keyspaces would
    // answer "missing" for blobs that are present, and a cache client acts on
    // that by re-uploading everything it already sent.
    const native = casBlobKey("iad", "QB1S4StvCIsDVFjI", "a".repeat(64), "blake3");
    const reapi = casBlobKey("iad", "QB1S4StvCIsDVFjI", "a".repeat(64), "sha256");
    expect(native).not.toBe(reapi);
    expect(reapi).toContain("/bazel/sha256/");
  });

  it("refuses sentinel tenants instead of deriving a shared keyspace", async () => {
    for (const sentinel of ["_anonymous", "_unknown", "_system", "_pending", "_public", ""]) {
      await expect(deriveTenantPrefix(vectors.tdk_hex, sentinel)).resolves.toBeNull();
    }
  });

  it("refuses a non-canonical tenant id exactly where the container fails closed", async () => {
    for (const bad of [
      "t1",
      "D863FAFB-17C3-4EC3-92F6-B5A85C27D7BD", // uppercase is not the canonical form
      "d863fafb17c34ec392f6b5a85c27d7bd", // unhyphenated
      "d863fafb-17c3-4ec3-92f6-b5a85c27d7b", // one char short
      "../../etc/passwd",
    ]) {
      await expect(deriveTenantPrefix(vectors.tdk_hex, bad)).resolves.toBeNull();
      expect(uuidToBytes(bad)).toBeNull();
    }
  });

  it("rejects a TDK that is not 32 bytes rather than deriving from a short key", async () => {
    await expect(deriveTenantPrefix("0102", vectors.cases[0].tenant)).rejects.toThrow();
  });

  it("returns a probe key only for a canonical sha256 digest", async () => {
    const c = vectors.cases[0];
    await expect(findMissingProbeKey(vectors.tdk_hex, c.region, c.tenant, c.sha256_digest))
      .resolves.toBe(c.sha256_key);
    for (const bad of ["", "zz", "A".repeat(64), "a".repeat(63), "a".repeat(65)]) {
      await expect(findMissingProbeKey(vectors.tdk_hex, c.region, c.tenant, bad)).resolves.toBeNull();
    }
  });

  it("hashes the UUID as 16 raw big-endian bytes, not as its text", async () => {
    const bytes = uuidToBytes("d863fafb-17c3-4ec3-92f6-b5a85c27d7bd");
    expect(bytes).not.toBeNull();
    expect(bytes!.length).toBe(16);
    expect(bytes![0]).toBe(0xd8);
    expect(bytes![15]).toBe(0xbd);
  });
});
