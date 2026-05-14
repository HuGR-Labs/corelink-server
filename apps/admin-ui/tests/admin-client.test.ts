// WI-S16-005 — admin-client tests:
//   - X-Admin-Operation-Reason header is required + propagated on ops mutations.
//   - listAuditEvents builds the correct query string.
//   - exportAudit hits POST /v1/admin/audit/export.

import { describe, it, expect, vi } from "vitest";
import { AdminClient, AdminClientError } from "@/lib/admin-client";

function makeFetch(impl: (url: string, init?: RequestInit) => Promise<Response>) {
  return vi.fn(impl) as unknown as typeof fetch;
}

describe("AdminClient", () => {
  it("adds X-Admin-Operation-Reason on approveOp", async () => {
    const captured: { url: string; headers: Record<string, string>; method: string }[] = [];
    const fetchImpl = makeFetch(async (url, init) => {
      const h: Record<string, string> = {};
      new Headers(init?.headers).forEach((v, k) => {
        h[k.toLowerCase()] = v;
      });
      captured.push({ url, headers: h, method: init?.method ?? "GET" });
      return new Response(JSON.stringify({ op_id: "op_1" }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });
    const client = new AdminClient({ baseUrl: "https://api.test", fetchImpl });
    await client.approveOp("op_1", "rotation justified");
    expect(captured).toHaveLength(1);
    expect(captured[0]!.method).toBe("POST");
    expect(captured[0]!.headers["x-admin-operation-reason"]).toBe(
      "rotation justified",
    );
    expect(captured[0]!.url).toBe("https://api.test/v1/admin/ops/op_1/approve");
  });

  it("refuses to send ops mutation when reason is empty", async () => {
    const fetchImpl = makeFetch(async () => new Response("", { status: 200 }));
    const client = new AdminClient({ baseUrl: "https://api.test", fetchImpl });
    await expect(client.rejectOp("op_1", "")).rejects.toBeInstanceOf(AdminClientError);
  });

  it("builds audit query string from filter", async () => {
    let capturedUrl = "";
    const fetchImpl = makeFetch(async (url) => {
      capturedUrl = url;
      return new Response(JSON.stringify({ rows: [], next_cursor: null }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });
    const client = new AdminClient({ baseUrl: "https://api.test", fetchImpl });
    await client.listAuditEvents({
      tenant_id: "tenant_a",
      event_types: ["auth.login", "byok.cmk_rotated"],
      severity: ["critical"],
    });
    expect(capturedUrl).toContain("/v1/admin/audit?");
    expect(capturedUrl).toContain("tenant_id=tenant_a");
    expect(capturedUrl).toContain("event_types=auth.login%2Cbyok.cmk_rotated");
    expect(capturedUrl).toContain("severity=critical");
  });

  it("exportAudit POSTs to /v1/admin/audit/export", async () => {
    let captured: { url: string; method: string; body: string } | null = null;
    const fetchImpl = makeFetch(async (url, init) => {
      captured = {
        url,
        method: init?.method ?? "GET",
        body: typeof init?.body === "string" ? init.body : "",
      };
      return new Response(
        JSON.stringify({
          signed_url: "https://signed.example/abc",
          expires_at: "2026-05-15T10:00:00Z",
          format: "csv",
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    });
    const client = new AdminClient({ baseUrl: "https://api.test", fetchImpl });
    const res = await client.exportAudit({ tenant_id: "tenant_a" }, "csv");
    expect(captured).not.toBeNull();
    expect(captured!.method).toBe("POST");
    expect(captured!.url).toBe("https://api.test/v1/admin/audit/export");
    expect(JSON.parse(captured!.body)).toMatchObject({
      format: "csv",
      filter: { tenant_id: "tenant_a" },
    });
    expect(res.signed_url).toContain("signed.example");
  });
});
