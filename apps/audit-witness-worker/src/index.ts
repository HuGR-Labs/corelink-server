import { blake3 } from "@noble/hashes/blake3";
import type {
  DurableObject,
  DurableObjectNamespace,
  DurableObjectState,
  DurableObjectStorage,
  DurableObjectTransaction,
} from "@cloudflare/workers-types";

const ZERO_HASH = "0".repeat(64);
const HASH_RE = /^[0-9a-f]{64}$/;
const PART_RE = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const TENANT_ID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const HEAD_DOMAIN = bytes("corelink/audit-chain/head-record/v1\0");
const WITNESS_DOMAIN = bytes("corelink/audit-chain/head-witness/v1\0");
const RECEIPT_DOMAIN = bytes("corelink/audit-chain/head-witness-receipt/v1\0");
const LATEST_DOMAIN = bytes("corelink/audit-chain/head-witness-latest/v1\0");
const LATEST_KEY = "latest";
const META_KEY = "meta";
const REGIONS = new Set(["wnam", "enam", "weur", "sam", "apac", "afr"]);

export interface Env {
  HEAD_WITNESS: DurableObjectNamespace;
  WITNESS_APPEND_TOKEN: string;
  WITNESS_RECEIPT_SIGNING_SEED_HEX: string;
  WITNESS_RECEIPT_KEY_ID: string;
  WITNESS_ID: string;
}

interface WitnessObject {
  head_message_b64: string;
  head_record_hash: string;
  head_signature_b64: string;
  previous_witness_hash: string;
  region: string;
  tenant_id: string;
  witness_sequence: number;
  witness_version: 1;
}

export interface AppendRequest {
  append_request_version: 1;
  expected_latest: { witness_record_hash: string; witness_sequence: number } | null;
  witness_jcs_b64: string;
}

export interface WitnessReceipt {
  receipt_jcs_b64: string;
  witness_key_id: number;
  receipt_signature_b64: string;
  witness_jcs_b64: string;
  witness_record_hash: string;
  witness_sequence: number;
}

interface LatestRequest {
  challenge_b64: string;
  latest_request_version: 1;
  region: string;
  tenant_id: string;
}

interface StoredLatest extends WitnessReceipt {
  tenant_id: string;
  region: string;
}

interface StoredMeta { witness_id: string; protocol_version: 1 }

function publicReceipt(stored: StoredLatest): WitnessReceipt {
  return {
    receipt_jcs_b64: stored.receipt_jcs_b64,
    witness_key_id: stored.witness_key_id,
    receipt_signature_b64: stored.receipt_signature_b64,
    witness_jcs_b64: stored.witness_jcs_b64,
    witness_record_hash: stored.witness_record_hash,
    witness_sequence: stored.witness_sequence,
  };
}

