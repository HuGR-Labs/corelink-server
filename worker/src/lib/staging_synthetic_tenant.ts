/** Exact-run staging-only synthetic tenant provisioner for the BYOK teardown lane. */

import type { D1Database } from "@cloudflare/workers-types";
import type { Env } from "../index_common.js";

const ADMISSION_HEADER = "x-corelink-staging-load-admission";
const REQUEST_ID_HEADER = "x-corelink-staging-request-id";
const ADMISSION_DOMAIN = "corelink/staging-ownership-envelope/v1\0";
const TENANT_LOCATOR_DOMAIN = "corelink/staging-synthetic-tenant-ref/v1\0";
const MAX_ENVELOPE_BYTES = 512;
const MAX_LIFETIME_MS = 15 * 60 * 1_000;
const APPROVED_STAGING_REGION = "wnam";

type Context = Readonly<{
  runId: string;
  scenario: "byok";
  targetDeploymentSha: string;
  issuedAtMs: number;
  expiresAtMs: number;
  requestId: string;
}>;

type Db = Pick<D1Database, "prepare" | "batch">;

export type SyntheticTenantLocator = Readonly<{
  run_id: string;
  scenario: "byok";
  /** Lowercase SHA-256 over the frozen length-framed exact-run locator fields. */
  tenant_ref: string;
}>;

const INVALID = "staging synthetic tenant admission is invalid";

function reject(): never {
  throw new Error(INVALID);
}

function canonicalMs(value: string): number {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) reject();
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || String(parsed) !== value) reject();
  return parsed;
}

function hex(value: string, length: number): boolean {
  return value.length === length && /^[0-9a-f]+$/.test(value);
}

