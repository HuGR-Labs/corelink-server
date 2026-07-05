/**
 * GitHub App install callback — the identity-gated install→map provisioning
 * (Option B: the signup-worker owns the install flow end-to-end).
 *
 * Flow: a Clerk-authed tenant clicks "Install" (admin-ui) → CoreLink mints a
 * signed `state = tenant_id` ([`./github_install_state`]) → GitHub App install →
 * GitHub redirects here (the App's `setup_url`) with `installation_id` + `state`.
 * This handler:
 *   1. VERIFIES the signed state → the authenticated `tenant_id` (403 if bad —
 *      NEVER binds a tenant off the raw installation id alone; that is the
 *      lazy-provision DP3 forbids).
 *   2. Mints a short-lived App JWT (RS256) and exchanges it for an installation
 *      access token, then lists the installation's repositories.
 *   3. Persists `tenant_gh_installation_map` + `runner_repo_allowlist` via the
 *      shared idempotent write ([`./github_provision.writeInstallationProvision`]).
 *   4. Redirects the browser back to the admin-ui with a result flag.
 *
 * Inert (503) until `GITHUB_APP_ID` + `GITHUB_APP_PRIVATE_KEY` (PKCS#8 PEM) +
 * `INSTALL_STATE_SIGNING_KEY` are bound.
 */

import { writeInstallationProvision } from "./github_provision.js";
import { verifyInstallState } from "./github_install_state.js";

export interface InstallCallbackEnv {
  /** Numeric GitHub App id (`iss` of the App JWT). */
  GITHUB_APP_ID?: string;
  /** App private key, **PKCS#8** PEM (`-----BEGIN PRIVATE KEY-----`). */
  GITHUB_APP_PRIVATE_KEY?: string;
  /** HMAC key for the signed install `state` (shared with the admin-ui mint). */
  INSTALL_STATE_SIGNING_KEY?: string;
  /** D1 binding holding the installation map + repo allowlist (0084/0085). */
  CONFIG_DB?: D1Database;
  /** admin-ui base to redirect back to after provisioning (optional). */
  ADMIN_UI_PUBLIC_URL?: string;
}

const GH_API = "https://api.github.com";

/** base64url (no padding) of raw bytes. */
function b64url(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** base64url of a JSON value's UTF-8 encoding. */
function b64urlJson(value: unknown): string {
  return b64url(new TextEncoder().encode(JSON.stringify(value)));
}

/** Decode a PKCS#8 PEM into its DER bytes for WebCrypto `importKey`. */
function pkcs8DerFromPem(pem: string): Uint8Array {
  const body = pem
    .replace(/-----BEGIN [^-]+-----/g, "")
    .replace(/-----END [^-]+-----/g, "")
    .replace(/\s+/g, "");
  const bin = atob(body);
  const der = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) der[i] = bin.charCodeAt(i);
  return der;
}

/**
 * Mint a short-lived RS256 App JWT (`iss`=app id, ~9-minute window with a 60s
 * backdated `iat` to tolerate clock skew — GitHub caps the App JWT at 10m).
 */
