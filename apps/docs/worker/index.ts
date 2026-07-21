/**
 * corelink-docs edge Worker.
 *
 * Serves the Docusaurus static build (baseUrl `/corelink/docs/`) at the
 * canonical product path `https://humangr.com/corelink/docs`.
 *
 * Why a shim is needed: Docusaurus's `baseUrl` rewrites in-page URLs but NOT
 * the on-disk output layout — files still land at `build/<path>` (e.g.
 * `build/how-to/sdk-cli/…​.html`, `build/assets/js/…`). Cloudflare Static
 * Assets serves `build/` at the ROUTE root, so an incoming request for
 * `/corelink/docs/assets/js/x` would look for `build/corelink/docs/assets/js/x`
 * and miss. We strip the `/corelink/docs` mount prefix, then delegate to the
 * ASSETS binding — which applies `_headers` (CSP), `_redirects`, html handling
 * and the 404 page from the build.
 *
 * The zone route (`humangr.com/corelink/docs/*`, see wrangler.toml) is more
 * specific than the admin-ui app's `/corelink/*`, so only docs traffic reaches
 * this Worker; the app is untouched.
 */

const CANONICAL_HOST = "humangr.com";
const MOUNT = "/corelink/docs";

interface Env {
  ASSETS: { fetch(request: Request): Promise<Response> };
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    // Legacy-host 301: the retired `corelink-docs.humangr.com` subdomain (and
    // any other non-canonical host that is ever pointed here) redirects to the
    // canonical product path, preserving the deep path. Replaces the old Pages
    // `functions/_middleware.ts` pages.dev guard (Workers have no auto
    // pages.dev subdomain). Only fires if such a host is actually routed here.
    if (url.hostname !== CANONICAL_HOST) {
      const dest = new URL(url.toString());
      dest.hostname = CANONICAL_HOST;
      dest.pathname = url.pathname === "/" ? `${MOUNT}/` : `${MOUNT}${url.pathname}`;
      return Response.redirect(dest.toString(), 301);
    }

    // Canonicalize the bare mount to a trailing slash (baseUrl is a directory).
    if (url.pathname === MOUNT) {
      url.pathname = `${MOUNT}/`;
      return Response.redirect(url.toString(), 308);
    }

    // Strip the mount prefix so the lookup matches the `build/` layout. Keeps
    // the leading slash: `/corelink/docs/how-to/x` -> `/how-to/x`.
    if (url.pathname.startsWith(`${MOUNT}/`)) {
      url.pathname = url.pathname.slice(MOUNT.length);
    }

    return env.ASSETS.fetch(new Request(url.toString(), request));
  },
};
