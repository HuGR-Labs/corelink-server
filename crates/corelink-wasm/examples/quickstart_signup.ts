// quickstart_signup — Provision a new CoreLink tenant via POST /v1/signup.
//
// Customer concept: a tenant is the top-level isolation boundary in
// CoreLink. Signup is atomic: tenant row + DPA acceptance + first PAT
// are committed together (INV-ONBOARD-ATOMIC-PROVISIONING).
//
// Run: CORELINK_PAT=$PAT tsx examples/quickstart_signup.ts

const api = process.env.CORELINK_API_URL ?? "https://sandbox.corelink.humangr.com";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}

const idem = `idem-${crypto.randomUUID()}`;
const body = {
  clerk_event_id: "evt_demo_quickstart",
  correlation_id: crypto.randomUUID(),
  email_hash: "0".repeat(64),
  idempotency_key: idem,
  locale: "en-US",
};

try {
  const resp = await fetch(`${api}/v1/signup`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${pat}`,
      "Content-Type": "application/json",
      "Idempotency-Key": idem,
    },
    body: JSON.stringify(body),
  });
  const text = await resp.text();
  console.log(`status=${resp.status} body=${text}`);
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
