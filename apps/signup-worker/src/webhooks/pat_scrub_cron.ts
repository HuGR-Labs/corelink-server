/**
 * Guaranteed PAT-plaintext scrub cron (CTRL-CRED-001).
 *
 * The signup-worker writes the freshly-minted PAT plaintext into the user's
 * Clerk **`private_metadata`** (`{ pat_plaintext, pat_revealed_at }`) so the
 * `/welcome` page can reveal it once via a server-side Clerk Backend API read.
 * The intended clear is the client-driven `/welcome` reveal — but if the user
 * never opens `/welcome`, that clear never runs and the secret would persist.
 *
 * This Cron Trigger handler GUARANTEES the secret does not stay resident: it
 * lists every Clerk user and PATCHes `private_metadata.pat_plaintext` to null
 * for any user whose reveal is older than `PAT_REVEAL_TTL_MS` (default 1h). A
 * fresh reveal (within the TTL) is left intact so a just-signed-up user can
 * still complete the one-time reveal.
 *
 * Reveal-age source: `private_metadata.pat_revealed_at` (epoch-ms), co-written
 * with the plaintext at signup. A row that carries a `pat_plaintext` but NO (or
 * a malformed) `pat_revealed_at` is treated as STALE and scrubbed — fail-closed,
 * so a legacy / mis-written secret is never left resident forever.
 *
 * NEVER log `pat_plaintext`.
 *
 * Inert (no-op, skipped:true) until `CLERK_SECRET_KEY` is bound.
 */

/** Minimal env surface the scrub needs. */
export interface PatScrubCronEnv {
  /** Clerk Backend API secret key (sk_test_... / sk_live_...). */
  CLERK_SECRET_KEY?: string;
}

/**
 * Reveal TTL: how long a freshly-revealed PAT may remain in private_metadata
 * before the cron force-scrubs it. Short by design — the /welcome reveal is a
 * one-time, immediate action; an hour is ample slack for the user to land on
 * the page after the webhook fires.
 */
export const PAT_REVEAL_TTL_MS = 60 * 60 * 1000;

/** Clerk Backend API page size for the user list (Clerk max is 500). */
const PAGE_LIMIT = 100;
/** Hard cap on pages per sweep so a runaway never hangs the cron. */
const MAX_PAGES = 200;

/** Clerk Backend API base. */
const CLERK_API = "https://api.clerk.com/v1";

/** Shape of the bits of a Clerk user we read (everything else ignored). */
interface ClerkUserLite {
  id: string;
  private_metadata?: Record<string, unknown> | null;
}

/**
 * List one page of Clerk users (backend API). Returns the parsed page; throws
 * on a non-2xx so the sweep can surface the failure (caught by the caller).
 */
async function listUsersPage(
  secret: string,
  offset: number,
): Promise<ClerkUserLite[]> {
  const url = `${CLERK_API}/users?limit=${PAGE_LIMIT}&offset=${offset}&order_by=-created_at`;
  const resp = await fetch(url, {
    headers: { authorization: `Bearer ${secret}` },
  });
  if (!resp.ok) {
    throw new Error(`clerk_list_users_failed_${resp.status}`);
  }
  // Clerk returns a bare JSON array for GET /v1/users.
  const body = (await resp.json()) as ClerkUserLite[];
  return Array.isArray(body) ? body : [];
}

/**
 * Decide whether a user's PAT plaintext must be scrubbed.
 *  - no `pat_plaintext` present       → nothing to do (false),
 *  - `pat_revealed_at` missing/NaN    → STALE, scrub (fail-closed),
 *  - reveal older than the TTL        → scrub,
 *  - reveal within the TTL            → keep (let /welcome reveal it once).
 */
export function shouldScrub(
  privateMetadata: Record<string, unknown> | null | undefined,
  nowMs: number,
): boolean {
  if (!privateMetadata) return false;
  const pat = privateMetadata["pat_plaintext"];
  if (typeof pat !== "string" || pat.length === 0) return false;
  const revealedAt = Number(privateMetadata["pat_revealed_at"]);
  if (!Number.isFinite(revealedAt) || revealedAt <= 0) {
    // Secret present but no usable reveal clock → treat as stale, scrub.
    return true;
  }
  return nowMs - revealedAt >= PAT_REVEAL_TTL_MS;
}

/**
 * Clear `private_metadata.pat_plaintext` (+ its clock) for one user via the
 * Clerk Backend API. Clerk merges `private_metadata`, and a `null` value
 * REMOVES the key. NEVER throws (errors reported as `false`) so one bad user
 * does not abort the sweep. NEVER log the plaintext.
 */
export async function scrubUser(
  secret: string,
  userId: string,
): Promise<boolean> {
  try {
    const resp = await fetch(
      `${CLERK_API}/users/${encodeURIComponent(userId)}`,
      {
        method: "PATCH",
        headers: {
          authorization: `Bearer ${secret}`,
          "content-type": "application/json",
        },
        // null removes the key under Clerk's metadata-merge semantics.
        body: JSON.stringify({
          private_metadata: { pat_plaintext: null, pat_revealed_at: null },
        }),
      },
    );
    return resp.ok;
  } catch (err) {
    console.error(
      `[pat-scrub-cron] scrub PATCH threw user_id=${userId}: ${(err as Error).message.slice(0, 120)}`,
    );
    return false;
  }
}

/**
 * Run the PAT-plaintext scrub sweep. Pages through all Clerk users and scrubs
 * the stale reveals. Returns the counts.
 *
 * `skipped: true` (zero counts) when `CLERK_SECRET_KEY` is unbound.
 */
export async function runPatScrubSweep(
  env: PatScrubCronEnv,
  nowMs: number,
): Promise<{ scanned: number; scrubbed: number; failed: number; skipped: boolean }> {
  const secret = env.CLERK_SECRET_KEY;
  if (!secret) {
    return { scanned: 0, scrubbed: 0, failed: 0, skipped: true };
  }

  let scanned = 0;
  let scrubbed = 0;
  let failed = 0;

  for (let page = 0; page < MAX_PAGES; page += 1) {
    let users: ClerkUserLite[];
    try {
      users = await listUsersPage(secret, page * PAGE_LIMIT);
    } catch (err) {
      // List failure (transient / rate-limit) — stop the sweep; the hourly
      // tick retries. Best-effort by design (re-sweep is idempotent).
      console.error(
        `[pat-scrub-cron] list page=${page} failed: ${(err as Error).message.slice(0, 120)}`,
      );
      break;
    }
    if (users.length === 0) break; // past the last page.

    for (const u of users) {
      scanned += 1;
      if (!shouldScrub(u.private_metadata, nowMs)) continue;
      const ok = await scrubUser(secret, u.id);
      if (ok) scrubbed += 1;
      else failed += 1;
    }

    if (users.length < PAGE_LIMIT) break; // last (partial) page.
  }

  return { scanned, scrubbed, failed, skipped: false };
}
