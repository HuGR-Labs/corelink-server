# WP-09: Dashboard UI — DevEnv Management

**Status:** `NOT_STARTED`  
**Owner:** Frontend TL  
**Depends On:** WP-08  
**Estimate:** 2 days  
**Priority:** P1 (High)

---

## 1. Objective

Build the DevEnv management UI in the CoreLink admin dashboard:
- DevEnv list with status badges
- Create DevEnv modal (workspace name, profile name)
- DevEnv detail view with live status
- WebSocket connection buttons (noVNC, ttyd, code-server)
- Real-time status updates via polling/WebSocket
- Resize controls for terminal/desktop

---

## 2. Scope

### In Scope
- React/TypeScript components in `apps/admin-ui`
- DevEnv list page (`/devenv`)
- Create DevEnv modal
- DevEnv detail page (`/devenv/:id`)
- WebSocket connection handlers (iframe or new tab)
- Real-time status polling (10s interval)
- Resize control UI
- Error handling and loading states

### Out of Scope
- Backend API → WP-08
- Authentication → existing Clerk integration
- Billing UI → separate billing page

---

## 3. Technical Specification

### 3.1 File Structure (matches existing `src/components/<area>/<Name>Client.tsx` pattern)

```
apps/admin-ui/
├── src/
│   ├── components/
│   │   └── devenv/
│   │       ├── DevenvList.tsx              ← list + EmptyState + Skeleton
│   │       ├── DevenvListClient.tsx        ← "use client" consumer, wraps ToastProvider
│   │       ├── CreateDevenvModal.tsx       ← Field + Input + Button; uses Modal
│   │       ├── DevenvDetail.tsx            ← detail page, status + connections + resize
│   │       ├── DevenvDetailClient.tsx      ← "use client" consumer
│   │       ├── ConnectionButtons.tsx       ← three Button + Tooltip + openInNewTab
│   │       ├── StatusBadge.tsx             ← Linear Badge with tone + dot
│   │       ├── ResizeControls.tsx          ← Field + Input + Button presets
│   │       ├── DevenvSkeleton.tsx          ← uses Skeleton from Linear kit
│   │       ├── useDevenvList.ts            ← LIST_REFRESH_MS=30s polling hook
│   │       ├── useDevenvStatus.ts          ← STATUS_REFRESH_MS=10s polling hook
│   │       └── useDevenvActions.ts         ← useCreate/Stop/Snapshot/ResizeDevenv
│   ├── lib/
│   │   ├── customer-types.ts               ← ADD: Devenv, DevenvStatus, DevenvStatusResponse,
│   │   │                                       CreateDevenvInput, CreateDevenvResponse,
│   │   │                                       SnapshotResponse, ResizeInput, DevenvListResponse,
│   │   │                                       DevenvConnectionUrls, DevenvPort
│   │   └── customer-client.ts              ← ADD: listDevenvs(), createDevenv(),
│   │                                           getDevenvStatus(), stopDevenv(),
│   │                                           snapshotDevenv(force), resizeDevenv(w,h),
│   │                                           connectionUrlsFor()
│   └── app/
│       └── [locale]/
│           └── (authenticated)/
│               └── customer/
│                   └── devenv/
│                       ├── page.tsx        ← list (CustomerGuard + <DevenvListClient />)
│                       └── [devenvId]/
│                           └── page.tsx    ← detail (server: auth() + <DevenvDetailClient />)
└── e2e/
    └── devenv.spec.ts                      ← Playwright: create → connect → resize → stop
```

**Path note:** the route is `/[locale]/customer/devenv` (under the customer
subtree, sibling of `/customer/workspaces`, `/customer/runners`). The route
segment `[devenvId]` is kept for future-proofing + foreign-id defense (the
DO is per-tenant, so a foreign id would 404 at the DO anyway, but the page
short-circuits with `notFound()` to avoid a wasted fetch).

**Cross-WP note:** There is exactly one DevEnv per tenant (WP-08 §3.2 GET handler
returns a 0-or-1 list). The `devenvId` in the detail route is the tenantId
server-inferred; the route segment is kept for future-proofing and 404-on-foreign-id
defense. The API client does NOT pass `devenvId` in any request URL.

### 3.2 Type Definitions (added to `src/lib/customer-types.ts`)

```typescript
// Append to apps/admin-ui/src/lib/customer-types.ts
//
// Aligned byte-for-byte with WP-08 §3.2/§3.3 (OpenAPI). One DevEnv per tenant;
// the server returns 0 or 1 row from `GET /v1/customer/devenv` and uses the
// tenantId (server-inferred) as the `devenv_id`. `DevenvStatus` matches the
// payload from `GET /v1/customer/devenv/status` (see WP-08 §3.2).
//
// Connection URLs are server-issued on create (signed, short-lived) and
// CACHED on the Devenv record so the list-card can render "Open noVNC"
// without a re-fetch. The list endpoint does NOT currently return connection
// URLs — see CROSS-WP § 9.

export type DevenvStatus =
  | "starting"
  | "running"
  | "stopping"
  | "stopped"
  | "errored";

export type DevenvTier = "standard-2" | "standard-4" | "power-8" | "ultra-16";

export interface DevenvPort {
  port: number;       // 6080 (noVNC) | 7681 (ttyd) | 8080 (code-server)
  healthy: boolean;
}

export interface DevenvConnectionUrls {
  /** Server-issued URL for the noVNC WebSocket upgrade. Opaque to the client. */
  vnc_url: string;
  /** Server-issued URL for the ttyd WebSocket upgrade. */
  tty_url: string;
  /** Server-issued URL for the code-server WebSocket upgrade. */
  code_url: string;
}

export interface Devenv {
  devenv_id: string;
  workspace_name: string;
  profile_name: string;
  tier?: DevenvTier;
  status: DevenvStatus;
  created_at: number;       // epoch millis (server-side)
  started_at: number | null;
  ports: number[];
  /** May be undefined on list response (server omits when stopped). */
  connection_urls?: DevenvConnectionUrls;
}

export interface DevenvStatusResponse {
  status: DevenvStatus;
  workspace_name: string;
  profile_name: string;
  tier?: DevenvTier;
  started_at: number | null;
  ports: DevenvPort[];
}

export interface CreateDevenvInput {
  workspace_name: string;
  profile_name?: string; // default "browser-profile" (server side)
  tier?: DevenvTier;     // default "standard-4" (server side)
}

// The container replies 201 with the connection URLs as FLAT top-level fields
// (`vnc_url`, `tty_url`, `code_url` — see WP-08 §4 `DevenvCreated`), not nested
// under `connection_urls`. The client method that wraps this type normalizes
// the flat fields into `Devenv.connection_urls` before handing the row to
// callers, so screens only ever see the nested shape (matches the list
// endpoint). Keeping a single source of truth here prevents the
// `dev.connection_urls.vnc_url` vs `dev.vnc_url` divergence.
export interface CreateDevenvResponse extends Devenv {
  /** Server-issued URL for the noVNC WebSocket upgrade. Wire-only. */
  vnc_url: string;
  /** Server-issued URL for the ttyd WebSocket upgrade. Wire-only. */
  tty_url: string;
  /** Server-issued URL for the code-server WebSocket upgrade. Wire-only. */
  code_url: string;
}

export interface SnapshotResponse {
  ok: true;
  profile_snapshot: { root: string; bytes_total: number };
  workspace_snapshot: { root: string; bytes_total: number };
}

export interface ResizeInput {
  width: number;  // 1..8192
  height: number; // 1..8192
}

export interface DevenvListResponse {
  devenvs: Devenv[]; // length 0 or 1
}
```