function bytesToHex(value: ArrayBuffer): string {
  return Array.from(new Uint8Array(value), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function verifyAdmission(request: Request, env: Env, nowMs: number): Promise<Context> {
  const encoded = request.headers.get(ADMISSION_HEADER);
  const expectedRequestId = request.headers.get(REQUEST_ID_HEADER);
  const keyText = env.CORELINK_STAGING_LOAD_TEST_ADMISSION_KEY ?? "";
  if (
    env.CORELINK_ENVIRONMENT !== "staging" || env.ENVIRONMENT !== "staging" ||
    request.body !== null || new URL(request.url).search !== "" ||
    !encoded || !expectedRequestId || !hex(expectedRequestId, 32) ||
    new TextEncoder().encode(encoded).byteLength > MAX_ENVELOPE_BYTES ||
    encoded.includes("\n") || encoded.includes("\r") ||
    new TextEncoder().encode(keyText).byteLength < 32 || !Number.isSafeInteger(nowMs) || nowMs < 0
  ) reject();

  const fields = encoded.split(".");
  if (fields.length !== 9) reject();
  const [version, runId, scenario, targetEnvironment, deploymentSha, issuedRaw, expiresRaw, requestId, tag] = fields as [
    string, string, string, string, string, string, string, string, string,
  ];
  if (
    version !== "v1" || !/^[1-9][0-9]{0,19}$/.test(runId) || scenario !== "byok" ||
    targetEnvironment !== "staging" || !hex(deploymentSha, 40) ||
    !hex(requestId, 32) || requestId !== expectedRequestId || !hex(tag, 64)
  ) reject();

  const issuedAtMs = canonicalMs(issuedRaw);
  const expiresAtMs = canonicalMs(expiresRaw);
  if (
    expiresAtMs <= issuedAtMs || expiresAtMs - issuedAtMs > MAX_LIFETIME_MS ||
    issuedAtMs > nowMs || nowMs >= expiresAtMs
  ) reject();

  const key = await crypto.subtle.importKey(
    "raw", new TextEncoder().encode(keyText), { name: "HMAC", hash: "SHA-256" }, false, ["verify"],
  );
  const signed = new TextEncoder().encode(`${ADMISSION_DOMAIN}${fields.slice(0, 8).join(".")}`);
  const tagBytes = new Uint8Array(32);
  for (let i = 0; i < tagBytes.length; i++) tagBytes[i] = Number.parseInt(tag.slice(i * 2, i * 2 + 2), 16);
  if (!(await crypto.subtle.verify("HMAC", key, tagBytes, signed))) reject();

  return Object.freeze({ runId, scenario, targetDeploymentSha: deploymentSha, issuedAtMs, expiresAtMs, requestId });
}

function prefix(length: number): Uint8Array {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigUint64(0, BigInt(length), false);
  return bytes;
}

/** Frozen cross-language #2664/#2666 tenant locator hash. */
export async function computeSyntheticTenantRef(
  runId: string,
  scenario: "byok",
  targetDeploymentSha: string,
  tenantId: string,
): Promise<string> {
  const encoder = new TextEncoder();
  const parts = [runId, scenario, targetDeploymentSha, tenantId]
    .map((part) => encoder.encode(part));
  const domain = encoder.encode(TENANT_LOCATOR_DOMAIN);
  const input = new Uint8Array(domain.byteLength + parts.reduce((sum, part) => sum + 8 + part.byteLength, 0));
  let offset = 0;
  input.set(domain, offset);
  offset += domain.byteLength;
  for (const part of parts) {
    const size = prefix(part.byteLength);
    input.set(size, offset);
    offset += size.byteLength;
    input.set(part, offset);
    offset += part.byteLength;
  }
  return bytesToHex(await crypto.subtle.digest("SHA-256", input));
}

const BASELINE_TABLES = [
  "tenant_byok_config", "tenant_byok_secret", "tenant_byok_config_history", "tenant_byok_secret_history",
  "byok_tenant_gate", "byok_transition_fence", "byok_data_intent", "byok_logical_object_generation",
  "byok_logical_object_publication", "byok_transition_commit_guard", "byok_control_outcome",
  "byok_backfill_run", "byok_activation_guard", "byok_activation_source_capture", "byok_activation_intent",
  "byok_activation_key_health", "byok_activation_source_object", "byok_object_purge_item",
  "byok_purge_identity_quarantine", "byok_envelope",
] as const;

function baselineExistsSql(): string {
  return BASELINE_TABLES.map((table) => `EXISTS (SELECT 1 FROM ${table} WHERE tenant_id = ?1)`).join(" OR ");
}

/** Provision one fresh run-bound BYOK tenant; the returned locator omits tenant_id. */
export async function provisionSyntheticByokTenant(
  request: Request,
  env: Env,
  nowMs = Date.now(),
  db: Db = env.CONFIG_DB,
): Promise<SyntheticTenantLocator> {
  const context = await verifyAdmission(request, env, nowMs);
  const run = await db.prepare(
    "SELECT target_deployment_sha FROM staging_load_test_runs " +
      "WHERE run_id = ?1 AND scenario = 'byok' AND target_environment = 'staging' AND state = 'open' LIMIT 1",
  ).bind(context.runId).first<{ target_deployment_sha: string }>();
  if (!run || run.target_deployment_sha !== context.targetDeploymentSha) reject();

  // The identity is generated here, never accepted from the caller. The
  // marker guard in the D1 batch makes a replay of this run fail closed.
  const tenantId = crypto.randomUUID();
  const tenantRef = await computeSyntheticTenantRef(
    context.runId, context.scenario, context.targetDeploymentSha, tenantId,
  );
  const at = nowMs;

  const statements = [
    // 0151's marker key allows multiple tenant IDs per run, so enforce its
    // one-tenant child contract within the same serialized D1 transaction.
    db.prepare(
      "INSERT INTO staging_load_test_synthetic_tenants " +
        "(run_id, scenario, tenant_id, baseline_marker, marked_at_ms) " +
        "SELECT ?2, 'byok', ?1, 'tenant_already_provisioned', ?3 " +
        "WHERE EXISTS (SELECT 1 FROM staging_load_test_synthetic_tenants WHERE run_id = ?2 AND scenario = 'byok')",
    ).bind(tenantId, context.runId, at),
    // An invalid marker is deliberately attempted only if any prior tenant or
    // BYOK state exists. Its CHECK failure aborts the whole D1 batch.
    db.prepare(
      "INSERT INTO staging_load_test_synthetic_tenants " +
        "(run_id, scenario, tenant_id, baseline_marker, marked_at_ms) " +
        `SELECT ?2, 'byok', ?1, 'preexisting_state_detected', ?3 WHERE ` +
        `(EXISTS (SELECT 1 FROM tenant WHERE tenant_id = ?1) OR ${baselineExistsSql()})`,
    ).bind(tenantId, context.runId, at),
    db.prepare(
      "INSERT INTO tenant (tenant_id, primary_region, created_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?3)",
    ).bind(tenantId, APPROVED_STAGING_REGION, at),
    db.prepare(
      "INSERT INTO staging_load_test_synthetic_tenants " +
        "(run_id, scenario, tenant_id, baseline_marker, marked_at_ms) " +
        "VALUES (?1, 'byok', ?2, 'generation_zero_empty', ?3)",
    ).bind(context.runId, tenantId, at),
    // Same-transaction readback guard: 0118's tenant trigger must have inserted
    // generation zero, and no BYOK material may appear before this commit.
    db.prepare(
      "INSERT INTO staging_load_test_synthetic_tenants " +
        "(run_id, scenario, tenant_id, baseline_marker, marked_at_ms) " +
        `SELECT ?2, 'byok', ?1, 'baseline_readback_failed', ?3 WHERE ` +
        `(NOT EXISTS (SELECT 1 FROM byok_tenant_gate WHERE tenant_id = ?1 AND current_generation = 0) ` +
        `OR ${BASELINE_TABLES.filter((table) => table !== "byok_tenant_gate").map((table) => `EXISTS (SELECT 1 FROM ${table} WHERE tenant_id = ?1)`).join(" OR ")})`,
    ).bind(tenantId, context.runId, at),
  ];
  await db.batch(statements);
  return Object.freeze({
    run_id: context.runId,
    scenario: "byok",
    tenant_ref: tenantRef,
  });
}

export function stagingSyntheticTenantAuth(request: Request, env: Env): Response | null {
  if (request.method !== "POST") return new Response(null, { status: 405, headers: { allow: "POST" } });
  if (env.CORELINK_ENVIRONMENT !== "staging" || env.ENVIRONMENT !== "staging") {
    return Response.json({ error: "unavailable" }, { status: 404 });
  }
  return null;
}

/** Handler keeps failures fixed and never returns or logs credentials/tenant identity. */
export async function handleStagingSyntheticTenant(request: Request, env: Env): Promise<Response> {
  const preflight = stagingSyntheticTenantAuth(request, env);
  if (preflight) return preflight;
  try {
    const locator = await provisionSyntheticByokTenant(request, env);
    return Response.json({ locator }, { status: 201, headers: { "cache-control": "no-store" } });
  } catch {
    return Response.json({ error: INVALID }, { status: 403, headers: { "cache-control": "no-store" } });
  }
}
