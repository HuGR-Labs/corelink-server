// functions/_middleware.ts — lock down *.pages.dev traffic (E1, 2026-05-28).
//
// admin-ui = BLOCK; docs / hugr-site = 301 redirect to canonical custom domain.
// Reason: api.cloudflare.com Pages PATCH schema has no field to disable the
// pages.dev subdomain; this middleware is the documented redirect-by-Host path
// (see specs/_audits/2026-05-28-security-exposure-review.md §E1 remediation
// runbook).
//
// admin-ui CANNOT redirect because the same Pages Functions also bind live
// Stripe / Clerk / Resend secrets at request time — any executed handler on a
// pages.dev request is a secret-exposure vector. We therefore BLOCK with a
// bare 404 before any downstream handler runs.
//
// The redirect branch is retained verbatim (with the future canonical host)
// for symmetry with apps/docs and apps/hugr-site, but never fires because
// MODE === "block".

// Minimal local type so we don't pull in @cloudflare/workers-types just for
// this file (the runtime supplies the real PagesFunction signature).
type PagesFunction = (ctx: {
  request: Request;
  next: () => Promise<Response>;
}) => Promise<Response> | Response;

export const onRequest: PagesFunction = async (ctx) => {
  const url = new URL(ctx.request.url);
  if (url.hostname.endsWith(".pages.dev")) {
    // CONFIG (per project): admin-ui = "block"; docs + hugr-site = "redirect".
    const MODE: "block" | "redirect" = "block";
    const CANONICAL_HOST = "app.corelink.humangr.com";
    if (MODE === "block") {
      return new Response("Not Found", { status: 404 });
    }
    url.hostname = CANONICAL_HOST;
    return Response.redirect(url.toString(), 301);
  }
  return ctx.next();
};