export async function mintAppJwt(
  appId: string,
  privateKeyPem: string,
  nowMs: number,
): Promise<string> {
  const key = await crypto.subtle.importKey(
    "pkcs8",
    pkcs8DerFromPem(privateKeyPem),
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const nowS = Math.floor(nowMs / 1000);
  const header = { alg: "RS256", typ: "JWT" };
  const payload = { iat: nowS - 60, exp: nowS + 9 * 60, iss: appId };
  const signingInput = `${b64urlJson(header)}.${b64urlJson(payload)}`;
  const sig = new Uint8Array(
    await crypto.subtle.sign(
      "RSASSA-PKCS1-v1_5",
      key,
      new TextEncoder().encode(signingInput),
    ),
  );
  return `${signingInput}.${b64url(sig)}`;
}

const GH_HEADERS = {
  accept: "application/vnd.github+json",
  "user-agent": "corelink-signup-worker",
  "x-github-api-version": "2022-11-28",
} as const;

/** Exchange the App JWT for an installation access token. */
async function installationToken(jwt: string, installationId: string): Promise<string | null> {
  const resp = await fetch(
    `${GH_API}/app/installations/${encodeURIComponent(installationId)}/access_tokens`,
    { method: "POST", headers: { ...GH_HEADERS, authorization: `Bearer ${jwt}` } },
  );
  if (!resp.ok) return null;
  const body = (await resp.json().catch(() => null)) as { token?: string } | null;
  return typeof body?.token === "string" ? body.token : null;
}

/** List the installation's repositories as `owner/repo` full names (paginated). */
async function installationRepos(token: string): Promise<string[]> {
  const out: string[] = [];
  for (let page = 1; page <= 10; page++) {
    const resp = await fetch(`${GH_API}/installation/repositories?per_page=100&page=${page}`, {
      headers: { ...GH_HEADERS, authorization: `Bearer ${token}` },
    });
    if (!resp.ok) break;
    const body = (await resp.json().catch(() => null)) as {
      repositories?: Array<{ full_name?: string }>;
    } | null;
    const repos = body?.repositories ?? [];
    for (const r of repos) {
      if (typeof r.full_name === "string" && r.full_name.length > 0) out.push(r.full_name);
    }
    if (repos.length < 100) break; // last page
  }
  return out;
}

/** Redirect the browser back to the admin-ui (or a plain 200) with a result. */
function done(env: InstallCallbackEnv, ok: boolean, detail: string): Response {
  const base = env.ADMIN_UI_PUBLIC_URL?.replace(/\/+$/, "");
  if (base) {
    const q = ok ? "runner_install=ok" : `runner_install=error&reason=${encodeURIComponent(detail)}`;
    return new Response(null, { status: 302, headers: { location: `${base}/settings/runners?${q}` } });
  }
  return new Response(ok ? "runner install provisioned" : `runner install failed: ${detail}`, {
    status: ok ? 200 : 502,
    headers: { "cache-control": "no-store" },
  });
}

/**
 * `GET /install/github/callback?installation_id=…&state=…&setup_action=install`
 */
export async function handleInstallGithubCallback(
  request: Request,
  env: InstallCallbackEnv,
): Promise<Response> {
  const appId = env.GITHUB_APP_ID;
  const privateKey = env.GITHUB_APP_PRIVATE_KEY;
  const stateKey = env.INSTALL_STATE_SIGNING_KEY;
  const db = env.CONFIG_DB;
  if (!appId || !privateKey || !stateKey || !db) {
    // Inert until fully configured — never partial-provision.
    return new Response("github app install flow not configured", { status: 503 });
  }

  const url = new URL(request.url);
  const installationId = url.searchParams.get("installation_id");
  const state = url.searchParams.get("state");
  if (!installationId || !state) {
    return done(env, false, "missing installation_id or state");
  }

  // 1. IDENTITY: the signed state is the ONLY authenticated tenant binding.
  const verified = await verifyInstallState(state, stateKey, Date.now());
  if (!verified) {
    return new Response("invalid or expired install state", { status: 403 });
  }
  const tenantId = verified.tenantId;

  // 2. Mint the App JWT → installation token → repos.
  let repos: string[];
  try {
    const jwt = await mintAppJwt(appId, privateKey, Date.now());
    const token = await installationToken(jwt, installationId);
    if (!token) {
      return done(env, false, "installation token exchange failed");
    }
    repos = await installationRepos(token);
  } catch (e) {
    return done(env, false, `github api error: ${(e as Error).message}`);
  }

  // 3. Persist the map + allowlist (idempotent, shared write path).
  try {
    await writeInstallationProvision(db, {
      installationId,
      tenantId,
      repos,
      nowMs: Date.now(),
    });
  } catch {
    return done(env, false, "provision persist failed");
  }

  return done(env, true, `${repos.length} repos`);
}
