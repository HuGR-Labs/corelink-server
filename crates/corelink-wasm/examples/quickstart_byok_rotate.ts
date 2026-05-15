// quickstart_byok_rotate — File a BYOK rotation op via POST /v1/admin/ops.
//
// Customer concept: privileged ops are dual-approval gated
// (CTRL-DUAL-APPROVAL-001). PENDING until a second admin approves via
// POST /v1/admin/ops/{op_id}/approve.
//
// Run: CORELINK_PAT=$ADMIN_PAT tsx examples/quickstart_byok_rotate.ts

const api = process.env.CORELINK_API_URL ?? "https://sandbox.corelink.dev";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}

const idem = `idem-${crypto.randomUUID()}`;
try {
  const resp = await fetch(`${api}/v1/admin/ops`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${pat}`,
      "Content-Type": "application/json",
      "Idempotency-Key": idem,
    },
    body: JSON.stringify({
      op_type: "byok_rotate",
      reason: "scheduled quarterly rotation",
      params: { key_alias: "primary", target_region: "eu-west-1" },
    }),
  });
  const text = await resp.text();
  console.log(`status=${resp.status} body=${text}`);
  console.log("NOTE: op is PENDING until a second admin approves it.");
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
