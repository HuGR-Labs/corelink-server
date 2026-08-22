// quickstart_list — List all PATs via GET /v1/pats.
//
// Customer concept: server-side authoritative credential inventory.
// Plaintext tokens are never returned (only metadata + last-4 prefix).
//
// Run: CORELINK_PAT=$PAT tsx examples/quickstart_list.ts

const api = process.env.CORELINK_API_URL ?? "https://corelink-api.humangr.com";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}

try {
  const resp = await fetch(`${api}/v1/pats`, {
    method: "GET",
    headers: { Authorization: `Bearer ${pat}`, Accept: "application/json" },
  });
  if (!resp.ok) {
    const body = await resp.text();
    console.error(`server returned ${resp.status}: ${body}`);
    process.exit(1);
  }
  const payload = (await resp.json()) as { items?: unknown[] };
  const count = Array.isArray(payload.items) ? payload.items.length : 0;
  console.log(`status=${resp.status} pat_count=${count}`);
  console.log(JSON.stringify(payload, null, 2));
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
