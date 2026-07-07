/**
 * Backend `/v1/*` API mocks — WI-S16-007.
 *
 * Tests are FE-only. Backend handlers live in `corelink-go/` + workers and
 * are validated by their own integration tests (S-03 / S-09 / S-10 / S-11 /
 * S-13). Here we just need deterministic, schema-shaped responses so the UI
 * renders happy / sad paths.
 *
 * Usage:
 *   ```ts
 *   import { installApiMocks } from "../fixtures/api-mocks";
 *   test.beforeEach(async ({ page }) => { await installApiMocks(page); });
 *   ```
 */

import type { Page, Route } from "@playwright/test";

interface RouteState {
  /** consents granted in-memory so withdraw test can find them */
  consents: Array<{
    consent_id: string;
    purpose: string;
    locale: string;
    granted_at: string;
    withdrawn_at?: string;
  }>;
  /** DSR requests submitted */
  dsrRequests: Array<{
    request_id: string;
    action: string;
    submitted_at: string;
    sla_due_at: string;
    status: "pending" | "processing" | "completed";
  }>;
  /** ops awaiting dual-approval */
  pendingOps: Array<{
    op_id: string;
    type: string;
    submitted_by: string;
    submitted_at: string;
    approved_by?: string;
    status: "pending" | "approved" | "rejected" | "executed";
  }>;
}

function freshState(): RouteState {
  return {
    consents: [
      {
        consent_id: "c_existing_1",
        purpose: "analytics",
        locale: "en",
        granted_at: "2026-05-01T10:00:00Z",
      },
    ],
    dsrRequests: [],
    pendingOps: [
      {
        op_id: "op_pending_1",
        type: "tenant_delete",
        submitted_by: "user_e2e_newdev",
        submitted_at: "2026-05-13T09:00:00Z",
        status: "pending",
      },
    ],
  };
}

function makeRfc7807(status: number, title: string, detail: string) {
  return {
    type: "about:blank",
    title,
    status,
    detail,
  };
}

function makeReceipt(payload: Record<string, unknown>): string {
  // Synthetic JWT: header.payload.signature — base64url segments. The UI
  // only verifies non-emptiness + decodes payload for display.
  const header = Buffer.from(JSON.stringify({ alg: "ES256", typ: "JWT" })).toString("base64url");
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  const sig = Buffer.from("e2e-stub-signature").toString("base64url");
  return `${header}.${body}.${sig}`;
}

