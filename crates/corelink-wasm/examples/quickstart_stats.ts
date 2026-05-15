// quickstart_stats — Probe service health/stats via GET /api/health.
//
// Customer concept: liveness + readiness probe. Returns version,
// region, and per-dependency status. Anonymous endpoint.
//
// Run: tsx examples/quickstart_stats.ts

const api = process.env.CORELINK_API_URL ?? "https://sandbox.corelink.dev";

try {
  const resp = await fetch(`${api}/api/health`, {
    method: "GET",
    headers: { Accept: "application/json" },
  });
  if (!resp.ok) {
    const body = await resp.text();
    console.error(`server returned ${resp.status}: ${body}`);
    process.exit(1);
  }
  const payload: unknown = await resp.json();
  console.log(`status=${resp.status}`);
  console.log(JSON.stringify(payload, null, 2));
} catch (err) {
  console.error(`network_error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
