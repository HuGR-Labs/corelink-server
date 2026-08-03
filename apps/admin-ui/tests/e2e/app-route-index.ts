/**
 * Filesystem-truth resolver for App Router URLs — "does a `page` file actually
 * exist for this URL?", answered by walking `src/app` rather than by asking the
 * dev server (the component under suspicion).
 *
 * Why this exists: `warm-routes.ts` treats a route that stays `404` as fatal,
 * on the strength of a COMMENT asserting that every warmed route exists on
 * disk. A comment is not a check. Two different answers hide behind the same
 * `404`:
 *
 *   - the warm list names a path with no `page.tsx`  → a HARNESS defect; the
 *     list is wrong and must be fixed. No amount of retrying will help.
 *   - the path resolves to a real `page.tsx` on disk → the DEV SERVER is at
 *     fault; Next never put the entry in its route tree (see the recovery
 *     path in `warm-routes.ts`).
 *
 * Telling them apart is the whole value: it decides whether the next occurrence
 * gets "your warm list is wrong" or "turbopack lost the route tree again".
 *
 * Segment semantics implemented here mirror the App Router's own conventions:
 *   `(group)` / `@slot`  consume NO url segment (route groups + parallel slots)
 *   `[param]`            consumes exactly one
 *   `[...param]`         consumes one or more (catch-all)
 *   `[[...param]]`       consumes zero or more (optional catch-all)
 * Backtracking is required because a literal directory and a `[param]` sibling
 * can both match the same segment (`/en/admin/ops/op_byok_001` must fall
 * through `ops/page.tsx` into `ops/[op_id]/page.tsx`).
 */
import { existsSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * `apps/admin-ui/src/app`.
 *
 * Resolved from this module's own URL when that is a real `file:` URL (the
 * Playwright/tsx path), and otherwise by walking up from the working directory
 * — vitest rewrites `import.meta.url` to a non-file scheme, and this module has
 * to work under BOTH runners: Playwright's `globalSetup` executes it, and the
 * vitest regression lock imports it.
 */
function locateAppDir(): string {
  try {
    const here = import.meta.url;
    if (typeof here === "string" && here.startsWith("file:")) {
      const candidate = fileURLToPath(new URL("../../src/app", here));
      if (existsSync(candidate)) return candidate;
    }
  } catch {
    /* fall through to the cwd walk */
  }
  let dir = process.cwd();
  for (let depth = 0; depth < 6; depth += 1) {
    const candidate = join(dir, "src", "app");
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error(
    "app-route-index: could not locate apps/admin-ui/src/app from " +
      `${import.meta.url} or ${process.cwd()}`,
  );
}

export const APP_DIR = locateAppDir();

/** Page-file names Next accepts, in `next.config.ts` `pageExtensions` order. */
const PAGE_FILES = ["page.tsx", "page.ts", "page.jsx", "page.js"] as const;

function pageFileIn(dir: string): string | null {
  for (const name of PAGE_FILES) {
    const candidate = join(dir, name);
    if (existsSync(candidate)) return candidate;
  }
  return null;
}

function subdirectories(dir: string): string[] {
  try {
    return readdirSync(dir).filter((name) => {
      // `statSync` (not `dirent.isDirectory`) so a symlinked route directory
      // resolves the same way Next's own reader would follow it.
      try {
        return statSync(join(dir, name)).isDirectory();
      } catch {
        return false;
      }
    });
  } catch {
    return [];
  }
}

/** A directory that contributes no URL segment: `(group)` or `@slot`. */
function isTransparentSegment(name: string): boolean {
  return (name.startsWith("(") && name.endsWith(")")) || name.startsWith("@");
}

function walk(dir: string, segments: readonly string[]): string | null {
  if (segments.length === 0) {
    const page = pageFileIn(dir);
    if (page) return page;
  }

  for (const name of subdirectories(dir)) {
    const child = join(dir, name);

    // Route groups and parallel slots are URL-invisible: descend, consuming
    // nothing. This is what makes `(authenticated)` disappear from the URL.
    if (isTransparentSegment(name)) {
      const hit = walk(child, segments);
      if (hit) return hit;
      continue;
    }

    // Optional catch-all `[[...x]]` — matches zero or more remaining segments,
    // so it can also close out an already-exhausted path (`/sign-in`).
    if (name.startsWith("[[...") && name.endsWith("]]")) {
      const hit = walk(child, []);
      if (hit) return hit;
      continue;
    }

    if (segments.length === 0) continue;

    // Catch-all `[...x]` — one or more remaining segments.
    if (name.startsWith("[...") && name.endsWith("]")) {
      const hit = walk(child, []);
      if (hit) return hit;
      continue;
    }

    // Dynamic `[x]` — exactly one segment; or a literal directory name.
    const consumesOne =
      (name.startsWith("[") && name.endsWith("]")) || name === segments[0];
    if (consumesOne) {
      const hit = walk(child, segments.slice(1));
      if (hit) return hit;
    }
  }

  return null;
}

/**
 * The `page` file that serves `urlPath`, or `null` when nothing on disk does.
 *
 * `urlPath` is the app-relative path WITHOUT the `/corelink` basePath — the
 * same shape `warm-routes.ts` stores (`/en/admin/ops/op_byok_001`).
 */
export function resolveAppPage(urlPath: string, appDir: string = APP_DIR): string | null {
  const segments = urlPath.split("?")[0]?.split("/").filter(Boolean) ?? [];
  return walk(appDir, segments);
}

/**
 * Every directory Next must have read to discover `pageFile`, from `src/app`
 * down to the page's own directory.
 *
 * Used by the dev-server recovery path: the observed fault is a route tree
 * MISSING a nested entry, so the directory whose listing came back short is one
 * of these — touching all of them is what forces a re-read.
 */
export function directoryChainFor(pageFile: string, appDir: string = APP_DIR): string[] {
  const relative = pageFile.slice(appDir.length).split("/").filter(Boolean);
  // Drop the page file itself; keep every directory above it.
  const dirs = relative.slice(0, -1);
  const chain: string[] = [appDir];
  let current = appDir;
  for (const segment of dirs) {
    current = join(current, segment);
    chain.push(current);
  }
  return chain;
}