export async function installApiMocks(page: Page): Promise<void> {
  const state = freshState();

  await page.route(/\/v1\/.*/, async (route: Route) => {
    const req = route.request();
    const url = new URL(req.url());
    // Normalize away the `/api` base-URL prefix the customer client prepends
    // (customer-client resolveBaseUrl → "/api"), so both `/api/v1/*` and bare
    // `/v1/*` requests match the same branches.
    const path = url.pathname.replace(/^\/api/, "");
    const method = req.method();

    // ----- Tenants -----
    if (path === "/v1/tenants" && method === "POST") {
      const body = req.postDataJSON() as { name?: string } | null;
      return route.fulfill({
        status: 201,
        contentType: "application/json",
        body: JSON.stringify({
          tenant_id: "tenant_new_e2e",
          name: body?.name ?? "E2E Tenant",
          region: "us-east-1",
          created_at: new Date().toISOString(),
        }),
      });
    }

    // ----- PAT lifecycle -----
    if (path === "/v1/pats" && method === "POST") {
      return route.fulfill({
        status: 201,
        contentType: "application/json",
        body: JSON.stringify({
          pat_id: "pat_e2e_001",
          // PAT shown once — CTRL-CRED-001 reflection.
          token: "corelink_pat_live_E2E_DO_NOT_LEAK_abcdef0123456789",
          scope: "read-write",
          expires_at: "2027-05-14T00:00:00Z",
        }),
      });
    }
    if (path === "/v1/pats" && method === "GET") {
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          items: [
            {
              pat_id: "pat_e2e_001",
              name: "first-pat",
              scope: "read-write",
              last_used: null,
              expires_at: "2027-05-14T00:00:00Z",
            },
          ],
          cursor: null,
        }),
      });
    }

    // ----- Consent -----
    if (path === "/v1/consent/grant" && method === "POST") {
      const body = req.postDataJSON() as Record<string, unknown> | null;
      const consent_id = `c_e2e_${state.consents.length + 1}`;
      state.consents.push({
        consent_id,
        purpose: String(body?.["purpose"] ?? "unknown"),
        locale: String(body?.["locale"] ?? "en"),
        granted_at: new Date().toISOString(),
      });
      return route.fulfill({
        status: 201,
        contentType: "application/json",
        body: JSON.stringify({
          consent_id,
          receipt_jwt: makeReceipt({
            sub: "user_e2e",
            consent_id,
            purpose: body?.["purpose"],
            iat: Math.floor(Date.now() / 1000),
          }),
        }),
      });
    }
    if (path === "/v1/consent" && method === "GET") {
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ items: state.consents }),
      });
    }
    if (path.startsWith("/v1/consent/") && path.endsWith("/withdraw") && method === "POST") {
      const id = path.split("/")[3];
      const c = state.consents.find((x) => x.consent_id === id);
      if (!c) {
        return route.fulfill({
          status: 404,
          contentType: "application/problem+json",
          body: JSON.stringify(makeRfc7807(404, "Not Found", "consent not found")),
        });
      }
      c.withdrawn_at = new Date().toISOString();
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          consent_id: id,
          withdrawn_at: c.withdrawn_at,
          receipt_jwt: makeReceipt({ sub: "user_e2e", consent_id: id, action: "withdraw" }),
        }),
      });
    }

    // ----- DSR -----
    if (path === "/v1/dsr/requests" && method === "POST") {
      const body = req.postDataJSON() as { action?: string } | null;
      const request_id = `dsr_e2e_${state.dsrRequests.length + 1}`;
      const submitted_at = new Date().toISOString();
      // SLA 30d per S-11 R-S11-7.
      const sla_due_at = new Date(Date.now() + 30 * 24 * 3600 * 1000).toISOString();
      state.dsrRequests.push({
        request_id,
        action: body?.action ?? "access",
        submitted_at,
        sla_due_at,
        status: "pending",
      });
      return route.fulfill({
        status: 201,
        contentType: "application/json",
        body: JSON.stringify({
          request_id,
          submitted_at,
          sla_due_at,
          receipt_jwt: makeReceipt({
            sub: "user_e2e",
            request_id,
            action: body?.action,
            iat: Math.floor(Date.now() / 1000),
          }),
        }),
      });
    }
    if (path.startsWith("/v1/dsr/requests/") && method === "GET") {
      const id = path.split("/").pop()!;
      const r = state.dsrRequests.find((x) => x.request_id === id);
      if (!r) {
        return route.fulfill({
          status: 404,
          contentType: "application/problem+json",
          body: JSON.stringify(makeRfc7807(404, "Not Found", "DSR not found")),
        });
      }
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(r),
      });
    }

    // ----- Admin: audit -----
    if (path === "/v1/admin/audit" && method === "GET") {
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          items: [
            {
              event_id: "evt_001",
              type: "consent.granted",
              subject: "user_e2e_newdev",
              ts: "2026-05-13T10:00:00Z",
              merkle_proof: { root: "abc123", verified: true },
            },
            {
              event_id: "evt_002",
              type: "pat.created",
              subject: "user_e2e_newdev",
              ts: "2026-05-13T10:05:00Z",
              merkle_proof: { root: "def456", verified: true },
            },
          ],
          cursor: null,
        }),
      });
    }
    if (path.startsWith("/v1/admin/audit/") && method === "GET") {
      const id = path.split("/").pop()!;
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          event_id: id,
          type: "consent.granted",
          subject: "user_e2e_newdev",
          ts: "2026-05-13T10:00:00Z",
          merkle_proof: {
            root: "abc123",
            siblings: ["s1", "s2", "s3"],
            verified: true,
          },
          payload_redacted: { tenant_id: "tenant_acme" },
        }),
      });
    }

    // ----- Admin: ops dual-approval -----
    if (path === "/v1/admin/ops" && method === "GET") {
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ items: state.pendingOps, cursor: null }),
      });
    }
    if (path.startsWith("/v1/admin/ops/") && path.endsWith("/approve") && method === "POST") {
      const id = path.split("/")[4];
      const op = state.pendingOps.find((o) => o.op_id === id);
      if (!op) {
        return route.fulfill({ status: 404, body: "{}" });
      }
      const approver = (req.headers()["x-e2e-user"] ?? "").toString();
      if (approver && approver === op.submitted_by) {
        return route.fulfill({
          status: 409,
          contentType: "application/problem+json",
          body: JSON.stringify(
            makeRfc7807(
              409,
              "Conflict",
              "submitter cannot approve own op (CTRL-DUAL-APPROVAL)",
            ),
          ),
        });
      }
      op.status = "executed";
      op.approved_by = approver || "user_e2e_approver";
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(op),
      });
    }

    // ----- Customer dashboard reads (rich fixtures so the screens render with
    // real data, not skeletons). Shapes mirror src/lib/customer-types.ts. -----
    const json = (status: number, obj: unknown): Promise<void> =>
      route.fulfill({
        status,
        contentType: "application/json",
        body: JSON.stringify(obj),
      });

    if (path === "/v1/customer/overview" && method === "GET") {
      return json(200, {
        tenant_id: "tenant_acme",
        tenant_name: "Acme Corp",
        plan: "pro",
        usage: { period: "2026-06", cas_bytes: 4_812_344_321, reads: 124_502, writes: 8_712, quota_bytes: 536_870_912_000 },
        billing: { status: "active", next_invoice_at: "2026-07-01T00:00:00Z", amount_due_cents: 5000, currency: "usd" },
        byok: { status: "customer_managed", cmk_id: "arn:aws:kms:us-east-1:…:key/acme-cmk", last_rotated_at: "2026-05-20T12:00:00Z" },
        recent_activity: [
          { event_id: "cevt_1", ts: "2026-06-28T14:22:00Z", event_type: "pat.created", severity: "info", actor: "you@acme.test", summary: "created PAT 'ci-github'" },
          { event_id: "cevt_2", ts: "2026-06-27T09:10:00Z", event_type: "cache.write", severity: "info", actor: "ci-runner", summary: "48,201 objects written" },
        ],
      });
    }
    if (path === "/v1/customer/usage" && method === "GET") {
      return json(200, {
        period: url.searchParams.get("period") ?? "2026-06",
        cas_bytes: 4_812_344_321, reads: 124_502, writes: 8_712, request_count: 133_214, quota_bytes: 536_870_912_000,
        daily: Array.from({ length: 7 }, (_, i) => ({ day: `2026-06-${String(22 + i).padStart(2, "0")}`, reads: 14_000 + i * 1_200, writes: 800 + i * 90, cas_bytes: 600_000_000 + i * 30_000_000 })),
      });
    }
    if (path === "/v1/customer/billing" && method === "GET") {
      return json(200, {
        plan: "pro", status: "active", current_period_start: "2026-06-01T00:00:00Z", current_period_end: "2026-07-01T00:00:00Z",
        amount_due_cents: 5000, currency: "usd", cancel_at_period_end: false, invoices: [], payment_method: null,
      });
    }
    if (path === "/v1/customer/keys" && method === "GET") {
      return json(200, {
        pats: [
          { pat_id: "pat_001", name: "ci-github", scopes: ["cache:r", "cache:w"], created_at: "2026-06-28T14:22:00Z", last_used_at: "2026-06-30T08:00:00Z" },
          { pat_id: "pat_002", name: "local-dev", scopes: ["cache:r"], created_at: "2026-06-10T10:00:00Z", revoked_at: "2026-06-20T10:00:00Z" },
        ],
        byok: { status: "customer_managed", cmk_id: "arn:aws:kms:us-east-1:…:key/acme-cmk", last_rotated_at: "2026-05-20T12:00:00Z" },
      });
    }
    if (path === "/v1/customer/team" && method === "GET") {
      return json(200, {
        members: [
          { user_id: "u_owner", email: "you@acme.test", role: "Owner", joined_at: "2026-04-01T00:00:00Z", status: "active" },
          { user_id: "u_dev1", email: "dev@acme.test", role: "Developer", joined_at: "2026-05-12T00:00:00Z", status: "active" },
          { user_id: "u_inv", email: "newhire@acme.test", role: "Developer", joined_at: "2026-06-25T00:00:00Z", status: "invited" },
        ],
      });
    }

    // ----- Default: 404 (so unmocked endpoints surface in tests) -----
    return route.fulfill({
      status: 404,
      contentType: "application/problem+json",
      body: JSON.stringify(makeRfc7807(404, "Not Found", `unmocked ${method} ${path}`)),
    });
  });
}
