"use client";

import { useState } from "react";
import { decodeJwtPayload } from "@/lib/jwt-decode";
import { tFor, type Locale } from "@/i18n";
import { SlaCountdown } from "./SlaCountdown";
import { Button, Modal } from "@/components/ui/linear";

export interface JwtReceiptModalProps {
  locale: Locale;
  /** Raw signed JWT receipt returned by the backend. */
  token: string;
  /** Server-supplied SLA deadline (NEVER decoded for control decisions). */
  slaDeadline: string;
  onClose: () => void;
  /** Test injection for clipboard. */
  clipboard?: { writeText(text: string): Promise<void> };
  /** Inherited by SlaCountdown; tests pass 0 to disable the timer. */
  countdownRefreshMs?: number;
}

export function JwtReceiptModal(props: JwtReceiptModalProps) {
  const t = (k: string) => tFor(props.locale, k);
  const decoded = decodeJwtPayload(props.token);
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    const cb = props.clipboard ?? globalThis.navigator?.clipboard;
    if (!cb) return;
    await cb.writeText(props.token);
    setCopied(true);
  };

  const downloadHref =
    "data:application/jwt;base64," +
    (typeof btoa === "function"
      ? btoa(props.token)
      : Buffer.from(props.token, "utf-8").toString("base64"));

  return (
    <Modal
      open
      onClose={props.onClose}
      title={t("dsr.receipt.modal_title")}
      footer={
        <>
          <Button variant="ghost" onClick={handleCopy} data-testid="receipt-copy">
            {copied ? t("dsr.receipt.copied") : t("dsr.receipt.copy_jwt")}
          </Button>
          <a
            href={downloadHref}
            download={`dsr-receipt-${decoded?.request_id ?? "unknown"}.jwt`}
            data-testid="receipt-download"
            className="lin-btn lin-btn--ghost"
          >
            {t("dsr.receipt.download_jwt")}
          </a>
          <Button onClick={props.onClose} data-testid="receipt-close">
            {t("dsr.receipt.close")}
          </Button>
        </>
      }
    >
      <div data-testid="dsr-receipt-modal" className="lin-checklist">
        <dl>
          <dt>{t("dsr.receipt.request_id_label")}</dt>
          <dd data-testid="receipt-request-id">{decoded?.request_id ?? "—"}</dd>

          <dt>{t("dsr.receipt.action_label")}</dt>
          <dd data-testid="receipt-action">{decoded?.action ?? "—"}</dd>

          <dt>{t("dsr.receipt.jurisdiction_label")}</dt>
          <dd data-testid="receipt-jurisdiction">
            {decoded?.jurisdiction ?? "—"}
          </dd>

          <dt>{t("dsr.receipt.sla_deadline_label")}</dt>
          <dd data-testid="receipt-deadline">
            {props.slaDeadline}{" "}
            <SlaCountdown
              locale={props.locale}
              deadline={props.slaDeadline}
              refreshMs={props.countdownRefreshMs}
            />
          </dd>

          <dt>{t("dsr.receipt.jti_label")}</dt>
          <dd data-testid="receipt-jti">{decoded?.jti ?? "—"}</dd>

          <dt>{t("dsr.receipt.exp_label")}</dt>
          <dd data-testid="receipt-exp">
            {decoded?.exp ? new Date(decoded.exp * 1000).toISOString() : "—"}
          </dd>
        </dl>
      </div>
    </Modal>
  );
}
