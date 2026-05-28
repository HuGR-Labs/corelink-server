// functions/_middleware.ts — lock down *.pages.dev traffic (E1, 2026-05-28).
//
// Per-project matrix:
//   - admin-ui:  BLOCK (404)   — has live Stripe/Clerk/Resend secrets.
//   - docs:      REDIRECT 301  — public docs, preserve customer link redirects
//                                through the canonical host.
//   - hugr-site: REDIRECT 301  — public marketing.
//
// Reason: api.cloudflare.com Pages PATCH schema has no field to disable the
// auto-assigned pages.dev subdomain; this middleware is the documented
// redirect-by-Host path (see specs/_audits/2026-05-28-security-exposure-review.md
// §E1 remediation runbook).

// Minimal local type so we don't pull in @cloudflare/workers-types just for
// this file (the runtime supplies the real PagesFunction signature).
type PagesFunction = (ctx: {
  request: Request;
  next: () => Promise<Response>;
}) => Promise<Response> | Response;

const CANONICAL_HOST = "docs.corelink.humangr.com";

export const onRequest: PagesFunction = async (ctx) => {
  const url = new URL(ctx.request.url);
  if (url.hostname.endsWith(".pages.dev")) {
    url.hostname = CANONICAL_HOST;
    return Response.redirect(url.toString(), 301);
  }
  return ctx.next();
};
