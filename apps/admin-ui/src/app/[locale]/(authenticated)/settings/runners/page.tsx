/**
 * /[locale]/settings/runners — CI-runner connection surface.
 *
 * Home of the "Install GitHub App" button (the runner-consumption entry point)
 * and the landing target the signup-worker install callback redirects back to
 * with `?runner_install=ok` / `?runner_install=error&reason=…`. The button is a
 * plain link to `/api/install/github`, which resolves the tenant server-side,
 * mints the signed install state, and redirects into GitHub's App-install flow.
 */

import type { ReactElement } from "react";

import { withAppBasePath } from "@/lib/route-matcher";

interface PageProps {
  searchParams: Promise<{ runner_install?: string; reason?: string }>;
}

export default async function RunnerSettingsPage(props: PageProps): Promise<ReactElement> {
  const { runner_install: status, reason } = await props.searchParams;

  return (
    <main aria-labelledby="runner-settings-heading">
      <h1 id="runner-settings-heading">CI Runners</h1>

      {status === "ok" && (
        <p role="status">
          ✓ GitHub App installed — your repositories are now provisioned for CoreLink
          runners.
        </p>
      )}
      {status === "error" && (
        <p role="alert">
          Install failed: {reason ?? "unknown error"}. Please try again.
        </p>
      )}

      <p>
        Connect your GitHub organization so CoreLink can dispatch CI jobs to your
        runners against the shared cache. You&apos;ll be redirected to GitHub to choose
        which repositories to grant access to.
      </p>

      {/*
        Plain <a>, NOT next/link: this targets the `/api/install/github` API
        route, which mints the signed state server-side and issues a 302 to
        GitHub (cross-origin). next/link does client-side routing and cannot
        follow a cross-origin server redirect, so a full navigation is required.
      */}
      {/*
        Raw <a> is NOT auto-basePath'd by Next, so prefix explicitly with
        `/corelink` — else the click lands on the apex marketing site, not the
        install API route (the reported self-serve-UX break).
      */}
      {/* eslint-disable-next-line @next/next/no-html-link-for-pages */}
      <a href={withAppBasePath("/api/install/github")} role="button">
        Install GitHub App
      </a>
    </main>
  );
}
