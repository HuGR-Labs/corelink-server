import type { D1Database } from "@cloudflare/workers-types";
import { issueComputeGrant } from "./compute_budget_grant.js";
import type { ComputeBinding, RunnerDevEnvRpc } from "../types/devenv_rpc.js";

export const DEVENV_COMPUTE_VCPU_COUNT = 4 as const;
export const DEVENV_COMPUTE_MAXIMUM_WALL_MS = 28_800_000 as const;
interface EntitlementRow { max_concurrency?: unknown; max_vcpu_h?: unknown; }
function positiveInteger(value: unknown): value is number { return typeof value === "number" && Number.isSafeInteger(value) && value > 0; }
function positiveU32(value: unknown): value is number { return positiveInteger(value) && value <= 0xffff_ffff; }

/** Validate entitlement and stage the external reservation. The DO owns staged-obligation recovery;
 * the relay owns compensation when a later mint/start/adopt step fails. */
export async function prepareDevenvCompute(
  env: { CONFIG_DB: D1Database; COMPUTE_GRANT_SIGNING_KEY?: string; COMPUTE_GRANT_KEY_ID?: string },
  stub: Pick<RunnerDevEnvRpc, "prepareAuthorizedCompute">,
  tenantId: string,
  sessionUuid: string,
  nowMs: number,
): Promise<string | null> {
  const row = await env.CONFIG_DB.prepare("SELECT max_concurrency, max_vcpu_h FROM runners_entitlement WHERE tenant_id = ?1").bind(tenantId).first<EntitlementRow>();
  if (row === null || !positiveInteger(row.max_concurrency)) throw new Error("invalid DevEnv runners entitlement");
  if (row.max_vcpu_h === null || row.max_vcpu_h === undefined) return null;
  if (!positiveU32(row.max_vcpu_h)) throw new Error("invalid DevEnv compute ceiling");
  const token = await issueComputeGrant(env, { tenantId, workloadKind: "devenv", workloadId: sessionUuid, reservationId: sessionUuid,
    maxVcpuHours: row.max_vcpu_h, vcpuCount: DEVENV_COMPUTE_VCPU_COUNT, maximumWallMs: DEVENV_COMPUTE_MAXIMUM_WALL_MS }, nowMs);
  const binding: ComputeBinding = { token, reservationId: sessionUuid, tenantId, workloadKind: "devenv", workloadId: sessionUuid,
    vcpuCount: DEVENV_COMPUTE_VCPU_COUNT, maximumWallMs: DEVENV_COMPUTE_MAXIMUM_WALL_MS };
  await stub.prepareAuthorizedCompute(binding);
  return sessionUuid;
}
