// Explicit not-found page required for @cloudflare/next-on-pages.
//
// Two route segment configs combine to keep this off the synthetic
// `/_not-found` Node.js function that next-on-pages otherwise rejects:
//
//   1. `runtime = "edge"` — opt into the Edge runtime (overrides the root
//      layout's edge default explicitly; defensive against future changes).
//   2. `dynamic = "force-static"` — overrides the root layout's
//      `force-dynamic`. With force-static, Next.js renders this page at
//      build time and skips emitting a Node.js function, so the
//      `/_not-found` function disappears entirely from the Vercel output
//      and the next-on-pages edge-runtime gate passes.
//
// The body must remain pure JSX with no client hooks, no `headers()`, no
// dynamic imports — anything that forces SSR would defeat force-static.
export const runtime = "edge";
export const dynamic = "force-static";

export default function NotFound() {
  return (
    <html>
      <body>
        <h1>404 — Page not found</h1>
      </body>
    </html>
  );
}
