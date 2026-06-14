"use client";

/**
 * Error boundary for the authenticated route group (welcome / customer / admin).
 *
 * Defense-in-depth: a thrown Server Component error (e.g. a Clerk `auth()` call
 * that can't resolve its request context on the edge) would otherwise surface as
 * a bare HTTP 500. This boundary degrades it to a branded screen with a recovery
 * path. It does NOT replace fixing the underlying throw — it bounds the blast
 * radius so a single page never white-screens the app.
 */

import { useEffect } from "react";
import Link from "next/link";

export default function AuthenticatedError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}): React.ReactElement {
  useEffect(() => {
    // Surfaced to the browser console + Sentry (wired in next.config.ts).
    console.error("authenticated route error", error);
  }, [error]);

  return (
    <main className="mx-auto max-w-md p-8" role="alert">
      <h1 className="text-xl font-semibold">Something went wrong</h1>
      <p className="mt-4 text-sm text-gray-600">
        We couldn&apos;t load this page. This is usually a transient
        authentication hiccup — try again, or sign in.
      </p>
      <div className="mt-6 flex gap-3">
        <button
          type="button"
          onClick={reset}
          className="rounded bg-gray-900 px-4 py-2 text-sm font-medium text-white"
        >
          Try again
        </button>
        <Link
          href="/sign-in"
          className="rounded border border-gray-300 px-4 py-2 text-sm font-medium"
        >
          Go to sign in
        </Link>
      </div>
      {error.digest ? (
        <p className="mt-4 text-xs text-gray-400">Reference: {error.digest}</p>
      ) : null}
    </main>
  );
}
