import { describe, expect, it } from "vitest";

import { rejectUnprovenGrpcTransport } from "../src/grpc_transport_gate.js";

describe("rejectUnprovenGrpcTransport", () => {
  it("rejects native gRPC before it can reach the unproved transport bridge", async () => {
    const authorization = "Bearer never-reflect-this-test-token";
    const response = rejectUnprovenGrpcTransport(
      new Request(
        "https://corelink-api.humangr.com/build.bazel.remote.execution.v2.ContentAddressableStorage/BatchReadBlobs",
        {
          headers: {
            authorization,
            "content-type": "application/grpc+proto; charset=utf-8",
          },
        },
      ),
    );

    expect(response).not.toBeNull();
    expect(response?.status).toBe(503);
    expect(response?.headers.get("cache-control")).toBe("no-store");
    expect(response?.headers.get("content-type")).toBe("application/json; charset=utf-8");
    const body = await response?.text();
    expect(body).toBe('{"error":"GRPC_TRANSPORT_UNAVAILABLE"}');
    expect(body).not.toContain(authorization);
  });

  it("does not interfere with a non-gRPC request", () => {
    const response = rejectUnprovenGrpcTransport(
      new Request("https://corelink-api.humangr.com/_health", {
        headers: { "content-type": "application/json" },
      }),
    );

    expect(response).toBeNull();
  });
});
