// Customer-side "Workspaces" (W10) — clw snapshots + Pin.
//
// DATA-TRUTH (mirrors customer_d1.rs / customer-types.ts):
//   workspace list ... [not-wired] (BE-11) → client.listWorkspaces() throws
//                      NotWiredError by contract. There is NOTHING to probe, so
//                      we NEVER call it here (a call would only throw). Like the
//                      $-ceiling in UsageClient, we render a teaching EmptyState
//                      that explains WHAT will appear and HOW to make it appear
//                      (push a snapshot with `clw`) — never a fabricated row.
//
// This screen is a real "here's what this is + how to start" surface: two value
// -prop cards (Workspaces + Pin), the teaching list state, and a Pin explainer —
// so an empty tenant leaves knowing exactly what to do next.

"use client";

import React from "react";
import { Card, Callout, EmptyState, Button, HelpPopover } from "@/components/ui/linear";

// Where the "install the CLI" / "read the docs" CTAs point. Matches the docs
// host convention used across the customer surface (ConnectClient / Shell).
const DOCS_CLW = "https://docs.humangr.com/corelink/workspaces";
const DOCS_CLW_INSTALL = "https://docs.humangr.com/corelink/workspaces/install";

// The backend WP that closes the [not-wired] list surface — surfaced so the
// teaching state names WHEN the snapshot list turns live.
const WORKSPACES_WP = "BE-11 workspaces surface";

export function WorkspacesClient(): React.ReactElement {
  return (
    <div data-testid="workspaces-shell">
      {/* ── Value prop: Workspaces ─────────────────────────────────────── */}
      <Card title="What Workspaces do">
        <p className="lin-card__meta">
          A{" "}
          <strong>workspace</strong>{" "}
          <HelpPopover label="What is a workspace?">
            A workspace is a captured working tree — your repo checkout plus its
            build state — that <code>clw</code> uploads once and can restore
            anywhere. Think of it as a reproducible starting point your team and
            your runners can share.
          </HelpPopover>{" "}
          lets you{" "}
          <strong>snapshot</strong>{" "}
          <HelpPopover label="What is a snapshot?">
            A snapshot writes your workspace into content-addressed storage — every
            file is stored by its hash, so unchanged content is deduplicated and
            only new bytes are uploaded. Push one with{" "}
            <code>clw snapshot</code>.
          </HelpPopover>{" "}
          it to content-addressed storage, then{" "}
          <strong>hydrate</strong>{" "}
          <HelpPopover label="What is hydrate?">
            Hydrating restores a snapshot into a working directory. Because the
            content is already cached and deduplicated, a fresh machine or runner
            reconstructs the tree in seconds instead of re-cloning and rebuilding
            from scratch.
          </HelpPopover>{" "}
          it onto any machine in seconds — no re-clone, no cold rebuild.
        </p>
        <div data-testid="workspaces-valueprop-actions">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => window.open(DOCS_CLW, "_blank", "noopener,noreferrer")}
          >
            How Workspaces work
          </Button>
        </div>
      </Card>

      {/* ── Value prop: Pin ────────────────────────────────────────────── */}
      <Card title="What Pin does">
        <p className="lin-card__meta">
          <strong>Pin</strong>{" "}
          <HelpPopover label="What is Pin?">
            Pin guarantees a snapshot stays warm: its content is kept resident and
            exempt from eviction, so every hydrate is a fast cache hit. Pin is
            refcounted — a snapshot stays pinned while at least one reference holds
            it — and it is a metered add-on ($5/mo per 100&nbsp;GB pinned).
          </HelpPopover>{" "}
          keeps a snapshot guaranteed-warm so hydrates never pay a cold-fetch
          penalty. It is refcounted (kept while anything references it) and billed
          as a metered add-on — you pin only what you want kept hot.
        </p>
      </Card>

      {/* ── The snapshot list — [not-wired] (BE-11). Teach, never fabricate,
             and NEVER probe a method that throws by contract. ────────────── */}
      <Card title="Your snapshots">
        <div data-testid="workspaces-list-empty">
          <EmptyState
            title="Your snapshots appear here once you push one with clw"
            body={`Install the clw CLI and run “clw snapshot” — your workspace uploads to content-addressed storage and shows up here, ready to hydrate anywhere. The live list ships with the workspaces backend (${WORKSPACES_WP}).`}
            cta={
              <div data-testid="workspaces-list-cta">
                <Button
                  variant="primary"
                  size="sm"
                  onClick={() =>
                    window.open(DOCS_CLW_INSTALL, "_blank", "noopener,noreferrer")
                  }
                >
                  Install the clw CLI
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() =>
                    window.open(DOCS_CLW, "_blank", "noopener,noreferrer")
                  }
                >
                  Read the docs
                </Button>
              </div>
            }
          />
        </div>
      </Card>

      {/* ── Pin explainer (management lands with BE-11) ────────────────── */}
      <Card title="Pinning snapshots">
        <Callout tone="info">
          <strong>Pin keeps a snapshot hot.</strong> Once you have snapshots, you
          will be able to pin the ones your team hydrates most so they never fall
          out of cache. Pin is refcounted and metered ($5/mo per 100&nbsp;GB), so
          you pay only for what you keep warm. Pin management appears alongside your
          snapshots when the workspaces backend ships ({WORKSPACES_WP}).
        </Callout>
      </Card>
    </div>
  );
}

export default WorkspacesClient;
