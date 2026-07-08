// WI-S16-005 — /[locale]/admin/audit/[event_id] — single-event detail page
// (shareable URL).

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import MerkleProofViewer from "@/components/admin/MerkleProofViewer";
import { adminClient } from "@/lib/admin-client";
import type { AuditEventDetail } from "@/lib/types";
import { Callout, Card, CodeBlock } from "@/components/ui/linear";

export interface AuditEventPageProps {
  params: Promise<{ locale: string; event_id: string }>;
}

async function loadEvent(eventId: string): Promise<AuditEventDetail | null> {
  try {
    return await adminClient.getAuditEvent(eventId);
  } catch {
    return null;
  }
}

export default async function AuditEventPage({
  params,
}: AuditEventPageProps): Promise<React.ReactElement> {
  const { event_id } = await params;
  const event = await loadEvent(event_id);

  return (
    <RbacGuard>
      <div className="cx-shell lin">
        <div className="cx-main">
          <main aria-labelledby="event-heading">
            <h1 id="event-heading">Audit event {event_id}</h1>
            {!event && (
              <div role="alert" className="lin-mt">
                <Callout tone="danger">Event not found.</Callout>
              </div>
            )}
            {event && (
              <>
                <div className="lin-mt-lg">
                  <Card title="CloudEvent envelope">
                    <CodeBlock code={JSON.stringify(event.cloudevent, null, 2)} lang="json" />
                  </Card>
                </div>

                <div className="lin-mt">
                  <Card title="Payload">
                    <CodeBlock code={JSON.stringify(event.payload, null, 2)} lang="json" />
                  </Card>
                </div>

                <div className="lin-mt">
                  <MerkleProofViewer proof={event.merkle_proof} />
                </div>

                <div className="lin-mt">
                  <Card title="R2 storage">
                    <p className="lin-card__meta">
                      <a href={event.r2_url} rel="noreferrer">
                        {event.r2_url}
                      </a>
                    </p>
                  </Card>
                </div>
              </>
            )}
          </main>
        </div>
      </div>
    </RbacGuard>
  );
}
