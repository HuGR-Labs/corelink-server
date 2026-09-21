import { afterEach, describe, expect, it, vi } from "vitest";
import { readCredentialLifecycle, type CredentialLifecycleEnv } from "../src/lib/credential_lifecycle_client.js";

const TENANT = "123e4567-e89b-42d3-a456-426614174000";
const KEY = "k".repeat(32);
const ENV: CredentialLifecycleEnv = { FABRIC_CREDENTIAL_AUTHORITY_URL: "https://fabric.example/", FABRIC_CREDENTIAL_ISSUER_AUTH_KEY: KEY };
function response(body: unknown, status = 200) { return new Response(JSON.stringify(body), { status }); }
function valid(generation = "9007199254740993", tenantId = TENANT, suspended = false) { return { tenant_id: tenantId, generation, suspended }; }

describe("credential lifecycle client", () => {
  afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });
  it("sends canonical path, dedicated auth, no-store, and refuses redirects", async () => {
    const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(response(valid()));
    await expect(readCredentialLifecycle(ENV, TENANT)).resolves.toEqual({ tenantId: TENANT, generation: "9007199254740993" });
    expect(fetcher).toHaveBeenCalledWith("https://fabric.example/internal/v1/credentials/tenants/123e4567-e89b-42d3-a456-426614174000/lifecycle", expect.objectContaining({ method: "GET", redirect: "error" }));
    expect((fetcher.mock.calls[0]![1] as RequestInit).headers).toEqual({ "x-corelink-internal-auth": KEY, "Cache-Control": "no-store" });
  });
  it("rejects missing/unsafe configuration and tenant identifiers", async () => {
    await expect(readCredentialLifecycle({}, TENANT)).rejects.toThrow();
    await expect(readCredentialLifecycle({ ...ENV, FABRIC_CREDENTIAL_ISSUER_AUTH_KEY: "short" }, TENANT)).rejects.toThrow();
    await expect(readCredentialLifecycle({ ...ENV, FABRIC_CREDENTIAL_ISSUER_AUTH_KEY: `${KEY} ` }, TENANT)).rejects.toThrow();
    await expect(readCredentialLifecycle({ ...ENV, FABRIC_CREDENTIAL_ISSUER_AUTH_KEY: `${KEY}\u0001` }, TENANT)).rejects.toThrow();
    for (const url of ["http://fabric.example/", "https://u:p@fabric.example/", "https://fabric.example/path", "https://fabric.example/?x=1", "https://fabric.example/#x"]) await expect(readCredentialLifecycle({ ...ENV, FABRIC_CREDENTIAL_AUTHORITY_URL: url }, TENANT)).rejects.toThrow();
    await expect(readCredentialLifecycle(ENV, "00000000-0000-0000-0000-000000000000")).rejects.toThrow();
  });
  it.each(["11111111-1111-4111-8111-111111111111", "018f48a8-2c08-7f7e-8a1d-2c3d4e5f6071"])("accepts canonical v4 or v7 tenant ID %s", async tenantId => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(response(valid("1", tenantId)));
    await expect(readCredentialLifecycle(ENV, tenantId)).resolves.toEqual({ tenantId, generation: "1" });
  });
  it("requires exact active lifecycle response", async () => {
    for (const body of [{ ...valid(), suspended: true }, { ...valid(), tenant_id: "123e4567-e89b-12d3-a456-426614174001" }, { ...valid(), generation: "01" }, { ...valid(), generation: "9223372036854775808" }, { ...valid(), extra: 1 }, { tenant_id: TENANT, generation: "1" }]) { vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(response(body)); await expect(readCredentialLifecycle(ENV, TENANT)).rejects.toThrow(); vi.restoreAllMocks(); }
  });
  it("rejects non-200 without exposing response text", async () => { const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(response({ secret: "do not expose" }, 503)); await expect(readCredentialLifecycle(ENV, TENANT)).rejects.toThrow("rejected"); expect(fetcher).toHaveBeenCalledTimes(1); });
  it("cancels a non-200 response body without waiting for cancellation", async () => {
    let cancelled = false;
    const body = new ReadableStream<Uint8Array>({ start(controller) { controller.enqueue(new Uint8Array([1])); }, cancel() { cancelled = true; return new Promise<void>(() => {}); } });
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(body, { status: 503 }));
    await expect(readCredentialLifecycle(ENV, TENANT)).rejects.toThrow("rejected");
    expect(cancelled).toBe(true);
  });
  it("detects duplicate response keys", async () => { const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(`{"tenant_id":"${TENANT}","generation":"1","generation":"2","suspended":false}`)); await expect(readCredentialLifecycle(ENV, TENANT)).rejects.toThrow(); fetcher.mockResolvedValue(new Response(`{"tenant_id":"${TENANT}","generation":"1","\\u0067eneration":"2","suspended":false}`)); await expect(readCredentialLifecycle(ENV, TENANT)).rejects.toThrow(); });
  it("bounds oversized bodies and cancels hanging readers on timeout", async () => {
    const oversized = new ReadableStream<Uint8Array>({ start(controller) { controller.enqueue(new Uint8Array(4096)); controller.enqueue(new Uint8Array(1)); } });
    const fetcher = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(new Response(oversized)); await expect(readCredentialLifecycle(ENV, TENANT)).rejects.toThrow();
    vi.useFakeTimers(); let cancelled = false;
    const hanging = { getReader: () => ({ read: () => new Promise<never>(() => {}), cancel: () => { cancelled = true; return Promise.resolve(); }, releaseLock: () => {} }) };
    fetcher.mockResolvedValueOnce({ status: 200, body: hanging } as unknown as Response); const promise = readCredentialLifecycle(ENV, TENANT); const assertion = expect(promise).rejects.toThrow("timed out"); await vi.advanceTimersByTimeAsync(5001); await assertion; expect(cancelled).toBe(true);
  });
  it("bounds a fetcher ignoring abort and rejects failures", async () => { const fetcher = vi.spyOn(globalThis, "fetch").mockRejectedValueOnce(new Error("redirect")); await expect(readCredentialLifecycle(ENV, TENANT)).rejects.toThrow("unavailable"); vi.useFakeTimers(); fetcher.mockImplementationOnce(() => new Promise<Response>(() => {})); const promise = readCredentialLifecycle(ENV, TENANT); const assertion = expect(promise).rejects.toThrow("timed out"); await vi.advanceTimersByTimeAsync(5001); await assertion; });
});
