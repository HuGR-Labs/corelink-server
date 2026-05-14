// WI-S16-005 — /[locale]/admin/audit/[event_id] — single-event detail page
// (shareable URL).

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import MerkleProofViewer from "@/components/admin/MerkleProofViewer";
import { adminClient } from "@/lib/admin-client";
import type { AuditEventDetail } from "@/lib/types";

export interface AuditEventPageProps {
  params: { locale: string; event_id: string };
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
  const event = await loadEvent(params.event_id);

  return (
    <RbacGuard>
      <main aria-labelledby="event-heading">
        <h1 id="event-heading">Audit event {params.event_id}</h1>
        {!event && <p role="alert">Event not found.</p>}
        {event && (
          <>
            <section aria-label="CloudEvent envelope">
              <h2>CloudEvent envelope</h2>
              <pre>{JSON.stringify(event.cloudevent, null, 2)}</pre>
            </section>

            <section aria-label="Payload">
              <h2>Payload</h2>
              <pre>{JSON.stringify(event.payload, null, 2)}</pre>
            </section>

            <MerkleProofViewer proof={event.merkle_proof} />

            <section aria-label="R2 storage">
              <h2>R2 storage</h2>
              <p>
                <a href={event.r2_url} rel="noreferrer">
                  {event.r2_url}
                </a>
              </p>
            </section>
          </>
        )}
      </main>
    </RbacGuard>
  );
}
