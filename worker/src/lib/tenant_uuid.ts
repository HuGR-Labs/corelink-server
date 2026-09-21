/** Canonical tenant identity: raw lowercase RFC 4122 UUID v4 or v7. */
const TENANT_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[47][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

export function isCanonicalTenantUuid(value: unknown): value is string {
  return typeof value === "string" && TENANT_UUID.test(value);
}