function bytes(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((sum, part) => sum + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function hex(value: Uint8Array): string {
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function decodeHex32(value: string, label: string): Uint8Array {
  if (!HASH_RE.test(value)) throw new Error(`${label} must be 64 lowercase hex`);
  return Uint8Array.from(value.match(/../g) ?? [], (pair) => Number.parseInt(pair, 16));
}

function decodeBase64(value: string, expectedBytes: number, label: string): Uint8Array {
  if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) {
    throw new Error(`${label} is not padded standard base64`);
  }
  let raw: string;
  try {
    raw = atob(value);
  } catch {
    throw new Error(`${label} is not base64`);
  }
  const decoded = Uint8Array.from(raw, (char) => char.charCodeAt(0));
  if (decoded.length !== expectedBytes) throw new Error(`${label} has wrong decoded length`);
  if (toBase64(decoded) !== value) throw new Error(`${label} is not canonical base64`);
  return decoded;
}

function toBase64(value: Uint8Array): string {
  let raw = "";
  for (const byte of value) raw += String.fromCharCode(byte);
  return btoa(raw);
}

function u64be(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error("length is not a safe u64");
  const out = new Uint8Array(8);
  let rest = BigInt(value);
  for (let index = 7; index >= 0; index -= 1) {
    out[index] = Number(rest & 0xffn);
    rest >>= 8n;
  }
  return out;
}

/** Minimal RFC-8785-compatible serializer for the integer/string/null objects in this protocol. */
export function canonicalJson(value: unknown): string {
  if (value === null || typeof value === "boolean" || typeof value === "string") {
    return JSON.stringify(value);
  }
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) throw new Error("numbers must be non-negative safe integers");
    return String(value);
  }
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (typeof value === "object") {
    const record = value as Record<string, unknown>;
    return `{${Object.keys(record).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(record[key])}`).join(",")}}`;
  }
  throw new Error("unsupported JSON value");
}

function exactKeys(value: Record<string, unknown>, expected: string[], label: string): void {
  const actual = Object.keys(value).sort();
  const wanted = [...expected].sort();
  if (actual.length !== wanted.length || actual.some((key, index) => key !== wanted[index])) {
    throw new Error(`${label} has unknown or missing fields`);
  }
}

function parseWitnessJcs(raw: string): WitnessObject {
  if (bytes(raw).length > 16 * 1024) throw new Error("witness_jcs exceeds 16KiB");
  const parsed: unknown = JSON.parse(raw);
  if (parsed === null || Array.isArray(parsed) || typeof parsed !== "object") {
    throw new Error("witness_jcs must be an object");
  }
  const record = parsed as Record<string, unknown>;
  exactKeys(record, [
    "head_message_b64", "head_record_hash", "head_signature_b64",
    "previous_witness_hash", "region", "tenant_id", "witness_sequence", "witness_version",
  ], "witness_jcs");
  if (canonicalJson(record) !== raw) throw new Error("witness_jcs is not exact RFC-8785 canonical JSON");
  if (record.witness_version !== 1) throw new Error("unknown witness_version");
  if (!Number.isSafeInteger(record.witness_sequence) || (record.witness_sequence as number) < 0) {
    throw new Error("witness_sequence must be a non-negative safe integer");
  }
  if (typeof record.tenant_id !== "string" || !TENANT_ID_RE.test(record.tenant_id)) throw new Error("invalid tenant_id");
  if (typeof record.region !== "string" || !REGIONS.has(record.region)) throw new Error("invalid region");
  for (const key of ["head_message_b64", "head_record_hash", "head_signature_b64", "previous_witness_hash"] as const) {
    if (typeof record[key] !== "string") throw new Error(`${key} must be a string`);
  }
  decodeHex32(record.head_record_hash as string, "head_record_hash");
  decodeHex32(record.previous_witness_hash as string, "previous_witness_hash");
  const headJcs = decodeBase64(record.head_message_b64 as string, atob(record.head_message_b64 as string).length, "head_message_b64");
  const headSignature = decodeBase64(record.head_signature_b64 as string, 64, "head_signature_b64");
  const headText = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(headJcs);
  const head: unknown = JSON.parse(headText);
  if (head === null || Array.isArray(head) || typeof head !== "object") throw new Error("head message must be an object");
  const headRecord = head as Record<string, unknown>;
  exactKeys(headRecord, ["epoch_id", "epoch_ledger_hash", "epoch_ledger_sequence", "head_hash", "head_message_version", "next_sequence", "region", "signing_key_id", "tenant_id"], "head message");
  if (canonicalJson(headRecord) !== headText || headRecord.head_message_version !== 2) throw new Error("head message is not canonical v2");
  if (headRecord.tenant_id !== record.tenant_id || headRecord.region !== record.region) throw new Error("head partition mismatch");
  for (const key of ["epoch_id", "epoch_ledger_sequence", "next_sequence"] as const) {
    if (!Number.isSafeInteger(headRecord[key]) || (headRecord[key] as number) < 0) throw new Error(`invalid ${key}`);
  }
  if (!Number.isSafeInteger(headRecord.signing_key_id) || (headRecord.signing_key_id as number) <= 0) throw new Error("invalid signing_key_id");
  if (typeof headRecord.head_hash !== "string" || !HASH_RE.test(headRecord.head_hash) ||
      typeof headRecord.epoch_ledger_hash !== "string" || !HASH_RE.test(headRecord.epoch_ledger_hash)) {
    throw new Error("invalid v2 head hash");
  }
  if (!REGIONS.has(record.region)) throw new Error("non-canonical region");
  const computedHeadHash = hex(blake3(concat(HEAD_DOMAIN, u64be(headJcs.length), headJcs, headSignature)));
  if (computedHeadHash !== record.head_record_hash) throw new Error("head_record_hash mismatch");
  return record as unknown as WitnessObject;
}

function parseAppendRequest(value: unknown): AppendRequest {
  if (value === null || Array.isArray(value) || typeof value !== "object") throw new Error("request must be an object");
  const record = value as Record<string, unknown>;
  exactKeys(record, ["append_request_version", "expected_latest", "witness_jcs_b64"], "append request");
  if (record.append_request_version !== 1) throw new Error("unknown append_request_version");
  if (record.expected_latest !== null) {
    if (Array.isArray(record.expected_latest) || typeof record.expected_latest !== "object") throw new Error("invalid expected_latest");
    const expected = record.expected_latest as Record<string, unknown>;
    exactKeys(expected, ["witness_record_hash", "witness_sequence"], "expected_latest");
    if (typeof expected.witness_record_hash !== "string") throw new Error("invalid expected hash");
    decodeHex32(expected.witness_record_hash, "expected_latest.witness_record_hash");
    if (!Number.isSafeInteger(expected.witness_sequence) || (expected.witness_sequence as number) < 0) throw new Error("invalid expected sequence");
  }
  if (typeof record.witness_jcs_b64 !== "string") throw new Error("witness_jcs_b64 must be a string");
  const decoded = decodeBase64(record.witness_jcs_b64, atob(record.witness_jcs_b64).length, "witness_jcs_b64");
  if (decoded.length > 16 * 1024) throw new Error("witness_jcs exceeds 16KiB");
  return record as unknown as AppendRequest;
}

function appendWitnessJcs(append: AppendRequest): string {
  return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(
    decodeBase64(append.witness_jcs_b64, atob(append.witness_jcs_b64).length, "witness_jcs_b64"),
  );
}

async function signingKey(seedHex: string): Promise<CryptoKey> {
  const seed = decodeHex32(seedHex, "receipt signing seed");
  const prefix = Uint8Array.from([0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20]);
  return crypto.subtle.importKey("pkcs8", concat(prefix, seed), { name: "Ed25519" }, false, ["sign"]);
}

function parsePositiveKeyId(raw: string): number {
  if (!/^[1-9][0-9]*$/.test(raw)) throw new Error("invalid receipt key id");
  const value = Number(raw);
  if (!Number.isSafeInteger(value)) throw new Error("invalid receipt key id");
  return value;
}

async function signEnvelope(domain: Uint8Array, payload: string, seedHex: string): Promise<string> {
  const payloadBytes = bytes(payload);
  const signature = new Uint8Array(await crypto.subtle.sign(
    "Ed25519", await signingKey(seedHex), concat(domain, u64be(payloadBytes.length), payloadBytes),
  ));
  return toBase64(signature);
}

async function makeReceipt(witness: WitnessObject, witnessJcs: string, env: Env): Promise<WitnessReceipt> {
  const keyId = parsePositiveKeyId(env.WITNESS_RECEIPT_KEY_ID);
  if (!PART_RE.test(env.WITNESS_ID)) throw new Error("invalid witness id");
  const witnessRecordHash = hex(blake3(concat(WITNESS_DOMAIN, bytes(witnessJcs))));
  const receiptJcs = canonicalJson({
    committed_at_ms: Date.now(),
    head_record_hash: witness.head_record_hash,
    previous_witness_hash: witness.previous_witness_hash,
    witness_key_id: keyId,
    region: witness.region,
    tenant_id: witness.tenant_id,
    witness_id: env.WITNESS_ID,
    witness_record_hash: witnessRecordHash,
    witness_sequence: witness.witness_sequence,
    receipt_version: 1,
  });
  return {
    receipt_jcs_b64: toBase64(bytes(receiptJcs)),
    witness_key_id: keyId,
    receipt_signature_b64: await signEnvelope(RECEIPT_DOMAIN, receiptJcs, env.WITNESS_RECEIPT_SIGNING_SEED_HEX),
    witness_jcs_b64: toBase64(bytes(witnessJcs)),
    witness_record_hash: witnessRecordHash,
    witness_sequence: witness.witness_sequence,
  };
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", "cache-control": "no-store" } });
}

function fail(code: string, status: number): Response {
  return json({ error: code }, status);
}

function configReady(env: Env): boolean {
  return typeof env.WITNESS_APPEND_TOKEN === "string" && env.WITNESS_APPEND_TOKEN.length >= 32 &&
    typeof env.WITNESS_RECEIPT_SIGNING_SEED_HEX === "string" && HASH_RE.test(env.WITNESS_RECEIPT_SIGNING_SEED_HEX) &&
    typeof env.WITNESS_ID === "string" && PART_RE.test(env.WITNESS_ID) &&
    typeof env.WITNESS_RECEIPT_KEY_ID === "string" && /^[1-9][0-9]*$/.test(env.WITNESS_RECEIPT_KEY_ID) &&
    Number.isSafeInteger(Number(env.WITNESS_RECEIPT_KEY_ID));
}

async function boundedJson(request: Request): Promise<{ raw: string; value: unknown }> {
  const declared = Number(request.headers.get("content-length") ?? "0");
  if (Number.isFinite(declared) && declared > 24 * 1024) throw new Error("request too large");
  const raw = await request.text();
  if (bytes(raw).length > 24 * 1024) throw new Error("request too large");
  return { raw, value: JSON.parse(raw) as unknown };
}

async function authorized(request: Request, expected: string): Promise<boolean> {
  if (expected.length < 32) return false;
  const supplied = request.headers.get("authorization")?.replace(/^Bearer /, "") ?? "";
  const [a, b] = await Promise.all([crypto.subtle.digest("SHA-256", bytes(expected)), crypto.subtle.digest("SHA-256", bytes(supplied))]);
  const left = new Uint8Array(a); const right = new Uint8Array(b);
  let diff = expected.length ^ supplied.length;
  for (let index = 0; index < left.length; index += 1) diff |= left[index] ^ right[index];
  return diff === 0;
}

export class AuditHeadWitness implements DurableObject {
  private readonly storage: DurableObjectStorage;
  private readonly env: Env;

  constructor(state: DurableObjectState, env: Env) {
    this.storage = state.storage;
    this.env = env;
  }

  async fetch(request: Request): Promise<Response> {
    if (!configReady(this.env)) return fail("CONFIG_UNAVAILABLE", 503);
    const url = new URL(request.url);
    if (request.method === "POST" && url.pathname === "/latest") {
      let latestRequest: LatestRequest;
      try {
        const { value: candidate } = await boundedJson(request);
        if (candidate === null || Array.isArray(candidate) || typeof candidate !== "object") throw new Error("invalid latest request");
        const record = candidate as Record<string, unknown>;
        exactKeys(record, ["challenge_b64", "latest_request_version", "region", "tenant_id"], "latest request");
        if (record.latest_request_version !== 1 || typeof record.challenge_b64 !== "string" ||
            typeof record.tenant_id !== "string" || typeof record.region !== "string") throw new Error("invalid latest request");
        decodeBase64(record.challenge_b64, 32, "challenge_b64");
        if (!TENANT_ID_RE.test(record.tenant_id) || !REGIONS.has(record.region)) throw new Error("invalid partition");
        latestRequest = record as unknown as LatestRequest;
      } catch { return fail("INVALID_REQUEST", 400); }
      const meta = await this.storage.get<StoredMeta>(META_KEY);
      if (meta !== undefined && (meta.protocol_version !== 1 || meta.witness_id !== this.env.WITNESS_ID)) {
        return fail("WITNESS_IDENTITY_MISMATCH", 503);
      }
      const latest = await this.storage.get<StoredLatest>(LATEST_KEY);
      if (latest !== undefined && (latest.tenant_id !== latestRequest.tenant_id || latest.region !== latestRequest.region)) {
        return fail("PARTITION_MISMATCH", 403);
      }
      const latestJcs = canonicalJson({
        challenge_b64: latestRequest.challenge_b64,
        latest: latest ?? null,
        latest_version: 1,
        observed_at_ms: Date.now(),
        witness_key_id: parsePositiveKeyId(this.env.WITNESS_RECEIPT_KEY_ID),
        region: latestRequest.region,
        tenant_id: latestRequest.tenant_id,
        witness_id: this.env.WITNESS_ID,
      });
      return json({
        latest_jcs_b64: toBase64(bytes(latestJcs)),
        latest_signature_b64: await signEnvelope(LATEST_DOMAIN, latestJcs, this.env.WITNESS_RECEIPT_SIGNING_SEED_HEX),
      });
    }
    if (request.method !== "POST" || url.pathname !== "/append") return fail("NOT_FOUND", 404);
    let append: AppendRequest;
    let witness: WitnessObject;
    try {
      append = parseAppendRequest((await boundedJson(request)).value);
      witness = parseWitnessJcs(appendWitnessJcs(append));
    } catch {
      return fail("INVALID_REQUEST", 400);
    }
    const witnessJcs = appendWitnessJcs(append);
    const candidateHash = hex(blake3(concat(WITNESS_DOMAIN, bytes(witnessJcs))));
    let outcome: { receipt?: WitnessReceipt; error?: string } = {};
    await this.storage.transaction(async (txn: DurableObjectTransaction) => {
      const meta = await txn.get<StoredMeta>(META_KEY);
      if (meta !== undefined && (meta.protocol_version !== 1 || meta.witness_id !== this.env.WITNESS_ID)) {
        outcome = { error: "WITNESS_IDENTITY_MISMATCH" };
        return;
      }
      const latest = await txn.get<StoredLatest>(LATEST_KEY);
      const recordKey = `record:${String(witness.witness_sequence).padStart(18, "0")}`;
      const existing = await txn.get<StoredLatest>(recordKey);
      if (existing?.witness_record_hash === candidateHash) {
        outcome = { receipt: publicReceipt(existing) };
        return;
      }
      if (existing !== undefined || latest?.witness_record_hash === candidateHash) {
        outcome = { error: "SEQUENCE_CONFLICT" };
        return;
      }
      const expectedSequence = latest?.witness_sequence ?? null;
      const expectedHash = latest?.witness_record_hash ?? ZERO_HASH;
      if ((append.expected_latest?.witness_sequence ?? null) !== expectedSequence ||
          (append.expected_latest?.witness_record_hash ?? ZERO_HASH) !== expectedHash) {
        outcome = { error: "CAS_MISMATCH" }; return;
      }
      const nextSequence = latest === undefined ? 0 : latest.witness_sequence + 1;
      if (witness.witness_sequence !== nextSequence || witness.previous_witness_hash !== expectedHash) {
        outcome = { error: "CHAIN_MISMATCH" }; return;
      }
      if (latest !== undefined && (latest.tenant_id !== witness.tenant_id || latest.region !== witness.region)) {
        outcome = { error: "PARTITION_MISMATCH" }; return;
      }
      const receipt = await makeReceipt(witness, witnessJcs, this.env);
      const stored: StoredLatest = { ...receipt, tenant_id: witness.tenant_id, region: witness.region };
      if (meta === undefined) await txn.put(META_KEY, { witness_id: this.env.WITNESS_ID, protocol_version: 1 } satisfies StoredMeta);
      await txn.put(LATEST_KEY, stored);
      await txn.put(recordKey, stored);
      outcome = { receipt };
    });
    if (outcome.error === "CAS_MISMATCH") return fail("CAS_MISMATCH", 409);
    if (outcome.error === "CHAIN_MISMATCH") return fail("CHAIN_MISMATCH", 422);
    if (outcome.error === "PARTITION_MISMATCH") return fail("PARTITION_MISMATCH", 403);
    if (outcome.error === "SEQUENCE_CONFLICT") return fail("SEQUENCE_CONFLICT", 409);
    if (outcome.error === "WITNESS_IDENTITY_MISMATCH") return fail("WITNESS_IDENTITY_MISMATCH", 503);
    return outcome.receipt === undefined ? fail("INDETERMINATE", 503) : json(outcome.receipt, 201);
  }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (!configReady(env)) return fail("CONFIG_UNAVAILABLE", 503);
    if (!(await authorized(request, env.WITNESS_APPEND_TOKEN))) return fail("UNAUTHORIZED", 401);
    const url = new URL(request.url);
    if (request.method === "POST" && url.pathname === "/v1/audit-chain/head-witness/compare-and-append") {
      if (request.headers.get("content-type")?.split(";", 1)[0].trim().toLowerCase() !== "application/jcs+json") {
        return fail("UNSUPPORTED_MEDIA_TYPE", 415);
      }
      let body: AppendRequest; let witness: WitnessObject;
      try {
        const parsed = await boundedJson(request);
        body = parseAppendRequest(parsed.value);
        if (canonicalJson(parsed.value) !== parsed.raw) throw new Error("outer request is not canonical");
        witness = parseWitnessJcs(appendWitnessJcs(body));
      }
      catch { return fail("INVALID_REQUEST", 400); }
      const candidateHash = hex(blake3(concat(WITNESS_DOMAIN, bytes(appendWitnessJcs(body)))));
      if (request.headers.get("idempotency-key") !== candidateHash) return fail("INVALID_IDEMPOTENCY_KEY", 422);
      const stub = env.HEAD_WITNESS.get(env.HEAD_WITNESS.idFromName(`${witness.tenant_id}\0${witness.region}`));
      return stub.fetch(new Request("https://witness.internal/append", { method: "POST", body: JSON.stringify(body), headers: { "content-type": "application/json" } }));
    }
    if (request.method === "POST" && url.pathname === "/v1/audit-chain/head-witness/latest") {
      if (request.headers.get("content-type")?.split(";", 1)[0].trim().toLowerCase() !== "application/jcs+json") {
        return fail("UNSUPPORTED_MEDIA_TYPE", 415);
      }
      let latestRequest: LatestRequest;
      try {
        const parsed = await boundedJson(request);
        if (canonicalJson(parsed.value) !== parsed.raw) throw new Error("outer request is not canonical");
        latestRequest = parsed.value as LatestRequest;
        if (!TENANT_ID_RE.test(latestRequest.tenant_id) || !REGIONS.has(latestRequest.region)) throw new Error("invalid partition");
      } catch { return fail("INVALID_REQUEST", 400); }
      const tenant = latestRequest.tenant_id;
      const region = latestRequest.region;
      const stub = env.HEAD_WITNESS.get(env.HEAD_WITNESS.idFromName(`${tenant}\0${region}`));
      return stub.fetch(new Request("https://witness.internal/latest", { method: "POST", body: JSON.stringify(latestRequest), headers: { "content-type": "application/json" } }));
    }
    return fail("NOT_FOUND", 404);
  },
};