### 3.3 API Client (added to `src/lib/customer-client.ts`)

```typescript
// Append to apps/admin-ui/src/lib/customer-client.ts
//
// All methods go through `this.request<T>()` which already handles:
//   - Clerk session token (via `useCustomerClient`)
//   - `NEXT_PUBLIC_CORELINK_API_URL` base URL resolution
//   - E2E mock catch-all bypass (`NEXT_PUBLIC_E2E_TEST_MODE=1` skips token)
//   - JSON content-type + body serialization
//   - `CustomerClientError` on non-2xx
//
// URL contract: aligned with WP-08 §3.2. One DevEnv per tenant — the server
// infers `devenvId` from the session; we never pass it in the path.

import type {
  CreateDevenvInput,
  CreateDevenvResponse,
  Devenv,
  DevenvListResponse,
  DevenvStatusResponse,
  ResizeInput,
  SnapshotResponse,
} from "./customer-types";

export class CustomerClient {
  // ... existing methods unchanged ...

  /** [live → WP-08] List tenant's DevEnvs (0 or 1 row). */
  async listDevenvs(): Promise<Devenv[]> {
    const r = await this.request<DevenvListResponse>("/v1/customer/devenv");
    return r.devenvs;
  }

  /**
   * [live → WP-08] Create a new DevEnv for the calling tenant. The container
   * replies `201 { devenv_id, workspace_name, profile_name, status, vnc_url,
   * tty_url, code_url }` — same envelope-unwrap pattern as `createPat`/
   * `revokePat` (see `customer-client.ts:172-178`). The flat connection URLs
   * on the wire are normalized into `Devenv.connection_urls` before the row
   * is returned, so callers see a single shape (the nested one). Quota 403
   * surfaces as `CustomerClientError(403, …)` — the screen catches and
   * renders the upgrade EmptyState.
   */
  async createDevenv(input: CreateDevenvInput): Promise<CreateDevenvResponse> {
    const raw = await this.request<{
      devenv_id: string;
      workspace_name: string;
      profile_name: string;
      status: Devenv["status"];
      vnc_url: string;
      tty_url: string;
      code_url: string;
    }>("/v1/customer/devenv", {
      method: "POST",
      body: JSON.stringify(input),
    });
    return {
      ...raw,
      connection_urls: {
        vnc_url: raw.vnc_url,
        tty_url: raw.tty_url,
        code_url: raw.code_url,
      },
    };
  }

  /** [live → WP-08] Detailed status (running-time ports, health, etc.). */
  async getDevenvStatus(): Promise<DevenvStatusResponse> {
    return this.request<DevenvStatusResponse>("/v1/customer/devenv/status");
  }

  /** [live → WP-08] Stop and destroy the tenant's DevEnv. */
  async stopDevenv(): Promise<{ ok: true }> {
    return this.request<{ ok: true }>("/v1/customer/devenv", {
      method: "DELETE",
    });
  }

  /** [live → WP-08] Force snapshot of profile + workspace. */
  async snapshotDevenv(force = true): Promise<SnapshotResponse> {
    return this.request<SnapshotResponse>("/v1/customer/devenv/snapshot", {
      method: "POST",
      body: JSON.stringify({ force }),
    });
  }

  /** [live → WP-08] Resize the terminal/desktop viewport. */
  async resizeDevenv(input: ResizeInput): Promise<{ ok: true }> {
    return this.request<{ ok: true }>("/v1/customer/devenv/resize", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  /**
   * Returns the WebSocket connection URLs for the running DevEnv. The list
   * response does NOT include them today; the create response does. Callers
   * that only have a list-row (e.g. a card on `/devenv`) must read
   * `Devenv.connection_urls` (set on a subsequent detail fetch, or — once
   * WP-08 lands the change — on list).
   *
   * When `Devenv.connection_urls` is undefined, this returns `null` and the
   * screen should fall back to opening the detail page where the user can
   * fetch fresh URLs.
   */
  connectionUrlsFor(devenv: Devenv): DevenvConnectionUrls | null {
    if (!devenv.connection_urls) return null;
    const { vnc_url, tty_url, code_url } = devenv.connection_urls;
    if (!vnc_url || !tty_url || !code_url) return null;
    return { vnc_url, tty_url, code_url };
  }
}
```

### 3.4 Custom Hooks (live alongside `src/components/devenv/*.tsx`)

The repo's data-fetching pattern is `useCustomerClient()` + `useState` +
`useEffect` + `useCallback` (mirrors `WorkspacesClient.tsx:46-93`). We follow
that pattern exactly. No `@tanstack/react-query` — the dependency is not in
`package.json`.

```typescript
// apps/admin-ui/src/components/devenv/useDevenvList.ts
"use client";

import * as React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import type { Devenv } from "@/lib/customer-types";

const LIST_REFRESH_MS = 30_000;

/**
 * Fetches the tenant's DevEnv list once on mount and re-fetches every
 * 30s while `enabled` is true. Pause polling on unmount by passing
 * `enabled = false` (used when the screen is not active).
 */
export function useDevenvList(enabled = true): {
  data: Devenv[];
  loading: boolean;
  error: unknown | null;
  reload: () => Promise<void>;
} {
  const client = useCustomerClient();
  const [data, setData] = React.useState<Devenv[]>([]);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<unknown | null>(null);

  const reload = React.useCallback(async () => {
    try {
      const r = await client.listDevenvs();
      setData(r);
      setError(null);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [client]);

  React.useEffect(() => {
    if (!enabled) return;
    void reload();
    const id = window.setInterval(() => void reload(), LIST_REFRESH_MS);
    return () => window.clearInterval(id);
  }, [enabled, reload]);

  return { data, loading, error, reload };
}
```

