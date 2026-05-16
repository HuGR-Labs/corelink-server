// quickstart_portal_session — Bootstrap an enterprise-portal session
// via POST /v1/enterprise/inquire.
//
// Customer concept: enterprise prospects request contracted-tier
// provisioning (DPA + BAA + custom SLA). Returns a portal URL +
// short-lived session token.
//
// Run: CORELINK_PAT=$PAT tsx examples/quickstart_portal_session.ts

const api = process.env.CORELINK_API_URL ?? "https://sandbox.corelink.humangr.com";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}
const idem = `idem-${crypto.randomUUID()}`;

try {
  const resp = await fetch(`${api}/v1/enterprise/inquire`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${pat}`,
      "Content-Type": "application/json",
      "Idempotency-Key": idem,
    },
    body: JSON.stringify({
      company: "Acme Corp",
      contact_email_hash: "0".repeat(64),
      expected_seats: 250,
      use_case: "monorepo build cache for 80 engineers",
    }),
  });
  const text = await resp.text();
  console.log(`status=${resp.status} body=${text}`);
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
