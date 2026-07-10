"use client";

/**
 * DpaStep — the shared DPA click-through gate.
 *
 * Originally lived at `app/[locale]/team/invite/DpaStep.tsx` (moved here so the
 * upgrade/checkout flow can reuse the SAME component + legal text — the DPA
 * click-through must never be forked/duplicated). Two callers today:
 *   - `app/[locale]/team/invite/page.tsx` — the invite-a-second-member gate.
 *   - `components/UpgradeButton.tsx` — the paid-upgrade gate (the backend
 *     `POST /v1/onboarding/tier-select` 403s with `dpa_required` until the
 *     tenant has accepted the current DPA; INV-ONBOARD-DPA-FIRST).
 *
 * Behaviour (unchanged from the original wizard step):
 *   - Scroll-to-end gate (`hasScrolledToEnd`) before the accept button enables.
 *   - Acceptance produces an immutable `audit_event_id` from the server.
 *   - SHA-256 of the rendered notice text (`noticeTextHash`) is bound into the
 *     acceptance record so the exact bytes the user saw are forensically
 *     anchored. The hash MUST be computed from the same `dpaText` this
 *     component renders (both callers derive it from `lib/dpa-notice.ts`).
 *
 * UI: Linear design language (frozen kit + globals.css tokens). Testids
 * preserved so the existing team-invite tests keep passing.
 */

import * as React from "react";
import { t, type Locale } from "@/i18n/messages";
import { hasScrolledToEnd } from "@/lib/dpa-scroll";
import {
  acceptDpaAction,
  type DpaAccepted,
} from "@/app/[locale]/onboarding/actions";
import { Button, Callout, InlineError } from "@/components/ui/linear";

export interface DpaStepProps {
  locale: Locale;
  tenantId: string;
  dpaText: string;
  dpaVersion: string;
  noticeTextHash: string;
  onAccepted?: (auditEventId: string) => void;
  /**
   * Injected accept impl (test-only). Production leaves this unset and the
   * component calls the canonical `acceptDpaAction` server action — mirrors
   * `UpgradeButton`'s `fetchImpl` seam.
   */
  acceptImpl?: (input: {
    tenantId: string;
    dpaVersion: string;
    dpaLocale: Locale;
    noticeTextHash: string;
    uiCaptureTs: number;
  }) => Promise<DpaAccepted>;
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
      const accept = props.acceptImpl ?? acceptDpaAction;
      const res = await accept({
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
