import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { isCanonicalTenantUuid } from "../src/lib/tenant_uuid.js";

const vectors = JSON.parse(readFileSync(new URL("../../docs/contracts/compute-grant-identifiers-v1.json", import.meta.url), "utf8")) as { tenant_id: { accept: string[]; reject: string[] } };

describe("canonical tenant UUID", () => {
  it.each(vectors.tenant_id.accept)("accepts %s", tenantId => expect(isCanonicalTenantUuid(tenantId)).toBe(true));
  it.each(vectors.tenant_id.reject)("rejects %s", tenantId => expect(isCanonicalTenantUuid(tenantId)).toBe(false));
});