```typescript
// apps/admin-ui/src/components/devenv/useDevenvStatus.ts
"use client";

import * as React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import type { DevenvStatusResponse } from "@/lib/customer-types";

const STATUS_REFRESH_MS = 10_000;

/**
 * Polls `/v1/customer/devenv/status` every 10s while `enabled`. Same shape
 * as `useDevenvList`; the two could share an implementation but the
 * separate name documents the contract difference (list = 30s, status = 10s).
 */
export function useDevenvStatus(enabled = true): {
  data: DevenvStatusResponse | null;
  loading: boolean;
  error: unknown | null;
  reload: () => Promise<void>;
} {
  const client = useCustomerClient();
  const [data, setData] = React.useState<DevenvStatusResponse | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<unknown | null>(null);

  const reload = React.useCallback(async () => {
    try {
      const r = await client.getDevenvStatus();
      setData(r);
      setError(null);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [client]);

  React.useEffect(() => {
    if (!enabled) return;
    void reload();
    const id = window.setInterval(() => void reload(), STATUS_REFRESH_MS);
    return () => window.clearInterval(id);
  }, [enabled, reload]);

  return { data, loading, error, reload };
}
```

```typescript
// apps/admin-ui/src/components/devenv/useDevenvActions.ts
"use client";

import * as React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import { useToast } from "@/components/ui/linear";
import type {
  CreateDevenvInput,
  CreateDevenvResponse,
  ResizeInput,
  SnapshotResponse,
} from "@/lib/customer-types";
import { CustomerClientError } from "@/lib/customer-client";

/** Returns a `create` callback + `pending` flag. The screen calls `create`
 *  from the modal's submit handler and disables the submit button while
 *  `pending` is true. */
export function useCreateDevenv(): {
  create: (input: CreateDevenvInput) => Promise<CreateDevenvResponse | null>;
  pending: boolean;
} {
  const client = useCustomerClient();
  const { toast } = useToast();
  const [pending, setPending] = React.useState(false);

  const create = React.useCallback(
    async (input: CreateDevenvInput): Promise<CreateDevenvResponse | null> => {
      if (pending) return null;
      setPending(true);
      try {
        const r = await client.createDevenv(input);
        toast({ title: `DevEnv "${r.workspace_name}" created`, tone: "success" });
        return r;
      } catch (e) {
        if (e instanceof CustomerClientError && e.status === 403) {
          toast({
            title: "DevEnv quota reached for your plan",
            description: "Upgrade to add another DevEnv.",
            tone: "danger",
          });
        } else {
          toast({ title: "Couldn't create DevEnv", tone: "danger" });
        }
        return null;
      } finally {
        setPending(false);
      }
    },
    [client, pending, toast],
  );

  return { create, pending };
}

export function useStopDevenv(): {
  stop: () => Promise<boolean>;
  pending: boolean;
} {
  const client = useCustomerClient();
  const { toast } = useToast();
  const [pending, setPending] = React.useState(false);

  const stop = React.useCallback(async (): Promise<boolean> => {
    if (pending) return false;
    setPending(true);
    try {
      await client.stopDevenv();
      toast({ title: "DevEnv stopped", tone: "success" });
      return true;
    } catch {
      toast({ title: "Couldn't stop DevEnv", tone: "danger" });
      return false;
    } finally {
      setPending(false);
    }
  }, [client, pending, toast]);

  return { stop, pending };
}

export function useSnapshotDevenv(): {
  snapshot: (force?: boolean) => Promise<SnapshotResponse | null>;
  pending: boolean;
} {
  const client = useCustomerClient();
  const { toast } = useToast();
  const [pending, setPending] = React.useState(false);

  const snapshot = React.useCallback(
    async (force = true): Promise<SnapshotResponse | null> => {
      if (pending) return null;
      setPending(true);
      try {
        const r = await client.snapshotDevenv(force);
        toast({ title: "Snapshot saved", tone: "success" });
        return r;
      } catch {
        toast({ title: "Snapshot failed", tone: "danger" });
        return null;
      } finally {
        setPending(false);
      }
    },
    [client, pending, toast],
  );

  return { snapshot, pending };
}

export function useResizeDevenv(): {
  resize: (input: ResizeInput) => Promise<boolean>;
  pending: boolean;
} {
  const client = useCustomerClient();
  const { toast } = useToast();
  const [pending, setPending] = React.useState(false);

  const resize = React.useCallback(
    async (input: ResizeInput): Promise<boolean> => {
      if (pending) return false;
      setPending(true);
      try {
        await client.resizeDevenv(input);
        toast({ title: "Viewport resized", tone: "success" });
        return true;
      } catch {
        toast({ title: "Resize failed", tone: "danger" });
        return false;
      } finally {
        setPending(false);
      }
    },
    [client, pending, toast],
  );

  return { resize, pending };
}
```

### 3.5 Key Components (all use Linear kit primitives from `@/components/ui/linear`)

```tsx
// apps/admin-ui/src/components/devenv/StatusBadge.tsx
"use client";

import { Badge } from "@/components/ui/linear";
import type { DevenvStatus } from "@/lib/customer-types";

const STATUS_TONE: Record<DevenvStatus, "neutral" | "success" | "warn" | "danger"> = {
  starting: "warn",
  running: "success",
  stopping: "warn",
  stopped: "neutral",
  errored: "danger",
};

const STATUS_LABEL: Record<DevenvStatus, string> = {
  starting: "Starting (Waking up...)",
  running: "Running",
  stopping: "Stopping",
  stopped: "Stopped",
  errored: "Error",
};

export function StatusBadge({ status }: { status: DevenvStatus }) {
  return (
    <Badge tone={STATUS_TONE[status]} dot>
      {STATUS_LABEL[status]}
    </Badge>
  );
}
```

