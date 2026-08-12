#!/usr/bin/env node
// @ts-check
/**
 * clerk-session-minter.mjs — SOTA FAPI-direct Clerk-session minter for the
 * CoreLink e2e suite. NO browser, NO `window.Clerk`, NO app page.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHY THIS EXISTS
 * ─────────────────────────────────────────────────────────────────────────────
 * Several e2e journeys are GATED on a real Clerk **session JWT** (not a PAT):
 *
 *   - billing.rs            → CORELINK_E2E_CLERK_SESSION   (authed checkout / tier-select)
 *   - dsr.rs (request->gone)→ CORELINK_E2E_DSR_SESSION     (customer self-erasure request)
 *   - dsr.rs (full-flow)    → CORELINK_E2E_DSR_TEST_SESSION (self-driving erasure)
 *
 * These surfaces are authenticated by `verifyClerkSessionAndResolveTenant`
 * (worker/src/lib/clerk_auth.ts), which requires a token that:
 *   1. carries `Authorization: Bearer <jwt>`  (journeys call `bearer(&session)`),
 *   2. has `azp` ∈ CLERK_AZP_ALLOWLIST  →  "https://corelink-app.humangr.com".
 *      Clerk's FAPI derives `azp` from the **Origin header** of the request that
 *      mints the token — so we send `Origin: <CORELINK_APP_URL>` on every FAPI
 *      call (this is what stamps the exact azp; verified against clerk_auth.ts).
 *   3. has `iss` == CLERK_ISSUER_URL  (the exact-pin; the live FAPI issuer),
 *   4. has a `sub` (the Clerk user id) that maps to a CoreLink tenant row
 *      (tenant.clerk_user_id, written by the signup-worker on user.created).
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * THE ROBUST FLOW (FAPI-direct — no headless browser)
 * ─────────────────────────────────────────────────────────────────────────────
 * The OLD approach navigated headless Chromium to the prod app and waited for
 * `window.Clerk` — which FAILS, because the prod SPA does not expose ClerkJS to
 * a headless context ("window.Clerk never loaded"). The robust path drives
 * Clerk's Frontend API (FAPI) directly with the sanctioned backend primitives:
 *
 *   1. Backend API: POST /v1/users               → create throwaway user (id).
 *   2. Backend API: POST /v1/sign_in_tokens      → a one-time sign-in **ticket**
 *      ({ user_id }). This is Clerk's "sign a user in without a password" path.
 *   3. Backend API: POST /v1/testing_tokens      → a Testing Token (the
 *      documented bot-detection bypass, attached to FAPI as a query param).
 *   4. FAPI: POST https://<FAPI>/v1/client/sign_ins?__clerk_testing_token=<tt>
 *           &_clerk_js_version=<v>  body: strategy=ticket&ticket=<signInToken>
 *      → consumes the ticket, creates the client (Set-Cookie __client) and a
 *      completed sign_in carrying `created_session_id`.
 *   5. FAPI: POST https://<FAPI>/v1/client/sessions/<sessionId>/tokens?…
 *      (Origin: <CORELINK_APP_URL>) → { jwt } — the ~60s default `__session`
 *      JWT, the exact `Authorization: Bearer <jwt>` shape the worker expects.
 *   6. TENANT-READINESS GATE (GAP 1, default ON; `--no-wait` to skip): the
 *      throwaway user gets a CoreLink tenant ONLY when the signup-worker
 *      `user.created` webhook fires (async, seconds). Until then a session-authed
 *      call 401/403s (no tenant row). We POLL `${API}/v1/users/me` with the
 *      bearer until 200. CRITICAL: the JWT lives ~60s, so we RE-MINT a fresh JWT
 *      (step 5 again, full re-sign-in on session lapse) BEFORE every probe — the
 *      probe always uses a LIVE bearer. We succeed only on 200, so the emitted
 *      session is GUARANTEED tenant-ready; budget exhausted ⇒ FAIL LOUD.
 *   7. DPA ACCEPT (GAP 3, default ON; `--no-dpa` to skip): once the tenant exists,
 *      POST `${API}/v1/onboarding/dpa-accept` with the session bearer so the
 *      tenant has a `dpa_acceptances` row for the current DPA version. Without it
 *      the money-path gate (`tier-select`) 403s `dpa_required` and ~15 journeys
 *      stay GATED. Idempotent; version from `CORELINK_DPA_VERSION`.
 *   8. FRESHNESS (GAP 2): once ready, mint ONE final fresh JWT right before emit
 *      so the consumer gets a full ~60s window.
 *   9. Best-effort resolve the CoreLink tenant id (signup-worker webhook maps
 *      user.created → tenant; may lag — we poll, never write D1).
 *  10. Emit `export CORELINK_E2E_{CLERK,DSR,DSR_TEST}_SESSION=<jwt>` to stdout +
 *      an output file (default /tmp/e2e-clerk-session.sh); print user_id.
 *  11. `--refresh` re-mints a fresh JWT for the same user (60s TTL); it skips the
 *      readiness wait if a prior run already recorded the tenant as provisioned
 *      (sidecar `tenantReady` flag).
 *
 * Cookie handling: FAPI is cookie-stateful (the `__client` cookie ties the
 * sign_in to the session). We use a tiny in-process cookie jar (parse Set-Cookie,
 * resend on subsequent calls) — no browser, no extra dependency.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * FAIL LOUD — never a token we could not actually mint
 * ─────────────────────────────────────────────────────────────────────────────
 * Any non-2xx FAPI/Backend call aborts the run printing the METHOD, the full URL
 * (testing token redacted), the STATUS, and the FULL response body — so the next
 * run shows precisely which call/param is wrong. It NEVER prints a JWT it could
 * not mint, so a caller can never be fooled into running against a bad bearer.
 *
 * HARD RULE: this script only TALKS to Clerk + the (optional) CoreLink API. It
 * never writes D1, never touches other repo files.
 */

