"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { getDsrStatus } from "@/lib/dsr-client";
import type { DsrRequestDetail } from "@/lib/dsr-types";
import { SlaCountdown } from "@/components/dsr/SlaCountdown";
import { tFor, type Locale } from "@/i18n";
import { Badge, Button, Skeleton } from "@/components/ui/linear";

export interface DsrStatusDetailClientProps {
  locale: Locale;
  requestId: string;
  /** Test injection. */
  initialDetail?: DsrRequestDetail;
  token?: string;
}

export function DsrStatusDetailClient(props: DsrStatusDetailClientProps) {
  const t = (k: string) => tFor(props.locale, k);
  const [detail, setDetail] = useState<DsrRequestDetail | null>(
    props.initialDetail ?? null,
  );

  useEffect(() => {
    if (props.initialDetail || !props.token) return;
    void (async () => {
      const d = await getDsrStatus(props.requestId, { token: props.token! });
      setDetail(d);
    })();
  }, [props.requestId, props.token, props.initialDetail]);

  if (!detail) {
    return (
      <div className="cx-shell lin">
        <div className="cx-main">
          <main>
            <div
              data-testid="dsr-detail-loading"
              aria-busy="true"
              aria-label="Loading"
            >
              <Skeleton rows={4} />
            </div>
          </main>
        </div>
      </div>
    );
  }

  const hasDownload =
    (detail.action === "access" || detail.action === "portability") &&
    detail.status === "completed";

  return (
    <div className="cx-shell lin">
      <div className="cx-main">
        <main aria-labelledby="dsr-detail-title">
          <h1 id="dsr-detail-title">{t("dsr.detail.title")}</h1>
          <p>
            <Link
              href={`/${props.locale}/dsr/status`}
              data-testid="dsr-detail-back"
            >
              {t("dsr.detail.back_to_list")}
            </Link>
          </p>

          <section>
            <dl>
              <dt>{t("dsr.status.table_request_id")}</dt>
              <dd data-testid="dsr-detail-request-id">
                <code>{detail.request_id}</code>
              </dd>
              <dt>{t("dsr.status.table_action")}</dt>
              <dd>{t(`dsr.rights.${detail.action}.label`)}</dd>
              <dt>{t("dsr.status.table_status")}</dt>
              <dd>
                <Badge dot>{t(`dsr.status_states.${detail.status}`)}</Badge>
              </dd>
              <dt>{t("dsr.status.table_deadline")}</dt>
              <dd>
                <SlaCountdown
                  locale={props.locale}
                  deadline={detail.sla_deadline}
                />
              </dd>
            </dl>
          </section>

          <section aria-labelledby="dsr-timeline-title">
            <h2 id="dsr-timeline-title">{t("dsr.detail.timeline_title")}</h2>
            <ol data-testid="dsr-detail-timeline">
              {detail.timeline.map((evt, i) => (
                <li key={`${evt.at}-${i}`}>
                  <time dateTime={evt.at}>{evt.at}</time>{" "}
                  <strong>{t(`dsr.status_states.${evt.to}`)}</strong>
                  {evt.note ? ` — ${evt.note}` : null}
                </li>
              ))}
            </ol>
          </section>

          <section aria-labelledby="dsr-download-title">
            <h2 id="dsr-download-title">
              {t("dsr.detail.data_download_title")}
            </h2>
            {hasDownload && detail.data_download_url ? (
              <Button
                href={detail.data_download_url}
                data-testid="dsr-detail-download"
              >
                {t("dsr.detail.data_download_action")}
              </Button>
            ) : (
              <p>{t("dsr.detail.data_download_pending")}</p>
            )}
          </section>
        </main>
      </div>
    </div>
  );
}