```tsx
// apps/admin-ui/src/components/devenv/ConnectionButtons.tsx
//
// Auth model: the create / detail response carries server-issued, short-lived
// signed URLs for the three WebSocket upgrades (noVNC, ttyd, code-server).
// `window.open(url, "_blank", "noopener,noreferrer")` opens the URL in a
// fresh tab; the upgrade URL embeds the auth, so the new tab does NOT need
// a `window.opener` token.
//
// If `urls` is null (list response without server-issued URLs), we render
// a single "Open detail" Button that navigates to `/devenv/:id` where the
// status poll will fetch the URLs.
//
// The screen must NOT fall back to a hard-coded path like
// `/v1/customer/devenv/vnc` — that route is auth-gated and the browser
// cannot supply the Clerk bearer on the upgrade. (See H4 in the review.)
"use client";

import { Button, Tooltip } from "@/components/ui/linear";
import { useRouter } from "next/navigation";
import type { DevenvConnectionUrls, DevenvStatus } from "@/lib/customer-types";

interface ConnectionButtonsProps {
  status: DevenvStatus;
  urls: DevenvConnectionUrls | null;
  devenvId: string;
}

function openInNewTab(url: string): void {
  // `noopener,noreferrer` blocks the new tab from accessing `window.opener`
  // and strips the Referer header. Target name is a unique non-`_`-prefixed
  // string so each channel lands in its own tab (a `_blank_*` target is
  // reserved and silently collapses to `_blank`).
  window.open(url, `devenv_conn_${Date.now()}`, "noopener,noreferrer");
}

export function ConnectionButtons({ status, urls, devenvId }: ConnectionButtonsProps) {
  const router = useRouter();
  const isRunning = status === "running";
  const cta = isRunning ? "Open" : "Not running";

  if (!urls) {
    return (
      <Button variant="ghost" onClick={() => router.push(`/customer/devenv/${devenvId}`)}>
        Open detail to fetch connection URLs
      </Button>
    );
  }

  return (
    <div className="flex flex-col gap-2 sm:flex-row sm:flex-wrap">
      <Tooltip label={isRunning ? "Open noVNC desktop in a new tab" : "DevEnv is not running"}>
        <Button
          variant="primary"
          disabled={!isRunning}
          onClick={() => openInNewTab(urls.vnc_url)}
        >
          noVNC Desktop
        </Button>
      </Tooltip>
      <Tooltip label={isRunning ? "Open ttyd terminal in a new tab" : "DevEnv is not running"}>
        <Button
          variant="ghost"
          disabled={!isRunning}
          onClick={() => openInNewTab(urls.tty_url)}
        >
          Terminal (ttyd)
        </Button>
      </Tooltip>
      <Tooltip label={isRunning ? "Open code-server in a new tab" : "DevEnv is not running"}>
        <Button
          variant="ghost"
          disabled={!isRunning}
          onClick={() => openInNewTab(urls.code_url)}
        >
          VS Code (code-server)
        </Button>
      </Tooltip>
    </div>
  );
}
```

```tsx
// apps/admin-ui/src/components/devenv/ResizeControls.tsx
"use client";

import * as React from "react";
import { Button, Field, Input } from "@/components/ui/linear";
import { useResizeDevenv } from "./useDevenvActions";
import type { DevenvStatus } from "@/lib/customer-types";

const MIN_DIM = 1;
const MAX_DIM = 8192;
const PRESETS: Array<{ label: string; width: number; height: number }> = [
  { label: "Mobile", width: 375, height: 667 },
  { label: "Tablet", width: 768, height: 1024 },
  { label: "Laptop", width: 1366, height: 768 },
  { label: "Desktop", width: 1920, height: 1080 },
  { label: "4K", width: 3840, height: 2160 },
];

function clamp(n: number): number {
  if (!Number.isFinite(n)) return MIN_DIM;
  return Math.min(MAX_DIM, Math.max(MIN_DIM, Math.trunc(n)));
}

export function ResizeControls({ status }: { status: DevenvStatus }) {
  const [width, setWidth] = React.useState(1920);
  const [height, setHeight] = React.useState(1080);
  const { resize, pending } = useResizeDevenv();
  const isRunning = status === "running";

  return (
    <div className="space-y-4 rounded-lg border border-slate-200 bg-slate-50 p-4">
      <h3 className="text-base font-medium text-slate-900">Resize Viewport</h3>

      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field label="Width" htmlFor="devenv-resize-w">
          <Input
            id="devenv-resize-w"
            type="number"
            min={MIN_DIM}
            max={MAX_DIM}
            value={width}
            onChange={(e) => setWidth(clamp(Number(e.target.value)))}
          />
        </Field>
        <Field label="Height" htmlFor="devenv-resize-h">
          <Input
            id="devenv-resize-h"
            type="number"
            min={MIN_DIM}
            max={MAX_DIM}
            value={height}
            onChange={(e) => setHeight(clamp(Number(e.target.value)))}
          />
        </Field>
      </div>

      <div className="flex flex-wrap gap-2">
        {PRESETS.map((p) => (
          <Button
            key={p.label}
            variant="ghost"
            size="sm"
            onClick={() => {
              setWidth(p.width);
              setHeight(p.height);
            }}
          >
            <span className="font-medium">{p.label}</span>
            <span className="ml-2 text-xs text-slate-500">
              {p.width}×{p.height}
            </span>
          </Button>
        ))}
      </div>

      <Button
        variant="primary"
        onClick={() => void resize({ width, height })}
        disabled={!isRunning || pending}
        className="w-full"
      >
        {pending ? "Applying…" : "Apply Resize"}
      </Button>
    </div>
  );
}
```

