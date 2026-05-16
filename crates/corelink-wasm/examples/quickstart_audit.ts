// quickstart_audit — Stream audit events via GET /v1/admin/audit-events.
//
// Customer concept: every privileged op is Merkle-chained into the
// audit log (INV-AUDIT-APPEND-ONLY). Response includes chain_head_hash
// for client-side verification (CTRL-AUDIT-002).
//
// Run: CORELINK_PAT=$ADMIN_PAT tsx examples/quickstart_audit.ts

const api = process.env.CORELINK_API_URL ?? "https://sandbox.corelink.dev";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}

const url = new URL(`${api}/v1/admin/audit-events`);
url.searchParams.set("limit", "50");

try {
  const resp = await fetch(url, {
    method: "GET",
    headers: { Authorization: `Bearer ${pat}`, Accept: "application/json" },
  });
  if (!resp.ok) {
    const body = await resp.text();
    console.error(`server returned ${resp.status}: ${body}`);
    process.exit(1);
  }
  const payload = (await resp.json()) as { events?: unknown[]; chain_head_hash?: string };
  const count = Array.isArray(payload.events) ? payload.events.length : 0;
  const head = payload.chain_head_hash ?? "<none>";
  console.log(`status=${resp.status} events=${count} chain_head=${head}`);
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
