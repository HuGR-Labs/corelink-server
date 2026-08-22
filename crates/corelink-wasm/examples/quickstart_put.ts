// quickstart_put — Issue a new PAT via POST /v1/pats.
//
// Customer concept: PATs are the canonical credential. The plaintext
// token is shown-once (CTRL-CRED-001) — store it immediately.
//
// Run: CORELINK_PAT=$BOOTSTRAP_PAT tsx examples/quickstart_put.ts

const api = process.env.CORELINK_API_URL ?? "https://corelink-api.humangr.com";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}

const idem = `idem-${crypto.randomUUID()}`;
try {
  const resp = await fetch(`${api}/v1/pats`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${pat}`,
      "Content-Type": "application/json",
      "Idempotency-Key": idem,
    },
    body: JSON.stringify({ label: "ci-cache-rw", scopes: ["cache:r", "cache:w"], ttl_hours: 24 }),
  });
  const text = await resp.text();
  console.log(`status=${resp.status}`);
  console.log(`response=${text}`);
  console.log("NOTE: plaintext token shown once — store it now.");
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