import { writeFileSync, readFileSync, existsSync } from "node:fs";
import { randomBytes, createHash } from "node:crypto";

// ─────────────────────────────────────────────────────────────────────────────
// Config — every knob is env-driven with sane defaults for the LIVE instance.
// ─────────────────────────────────────────────────────────────────────────────

/** The CoreLink Clerk **secret key** (Backend API auth). `.env.local` holds it
 *  under either name; accept both. TEST keys are `sk_test_…`, live `sk_live_…`. */
const CLERK_SECRET_KEY =
  process.env.CLERK_SECRET_KEY || process.env.CLERK_LIVE_SECRET_KEY || "";

/** Publishable key (`pk_live_…` / `pk_test_…`). Used to derive the FAPI host and
 *  to bootstrap ClerkJS on the app page if needed. */
const CLERK_PUBLISHABLE_KEY = process.env.CLERK_PUBLISHABLE_KEY || "";

/** The user-facing app origin sign-in happens on. MUST be the host whose origin
 *  becomes the JWT `azp` (clerk_auth.ts CLERK_AZP_ALLOWLIST). Default = the live
 *  corelink-app host. */
const CORELINK_APP_URL = (
  process.env.CORELINK_APP_URL || "https://corelink-app.humangr.com"
).replace(/\/+$/, "");

/** The live Clerk Frontend API (FAPI) host for this instance. Default per the
 *  known live instance; override if the publishable key points elsewhere. We
 *  also try to derive it from the publishable key as a fallback. */
const CLERK_FAPI_HOST =
  process.env.CLERK_FAPI ||
  deriveFapiFromPublishableKey(CLERK_PUBLISHABLE_KEY) ||
  "clerk.corelink-app.humangr.com";

/** Optional CoreLink API endpoint for best-effort tenant resolution (never
 *  required; tenant id is emitted only if it can be resolved). */
const CORELINK_API_ENDPOINT = (
  process.env.CORELINK_API_ENDPOINT ||
  process.env.CORELINK_E2E_ENDPOINT ||
  ""
).replace(/\/+$/, "");

/** Email domain for the throwaway user. Backend-API-created users are email-
 *  verified by default, so deliverability is irrelevant — but the domain must be
 *  a syntactically valid one Clerk accepts. */
const EMAIL_DOMAIN = process.env.CORELINK_E2E_EMAIL_DOMAIN || "corelink-e2e.dev";

const CLERK_API = "https://api.clerk.com/v1";

/** Frontend API (FAPI) base URL for this instance. */
const FAPI_BASE = `https://${CLERK_FAPI_HOST}`;

/** ClerkJS version advertised to FAPI. FAPI requires `_clerk_js_version` on its
 *  endpoints; the exact value is lenient but must be present. Override via env
 *  if a future FAPI contract rejects this. */
const CLERK_JS_VERSION = process.env.CLERK_JS_VERSION || "5.57.0";

/** Where the tenant-readiness probe (GAP 1) calls `/v1/users/me`. Falls back to
 *  the live prod API host when no explicit endpoint is configured. */
const TENANT_PROBE_ENDPOINT =
  CORELINK_API_ENDPOINT || "https://corelink-api.humangr.com";

/** The session-authed readiness endpoint — 200 ⇒ tenant provisioned, 401/403 ⇒
 *  not yet (signup-worker `user.created` webhook lag). */
const TENANT_PROBE_PATH = "/v1/users/me";

/** Total budget to wait for the signup-worker to provision the tenant (~120s). */
const TENANT_WAIT_MS = Number(process.env.CORELINK_E2E_TENANT_WAIT_MS) || 120_000;

/** Delay between readiness probes. */
const TENANT_WAIT_INTERVAL_MS =
  Number(process.env.CORELINK_E2E_TENANT_WAIT_INTERVAL_MS) || 3_000;

// ─────────────────────────────────────────────────────────────────────────────
// CLI args:  node clerk-session-minter.mjs [outfile] [--refresh] [--no-wait]
// ─────────────────────────────────────────────────────────────────────────────
const argv = process.argv.slice(2);
const REFRESH = argv.includes("--refresh");
/** Tenant-readiness gate (GAP 1) is ON by default; `--no-wait` skips it. */
const WAIT_TENANT = !argv.includes("--no-wait");
/** DPA acceptance (GAP 3) is ON by default; `--no-dpa` skips it. Without it the
 * emitted session gates on `dpa_required` for every money-path journey. */
