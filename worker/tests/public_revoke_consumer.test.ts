/**
 * Regression: the F3.2 `_public` blob revocation endpoint (`/_internal/public/revoke`,
 * B1b) must gate on the ERASE consumer key (`CORELINK_ERASE_AUTH_KEY`), the same
 * dedicated irreversible-delete authority as `cas_erase` — NOT the admin key.
 *
 * The live prod blocker this pins (caught by end-to-end proof, 2026-08-15): the
 * endpoint was first shipped at `/_internal/admin/public/revoke`, which the worker
 * front-gate maps (via the `/_internal/admin/*` branch) to the `admin` consumer
 * (CORELINK_ADMIN_AUTH_KEY). But the container handler gates on the ERASE key
 * (finding H4 — an admin-key leak must not drive irreversible R2 deletes), so NO
 * single key could satisfy both gates and every call 401'd at the edge. Moving the
 * path OUT of `/_internal/admin/*` into the erase-consumer catch-all makes the edge
 * and the handler agree on the erase key. These tests pin that: public/revoke →
 * erase; it is NOT swept into the admin consumer.
 */
import { describe, it, expect } from "vitest";
import { internalConsumerForPath } from "../src/index.js";

describe("internalConsumerForPath — _public revocation is an erase-consumer surface", () => {
  it("routes /_internal/public/revoke to the erase consumer (NOT admin)", () => {
    expect(internalConsumerForPath("/_internal/public/revoke")).toBe("erase");
  });

  it("does NOT sweep public/revoke into the admin consumer", () => {
    // A regression guard: if a future refactor ever re-nested this under
    // /_internal/admin/*, it would gate on the admin key and 401 the erase-keyed
    // container handler — the exact live blocker this file exists to prevent.
    expect(internalConsumerForPath("/_internal/public/revoke")).not.toBe("admin");
  });

  it("leaves the real admin surface + the erase cascade unchanged", () => {
    expect(internalConsumerForPath("/_internal/admin/tenants")).toBe("admin");
    expect(internalConsumerForPath("/_internal/cas/x/y/erase")).toBe("erase");
  });
});
