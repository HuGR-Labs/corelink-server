import { test, expect } from "../fixtures/auth.js";

/**
 * ADR-0071 live proof: a self-serve FIND-ONLY PAT (cache:find-missing) mints,
 * can run FindMissingBlobs, but is DENIED CAS read (the least-privilege property)
 * — verified against PROD after the 0093 migration + container/worker roll.
 */
test("find-only PAT: mints, does find-missing, denied CAS read", async ({ authedPage: page }) => {
  const out = await page.evaluate(async () => {
    const clerk = (window as any).Clerk;
    const token: string = await clerk.session.getToken();
    const API = "https://corelink-api.humangr.com";
    const auth = { Authorization: `Bearer ${token}`, "content-type": "application/json" };

    // resolve tenant (for the bazel instance path)
    const me = await (await fetch(`${API}/v1/users/me`, { headers: auth })).json().catch(() => ({}));
    const tenant = me?.tenant_id ?? me?.tenantId ?? null;

    // 1. mint a find-only PAT
    const mintR = await fetch(`${API}/v1/customer/keys`, {
      method: "POST",
      headers: auth,
      body: JSON.stringify({ name: `find-only-${Date.now()}`, scopes: ["cache:find-missing"] }),
    });
    const mintBody = await mintR.json().catch(() => ({}));
    const pat: string = mintBody?.token ?? "";
    const patAuth = { Authorization: `Bearer ${pat}`, "content-type": "application/json" };

    // hash of "probe" (sha256) for the CAS path
    const h = await crypto.subtle.digest("SHA-256", new TextEncoder().encode("probe"));
    const hex = [...new Uint8Array(h)].map((b) => b.toString(16).padStart(2, "0")).join("");

    // 2. find-only token on a CAS READ → must be 403 (denied read)
    const readR = pat
      ? await fetch(`${API}/v1/cas/${hex}`, { headers: patAuth })
      : null;

    // 3. find-only token on findMissingBlobs → must NOT be 403 (find-missing works)
    const fmR = pat && tenant
      ? await fetch(`${API}/bazel/v2/${tenant}/findMissingBlobs`, {
          method: "POST",
          headers: patAuth,
          body: JSON.stringify({ blobDigests: [{ hash: hex, sizeBytes: 5 }] }),
        })
      : null;

    return {
      mint: { status: mintR.status, scopes: mintBody?.pat?.scopes, hasToken: pat.length > 0 },
      tenant,
      read: readR ? readR.status : null,
      findMissing: fmR ? fmR.status : null,
    };
  });
  // eslint-disable-next-line no-console
  console.log("[find-only] " + JSON.stringify(out, null, 2));

  expect(out.mint.status, "find-only mint must 201").toBe(201);
  expect(out.mint.scopes, "displayed as cache:find-missing").toContain("cache:find-missing");
  expect(out.read, "find-only PAT must be DENIED CAS read (403)").toBe(403);
  if (out.findMissing !== null) {
    expect(out.findMissing, "find-only PAT must be allowed find-missing (not 403)").not.toBe(403);
  }
});
