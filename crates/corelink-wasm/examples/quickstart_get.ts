// quickstart_get — Fetch the authenticated principal via GET /v1/users/me.
//
// Customer concept: canonical "who am I" probe. Verify PAT validity,
// scopes, and tenant binding before issuing follow-up calls.
//
// Run: CORELINK_PAT=$PAT tsx examples/quickstart_get.ts

const api = process.env.CORELINK_API_URL ?? "https://sandbox.corelink.dev";
const pat = process.env.CORELINK_PAT;
if (!pat) {
  console.error("error: CORELINK_PAT env var is required");
  process.exit(2);
}

try {
  const resp = await fetch(`${api}/v1/users/me`, {
    method: "GET",
    headers: { Authorization: `Bearer ${pat}`, Accept: "application/json" },
  });
  if (!resp.ok) {
    const body = await resp.text();
    console.error(`server returned ${resp.status}: ${body}`);
    process.exit(1);
  }
  const profile: unknown = await resp.json();
  console.log(`status=${resp.status}`);
  console.log(JSON.stringify(profile, null, 2));
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
