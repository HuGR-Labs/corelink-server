// quickstart_dsr_submit — Submit a DSR via POST /v1/privacy/dsr/{action}.
//
// Customer concept: GDPR/CCPA data-subject requests. Returns a
// request_id; poll GET /v1/privacy/dsr/{request_id}/status. Erasure
// produces a signed attestation (CTRL-ERASURE-ATTEST-001).
//
// Run: CORELINK_PAT=$PAT DSR_ACTION=export tsx examples/quickstart_dsr_submit.ts

const api = process.env.CORELINK_API_URL ?? "https://corelink-api.humangr.com";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}
const action = process.env.DSR_ACTION ?? "export";
const idem = `idem-${crypto.randomUUID()}`;

try {
  const resp = await fetch(`${api}/v1/privacy/dsr/${action}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${pat}`,
      "Content-Type": "application/json",
      "Idempotency-Key": idem,
    },
    body: JSON.stringify({
      subject_email_hash: "0".repeat(64),
      verification_token: "demo-verification-token",
      scope: ["profile", "audit_events"],
    }),
  });
  const text = await resp.text();
  console.log(`action=${action} status=${resp.status} body=${text}`);
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
