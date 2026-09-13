/** Signed, short-lived external compute grant issued by the server authority. */
const GRANT_TTL_MS = 90_000;
const MAX_WALL_MS = 8 * 60 * 60 * 1000;
const MAX_I64 = 0x7fff_ffff_ffff_ffffn;
const MAX_U32 = 0xffff_ffffn;
const MS_PER_VCPU_HOUR = 3_600_000n;
const MAX_TOKEN_BYTES = 8 * 1024;
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const WORKLOAD = /^[A-Za-z0-9:_./-]{1,256}$/;
const KEY_ID = /^[A-Za-z0-9_-]{1,64}$/;
function invalid(): Error { return new Error("invalid compute grant request"); }
function b64decode(value: string): Uint8Array {
  if (!value || value.length % 4 === 1 || !/^[A-Za-z0-9+/]*={0,2}$/.test(value)) throw invalid();
  try { return Uint8Array.from(atob(value), (c) => c.charCodeAt(0)); } catch { throw invalid(); }
}
function b64url(bytes: Uint8Array): string {
  let binary = ""; for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
function periodKey(ms: number): number { const d = new Date(ms); return d.getUTCFullYear() * 100 + d.getUTCMonth() + 1; }
export async function issueComputeGrant(
  env: { COMPUTE_GRANT_SIGNING_KEY?: string; COMPUTE_GRANT_KEY_ID?: string },
  input: { tenantId: string; workloadKind: "spawn_worker_runner" | "devenv"; workloadId: string; reservationId: string; maxVcpuHours: number; vcpuCount: number; maximumWallMs: number },
  nowMs: number,
): Promise<string> {
  const key = env.COMPUTE_GRANT_SIGNING_KEY;
  const keyId = env.COMPUTE_GRANT_KEY_ID;
  if (typeof key !== "string" || key.length === 0 || key.length > 1024 || typeof keyId !== "string" || !KEY_ID.test(keyId) ||
      (input.workloadKind !== "spawn_worker_runner" && input.workloadKind !== "devenv") ||
      !UUID.test(input.tenantId) || !UUID.test(input.reservationId) || !WORKLOAD.test(input.workloadId) ||
      !Number.isSafeInteger(input.maxVcpuHours) || input.maxVcpuHours <= 0 || BigInt(input.maxVcpuHours) > MAX_U32 ||
      !Number.isInteger(input.vcpuCount) || input.vcpuCount < 1 || input.vcpuCount > 16 ||
      !Number.isInteger(input.maximumWallMs) || input.maximumWallMs < 1 || input.maximumWallMs > MAX_WALL_MS ||
      !Number.isSafeInteger(nowMs) || nowMs < 0 || nowMs > 8_640_000_000_000_000) throw invalid();
  const expires = nowMs + GRANT_TTL_MS;
  const cap = BigInt(input.maxVcpuHours) * MS_PER_VCPU_HOUR;
  const grantEnd = expires + input.maximumWallMs;
  if (!Number.isSafeInteger(expires) || !Number.isSafeInteger(grantEnd) || cap <= 0n || cap > MAX_I64 || periodKey(grantEnd) !== periodKey(nowMs)) throw invalid();
  const payload = { v: 1 as const, key_id: keyId, tenant_id: input.tenantId, workload_kind: input.workloadKind,
    workload_id: input.workloadId, reservation_id: input.reservationId, period_key: periodKey(nowMs),
    ceiling_vcpu_ms: cap.toString(10), vcpu_count: input.vcpuCount, maximum_wall_ms: input.maximumWallMs,
    issued_at_ms: nowMs, expires_at_ms: expires };
  const bytes = new TextEncoder().encode(JSON.stringify(payload));
  try {
    const privateKey = await crypto.subtle.importKey("pkcs8", b64decode(key), { name: "Ed25519" }, false, ["sign"]);
    const signature = await crypto.subtle.sign({ name: "Ed25519" }, privateKey, bytes);
    const token = `${b64url(bytes)}.${b64url(new Uint8Array(signature))}`;
    if (new TextEncoder().encode(token).byteLength > MAX_TOKEN_BYTES) throw invalid();
    return token;
  } catch { throw invalid(); }
}
