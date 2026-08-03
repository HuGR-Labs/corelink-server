/**
 * `/[locale]/403` — the page a bounced user actually lands on.
 *
 * It typed `searchParams` as a plain object, but Next 16 makes it a PROMISE
 * (async request APIs). `searchParams?.reason` therefore read a property off a
 * Promise — always `undefined` — so every 403 rendered the generic fallback
 * reason and the specific one the caller passed was silently dropped.
 *
 * These tests await the component the way the App Router does, so a regression
 * to the synchronous shape fails here rather than in production.
 */

import * as React from "react";
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";

import ForbiddenPage from "@/app/[locale]/403/page";

async function renderForbidden(
  searchParams?: Promise<{ reason?: string | string[] }>,
): Promise<void> {
  const el = await ForbiddenPage({ searchParams });
  render(el);
}

describe("403 page surfaces the caller's reason", () => {
  it("renders the specific reason from ?reason=", async () => {
    await renderForbidden(
      Promise.resolve({ reason: "tenant scope mismatch" }),
    );
    expect(screen.getByText("tenant scope mismatch")).toBeInTheDocument();
    expect(screen.queryByText("operator role required")).toBeNull();
  });

  it("falls back to the generic reason when ?reason= is absent", async () => {
    await renderForbidden(Promise.resolve({}));
    expect(screen.getByText("operator role required")).toBeInTheDocument();
  });

  it("falls back when searchParams itself is absent", async () => {
    await renderForbidden(undefined);
    expect(screen.getByText("operator role required")).toBeInTheDocument();
  });

  it("uses the first value of a repeated ?reason= param", async () => {
    await renderForbidden(
      Promise.resolve({ reason: ["first reason", "second reason"] }),
    );
    expect(screen.getByText("first reason")).toBeInTheDocument();
  });

  it("falls back on a blank ?reason= rather than rendering an empty line", async () => {
    await renderForbidden(Promise.resolve({ reason: "   " }));
    expect(screen.getByText("operator role required")).toBeInTheDocument();
  });
});