const ACCEPT_DPA = !argv.includes("--no-dpa");
/** The DPA version to accept — MUST equal the deployed container's
 * `CORELINK_DPA_VERSION` or the accept 400s `dpa_version_mismatch`. Same env name
 * the server reads, with an e2e-specific override. Default is the CANONICAL prod
 * value `1.0.0` (docs/internal/secrets-checklist.md #146 + apps/admin-ui/src/
 * content/dpa.en.ts — a mismatch like `v3` 403s every checkout forever). */
const DPA_VERSION =
  process.env.CORELINK_DPA_VERSION ||
  process.env.CORELINK_E2E_DPA_VERSION ||
  "1.0.0";
const OUT_FILE =
  argv.find((a) => !a.startsWith("--")) || "/tmp/e2e-clerk-session.sh";
/** Sidecar that records the throwaway user's creds so --refresh can re-sign-in
 *  the SAME user (and so the caller knows which user_id to DSR-delete). */
const USER_FILE = `${OUT_FILE}.user.json`;

// ─────────────────────────────────────────────────────────────────────────────
// Small helpers
// ─────────────────────────────────────────────────────────────────────────────

/** Fail loud: print to stderr and exit non-zero. */
function die(msg) {
  console.error(`\n[clerk-session-minter] FATAL: ${msg}\n`);
  process.exit(1);
}

function log(msg) {
  console.error(`[clerk-session-minter] ${msg}`);
}

/** @param {number} ms */
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Derive the FAPI host from a publishable key. Clerk publishable keys are
 * `pk_(live|test)_<base64(frontendApiHost + "$")>`. Returns null on any parse
 * failure (caller falls back to the default / env override).
 */
function deriveFapiFromPublishableKey(pk) {
  try {
    if (!pk) return null;
    const b64 = pk.replace(/^pk_(live|test)_/, "");
    const decoded = Buffer.from(b64, "base64").toString("utf8");
    const host = decoded.replace(/\$$/, "").trim();
    return host && host.includes(".") ? host : null;
  } catch {
    return null;
  }
}

