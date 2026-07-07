// WI-S16-005 — slide-in drawer for an audit event with tabbed views.
//
// Linear kit: rendered as a glass Card with `lin-seg` tabs and dark CodeBlock
// panels (terminals stay dark per doctrine). Testids + tab roles preserved.

"use client";

import React from "react";
import type { AuditEventDetail } from "@/lib/types";
import { Button, Card, CodeBlock } from "@/components/ui/linear";
import MerkleProofViewer from "./MerkleProofViewer";

export type DrawerTab = "envelope" | "payload" | "merkle" | "raw";

export interface AuditEventDrawerProps {
  event: AuditEventDetail | null;
  open: boolean;
  onClose: () => void;
}

const TAB_LABEL: Record<DrawerTab, string> = {
  envelope: "Envelope",
  payload: "Payload",
  merkle: "Merkle",
  raw: "Raw",
};

export function AuditEventDrawer({
  event,
  open,
  onClose,
}: AuditEventDrawerProps): React.ReactElement | null {
  const [tab, setTab] = React.useState<DrawerTab>("envelope");
  if (!open || !event) return null;

  const closeBtn = (
    <Button variant="ghost" size="sm" onClick={onClose} data-testid="drawer-close">
      Close
    </Button>
  );

  return (
    <aside
      role="dialog"
      aria-label={`Audit event ${event.event_id}`}
      data-testid="audit-event-drawer"
      data-open={open}
      className="lin-mt"
    >
      <Card
        title={event.event_type}
        meta={`${event.event_id} · ${event.ts}`}
        actions={closeBtn}
      >
        <div className="lin-seg" role="tablist">
          {(["envelope", "payload", "merkle", "raw"] as DrawerTab[]).map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              className="lin-seg__opt"
              data-active={tab === t}
              data-testid={`drawer-tab-${t}`}
              onClick={() => setTab(t)}
            >
              {TAB_LABEL[t]}
            </button>
          ))}
        </div>

        <section role="tabpanel" data-testid={`drawer-panel-${tab}`} className="lin-mt">
          {tab === "envelope" && (
            <CodeBlock code={JSON.stringify(event.cloudevent, null, 2)} lang="json" />
          )}
          {tab === "payload" && (
            <CodeBlock code={JSON.stringify(event.payload, null, 2)} lang="json" />
          )}
          {tab === "merkle" && <MerkleProofViewer proof={event.merkle_proof} />}
          {tab === "raw" && <CodeBlock code={JSON.stringify(event, null, 2)} lang="json" />}
        </section>

        <p className="lin-card__meta lin-mt">
          R2 archive:{" "}
          <a href={event.r2_url} rel="noreferrer">
            {event.r2_url}
          </a>
        </p>
      </Card>
    </aside>
  );
}

export default AuditEventDrawer;