```tsx
// apps/admin-ui/src/components/devenv/CreateDevenvModal.tsx
"use client";

import * as React from "react";
import { Button, Field, Input, Modal } from "@/components/ui/linear";
import { useCreateDevenv } from "./useDevenvActions";
import type { DevenvTier } from "@/lib/customer-types";

const NAME_RE = /^[a-zA-Z0-9_-]+$/;

interface CreateDevenvModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated?: (devenvId: string) => void;
}

export function CreateDevenvModal({ open, onOpenChange, onCreated }: CreateDevenvModalProps) {
  const [workspaceName, setWorkspaceName] = React.useState("");
  const [profileName, setProfileName] = React.useState("browser-profile");
  const [tier, setTier] = React.useState<DevenvTier>("standard-4");
  const { create, pending } = useCreateDevenv();
  const [validationError, setValidationError] = React.useState<string | null>(null);

  function reset(): void {
    setWorkspaceName("");
    setProfileName("browser-profile");
    setTier("standard-4");
    setValidationError(null);
  }

  async function onSubmit(e: React.FormEvent): Promise<void> {
    e.preventDefault();
    const name = workspaceName.trim();
    if (!name) {
      setValidationError("Workspace name is required");
      return;
    }
    if (name.length > 128) {
      setValidationError("Workspace name must be 128 characters or fewer");
      return;
    }
    if (!NAME_RE.test(name)) {
      setValidationError("Use letters, digits, underscore, or hyphen only");
      return;
    }
    setValidationError(null);
    const r = await create({
      workspace_name: name,
      profile_name: profileName.trim() || undefined,
      tier,
    });
    if (r) {
      onCreated?.(r.devenv_id);
      reset();
      onOpenChange(false);
    }
  }

  return (
    <Modal
      open={open}
      onClose={() => {
        reset();
        onOpenChange(false);
      }}
      title="Create New DevEnv Sandbox"
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="secondary" onClick={() => onOpenChange(false)} disabled={pending}>
            Cancel
          </Button>
          <Button variant="primary" onClick={onSubmit} disabled={pending}>
            {pending ? "Creating…" : "Create Sandbox"}
          </Button>
        </div>
      }
    >
      <form onSubmit={onSubmit} className="space-y-4">
        <div>
          <Field label="Workspace name" htmlFor="devenv-create-name" required>
            <Input
              id="devenv-create-name"
              value={workspaceName}
              onChange={(e) => setWorkspaceName(e.target.value)}
              placeholder="my-project"
              autoFocus
              maxLength={128}
              required
            />
          </Field>
          {validationError != null ? (
            <div role="alert" className="mt-1 text-sm text-red-600">
              {validationError}
            </div>
          ) : null}
        </div>
        <Field label="Profile name" htmlFor="devenv-create-profile" help="Optional. Defaults to browser-profile.">
          <Input
            id="devenv-create-profile"
            value={profileName}
            onChange={(e) => setProfileName(e.target.value)}
            maxLength={128}
          />
        </Field>
        <Field label="Hardware Tier" htmlFor="devenv-create-tier" help="Scale-to-Infinity capacity. Billed per vCPU-second.">
          <select
            id="devenv-create-tier"
            className="w-full rounded border border-gray-700 bg-gray-900 px-3 py-2 text-sm text-white"
            value={tier}
            onChange={(e) => setTier(e.target.value as DevenvTier)}
          >
            <option value="standard-2">Standard (2 vCPU, 4 GB RAM)</option>
            <option value="standard-4">Standard (4 vCPU, 8 GB RAM) — Default</option>
            <option value="power-8">Power (8 vCPU, 16 GB RAM)</option>
            <option value="ultra-16">Ultra (16 vCPU, 32 GB RAM)</option>
          </select>
        </Field>
      </form>
    </Modal>
  );
}

```tsx
// apps/admin-ui/src/components/devenv/ConfirmStopDialog.tsx
"use client";

import { Button, ConfirmDialog } from "@/components/ui/linear";
import type { Devenv } from "@/lib/customer-types";

interface ConfirmStopDialogProps {
  devenv: Devenv | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
  pending: boolean;
}

export function ConfirmStopDialog({ devenv, onOpenChange, onConfirm, pending }: ConfirmStopDialogProps) {
  return (
    <ConfirmDialog
      open={devenv !== null}
      onClose={() => onOpenChange(false)}
      title="Stop this DevEnv?"
      body="The container will be terminated. Unsaved work in the terminal or editor will be lost. Snapshots are taken automatically on stop."
      confirmLabel={pending ? "Stopping…" : "Stop DevEnv"}
      onConfirm={onConfirm}
      danger
    />
  );
}
```

```tsx
// apps/admin-ui/src/components/devenv/DevenvSkeleton.tsx
"use client";

import { Card, Skeleton } from "@/components/ui/linear";

// Note: kit `Skeleton` accepts `rows` (count) and `width` (last-row width
// string), NOT `className`. Layout is achieved with wrapper flex/grid + a
// per-Skeleton `width` prop.
export function DevenvSkeleton() {
  return (
    <Card>
      <div className="space-y-3">
        <Skeleton rows={1} width="40%" />
        <Skeleton rows={1} width="64%" />
        <div className="flex gap-2 pt-2">
          <Skeleton rows={1} width="120px" />
          <Skeleton rows={1} width="120px" />
          <Skeleton rows={1} width="120px" />
        </div>
      </div>
    </Card>
  );
}
```

### 3.6 Screens (Next 15 App Router under `[locale]/(authenticated)/`)

```tsx
// apps/admin-ui/src/components/devenv/DevenvListClient.tsx
"use client";

import * as React from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  Callout,
  Card,
  EmptyState,
  InlineError,
  ToastProvider,
} from "@/components/ui/linear";
import { useDevenvList } from "./useDevenvList";
import { CreateDevenvModal } from "./CreateDevenvModal";
import { DevenvSkeleton } from "./DevenvSkeleton";
import { StatusBadge } from "./StatusBadge";
import { ConnectionButtons } from "./ConnectionButtons";
import { CustomerClientError } from "@/lib/customer-client";
import type { Devenv } from "@/lib/customer-types";

function formatTimestamp(ms: number | null): string {
  if (ms == null) return "—";
  return new Date(ms).toLocaleString();
}

