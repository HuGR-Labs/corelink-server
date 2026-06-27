#!/usr/bin/env node
// @ts-check
/**
 * clerk-session-minter.mjs — SOTA headless-browser Clerk-session minter for the
 * CoreLink e2e suite.
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
 *   2. has `azp` ∈ CLERK_AZP_ALLOWLIST  →  "https://corelink-app.humangr.com"
 *      (this script signs in ON that origin, so ClerkJS stamps that exact azp),
 *   3. has `iss` == CLERK_ISSUER_URL  (the exact-pin; the live FAPI issuer),
 *   4. has a `sub` (the Clerk user id) that maps to a CoreLink tenant row
 *      (tenant.clerk_user_id, written by the signup-worker on user.created).
 *
 * A Clerk session JWT CANNOT be minted purely server-side: Clerk's anti-fraud /
 * bot-detection blocks programmatic sign-in. The sanctioned automation path
 * (https://clerk.com/docs/testing/overview) is a **Testing Token**: a short-lived
 * Backend-API-issued token that, when attached to Frontend-API (FAPI) requests,
 * tells Clerk "this is an authorized automated test — skip bot detection." We
 * drive a real headless Chromium sign-in with that token attached.
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * WHAT IT DOES (the sanctioned flow)
 * ─────────────────────────────────────────────────────────────────────────────
 *   1. Create a throwaway Clerk user via the Backend API (verified email +
 *      generated password, skip_password_checks). Capture user_id.
 *   2. Fetch a Testing Token via the Backend API.
 *   3. Headless Chromium → load the live app origin (corelink-app.humangr.com),
 *      attach the testing token to every FAPI request (so bot-detection is
 *      bypassed), and sign in with email+password via ClerkJS in-page.
 *   4. Harvest the session JWT via `window.Clerk.session.getToken()` (the same
 *      ~60s `__session` JWT the dashboard sends as the bearer).
 *   5. Best-effort resolve the CoreLink tenant id (the signup-worker webhook
 *      maps user.created → tenant; may take a moment — we poll, never write D1).
 *   6. Emit `export CORELINK_E2E_{CLERK,DSR,DSR_TEST}_SESSION=<jwt>` to stdout
 *      and to an output file (default /tmp/e2e-clerk-session.sh); print user_id.
 *   7. `--refresh` re-harvests a fresh JWT for an already-created user (60s TTL).
 *
 * ─────────────────────────────────────────────────────────────────────────────
 * THE #1 RISK — surfaced loudly, never silently swallowed
 * ─────────────────────────────────────────────────────────────────────────────
 * The testing-token bot-detection bypass is the documented mechanism, but its
 * success against the LIVE production Clerk instance is not guaranteed (the
 * instance may have stricter fraud rules, or the token may be rejected). If
 * sign-in is still blocked, this script FAILS LOUD with the exact Clerk error
 * code/message — it NEVER prints a token it could not actually mint, so a caller
 * can never be fooled into running journeys against a bad bearer.
 *
 * HARD RULE: this script only TALKS to Clerk + the (optional) CoreLink API. It
 * never writes D1, never touches other repo files.
 */

import { writeFileSync, readFileSync, existsSync } from "node:fs";
import { randomBytes } from "node:crypto";

// ── Playwright (devDependency; installed via `npm i` in this dir) ─────────────
// Imported lazily with a clear remediation message if it (or the Chromium
// browser binary) is missing — see ensurePlaywright().
/** @type {import('playwright').BrowserType | null} */
let chromium = null;

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

/** Headless toggle (set CORELINK_E2E_HEADFUL=1 to watch the browser locally). */
const HEADLESS = process.env.CORELINK_E2E_HEADFUL !== "1";

const CLERK_API = "https://api.clerk.com/v1";

// ─────────────────────────────────────────────────────────────────────────────
// CLI args:  node clerk-session-minter.mjs [outfile] [--refresh] [--keep]
// ─────────────────────────────────────────────────────────────────────────────
const argv = process.argv.slice(2);
const REFRESH = argv.includes("--refresh");
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

