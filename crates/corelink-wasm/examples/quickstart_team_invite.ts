// quickstart_team_invite — File a team-invite op via POST /v1/admin/ops.
//
// Customer concept: adding tenant members flows through the admin-ops
// dual-approval gate (CTRL-DUAL-APPROVAL-001). On approval an invite
// email is dispatched via the verified-sender pipeline.
//
// Run: CORELINK_PAT=$ADMIN_PAT INVITEE_EMAIL=alice@example.com \
//        tsx examples/quickstart_team_invite.ts

const api = process.env.CORELINK_API_URL ?? "https://sandbox.corelink.humangr.com";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}
const invitee = process.env.INVITEE_EMAIL ?? "newmember@example.com";
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
      op_type: "team_invite",
      reason: "onboard new engineer",
      params: { email: invitee, role: "developer" },
    }),
  });
  const text = await resp.text();
  console.log(`status=${resp.status} body=${text}`);
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
