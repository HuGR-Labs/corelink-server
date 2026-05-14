// WI-S16-005 — slide-in drawer for an audit event with tabbed views.

"use client";

import React from "react";
import type { AuditEventDetail } from "@/lib/types";
import MerkleProofViewer from "./MerkleProofViewer";

export type DrawerTab = "envelope" | "payload" | "merkle" | "raw";

export interface AuditEventDrawerProps {
  event: AuditEventDetail | null;
  open: boolean;
  onClose: () => void;
}

export function AuditEventDrawer({
  event,
  open,
  onClose,
}: AuditEventDrawerProps): React.ReactElement | null {
  const [tab, setTab] = React.useState<DrawerTab>("envelope");
  if (!open || !event) return null;

  return (
    <aside
      role="dialog"
      aria-label={`Audit event ${event.event_id}`}
      data-testid="audit-event-drawer"
      data-open={open}
    >
      <header>
        <h2>{event.event_type}</h2>
        <p>
          <code>{event.event_id}</code> · <code>{event.ts}</code>
        </p>
        <button type="button" onClick={onClose} data-testid="drawer-close">
          Close
        </button>
      </header>

      <nav role="tablist">
        {(["envelope", "payload", "merkle", "raw"] as DrawerTab[]).map((t) => (
          <button
            key={t}
            type="button"
            role="tab"
            aria-selected={tab === t}
            data-testid={`drawer-tab-${t}`}
            onClick={() => setTab(t)}
          >
            {t}
          </button>
        ))}
      </nav>

      <section role="tabpanel" data-testid={`drawer-panel-${tab}`}>
        {tab === "envelope" && <pre>{JSON.stringify(event.cloudevent, null, 2)}</pre>}
        {tab === "payload" && <pre>{JSON.stringify(event.payload, null, 2)}</pre>}
        {tab === "merkle" && <MerkleProofViewer proof={event.merkle_proof} />}
        {tab === "raw" && <pre>{JSON.stringify(event, null, 2)}</pre>}
      </section>

      <footer>
        <p>
          R2 archive:{" "}
          <a href={event.r2_url} rel="noreferrer">
            {event.r2_url}
          </a>
        </p>
      </footer>
    </aside>
  );
}

export default AuditEventDrawer;
