/** Shared, request-scoped ownership provenance for synthetic staging writes. */

const ENVELOPE_DOMAIN = "corelink/staging-ownership-envelope/v1\0";
const RECEIPT_DOMAIN = "corelink-staging-load-test-resource-receipt-v1\0";
const MAX_ENVELOPE_BYTES = 512;
const MAX_LIFETIME_MS = 15 * 60 * 1_000;
const INVALID_ENVELOPE = "staging ownership envelope is invalid";

export type StagingOwnershipScenario =
  | "signup"
  | "webhook"
  | "dsr"
  | "cas"
  | "byok"
  | "endurance-2h"
  | "b103-cargo-write";

export type StagingOwnershipResourceClass =
  | "cas_reference"
  | "webhook_inbox"
  | "webhook_effect"
  | "dsr_artifact"
  | "dsr_obligation"
  | "audit_evidence"
  | "billing_audit"
  | "signup_artifact"
  | "byok_artifact";

export type StagingOwnershipContext = Readonly<{
  runId: string;
  scenario: StagingOwnershipScenario;
  targetDeploymentSha: string;
  issuedAtMs: number;
  expiresAtMs: number;
  requestId: string;
}>;

const verifiedContexts = new WeakSet<object>();
const scenarios = new Set<StagingOwnershipScenario>([
  "signup",
  "webhook",
  "dsr",
  "cas",
  "byok",
  "endurance-2h",
  "b103-cargo-write",
]);
const retainedClasses = new Set<StagingOwnershipResourceClass>([
  "cas_reference",
  "dsr_obligation",
  "audit_evidence",
  "billing_audit",
]);

function reject(): never {
  throw new Error(INVALID_ENVELOPE);
}

function lowerHex(value: string, length: number): boolean {
  return value.length === length && /^[0-9a-f]+$/.test(value);
}

function canonicalPositiveDecimal(value: string, maxLength: number): boolean {
  return value.length > 0 && value.length <= maxLength && /^[1-9][0-9]*$/.test(value);
}

function parseCanonicalMs(value: string): number {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) reject();
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 0 || String(parsed) !== value) reject();
  return parsed;
}

function hexBytes(value: string): ArrayBuffer {
  if (!lowerHex(value, 64)) reject();
  const bytes = new Uint8Array(32);
  for (let index = 0; index < value.length; index += 2) {
    bytes[index / 2] = Number.parseInt(value.slice(index, index + 2), 16);
  }
  return bytes.buffer;
}

function bytesToHex(value: ArrayBuffer): string {
  return Array.from(new Uint8Array(value), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Verify one bounded internal envelope. Absence is ordinary traffic. */
export async function verifyStagingOwnershipEnvelope(
  encoded: string | null,
  expectedScenario: StagingOwnershipScenario,
  expectedRequestId: string,
  nowMs: number,
  signingKey: string,
): Promise<StagingOwnershipContext | null> {
  if (encoded === null) return null;
  const encodedBytes = new TextEncoder().encode(encoded);
  if (encodedBytes.byteLength > MAX_ENVELOPE_BYTES || encoded.includes("\n") || encoded.includes("\r")) reject();

  const fields = encoded.split(".");
  if (fields.length !== 9) reject();
  const [version, runId, scenarioRaw, environment, sha, issuedRaw, expiresRaw, requestId, tag] = fields as [
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
    string,
  ];
  if (
    version !== "v1" ||
    environment !== "staging" ||
    !canonicalPositiveDecimal(runId, 20) ||
    !scenarios.has(scenarioRaw as StagingOwnershipScenario) ||
    scenarioRaw !== expectedScenario ||
    !lowerHex(sha, 40) ||
    !lowerHex(requestId, 32) ||
    requestId !== expectedRequestId ||
    !Number.isSafeInteger(nowMs) ||
    nowMs < 0
  ) reject();

  const issuedAtMs = parseCanonicalMs(issuedRaw);
  const expiresAtMs = parseCanonicalMs(expiresRaw);
  if (
    expiresAtMs <= issuedAtMs ||
    expiresAtMs - issuedAtMs > MAX_LIFETIME_MS ||
    issuedAtMs > nowMs ||
    nowMs >= expiresAtMs
  ) reject();

  const keyBytes = new TextEncoder().encode(signingKey);
  if (keyBytes.byteLength < 32) reject();
  const key = await crypto.subtle.importKey("raw", keyBytes, { name: "HMAC", hash: "SHA-256" }, false, ["verify"]);
  const payload = fields.slice(0, 8).join(".");
  const signed = new TextEncoder().encode(ENVELOPE_DOMAIN + payload);
  if (!(await crypto.subtle.verify("HMAC", key, hexBytes(tag), signed))) reject();

  const context: StagingOwnershipContext = Object.freeze({
    runId,
    scenario: scenarioRaw as StagingOwnershipScenario,
    targetDeploymentSha: sha,
    issuedAtMs,
    expiresAtMs,
    requestId,
  });
  verifiedContexts.add(context);
  return context;
}

function lengthPrefix(length: number): Uint8Array {
  const prefix = new Uint8Array(8);
  new DataView(prefix.buffer).setBigUint64(0, BigInt(length), false);
  return prefix;
}

async function receiptRef(
  context: StagingOwnershipContext,
  resourceClass: StagingOwnershipResourceClass,
  disposition: "disposable" | "retained",
  opaqueHandle: string,
): Promise<string> {
  const encoder = new TextEncoder();
  const parts = [context.runId, context.scenario, context.targetDeploymentSha, resourceClass, opaqueHandle, disposition]
    .map((value) => encoder.encode(value));
  const domain = encoder.encode(RECEIPT_DOMAIN);
  const size = domain.byteLength + parts.reduce((total, part) => total + 8 + part.byteLength, 0);
  const input = new Uint8Array(size);
  let offset = 0;
  input.set(domain, offset);
  offset += domain.byteLength;
  for (const part of parts) {
    input.set(lengthPrefix(part.byteLength), offset);
    offset += 8;
    input.set(part, offset);
    offset += part.byteLength;
  }
  return bytesToHex(await crypto.subtle.digest("SHA-256", input));
}

/** Build the statement that must share the domain mutation's D1 batch. */
export async function ownershipInsertStatement(
  db: D1Database,
  context: StagingOwnershipContext,
  resourceClass: StagingOwnershipResourceClass,
  disposition: "disposable" | "retained",
  opaqueHandle: string,
  registeredAtMs: number,
): Promise<D1PreparedStatement> {
  if (!verifiedContexts.has(context)) reject();
  if (
    opaqueHandle.length < 1 ||
    opaqueHandle.length > 512 ||
    /[\u0000-\u001f\u007f]/.test(opaqueHandle) ||
    !Number.isSafeInteger(registeredAtMs) ||
    registeredAtMs < 0 ||
    (retainedClasses.has(resourceClass) && disposition !== "retained")
  ) reject();
  const receipt = await receiptRef(context, resourceClass, disposition, opaqueHandle);
  return db.prepare(
    "INSERT INTO staging_load_test_resources " +
      "(run_id, scenario, resource_class, receipt_ref, opaque_handle, disposition, state, registered_at_ms) " +
      "VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'registered', ?7) " +
      "ON CONFLICT (run_id, scenario, resource_class, receipt_ref) DO NOTHING",
  ).bind(
    context.runId,
    context.scenario,
    resourceClass,
    receipt,
    opaqueHandle,
    disposition,
    registeredAtMs,
  );
}
