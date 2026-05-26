// Explicit not-found page required for @cloudflare/next-on-pages Edge Runtime.
// The built-in Next.js /_not-found is not picked up by next-on-pages without
// an explicit export const runtime declaration.
export const runtime = "edge";

export default function NotFound() {
  return (
    <html>
      <body>
        <h1>404 — Page not found</h1>
      </body>
    </html>
  );
}
