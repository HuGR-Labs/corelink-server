/**
 * Production-surface E2E — get-corelink-worker install script.
 *
 * The worker (apps/get-corelink-worker) serves the canonical CLI install
 * one-liner at https://get.corelink.io (and the legacy alias
 * https://corelink-get.humangr.com — both routes are configured in
 * wrangler.toml [env.prod.routes]).
 *
 * Contract invariants asserted here mirror the unit tests in
 * apps/get-corelink-worker/tests/install.test.ts but run against the LIVE
 * Worker so DNS / SSL / Worker route table regressions are caught.
 */

import { test, expect } from "@playwright/test";

const INSTALL_URL = process.env["E2E_INSTALL_URL"] ?? "https://get.corelink.io";

test.describe("get-corelink-worker install script (prod surface)", () => {
  test("serves install script with POSIX shebang + corelink-cli + --token marker", async ({
    request,
  }) => {
    // Query-string is ignored by the worker route (token is parsed at script
    // RUNTIME from sh args, not from the URL); we still pass `?token=test` to
    // verify the worker doesn't choke on or reflect the query.
    const res = await request.get(`${INSTALL_URL}/?token=test`);

    expect(res.status()).toBe(200);

    const contentType = res.headers()["content-type"] ?? "";
    expect(contentType).toMatch(/x-shellscript|text\/plain/);

    // Defense-in-depth headers must be present (catches a Worker config
    // regression that strips them).
    const cacheControl = res.headers()["cache-control"] ?? "";
    expect(cacheControl).toMatch(/no-store/);
    expect(res.headers()["x-content-type-options"]).toBe("nosniff");

    const body = await res.text();
    expect(body.startsWith("#!/bin/sh"), "must start with POSIX sh shebang").toBe(true);
    expect(body).toContain("corelink-cli");
    expect(body).toContain("--token=");
    // `set -eu` is the first executable line invariant.
    expect(body).toMatch(/^#!\/bin\/sh\s*\nset -eu\b/);
  });

  test("/healthz returns ok", async ({ request }) => {
    const res = await request.get(`${INSTALL_URL}/healthz`);
    expect(res.status()).toBe(200);
    const body = await res.text();
    expect(body.trim()).toBe("ok");
  });

  test("unknown path returns 404 (not 500)", async ({ request }) => {
    const res = await request.get(`${INSTALL_URL}/this-path-does-not-exist-x9k2`);
    expect(res.status()).toBe(404);
  });
});