/** Lazily import Playwright with an actionable error if absent. */
async function ensurePlaywright() {
  try {
    const pw = await import("playwright");
    chromium = pw.chromium;
  } catch {
    die(
      "playwright is not installed. Run, in scripts/e2e-real-client/:\n" +
        "    npm i\n" +
        "    npx playwright install chromium\n" +
        "(see CLERK-SESSION-README.md)",
    );
  }
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
// Steps 3+4 — headless sign-in with the testing token; harvest the session JWT.
// ─────────────────────────────────────────────────────────────────────────────
/**
 * @param {{email:string,password:string}} creds
 * @param {string} testingToken
 * @returns {Promise<string>} the harvested session JWT
 */
async function harvestSessionJwt(creds, testingToken) {
  await ensurePlaywright();
  if (!chromium) die("playwright chromium unavailable after import");

  const browser = await chromium.launch({ headless: HEADLESS });
  try {
    const context = await browser.newContext();

    // ── Attach the testing token to EVERY Frontend-API request ───────────────
    // The documented mechanism: Clerk's FAPI accepts the testing token as the
    // `__clerk_testing_token` query parameter; when present, bot detection is
    // skipped for that request. `@clerk/testing` patches fetch to do this; we do
    // it manually (no extra dep) by rewriting outbound requests to the FAPI host.
    await context.route("**/*", async (route) => {
      const req = route.request();
      const u = new URL(req.url());
      const isFapi =
        u.hostname === CLERK_FAPI_HOST ||
        u.hostname.endsWith(".clerk.accounts.dev") ||
        u.hostname.startsWith("clerk.");
      if (isFapi && !u.searchParams.has("__clerk_testing_token")) {
        u.searchParams.set("__clerk_testing_token", testingToken);
        return route.continue({ url: u.toString() });
      }
      return route.continue();
    });

    const page = await context.newPage();
    // Surface page console errors to aid debugging a blocked sign-in.
    page.on("console", (m) => {
      if (m.type() === "error") log(`page console.error: ${m.text()}`);
    });

    // Land on the app origin so ClerkJS initializes with azp = this origin.
    // Also pass the testing token in the URL (ClerkJS reads it from the query).
    const landing = `${CORELINK_APP_URL}/?__clerk_testing_token=${encodeURIComponent(
      testingToken,
    )}`;
    log(`navigating headless to ${CORELINK_APP_URL} …`);
    await page.goto(landing, { waitUntil: "domcontentloaded", timeout: 60_000 });

    // Wait for ClerkJS to be present + loaded on the page.
    await page
      .waitForFunction(
        () => !!(window.Clerk && (window.Clerk.loaded || window.Clerk.client)),
        { timeout: 30_000 },
      )
      .catch(() => {
        die(
          "window.Clerk never loaded on " +
            CORELINK_APP_URL +
            " — the app did not initialize ClerkJS (wrong CORELINK_APP_URL / " +
            "publishable key mismatch / page blocked).",
        );
      });

    // ── Drive sign-in via ClerkJS in-page, then harvest the session JWT ──────
    // We do email+password through Clerk.client.signIn, activate the session,
    // then call session.getToken() — the SAME default `__session` JWT the
    // dashboard sends as its bearer (azp = this origin, iss = the FAPI issuer).
    /** @type {{ ok: boolean, jwt?: string, error?: string }} */
    const result = await page.evaluate(
      async ({ email, password }) => {
        // @ts-ignore — Clerk is injected by the app.
        const Clerk = window.Clerk;
        try {
          // Ensure the singleton is loaded.
          if (Clerk.load) {
            try {
              await Clerk.load();
            } catch {
              /* already loaded */
            }
          }
          const signIn = Clerk.client.signIn;
          const attempt = await signIn.create({
            identifier: email,
            password,
          });
          if (attempt.status !== "complete") {
            return {
              ok: false,
              error:
                "sign-in not complete (status=" +
                attempt.status +
                "); first factor likely requires verification/captcha — " +
                JSON.stringify(attempt.firstFactorVerification || {}),
            };
          }
          await Clerk.setActive({ session: attempt.createdSessionId });
          // Default template token = the __session JWT (~60s TTL). This is the
          // exact bearer shape the worker verifier expects.
          const jwt = await Clerk.session.getToken();
          if (!jwt) return { ok: false, error: "getToken() returned null" };
          return { ok: true, jwt };
        } catch (e) {
          // Clerk throws ClerkAPIResponseError with .errors[]; surface verbatim.
          const errs =
            e && e.errors
              ? e.errors
                  .map((x) => (x.code || "?") + ": " + (x.message || x.longMessage || ""))
                  .join("; ")
              : (e && e.message) || String(e);
          return { ok: false, error: errs };
        }
      },
      creds,
    );

    if (!result.ok || !result.jwt) {
      const err = result.error || "unknown sign-in error";
      // The #1 risk, surfaced loud: distinguish a bot/captcha block so the caller
      // knows the testing-token bypass did NOT take.
      const looksLikeBotBlock = /captcha|bot|fraud|single_session|too_many|blocked/i.test(
        err,
      );
      die(
        "headless sign-in FAILED — NO token produced.\n" +
          "  Clerk error: " +
          err +
          "\n" +
          (looksLikeBotBlock
            ? "  ⚠ This looks like the bot-detection / captcha gate STILL firing\n" +
              "    despite the testing token (the known #1 risk against a hardened\n" +
              "    LIVE instance). Confirm Testing Tokens are enabled for this\n" +
              "    instance, or run against the test instance. See README.\n"
            : "  Check email/password, the app origin, and that the user exists.\n"),
      );
    }

    log("harvested a session JWT (sign-in succeeded).");
    return result.jwt;
  } finally {
    await browser.close();
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 5 — best-effort tenant resolution (NEVER writes D1).
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
// Step 6 — emit the exports (stdout + file).
// ─────────────────────────────────────────────────────────────────────────────
function emit({ jwt, userId, tenantId }) {
  const lines = [
    "# Generated by clerk-session-minter.mjs — Clerk session JWT (~60s TTL!).",
    `# Minted at ${new Date().toISOString()} for Clerk user_id=${userId}`,
    "# Re-run with --refresh BEFORE the 60s TTL expires for a fresh JWT.",
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
  log("⚠ The session JWT lives ~60s. Use it immediately or --refresh.");
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
      "WARNING: CLERK_PUBLISHABLE_KEY not set — relying on the app page to load " +
        "ClerkJS itself and on the default/env FAPI host (" +
        CLERK_FAPI_HOST +
        ").",
    );
  }
  log(`app origin   : ${CORELINK_APP_URL}`);
  log(`FAPI host    : ${CLERK_FAPI_HOST}`);
  log(`output file  : ${OUT_FILE}`);
  log(`mode         : ${REFRESH ? "refresh (reuse existing user)" : "fresh user"}`);

  /** @type {{userId:string,email:string,password:string}} */
  let creds;

  if (REFRESH) {
    const saved = loadUser();
    if (!saved?.email || !saved?.password || !saved?.userId) {
      die(
        `--refresh needs a prior run's user sidecar at ${USER_FILE} ` +
          "(email/password/userId). Run once WITHOUT --refresh first.",
      );
    }
    creds = saved;
    log(`refresh: reusing user_id=${creds.userId}`);
  } else {
    const u = await createThrowawayUser();
    creds = u;
    saveUser(u);
  }

  // The testing token is short-lived; always fetch a fresh one (even on refresh).
  const testingToken = await fetchTestingToken();

  const jwt = await harvestSessionJwt(
    { email: creds.email, password: creds.password },
    testingToken,
  );

  const tenantId = await resolveTenantBestEffort(jwt);

  emit({ jwt, userId: creds.userId, tenantId });
}

main().catch((e) => die(e?.stack || e?.message || String(e)));