function DevenvListInner(): React.ReactElement {
  const router = useRouter();
  const { data, loading, error, reload } = useDevenvList(true);
  const [showCreate, setShowCreate] = React.useState(false);

  if (loading) {
    return (
      <div className="space-y-3">
        <DevenvSkeleton />
        <DevenvSkeleton />
      </div>
    );
  }

  if (error) {
    return (
      <InlineError
        error={error}
        onRetry={() => void reload()}
        message={
          error instanceof CustomerClientError && error.status === 403
            ? "DevEnv is not available on your current plan."
            : "Couldn't load your DevEnv."
        }
      />
    );
  }

  if (data.length === 0) {
    return (
      <>
        <EmptyState
          title="No DevEnv yet"
          description="Spin up a persistent cloud environment with browser, terminal, and editor."
          cta={
            <Button variant="primary" onClick={() => setShowCreate(true)}>
              Create your first DevEnv
            </Button>
          }
        />
        <CreateDevenvModal
          open={showCreate}
          onOpenChange={setShowCreate}
          onCreated={(id) => router.push(`/customer/devenv/${id}`)}
        />
      </>
    );
  }

  const devenv = data[0];
  return (
    <div className="space-y-4">
      <Card>
        <div className="space-y-4">
          <div className="flex items-start justify-between gap-4">
            <div>
              <h2 className="text-xl font-semibold text-slate-900">
                {devenv.workspace_name}
              </h2>
              <p className="text-sm text-slate-600">
                Profile: <span className="font-mono">{devenv.profile_name}</span>
              </p>
              <p className="text-xs text-slate-500">
                Started: {formatTimestamp(devenv.started_at)}
              </p>
            </div>
            <StatusBadge status={devenv.status} />
          </div>

          <ConnectionButtons
            status={devenv.status}
            urls={devenv.connection_urls ?? null}
            devenvId={devenv.devenv_id}
          />

          <div className="flex flex-wrap gap-2">
            <Button
              variant="ghost"
              onClick={() => router.push(`/customer/devenv/${devenv.devenv_id}`)}
            >
              Open detail
            </Button>
            <Button variant="ghost" onClick={() => void reload()}>
              Refresh
            </Button>
          </div>
        </div>
      </Card>

      <Callout tone="warn" title="Polling active">
        This view auto-refreshes every 30 seconds while open. Click <em>Open detail</em> for live
        status, resize, snapshot, and stop.
      </Callout>

      <CreateDevenvModal
        open={showCreate}
        onOpenChange={setShowCreate}
        onCreated={(id) => router.push(`/customer/devenv/${id}`)}
      />
    </div>
  );
}

/** Public entry — wraps the screen in a local ToastProvider so `useToast` works
 *  (mirrors the pattern in `WorkspacesClient.tsx:354-…`). */
export function DevenvListClient(): React.ReactElement {
  return (
    <ToastProvider>
      <DevenvListInner />
    </ToastProvider>
  );
}
```

```tsx
// apps/admin-ui/src/app/[locale]/(authenticated)/customer/devenv/page.tsx
// The DevEnv surface lives under the customer dashboard tree (matches the
// pattern of /customer/workspaces, /customer/runners, etc.).
import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import { DevenvListClient } from "@/components/devenv/DevenvListClient";

export default function DevenvPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-devenv-heading">
        <h1 id="customer-devenv-heading">DevEnvironments</h1>
        <p>
          Persistent cloud development environments with browser, terminal, and editor.
        </p>
        <DevenvListClient />
      </main>
    </CustomerGuard>
  );
}
```

```tsx
// apps/admin-ui/src/components/devenv/DevenvDetailClient.tsx
"use client";

import * as React from "react";
import { useRouter } from "next/navigation";
import {
  Button,
  Callout,
  Card,
  ConfirmDialog,
  InlineError,
  Stat,
  ToastProvider,
} from "@/components/ui/linear";
import { useDevenvStatus } from "./useDevenvStatus";
import { useStopDevenv, useSnapshotDevenv } from "./useDevenvActions";
import { ConnectionButtons } from "./ConnectionButtons";
import { ResizeControls } from "./ResizeControls";
import { StatusBadge } from "./StatusBadge";
import type { Devenv } from "@/lib/customer-types";

function DevenvDetailInner({ initial }: { initial: Devenv }): React.ReactElement {
  const router = useRouter();
  const { data, loading, error, reload } = useDevenvStatus(true);
  const { stop, pending: stopping } = useStopDevenv();
  const { snapshot, pending: snapshotting } = useSnapshotDevenv();
  const [confirmStop, setConfirmStop] = React.useState(false);

  const status = data?.status ?? initial.status;
  const workspaceName = data?.workspace_name ?? initial.workspace_name;

  async function onConfirmStop(): Promise<void> {
    const ok = await stop();
    if (ok) {
      setConfirmStop(false);
      router.push("/customer/devenv");
    }
  }

  return (
    <div className="space-y-6">
      <header className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <h1 className="text-3xl font-bold text-slate-900">{workspaceName}</h1>
          <p className="text-slate-600">
            Profile: <span className="font-mono">{initial.profile_name}</span>
          </p>
        </div>
        <div className="flex items-center gap-3">
          <StatusBadge status={status} />
          <Button
            variant="ghost"
            onClick={() => void reload()}
            disabled={loading}
          >
            {loading ? "Refreshing…" : "Refresh"}
          </Button>
          <Button
            variant="danger"
            onClick={() => setConfirmStop(true)}
            disabled={stopping || status === "stopped"}
          >
            Stop DevEnv
          </Button>
        </div>
      </header>

      {error && (
        <InlineError
          error={error}
          onRetry={() => void reload()}
          message="Couldn't refresh DevEnv status."
        />
      )}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <Card>
          <Stat label="Status" value={status} />
        </Card>
        <Card>
          <Stat
            label="Open ports"
            value={
              data?.ports
                ?.filter((p) => p.healthy)
                .map((p) => p.port)
                .join(", ") ?? "—"
            }
          />
        </Card>
        <Card>
          <Stat
            label="Started"
            value={
              data?.started_at
                ? new Date(data.started_at).toLocaleString()
                : "—"
            }
          />
        </Card>
      </div>

      <Card>
        <div className="space-y-4">
          <h2 className="text-lg font-semibold text-slate-900">Connections</h2>
          <p className="text-sm text-slate-600">
            Opens the noVNC desktop, ttyd terminal, or code-server editor in a new tab using a
            server-issued, short-lived signed URL.
          </p>
          <ConnectionButtons
            status={status}
            urls={initial.connection_urls ?? null}
            devenvId={initial.devenv_id}
          />
        </div>
      </Card>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        <ResizeControls status={status} />
        <Card>
          <div className="space-y-4">
            <h2 className="text-lg font-semibold text-slate-900">Snapshot</h2>
            <p className="text-sm text-slate-600">
              Force a snapshot of the browser profile and workspace. Snapshots also run
              automatically on stop and every 6 hours while running.
            </p>
            <Button
              variant="primary"
              onClick={() => void snapshot(true)}
              disabled={snapshotting || status !== "running"}
            >
              {snapshotting ? "Snapshotting…" : "Snapshot now"}
            </Button>
          </div>
        </Card>
      </div>

      <Callout tone="neutral" title="Detail view auto-refreshes every 10 seconds">
        The status, ports, and connection availability reflect the latest container health
        check.
      </Callout>

      <ConfirmDialog
        open={confirmStop}
        onClose={() => setConfirmStop(false)}
        title="Stop this DevEnv?"
        body="The container will be terminated. Unsaved work in the terminal or editor will be lost. Snapshots are taken automatically on stop."
        confirmLabel={stopping ? "Stopping…" : "Stop DevEnv"}
        onConfirm={() => void onConfirmStop()}
        danger
      />
    </div>
  );
}

