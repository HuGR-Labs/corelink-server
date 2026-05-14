// WI-S14-001 — DO region Worker stub.
// This stub is deployed by Terraform to establish the Worker name + bindings.
// The production Worker script is deployed via the Worker deploy pipeline
// (cosign-signed; RB-FM-206 §3). Terraform manages bindings only.
//
// DO jurisdiction: configured via wrangler.toml `[durable_objects]` or
// Cloudflare Dashboard. verify_do_jurisdiction.sh validates post-deploy.

export default {
  async fetch(request, env, ctx) {
    return new Response(
      JSON.stringify({
        error: "region_stub",
        message: "Production Worker not yet deployed to this region. See deploy pipeline.",
        region: env.REGION_NAME || "unknown",
      }),
      {
        status: 503,
        headers: { "Content-Type": "application/json" },
      }
    );
  },
};
