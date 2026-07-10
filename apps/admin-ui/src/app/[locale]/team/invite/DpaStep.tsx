"use client";

/**
 * DpaStep — moved out of the signup onboarding wizard (was at
 * `app/[locale]/onboarding/dpa/DpaStep.tsx`) per Phase-0 PLG framework §4:
 * a single-user tenant is its own data controller and subject; DPA only
 * becomes legally necessary when a tenant **invites a second member**,
 * because at that point the tenant begins processing personal data of
 * another natural person.
 *
 * Gating logic (called by `app/[locale]/team/invite/page.tsx`):
 *   1. Before the invite email is sent, check `tenant.dpa_accepted_at`.
 *   2. If absent, render this component to block the invite action until
 *      the controller has scrolled + clicked accept.
 *   3. On accept, POST `/v1/tenants/{id}/dpa-accept`, then re-enter the
 *      invite flow with the captured audit_event_id stamped on the invite.
 *
 * Behaviour preserved from the original wizard step:
 *   - Scroll-to-end gate (`hasScrolledToEnd`) before the accept button enables.
 *   - Acceptance produces an immutable `audit_event_id` from the server.
 *   - SHA-256 of the rendered notice text is bound into the acceptance record
 *     so the exact bytes the user saw are forensically anchored.
 *
 * UI: migrated to the Linear design language (frozen kit + globals.css tokens).
 * The scroll-gate container is a bounded scroll region (functional, not
 * decoration); its chrome comes from the kit. Testids preserved.
 */

import * as React from "react";
import { t, type Locale } from "@/i18n/messages";
import { hasScrolledToEnd } from "@/lib/dpa-scroll";
import { acceptDpaAction } from "@/app/[locale]/onboarding/actions";
import { Button, Callout, InlineError } from "@/components/ui/linear";

export interface DpaStepProps {
  locale: Locale;
  tenantId: string;
  dpaText: string;
  dpaVersion: string;
  noticeTextHash: string;
  onAccepted?: (auditEventId: string) => void;
}

export function DpaStep(props: DpaStepProps): React.ReactElement {
  const [scrolled, setScrolled] = React.useState(false);
  const [auditEventId, setAuditEventId] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [submitting, setSubmitting] = React.useState(false);

  const onScroll = React.useCallback(
    (e: React.UIEvent<HTMLDivElement>) => {
      const el = e.currentTarget;
      if (
        hasScrolledToEnd({
          scrollTop: el.scrollTop,
          scrollHeight: el.scrollHeight,
          clientHeight: el.clientHeight,
        })
      ) {
        setScrolled(true);
      }
    },
    [],
  );

  async function onAccept(): Promise<void> {
    setSubmitting(true);
    try {
      const res = await acceptDpaAction({
        tenantId: props.tenantId,
        dpaVersion: props.dpaVersion,
        dpaLocale: props.locale,
        noticeTextHash: props.noticeTextHash,
        // Real consent field: the moment the controller clicked "accept".
        uiCaptureTs: Date.now(),
      });
      setAuditEventId(res.audit_event_id);
      props.onAccepted?.(res.audit_event_id);
    } catch (err) {
      setError((err as Error).message);
      setSubmitting(false);
    }
  }

  return (
    <section
      data-testid="team-invite-dpa"
      className="lin-checklist"
      aria-labelledby="dpa-title"
    >
      <h2 id="dpa-title">{t(props.locale, "onboarding.dpa.title")}</h2>

      {/* Scroll-gate region — bounded + scrollable is functional behaviour;
          the accept button enables only once the reader reaches the end. */}
      <div
        data-testid="dpa-scroller"
        onScroll={onScroll}
        className="lin-code max-h-96 overflow-y-auto"
      >
        <pre>{props.dpaText}</pre>
      </div>

      {!scrolled ? (
        <div className="lin-card__meta" data-testid="dpa-scroll-hint">
          {t(props.locale, "onboarding.dpa.scroll_hint")}
        </div>
      ) : null}

      {error ? (
        <div role="alert" data-testid="dpa-error">
          <InlineError error={error} />
        </div>
      ) : null}

      {auditEventId ? (
        <div data-testid="dpa-audit-id">
          <Callout tone="info">
            Accepted — audit reference <code>{auditEventId}</code>.
          </Callout>
        </div>
      ) : null}

      <div>
        <Button
          disabled={!scrolled || submitting}
          loading={submitting}
          onClick={onAccept}
          data-testid="dpa-accept"
        >
          {t(props.locale, "onboarding.dpa.accept")}
        </Button>
      </div>
    </section>
  );
}
