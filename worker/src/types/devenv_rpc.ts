/** Server-to-runners control capability; never accepted as a public HTTP body. */
export interface AuthorizedDevenvInput {
  config: { workspaceName: string; profileName: string; tier: string };
  grant: {
    tenantId: string;
    sessionUuid: string;
    casPat: string;
    patId: string;
    expiresAtMs: number;
    lifecycleGeneration: string;
    computeReservationId?: string;
  };
}
export interface ComputeBinding {
  token: string;
  reservationId: string;
  tenantId: string;
  workloadKind: "devenv";
  workloadId: string;
  vcpuCount: 4;
  maximumWallMs: 28_800_000;
}
export interface AuthorizedDevenvAck {
  sessionUuid: string;
  status: "starting" | "running";
}
/** The brand is required by the Cloudflare DurableObjectNamespace RPC generic. */
export interface RunnerDevEnvRpc extends Rpc.DurableObjectBranded {
  startAuthorizedDevenv(input: AuthorizedDevenvInput): Promise<AuthorizedDevenvAck>;
  prepareAuthorizedCompute(binding: ComputeBinding): Promise<void>;
  abandonAuthorizedCompute(reservationId: string): Promise<void>;
}