export function DevenvDetailClient({ initial }: { initial: Devenv }): React.ReactElement {
  return (
    <ToastProvider>
      <DevenvDetailInner initial={initial} />
    </ToastProvider>
  );
}
```

```tsx
// apps/admin-ui/src/app/[locale]/(authenticated)/customer/devenv/[devenvId]/page.tsx
//
// Server component — fetches the initial DevEnv row (so the detail renders
// without a client-side waterfall) and hands it to the client component.
// Mirrors the real auth pattern from app/[locale]/upgrade/page.tsx:65-76.
import { notFound, redirect } from "next/navigation";
import { auth } from "@clerk/nextjs/server";
import { CustomerClient } from "@/lib/customer-client";
import { DevenvDetailClient } from "@/components/devenv/DevenvDetailClient";
import type { Devenv } from "@/lib/customer-types";

export default async function DevenvDetailPage(props: {
  params: Promise<{ locale: string; devenvId: string }>;
}): Promise<React.ReactElement> {
  const { locale, devenvId } = await props.params;
  const session = await auth();
  if (!session?.userId) redirect(`/${locale}/sign-in`);

  // Per WP-08 §3.2 the devenv_id equals the tenant id. The Clerk session
  // resolves to the orgId for the multi-tenant case or the userId for the
  // self-serve solo-tenant case — same pattern as customer-types / layout.
  // Foreign ids are rejected fail-closed (the DO is per-tenant anyway, so
  // this is defense-in-depth against probing across tenants).
  const tenantId = session.orgId ?? session.userId;
  if (devenvId !== tenantId) notFound();

  const client = new CustomerClient({
    getToken: () => Promise.resolve(session.getToken()),
  });
  let initial: Devenv;
  try {
    const list = await client.listDevenvs();
    initial = list.find((d) => d.devenv_id === devenvId) ?? list[0];
    if (!initial) notFound();
  } catch {
    notFound();
  }

  return <DevenvDetailClient initial={initial} />;
}
```

---

## 4. Acceptance Criteria (DoD)

| # | Criterion | Verification Method |
|---|-----------|---------------------|
| 1 | DevEnv list loads and displays status | Page loads → shows empty state or DevEnv card |
| 2 | Create modal validates workspace name | Invalid name → inline error; valid → submits |
| 3 | Create DevEnv shows loading then success | Modal → loading spinner → success toast → list refreshes |
| 4 | DevEnv card shows real-time status | Status badge updates via 30s list polling + 10s detail polling |
| 5 | Connection buttons open correct URLs | Click → new tab with server-issued signed WebSocket URL |
| 6 | Buttons disabled when not running | `disabled` + `Tooltip` explaining why |
| 7 | Resize controls apply dimensions | Apply → API call → success toast |
| 8 | Status badge tones match Linear kit | Running=success, Starting=warn, Stopped=neutral, Error=danger |
| 9 | Empty state shows helpful message | No DevEnvs → `EmptyState` with "Create your first DevEnv" CTA |
| 10 | Error states handled gracefully | Network error → `InlineError` + retry button + `toast({ tone: "danger" })` |
| 11 | Mobile responsive | `flex-col sm:flex-row`; `grid-cols-1 lg:grid-cols-2`; tested at 375 / 768 / 1366 px |
| 12 | Stop is gated by ConfirmDialog | Destructive action requires explicit confirm |
| 13 | Quota 403 surfaces upgrade guidance | 403 → toast + EmptyState on list; link to `/upgrade?plan=…` |
| 14 | All data flows through `useCustomerClient` | No raw `fetch`; no invented `apiClient` |

---

## 5. Invariants

| Invariant | Description |
|-----------|-------------|
| **I1** | All API calls include the Clerk bearer (`useCustomerClient` → `CustomerClient.request`) |
| **I2** | Polling interval 10s for detail, 30s for list; `setInterval` cleared on unmount |
| **I3** | Connection buttons only enabled when `status === "running"`; tooltip explains otherwise |
| **I4** | Resize inputs clamped to 1-8192 via the `clamp()` helper |
| **I5** | Modal closes on successful create (`onOpenChange(false)`) and resets state |
| **I6** | Errors surface as `toast({ tone: "danger" })` + `InlineError` with `onRetry`; no `toast.error` (not in the kit) |
| **I7** | Stop is gated by `ConfirmDialog`; never a bare Button with `onClick={stop}` |
| **I8** | `Devenv.connection_urls` may be absent on list; the UI must fall back to "Open detail" rather than inventing a URL |

---

## 5. Quality Standards (SOTA)

| Standard | Requirement |
|----------|-------------|
| **Type Safety** | Full TypeScript; no `any`; `CustomerClientError` for all error handling |
| **Accessibility** | `Field`/`Input` for label binding; `aria-label` on icon-only / action buttons; `Tooltip` explains disabled state; `Modal` (Radix) handles focus trap |
| **Performance** | `useCustomerClient` latest-ref pattern (no churn); `useCallback` for `reload`; polling cleared on unmount |
| **Error Handling** | `toast({ title, tone: "danger" })` for transient errors; `InlineError` with `onRetry` for inline errors; `EmptyState` for no-data |
| **Responsive** | Mobile-first; `flex-col sm:flex-row`; `grid-cols-1 lg:grid-cols-2`; no fixed widths above `sm` |
| **Data Layer** | All calls go through `CustomerClient`; never raw `fetch`; never invent a new client |
| **Iconography** | Inline SVG (matches `CustomerNav.tsx:23-26`) or text labels — do NOT import `lucide-react` (not in `package.json`) |

---

## 6. Completeness Checklist

- [ ] `DevenvList` + `DevenvListClient` components with `Skeleton` loading + `EmptyState`
- [ ] `CreateDevenvModal` with `Field` + `Input` + inline validation (NAME_RE, max 128)
- [ ] `DevenvDetail` + `DevenvDetailClient` with 10s status polling
- [ ] `ConnectionButtons` with `Tooltip` + `disabled` per status, server-issued URLs only
- [ ] `ResizeControls` with 5 presets + custom `clamp()` + `Apply Resize` Button
- [ ] `StatusBadge` with Linear `Badge` `tone` mapping (running=success, errored=danger, …)
- [ ] `ConfirmStopDialog` wrapping `ConfirmDialog` with `danger`
- [ ] `DevenvSkeleton` using Linear `Skeleton`
- [ ] `useDevenvList`, `useDevenvStatus`, `useDevenvActions` hooks (no React Query)
- [ ] `CustomerClient` extended with `listDevenvs` / `createDevenv` / `getDevenvStatus` / `stopDevenv` / `snapshotDevenv` / `resizeDevenv` / `connectionUrlsFor`
- [ ] `customer-types.ts` extended with `Devenv`, `DevenvStatus`, `DevenvStatusResponse`, `CreateDevenvInput`, `CreateDevenvResponse`, `SnapshotResponse`, `ResizeInput`, `DevenvListResponse`, `DevenvConnectionUrls`, `DevenvPort`
- [ ] Routing: `/devenv` (list) + `/devenv/[devenvId]` (detail), via Next App Router under `[locale]/(authenticated)/`
- [ ] Unit tests: `apps/admin-ui/src/components/devenv/DevenvList.test.tsx` (renders empty + 1-card),
  `apps/admin-ui/src/components/devenv/StatusBadge.test.tsx` (tone per status),
  `apps/admin-ui/src/components/devenv/useDevenvList.test.ts` (calls `listDevenvs` once on mount,
  re-fetches on interval, surfaces `CustomerClientError` to `error`),
  `apps/admin-ui/src/lib/customer-client.test.ts` (verifies `listDevenvs` unwraps the
  `{ devenvs: [...] }` envelope, mirroring the `createPat` envelope-unwrap pattern)
- [ ] Playwright E2E: `apps/admin-ui/e2e/devenv.spec.ts` (create → detail → resize → stop),
  backed by mock fixtures added to `apps/admin-ui/src/lib/e2e-mock-fixtures.ts`
  (`mockDevenvs`, `mockDevenvStatus`) and registered in the catch-all route
- [ ] Accessibility audit (axe-core via `pnpm test:a11y`)
- [ ] Code review completed by Frontend TL
- [ ] TechLead sign-off after Iteration-2 review passes

---

## 6. Self-Check Points (Agent Evaluation)

### Self-Check 1: Real-Time Updates
> **Question:** Does the UI reflect real DevEnv state changes?
> 
> **Verification:**
> - [ ] List polls every 30s; detail polls every 10s
> - [ ] Status badge updates without full page reload
> - [ ] Connection buttons enable/disable based on status
> - [ ] Manual refresh button available

### Self-Check 2: WebSocket Connection UX
> **Question:** Is connecting to DevEnv seamless?
> 
> **Verification:**
> - [ ] Buttons open in new tab via `window.open(url, "devenv_conn_<ts>", "noopener,noreferrer")` (unique target name, not reserved `_blank_*`)
> - [ ] URLs are server-issued on create; list-card falls back to "Open detail" when `connection_urls` is absent
> - [ ] Disabled state with `Tooltip` when not running (`isRunning` check)
> - [ ] `aria-label` describes target on every Button (e.g. "Open noVNC desktop in a new tab")

### Self-Check 3: Error Handling Completeness
> **Question:** Are all error paths handled gracefully?
> 
> **Verification:**
> - [ ] API errors → `toast({ title, tone: "danger" })` + `InlineError` with `onRetry`
> - [ ] Network errors → `InlineError` banner + `reload()` on retry; no manual polling reset needed
> - [ ] Validation errors → `Field` `error` prop shows inline message
> - [ ] 403 quota → `CustomerClientError(403)` → `toast` + `EmptyState` with upgrade CTA
> - [ ] Stop is destructive → `ConfirmDialog` with `danger` gates the action

---

## 8. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| WebSocket auth fails in a new tab (Clerk bearer cannot be sent by `window.open`) | High | High | Server-issued, short-lived signed URLs on create + detail; list-card falls back to "Open detail" if absent. See H4. |
| Polling too aggressive (10s/30s on every open screen) | Low | Medium | `setInterval` cleared on unmount; `enabled` flag to pause when screen is not active |
| Mobile layout broken | Medium | Medium | `flex-col sm:flex-row`; `grid-cols-1 lg:grid-cols-2`; Playwright run at 375 / 768 / 1366 px |
| Auth token expiry during long session | Low | Medium | `useCustomerClient` always reads the freshest `getToken`; toast on 401 redirects to re-auth |
| `lucide-react` accidentally added to dependencies | Medium | Low | The repo's existing convention is inline SVGs; PR-review guard against adding the dep |
| Quota race: 403 surfaces as a generic toast | Medium | Medium | 403 in `useCreateDevenv` / `useDevenvList` → EmptyState / toast with `/upgrade?plan=…` link |
| `connection_urls` absent on list response | High | Medium | UI handles `urls === null` by rendering an "Open detail" Button; cross-WP ask for WP-08 to include URLs on list |

---

## 7. Sign-Off

| Role | Name | Signature | Date |
|------|------|-----------|------|
| Author | | | |
| Reviewer (Frontend TL) | | | |
| QA (E2E owner) | | | |
| Approver (TechLead) | | | |

---

## 9. Cross-WP Coordination

| Need | Direction | Action |
|------|-----------|--------|
| Drop `devenvId` from URL paths (one DevEnv per tenant) | WP-08 → WP-09 | Confirmed: WP-09 never passes `devenvId`; WP-08 routes are tenant-inferred |
| Include `connection_urls` in the LIST response | WP-08 ← WP-09 | **REQUEST:** add `vnc`/`tty`/`code` to `Devenv` (signed, short-lived) so list-cards can render "Open noVNC" without a detail fetch |
| DevenvStatusResponse field set | WP-08 → WP-09 | WP-09 uses only fields WP-08 §3.3 defines; `uptime_ms` / `container_alive` / `ws_connections` / `health_check_failures` / `last_check_at` are NOT used |
| 403 quota upgrade link | WP-07 → WP-09 | WP-09 uses `/upgrade?plan=devenv_pro` (TBD by WP-07 / pricing) |
| Toast auto-dismiss | Linear kit → WP-09 | **REQUEST:** confirm/extend kit to auto-dismiss at 5s; today the kit has no auto-dismiss (claim removed from I4) |
| Iconography | All | Inline SVG only; do NOT add `lucide-react` |

---

**END OF WP-09**