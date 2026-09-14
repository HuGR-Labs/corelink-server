/** Signed, short-lived external compute grant issued by the server authority. */
const MAX_WALL_MS = 8 * 60 * 60 * 1000;
const GRANT_TTL_MS = 90_000;
const MAX_TOKEN_BYTES = 8 * 1024;
const MAX_U32 = 0xffff_ffffn;
const MAX_I64 = 0x7fff_ffff_ffff_ffffn;
const MS_PER_VCPU_HOUR = 3_600_000n;
const UUID_RE = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const WORKLOAD_ID_RE = /^[A-Za-z0-9:_./-]{1,256}$/;
const KEY_ID_RE = /^[A-Za-z0-9_-]{1,64}$/;

interface ComputeGrantPayload {
  v: 1; key_id: string; tenant_id: string;
  workload_kind: "spawn_worker_runner" | "devenv"; workload_id: string; reservation_id: string;
  period_key: number; ceiling_vcpu_ms: string; vcpu_count: number; maximum_wall_ms: number;
  issued_at_ms: number; expires_at_ms: number;
}

function invalid(): Error { return new Error("invalid compute grant request"); }
function validDate(ms: number): boolean { return Number.isSafeInteger(ms) && ms >= 0 && ms <= 8_640_000_000_000_000; }
function periodKey(ms: number): number { const d = new Date(ms); return d.getUTCFullYear() * 100 + d.getUTCMonth() + 1; }
function nextPeriodStart(ms: number): number { const d = new Date(ms); return Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 1); }
function decodeBase64(value: string): Uint8Array {
  if (!value || value.length % 4 === 1 || !/^[A-Za-z0-9+/]*={0,2}$/.test(value)) throw invalid();
  try { return Uint8Array.from(atob(value), (c) => c.charCodeAt(0)); } catch { throw invalid(); }
}
function encodeBase64Url(bytes: Uint8Array): string {
  let binary = ""; for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}

export async function issueComputeGrant(
  env: { COMPUTE_GRANT_SIGNING_KEY?: string; COMPUTE_GRANT_KEY_ID?: string },
  input: { tenantId: string; workloadKind: "spawn_worker_runner" | "devenv"; workloadId: string; reservationId: string; maxVcpuHours: number; vcpuCount: number; maximumWallMs: number },
  nowMs: number,
): Promise<string> {
  if (!env || typeof env !== "object" || !input || typeof input !== "object") throw invalid();
  const key = env.COMPUTE_GRANT_SIGNING_KEY;
  const keyId = env.COMPUTE_GRANT_KEY_ID;
  if (typeof key !== "string" || key.length === 0 || key.length > 1024 || typeof keyId !== "string" || !KEY_ID_RE.test(keyId) ||
      !UUID_RE.test(input.tenantId) || !UUID_RE.test(input.reservationId) || !WORKLOAD_ID_RE.test(input.workloadId) ||
      (input.workloadKind !== "spawn_worker_runner" && input.workloadKind !== "devenv") ||
      !Number.isSafeInteger(input.maxVcpuHours) || input.maxVcpuHours <= 0 || BigInt(input.maxVcpuHours) > MAX_U32 ||
      !Number.isInteger(input.vcpuCount) || input.vcpuCount < 1 || input.vcpuCount > 16 ||
      !Number.isInteger(input.maximumWallMs) || input.maximumWallMs < 1 || input.maximumWallMs > MAX_WALL_MS || !validDate(nowMs)) throw invalid();
  const expires = nowMs + GRANT_TTL_MS;
  const grantEnd = expires + input.maximumWallMs;
  const period = periodKey(nowMs);
  if (!Number.isSafeInteger(expires) || !validDate(grantEnd) || grantEnd > nextPeriodStart(nowMs) || period < 197001 || period > 999912) throw invalid();
  const cap = BigInt(input.maxVcpuHours) * MS_PER_VCPU_HOUR;
  if (cap <= 0n || cap > MAX_I64) throw invalid();
  const payload: ComputeGrantPayload = { v: 1, key_id: keyId, tenant_id: input.tenantId, workload_kind: input.workloadKind,
    workload_id: input.workloadId, reservation_id: input.reservationId, period_key: period, ceiling_vcpu_ms: cap.toString(10),
    vcpu_count: input.vcpuCount, maximum_wall_ms: input.maximumWallMs, issued_at_ms: nowMs, expires_at_ms: expires };
  const bytes = new TextEncoder().encode(JSON.stringify(payload));
  try {
    const privateKey = await crypto.subtle.importKey("pkcs8", decodeBase64(key), { name: "Ed25519" }, false, ["sign"]);
    const signature = await crypto.subtle.sign({ name: "Ed25519" }, privateKey, bytes);
    const token = `${encodeBase64Url(bytes)}.${encodeBase64Url(new Uint8Array(signature))}`;
    if (new TextEncoder().encode(token).byteLength > MAX_TOKEN_BYTES) throw invalid();
    return token;
  } catch { throw invalid(); }
}
