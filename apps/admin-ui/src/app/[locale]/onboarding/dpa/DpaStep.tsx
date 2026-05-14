"use client";

import * as React from "react";
import { Wizard } from "@/components/onboarding/Wizard";
import { t, type Locale } from "@/i18n/messages";
import { hasScrolledToEnd } from "@/lib/dpa-scroll";
import { acceptDpaAction } from "../actions";
import { loadState, saveState } from "@/lib/onboarding-state";

export interface DpaStepProps {
  locale: Locale;
  dpaText: string;
  dpaVersion: string;
  noticeTextHash: string;
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
    const persisted = loadState();
    if (!persisted.tenantId) {
      setError("Missing tenant_id");
      setSubmitting(false);
      return;
    }
    try {
      const res = await acceptDpaAction({
        tenantId: persisted.tenantId,
        dpaVersion: props.dpaVersion,
        dpaLocale: props.locale,
        noticeTextHash: props.noticeTextHash,
      });
      setAuditEventId(res.audit_event_id);
      saveState({ step: "region-plan", tenantId: persisted.tenantId });
      window.location.assign(`/${props.locale}/onboarding/region-plan`);
    } catch (err) {
      setError((err as Error).message);
      setSubmitting(false);
    }
  }

  return (
    <Wizard current="dpa">
      <h1>{t(props.locale, "onboarding.dpa.title")}</h1>
      <div
        data-testid="dpa-scroller"
        onScroll={onScroll}
        style={{ maxHeight: "60vh", overflow: "auto" }}
      >
        <pre>{props.dpaText}</pre>
      </div>
      {!scrolled ? (
        <p data-testid="dpa-scroll-hint">
          {t(props.locale, "onboarding.dpa.scroll_hint")}
        </p>
      ) : null}
      {error ? (
        <p role="alert" data-testid="dpa-error">
          {error}
        </p>
      ) : null}
      {auditEventId ? (
        <p data-testid="dpa-audit-id">audit: {auditEventId}</p>
      ) : null}
      <button
        type="button"
        disabled={!scrolled || submitting}
        onClick={onAccept}
        data-testid="dpa-accept"
      >
        {t(props.locale, "onboarding.dpa.accept")}
      </button>
    </Wizard>
  );
}
