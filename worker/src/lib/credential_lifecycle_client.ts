export interface CredentialLifecycleEnv {
  FABRIC_CREDENTIAL_AUTHORITY_URL?: string;
  FABRIC_CREDENTIAL_ISSUER_AUTH_KEY?: string;
}

export interface CredentialLifecycleSnapshot {
  tenantId: string;
  generation: string;
}

const MAX_BODY_BYTES = 4 * 1024;
const DEADLINE_MS = 5_000;
const I64_MAX = 9_223_372_036_854_775_807n;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const DECIMAL = /^(0|[1-9][0-9]*)$/;

function fail(message: string): never {
  throw new Error(message);
}

function validateTenant(tenantId: string): void {
  if (typeof tenantId !== "string" || tenantId !== tenantId.toLowerCase() || !UUID.test(tenantId) || /^0{8}-0{4}-0{4}-0{4}-0{12}$/.test(tenantId)) {
    fail("invalid tenant id");
  }
}

function validateEndpoint(raw: string): URL {
  let url: URL;
  try { url = new URL(raw); } catch { fail("credential authority URL is invalid"); }
  if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
    fail("credential authority URL must be an HTTPS origin");
  }
  return url;
}

function validateKey(key: string): void {
  if (typeof key !== "string" || key.length < 32 || key.length > 4096 || [...key].some(c => c.charCodeAt(0) < 0x21 || c.charCodeAt(0) > 0x7e)) {
    fail("credential authority key is invalid");
  }
}

function validateGeneration(generation: unknown): generation is string {
  if (typeof generation !== "string" || generation.length > 19 || !DECIMAL.test(generation)) return false;
  try { return BigInt(generation) <= I64_MAX; } catch { return false; }
}

async function boundedBody(response: Response, signal: AbortSignal): Promise<string> {
  if (!response.body) return "";
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  let onAbort: (() => void) | undefined;
  const aborted = new Promise<never>((_, reject) => {
    onAbort = () => {
      void reader.cancel("credential lifecycle request aborted").catch(() => {});
      reject(new Error("credential lifecycle request timed out"));
    };
    if (signal.aborted) onAbort();
    else signal.addEventListener("abort", onAbort, { once: true });
  });
  try {
    for (;;) {
      const part = await Promise.race([reader.read(), aborted]);
      if (part.done) break;
      total += part.value.byteLength;
      if (total > MAX_BODY_BYTES) {
        void reader.cancel("response too large").catch(() => {});
        fail("credential lifecycle response too large");
      }
      chunks.push(part.value);
    }
  } finally {
    if (onAbort) signal.removeEventListener("abort", onAbort);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return new TextDecoder().decode(bytes);
}

function decodedTopLevelKeys(raw: string): string[] {
  const keys: string[] = [];
  let depth = 0;
  let expectKey = false;
  for (let i = 0; i < raw.length; i++) {
    const char = raw[i];
    if (char === '"') {
      const start = i++;
      let escaped = false;
      for (; i < raw.length; i++) {
        const current = raw[i];
        if (escaped) { escaped = false; continue; }
        if (current === "\\") { escaped = true; continue; }
        if (current === '"') break;
      }
      if (i >= raw.length) fail("invalid credential lifecycle response");
      if (depth === 1 && expectKey) {
        let decoded: unknown;
        try { decoded = JSON.parse(raw.slice(start, i + 1)); } catch { fail("invalid credential lifecycle response"); }
        if (typeof decoded !== "string") fail("invalid credential lifecycle response");
        keys.push(decoded);
        expectKey = false;
      }
      continue;
    }
    if (char === "{") { depth++; if (depth === 1) expectKey = true; continue; }
    if (char === "}") { depth--; continue; }
    if (depth === 1 && char === ",") { expectKey = true; continue; }
    if (depth === 1 && char === ":") { expectKey = false; }
  }
  if (depth !== 0) fail("invalid credential lifecycle response");
  return keys;
}

function parseSnapshot(raw: string, tenantId: string): CredentialLifecycleSnapshot {
  const keyCounts = new Map<string, number>();
  for (const key of decodedTopLevelKeys(raw)) keyCounts.set(key, (keyCounts.get(key) ?? 0) + 1);
  if (["tenant_id", "generation", "suspended"].some(key => keyCounts.get(key) !== 1)) fail("invalid credential lifecycle response");
  let parsed: unknown;
  try { parsed = JSON.parse(raw); } catch { fail("invalid credential lifecycle response"); }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) fail("invalid credential lifecycle response");
  const value = parsed as Record<string, unknown>;
  if (Object.keys(value).length !== 3 || value["tenant_id"] !== tenantId || !validateGeneration(value["generation"]) || value["suspended"] !== false) {
    fail("invalid credential lifecycle response");
  }
  return { tenantId, generation: value["generation"] };
}

export async function readCredentialLifecycle(env: CredentialLifecycleEnv, tenantId: string): Promise<CredentialLifecycleSnapshot> {
  validateTenant(tenantId);
  if (typeof env.FABRIC_CREDENTIAL_AUTHORITY_URL !== "string" || env.FABRIC_CREDENTIAL_AUTHORITY_URL.length === 0) fail("credential authority URL is unavailable");
  if (typeof env.FABRIC_CREDENTIAL_ISSUER_AUTH_KEY !== "string") fail("credential authority key is unavailable");
  const endpoint = validateEndpoint(env.FABRIC_CREDENTIAL_AUTHORITY_URL);
  const key = env.FABRIC_CREDENTIAL_ISSUER_AUTH_KEY;
  validateKey(key);
  const controller = new AbortController();
  let timedOut = false;
  let rejectDeadline!: (error: Error) => void;
  const deadline = new Promise<never>((_, reject) => { rejectDeadline = reject; });
  const timer = setTimeout(() => { timedOut = true; controller.abort(); rejectDeadline(new Error("credential lifecycle request timed out")); }, DEADLINE_MS);
  try {
    let response: Response;
    try {
      response = await Promise.race([
        fetch(new URL(`/internal/v1/credentials/tenants/${encodeURIComponent(tenantId)}/lifecycle`, endpoint).toString(), {
          method: "GET", redirect: "error", cache: "no-store", signal: controller.signal,
          headers: { "x-corelink-internal-auth": key },
        }),
        deadline,
      ]);
    } catch {
      fail(timedOut ? "credential lifecycle request timed out" : "credential lifecycle request unavailable");
    }
    if (response.status !== 200) fail("credential lifecycle request rejected");
    return parseSnapshot(await boundedBody(response, controller.signal), tenantId);
  } finally {
    clearTimeout(timer);
  }
}