/** Backend API fetch with the secret key; throws a precise error on non-2xx. */
async function clerkBackend(path, init = {}) {
  const res = await fetch(`${CLERK_API}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${CLERK_SECRET_KEY}`,
      "Content-Type": "application/json",
      ...(init.headers || {}),
    },
  });
  const text = await res.text();
  let json;
  try {
    json = text ? JSON.parse(text) : {};
  } catch {
    json = { raw: text };
  }
  if (!res.ok) {
    const errs = Array.isArray(json?.errors)
      ? json.errors
          .map((e) => `${e.code ?? "?"}: ${e.message ?? e.long_message ?? ""}`)
          .join("; ")
      : text.slice(0, 400);
    throw new Error(`Clerk Backend API ${path} → ${res.status}: ${errs}`);
  }
  return json;
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 1 — create the throwaway Clerk user (verified email + known password).
// ─────────────────────────────────────────────────────────────────────────────
async function createThrowawayUser() {
  const rand = randomBytes(6).toString("hex");
  const email = `corelink-e2e-${Date.now()}-${rand}@${EMAIL_DOMAIN}`;
  // Strong generated password; skip_password_checks lets us use it regardless of
  // the instance's password policy. We KNOW it (we generated it) → can sign in.
  const password = `E2e!${randomBytes(18).toString("base64url")}`;

  log(`creating throwaway user ${email} …`);
  const user = await clerkBackend("/users", {
    method: "POST",
    body: JSON.stringify({
      email_address: [email],
      password,
      skip_password_checks: true,
      // Mark this user as test-origin so it is trivially identifiable for cleanup.
      private_metadata: { e2e: true, source: "clerk-session-minter" },
    }),
  });
  if (!user?.id) {
    die(`user creation returned no id: ${JSON.stringify(user).slice(0, 300)}`);
  }
  log(`created user_id=${user.id}`);
  return { userId: user.id, email, password };
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 2 — fetch a Testing Token (the documented bot-detection bypass).
// ─────────────────────────────────────────────────────────────────────────────
async function fetchTestingToken() {
  log("fetching a Clerk Testing Token …");
  const tok = await clerkBackend("/testing_tokens", { method: "POST" });
  const token = tok?.token;
  if (!token) {
    die(
      "testing_tokens returned no token. Testing Tokens require a TEST instance " +
        "OR an instance with testing enabled. Against a hardened LIVE instance " +
        "this is the #1 failure mode — see CLERK-SESSION-README.md.",
    );
  }
  log("got testing token (bot-detection bypass armed).");
  return token;
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 2b — Backend API: mint a one-time sign-in token (the "ticket").
// ─────────────────────────────────────────────────────────────────────────────
/**
 * `POST /v1/sign_in_tokens { user_id }` → a one-time ticket that the FAPI
 * `strategy=ticket` flow consumes to complete a sign-in WITHOUT a password.
 * This is Clerk's sanctioned "sign a user in programmatically" primitive.
 *
 * @param {string} userId
 * @returns {Promise<string>} the sign-in token (ticket)
 */
async function createSignInToken(userId) {
  log(`creating a sign-in token (ticket) for user_id=${userId} …`);
  const res = await clerkBackend("/sign_in_tokens", {
    method: "POST",
    body: JSON.stringify({ user_id: userId }),
  });
  const token = res?.token;
  if (!token) {
    die(
      "sign_in_tokens returned no token: " +
        JSON.stringify(res).slice(0, 300),
    );
  }
  log("got sign-in token (ticket).");
  return token;
}

// ─────────────────────────────────────────────────────────────────────────────
// FAPI helpers — a tiny cookie-aware client (no browser, no extra dependency).
// ─────────────────────────────────────────────────────────────────────────────

/**
 * A minimal cookie jar: stores `Set-Cookie` name=value pairs and replays them on
 * subsequent requests. FAPI is cookie-stateful (`__client` ties the sign_in to
 * the client + session), so the same jar MUST span the whole FAPI exchange.
 */
function makeCookieJar() {
  /** @type {Map<string,string>} */
  const jar = new Map();
  return {
    /** @param {Response} res */
    store(res) {
      /** @type {string[]} */
      let setCookies = [];
      // Node 18.14+/undici: getSetCookie() returns the array un-collapsed.
      if (typeof res.headers.getSetCookie === "function") {
        setCookies = res.headers.getSetCookie();
      } else {
        const sc = res.headers.get("set-cookie");
        if (sc) setCookies = [sc];
      }
      for (const c of setCookies) {
        const pair = c.split(";")[0];
        const eq = pair.indexOf("=");
        if (eq > 0) jar.set(pair.slice(0, eq).trim(), pair.slice(eq + 1).trim());
      }
    },
    header() {
      if (jar.size === 0) return undefined;
      return [...jar.entries()].map(([k, v]) => `${k}=${v}`).join("; ");
    },
  };
}

/** Redact the testing token in a URL for safe logging/errors. */
function redactUrl(url) {
  return String(url).replace(/(__clerk_testing_token=)[^&]+/, "$1<redacted>");
}

/**
 * Cookie-aware FAPI request. Attaches `_clerk_js_version` + the testing token,
 * sets `Origin`/`Referer` to the app origin (this is what stamps the JWT `azp`),
 * carries the cookie jar, and FAILS LOUD with the method, full (redacted) URL,
 * status, and FULL response body on any non-2xx.
 *
 * @param {ReturnType<typeof makeCookieJar>} jar
 * @param {string} path
 * @param {{method?:string, form?:Record<string,string>, testingToken:string}} opts
 * @returns {Promise<any>} parsed JSON body
 */
async function fapiFetch(jar, path, { method = "GET", form, testingToken }) {
  const url = new URL(`${FAPI_BASE}${path}`);
  url.searchParams.set("_clerk_js_version", CLERK_JS_VERSION);
  if (testingToken) url.searchParams.set("__clerk_testing_token", testingToken);

  /** @type {Record<string,string>} */
  const headers = {
    // azp is derived by FAPI from the Origin header → pin it to the app origin
    // so the minted JWT passes clerk_auth.ts CLERK_AZP_ALLOWLIST.
    Origin: CORELINK_APP_URL,
    Referer: `${CORELINK_APP_URL}/`,
    Accept: "application/json",
  };
  const cookie = jar.header();
  if (cookie) headers.Cookie = cookie;

  let body;
  if (form) {
    headers["Content-Type"] = "application/x-www-form-urlencoded";
    body = new URLSearchParams(form).toString();
  }

  const res = await fetch(url, { method, headers, body });
  jar.store(res);

  const text = await res.text();
  let json;
  try {
    json = text ? JSON.parse(text) : {};
  } catch {
    json = { raw: text };
  }
  if (!res.ok) {
    throw new Error(
      `Clerk FAPI ${method} ${redactUrl(url)} → ${res.status}\n` +
        `  response body: ${text.slice(0, 1500)}`,
    );
  }
  return json;
}

// ─────────────────────────────────────────────────────────────────────────────
// Steps 4+5 — FAPI-direct sign-in via the ticket; mint the ~60s session JWT.
//
// Split into pieces so the tenant-readiness poll (GAP 1) can RE-MINT a fresh JWT
// (the JWT lives ~60s; the underlying Clerk *session* lives far longer) without
// re-creating the user / re-consuming a one-time ticket on every probe:
//
//   establishFapiSession() — consume the ticket, create the __client + session,
//                            resolve the session id (the expensive, once part).
//   mintTokenFromSession() — POST sessions/<id>/tokens → a fresh ~60s JWT
//                            (cheap; safe to call repeatedly on the live session).
//   freshSignInAndMint()   — the full cold path (ticket+testing token+session+jwt).
//   remintJwt()            — fresh JWT, cheap path first, full re-sign-in on lapse.
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Drive the Frontend API to consume the one-time ticket and establish the client
 * + session (no browser, no window.Clerk):
 *   (a) POST /v1/client/sign_ins  strategy=ticket&ticket=<signInToken>  → a
 *       completed sign_in carrying created_session_id (creates the __client).
 *   (b) fall back to GET /v1/client to read the active session id if absent.
 * Returns the cookie jar (carries __client) and the resolved session id so a
 * caller can mint fresh JWTs against the SAME live session repeatedly.
 *
 * @param {string} signInToken  the Backend-API ticket
 * @param {string} testingToken the Backend-API testing token (bot bypass)
 * @returns {Promise<{jar: ReturnType<typeof makeCookieJar>, sessionId: string}>}
 */
async function establishFapiSession(signInToken, testingToken) {
  const jar = makeCookieJar();

  // (a) Consume the ticket — creates the client + a completed sign_in.
  log("FAPI: POST /v1/client/sign_ins (strategy=ticket) …");
  const signInRes = await fapiFetch(jar, "/v1/client/sign_ins", {
    method: "POST",
    testingToken,
    form: { strategy: "ticket", ticket: signInToken },
  });
  // FAPI envelope: { client, response } where `response` is the SignIn object.
  const signIn = signInRes?.response ?? signInRes;
  let sessionId = signIn?.created_session_id || null;

  if (!sessionId) {
    if (signIn?.status && signIn.status !== "complete") {
      die(
        "FAPI sign-in did not complete (status=" +
          signIn.status +
          "). The ticket did not fully sign the user in. Full sign_in object:\n" +
          JSON.stringify(signIn, null, 2).slice(0, 1500),
      );
    }
    // (b) Fall back to reading the active session off the client.
    log("FAPI: GET /v1/client (resolve active session) …");
    const clientRes = await fapiFetch(jar, "/v1/client", {
      method: "GET",
      testingToken,
    });
    const client = clientRes?.response ?? clientRes;
    sessionId =
      client?.last_active_session_id ||
      client?.sessions?.[0]?.id ||
      null;
  }

  if (!sessionId) {
    die(
      "could not resolve a session id from the FAPI sign-in / client. " +
        "sign_ins response:\n" + JSON.stringify(signInRes, null, 2).slice(0, 1500),
    );
  }
  log(`FAPI session id = ${sessionId}`);
  return { jar, sessionId };
}

/**
 * Mint a fresh default ~60s session JWT against an already-established session
 * (no template) — carries azp = Origin. Cheap; the live Clerk session outlives
 * any single JWT, so this is the right primitive to refresh a near-expired token.
 *
 * @param {ReturnType<typeof makeCookieJar>} jar
 * @param {string} sessionId
 * @param {string} testingToken
 * @returns {Promise<string>} the session JWT
 */
async function mintTokenFromSession(jar, sessionId, testingToken) {
  log(`FAPI: POST /v1/client/sessions/${sessionId}/tokens …`);
  const tokenRes = await fapiFetch(
    jar,
    `/v1/client/sessions/${sessionId}/tokens`,
    { method: "POST", testingToken },
  );
  // The tokens endpoint returns { object:"token", jwt }; tolerate the wrapped shape.
  const jwt = tokenRes?.jwt || tokenRes?.response?.jwt || null;
  if (!jwt) {
    die(
      "FAPI token mint returned no jwt:\n" +
        JSON.stringify(tokenRes, null, 2).slice(0, 800),
    );
  }
  return jwt;
}

/**
 * The full cold path: mint a one-time ticket + testing token, establish the FAPI
 * session, and mint the first JWT. Returns the JWT plus the live session context
 * (jar/sessionId/testingToken) so callers can cheaply re-mint fresh JWTs later.
 *
 * @param {{userId:string}} creds
 * @returns {Promise<{jwt:string, jar:ReturnType<typeof makeCookieJar>, sessionId:string, testingToken:string}>}
 */
async function freshSignInAndMint(creds) {
  // One-time ticket + testing token are both short-lived → mint fresh each time.
  const signInToken = await createSignInToken(creds.userId);
  const testingToken = await fetchTestingToken();
  const { jar, sessionId } = await establishFapiSession(signInToken, testingToken);
  const jwt = await mintTokenFromSession(jar, sessionId, testingToken);
  log("minted a session JWT via FAPI (sign-in succeeded).");
  return { jwt, jar, sessionId, testingToken };
}

/**
 * Return a session context carrying a FRESH ~60s JWT. Tries the cheap path first
 * (re-mint against the existing live session); if that fails (session/client
 * lapsed, testing token stale, etc.) it transparently does a full re-sign-in.
 *
 * @param {{jar:ReturnType<typeof makeCookieJar>, sessionId:string, testingToken:string}} ctx
 * @param {{userId:string}} creds
 * @returns {Promise<{jwt:string, jar:ReturnType<typeof makeCookieJar>, sessionId:string, testingToken:string}>}
 */
async function remintJwt(ctx, creds) {
  try {
    const jwt = await mintTokenFromSession(ctx.jar, ctx.sessionId, ctx.testingToken);
    return { ...ctx, jwt };
  } catch (e) {
    log(
      "cheap JWT re-mint failed (session/client likely lapsed) → full re-sign-in: " +
        (e?.message || String(e)).slice(0, 200),
    );
    return await freshSignInAndMint(creds);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 6 — tenant-readiness gate (GAP 1). The throwaway user only gets a CoreLink
// tenant when the signup-worker `user.created` webhook fires (async, seconds).
// Until then a session-authed call returns 401/403 (no tenant row for the
// clerk_user_id). We POLL /v1/users/me until it returns 200 — and because the JWT
// lives ~60s (GAP 2), we RE-MINT a fresh JWT before EVERY probe so the probe
// always uses a LIVE bearer. We succeed (return) only on 200, so the emitted
// session is GUARANTEED tenant-ready. Budget exhausted still-401 ⇒ FAIL LOUD.
// ─────────────────────────────────────────────────────────────────────────────
/**
 * @param {{jwt:string, jar:ReturnType<typeof makeCookieJar>, sessionId:string, testingToken:string}} ctx
 * @param {{userId:string}} creds
 * @returns {Promise<{jwt:string, jar:ReturnType<typeof makeCookieJar>, sessionId:string, testingToken:string}>}
 *          the session context, with a fresh JWT, proven tenant-ready (200).
 */
async function waitForTenantReady(ctx, creds) {
  const probeUrl = `${TENANT_PROBE_ENDPOINT}${TENANT_PROBE_PATH}`;
  const deadline = Date.now() + TENANT_WAIT_MS;
  log(
    `waiting for tenant provisioning: polling ${probeUrl} ` +
      `(budget ~${Math.round(TENANT_WAIT_MS / 1000)}s, re-minting a fresh JWT ` +
      `before each probe) …`,
  );
  let attempt = 0;
  for (;;) {
    attempt++;
    // Re-mint a LIVE JWT before EACH probe (the previous one may be ~/over 60s).
    ctx = await remintJwt(ctx, creds);

    let status = 0;
    let body = "";
    try {
      const res = await fetch(probeUrl, {
        headers: { Authorization: `Bearer ${ctx.jwt}` },
      });
      status = res.status;
      if (status === 200) {
        log(
          `tenant ready: ${probeUrl} → 200 (attempt ${attempt}) — session is USABLE.`,
        );
        return ctx;
      }
      body = await res.text().catch(() => "");
    } catch (e) {
      // Transient network error against the probe — treat like not-ready, retry.
      body = `fetch error: ${e?.message || String(e)}`;
    }

    // A status other than 200/401/403 is NOT a provisioning lag → fail loud now.
    if (status !== 0 && status !== 401 && status !== 403) {
      die(
        `tenant readiness probe GET ${probeUrl} returned unexpected ${status} ` +
          `(expected 200 when ready, or 401/403 while provisioning). Body:\n` +
          body.slice(0, 600),
      );
    }

    const remaining = deadline - Date.now();
    if (remaining <= 0) {
      die(
        `tenant never provisioned within ${Math.round(TENANT_WAIT_MS / 1000)}s — ` +
          "the signup-worker `user.created` webhook never created a tenant row " +
          `for clerk_user_id=${creds.userId}. Check Svix (webhook delivery) and ` +
          `the signup-worker logs. Last probe: ${probeUrl} → ${status || "ERR"}` +
          (body ? `\n  body: ${body.slice(0, 400)}` : ""),
      );
    }
    log(
      `tenant not ready yet (${probeUrl} → ${status || "ERR"}); waiting ` +
        `${Math.round(TENANT_WAIT_INTERVAL_MS / 1000)}s ` +
        `(attempt ${attempt}, ~${Math.round(remaining / 1000)}s left) …`,
    );
    await sleep(Math.min(TENANT_WAIT_INTERVAL_MS, Math.max(0, remaining)));
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 7 — best-effort tenant resolution (NEVER writes D1).
// ─────────────────────────────────────────────────────────────────────────────
/**
 * The signup-worker webhook maps user.created → a tenant row
 * (tenant.clerk_user_id). That mapping can lag a few seconds after user
 * creation. We do a best-effort poll of a Clerk-session-authenticated CoreLink
 * endpoint that echoes the tenant id, IF an API endpoint is configured. If we
 * cannot resolve it, we return null and emit a note — the session vars are still
 * fully usable; only CORELINK_E2E_DSR_TEST_TENANT is omitted.
 *
 * @param {string} jwt
 * @returns {Promise<string|null>}
 */
async function resolveTenantBestEffort(jwt) {
  if (!CORELINK_API_ENDPOINT) {
    log(
      "tenant resolution skipped (CORELINK_API_ENDPOINT not set). The tenant " +
        "mapping is created by the signup-worker webhook; CORELINK_E2E_DSR_TEST_TENANT " +
        "will be omitted.",
    );
    return null;
  }
  // Try a couple of session-authed endpoints that may echo the tenant id. These
  // are best-effort; any shape that surfaces a tenant id wins. We never fail the
  // whole run on this.
  const candidates = [
    "/v1/customer/account",
    "/v1/onboarding/status",
    "/v1/customer/billing",
  ];
  for (let attempt = 0; attempt < 6; attempt++) {
    for (const path of candidates) {
      try {
        const res = await fetch(`${CORELINK_API_ENDPOINT}${path}`, {
          headers: { Authorization: `Bearer ${jwt}` },
        });
        if (!res.ok) continue;
        const body = await res.json().catch(() => ({}));
        const tid =
          body?.tenant_id || body?.tenant?.id || body?.tenantId || null;
        if (tid) {
          log(`resolved tenant_id=${tid} via ${path}`);
          return String(tid);
        }
      } catch {
        /* keep trying */
      }
    }
    // Webhook lag: wait and retry.
    await new Promise((r) => setTimeout(r, 3000));
  }
  log(
    "tenant_id not resolvable via the configured API (webhook lag or no echo " +
      "endpoint). CORELINK_E2E_DSR_TEST_TENANT omitted — supply it manually if " +
      "needed for the DSR full-flow.",
  );
  return null;
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 6.5 — DPA onboarding accept (GAP 3). The money-path gate
// (`/v1/onboarding/tier-select`) 403s `dpa_required` until the tenant has a
// `dpa_acceptances` row for the CURRENT DPA version (INV-ONBOARD-DPA-FIRST). The
// readiness gate (GAP 1) only proves the tenant ROW exists; it does NOT accept
// the DPA. Without this, ~15 Clerk-session journeys (billing / dsr /
// runner_purchase) stay GATED on `dpa_required`. Idempotent: a re-accept for the
// same (tenant, version) is a no-op success. The deployed container's writer
// (`routes/dpa_accept.rs::validate_request`) only checks version == current +
// a 64-hex notice hash (it writes the client-attested "what the user saw" hash
// straight to D1 — no server-side notice registry on that path), so we attest the
// SHA-256 of a labelled fixture notice.
// ─────────────────────────────────────────────────────────────────────────────
const DPA_ACCEPT_PATH = "/v1/onboarding/dpa-accept";
const FIXTURE_NOTICE_TEXT =
  "corelink-e2e fixture DPA notice — accepted by clerk-session-minter.mjs";

/**
 * POST the DPA acceptance with a fresh session bearer. Fail-loud on anything but
 * a 2xx (a mismatch would silently leave the money-path journeys gated).
 * @param {{jwt:string}} ctx
 */
async function acceptDpa(ctx) {
  const url = `${TENANT_PROBE_ENDPOINT}${DPA_ACCEPT_PATH}`;
  const noticeHash = createHash("sha256")
    .update(FIXTURE_NOTICE_TEXT)
    .digest("hex");
  const payload = {
    dpa_version: DPA_VERSION,
    dpa_locale: "en",
    notice_text_hash: noticeHash,
  };
  log(`accepting DPA (version=${DPA_VERSION}, locale=en) → POST ${url} …`);
  let res;
  try {
    res = await fetch(url, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${ctx.jwt}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });
  } catch (e) {
    die(`DPA accept POST ${url} failed to connect: ${e?.message || String(e)}`);
  }
  if (res.ok) {
    log(
      `DPA accepted (${res.status}) — tenant is now DPA-onboarded; the ` +
        "money-path journeys (billing / dsr / runner_purchase) can un-gate.",
    );
    return;
  }
  const text = await res.text().catch(() => "");
  if (res.status === 400 && text.includes("dpa_version_mismatch")) {
    die(
      `DPA accept rejected: the server's current DPA version != "${DPA_VERSION}". ` +
        "Set CORELINK_DPA_VERSION (or CORELINK_E2E_DPA_VERSION) to the deployed " +
        `container's CORELINK_DPA_VERSION and re-run. Server said:\n${text.slice(0, 300)}`,
    );
  }
  die(
    `DPA accept POST ${url} returned ${res.status} (expected 2xx). The emitted ` +
      "session would still gate on `dpa_required` for tier-select/billing. Body:\n" +
      text.slice(0, 500),
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 7 — emit the exports (stdout + file).
// ─────────────────────────────────────────────────────────────────────────────
function emit({ jwt, userId, tenantId, tenantReady }) {
  const lines = [
    "# Generated by clerk-session-minter.mjs — Clerk session JWT (~60s TTL!).",
    `# Minted at ${new Date().toISOString()} for Clerk user_id=${userId}`,
    tenantReady
      ? "# Tenant-ready: /v1/users/me returned 200 before emit — session is USABLE NOW."
      : "# NOTE: tenant-readiness NOT verified (--no-wait) — may 401 until the webhook fires.",
    "# Use IMMEDIATELY (run the Clerk-session journeys within ~60s) or --refresh.",
    `export CORELINK_E2E_CLERK_SESSION='${jwt}'`,
    `export CORELINK_E2E_DSR_SESSION='${jwt}'`,
    `export CORELINK_E2E_DSR_TEST_SESSION='${jwt}'`,
  ];
  if (tenantId) {
    lines.push(`export CORELINK_E2E_DSR_TEST_TENANT='${tenantId}'`);
  }
  const out = lines.join("\n") + "\n";

  writeFileSync(OUT_FILE, out, { mode: 0o600 });

  // (The user-cred sidecar for --refresh/cleanup is written in main() via
  //  saveUser(), kept separate so this source-able exports file stays clean.)

  // stdout = the source-able exports (so `eval "$(node clerk-session-minter.mjs)"`
  // works); everything else went to stderr via log().
  process.stdout.write(out);

  log(`wrote exports → ${OUT_FILE}`);
  log(`Clerk user_id (DSR-delete this on cleanup): ${userId}`);
  log(
    "⚠ The session JWT lives ~60s — run the Clerk-session journeys NOW " +
      "(source this env LAST, right before the run) or --refresh.",
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// User-cred sidecar persistence (for --refresh + cleanup).
// ─────────────────────────────────────────────────────────────────────────────
function saveUser(rec) {
  writeFileSync(USER_FILE, JSON.stringify(rec, null, 2) + "\n", { mode: 0o600 });
}
function loadUser() {
  if (!existsSync(USER_FILE)) return null;
  try {
    return JSON.parse(readFileSync(USER_FILE, "utf8"));
  } catch {
    return null;
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// main
// ─────────────────────────────────────────────────────────────────────────────
async function main() {
  // Preconditions — fail loud and early.
  if (!CLERK_SECRET_KEY) {
    die(
      "CLERK_SECRET_KEY (or CLERK_LIVE_SECRET_KEY) is not set. The Backend API " +
        "needs it to create the user + mint a testing token. Source .env.local.",
    );
  }
  if (!CLERK_PUBLISHABLE_KEY) {
    log(
      "note: CLERK_PUBLISHABLE_KEY not set — using the default/env FAPI host (" +
        CLERK_FAPI_HOST +
        "). The FAPI-direct flow does not require the publishable key.",
    );
  }
  log(`app origin   : ${CORELINK_APP_URL}`);
  log(`FAPI host    : ${CLERK_FAPI_HOST}`);
  log(`output file  : ${OUT_FILE}`);
  log(`mode         : ${REFRESH ? "refresh (reuse existing user)" : "fresh user"}`);
  log(`tenant gate  : ${WAIT_TENANT ? `wait (~${Math.round(TENANT_WAIT_MS / 1000)}s budget, probe ${TENANT_PROBE_ENDPOINT}${TENANT_PROBE_PATH})` : "DISABLED (--no-wait)"}`);

  /** @type {{userId:string,email:string,password:string,tenantReady?:boolean}} */
  let creds;
  /** Whether a prior run already proved this user's tenant exists (sidecar). */
  let tenantAlreadyProvisioned = false;

  if (REFRESH) {
    const saved = loadUser();
    if (!saved?.email || !saved?.password || !saved?.userId) {
      die(
        `--refresh needs a prior run's user sidecar at ${USER_FILE} ` +
          "(email/password/userId). Run once WITHOUT --refresh first.",
      );
    }
    creds = saved;
    tenantAlreadyProvisioned = saved.tenantReady === true;
    log(
      `refresh: reusing user_id=${creds.userId}` +
        (tenantAlreadyProvisioned
          ? " (tenant already provisioned — skipping readiness wait)"
          : ""),
    );
  } else {
    const u = await createThrowawayUser();
    creds = u;
    saveUser(u);
  }

  // Cold path: create the FAPI session + the first JWT.
  let ctx = await freshSignInAndMint(creds);

  // GAP 1 — tenant-readiness gate. Block until /v1/users/me → 200, re-minting a
  // fresh JWT before each probe (GAP 2). Skipped on --no-wait or when a prior
  // run already recorded the tenant as provisioned.
  if (WAIT_TENANT && !tenantAlreadyProvisioned) {
    ctx = await waitForTenantReady(ctx, creds);
    tenantAlreadyProvisioned = true;
    // Persist the flag so a later --refresh skips the wait.
    saveUser({ ...creds, tenantReady: true });
  } else if (!WAIT_TENANT) {
    log(
      "--no-wait: skipping the tenant-readiness gate — the emitted session may " +
        "401 until the signup-worker provisions the tenant.",
    );
  } else {
    log("tenant already provisioned (sidecar) — skipping the readiness wait.");
  }

  // GAP 3 — accept the current DPA so the money-path journeys don't 403
  // `dpa_required`. Only when the tenant is proven to exist (an accept needs the
  // tenant row); `--no-dpa` skips it. Uses a fresh bearer (the accept is
  // session-authed and the JWT lives ~60s).
  if (tenantAlreadyProvisioned && ACCEPT_DPA) {
    ctx = await remintJwt(ctx, creds);
    await acceptDpa(ctx);
  } else if (!ACCEPT_DPA) {
    log(
      "--no-dpa: skipping DPA acceptance — the emitted session will gate on " +
        "`dpa_required` for tier-select / billing / runner-purchase journeys.",
    );
  } else {
    log(
      "tenant not proven ready (--no-wait, no sidecar) — skipping DPA accept; " +
        "the session may gate on `dpa_required`.",
    );
  }

  // GAP 2 / freshness — mint ONE final fresh JWT right before emitting so the
  // consumer gets a full ~60s window (the probe loop + DPA accept spent some of
  // the last one).
  ctx = await remintJwt(ctx, creds);

  const tenantId = await resolveTenantBestEffort(ctx.jwt);

  emit({
    jwt: ctx.jwt,
    userId: creds.userId,
    tenantId,
    tenantReady: tenantAlreadyProvisioned,
  });
}

main().catch((e) => die(e?.stack || e?.message || String(e)));
