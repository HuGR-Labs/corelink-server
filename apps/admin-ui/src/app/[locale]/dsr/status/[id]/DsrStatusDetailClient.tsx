"use client";

import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { getDsrStatus } from "@/lib/dsr-client";
import type { DsrRequestDetail, DsrStatus } from "@/lib/dsr-types";
import { SlaCountdown } from "@/components/dsr/SlaCountdown";
import { tFor, type Locale } from "@/i18n";
import {
  Badge,
  Button,
  Card,
  CopyField,
  InlineError,
  Skeleton,
} from "@/components/ui/linear";

export interface DsrStatusDetailClientProps {
  locale: Locale;
  requestId: string;
  /** Test injection. */
  initialDetail?: DsrRequestDetail;
  /** Clerk session token resolved server-side in `page.tsx`. */
  token?: string;
}

/** DSR status → Linear Badge tone (semantic status dots only). */
const STATUS_TONE: Record<DsrStatus, "neutral" | "success" | "warn" | "danger"> =
  {
    pending: "warn",
    in_progress: "neutral",
    completed: "success",
    rejected: "danger",
  };

export function DsrStatusDetailClient(props: DsrStatusDetailClientProps) {
  const t = (k: string) => tFor(props.locale, k);
  const [detail, setDetail] = useState<DsrRequestDetail | null>(
    props.initialDetail ?? null,
  );
  const [loading, setLoading] = useState<boolean>(
    props.initialDetail == null && props.token != null,
  );
  const [error, setError] = useState<unknown | null>(null);

  const load = useCallback(() => {
    if (!props.token) return;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const d = await getDsrStatus(props.requestId, { token: props.token! });
        setDetail(d);
      } catch (e) {
        setError(e);
      } finally {
        setLoading(false);
      }
    })();
  }, [props.requestId, props.token]);

  useEffect(() => {
    if (props.initialDetail) return;
    if (!props.token) {
      // Unauthenticated (prod-unreachable: the route is Clerk-protected).
      // Stop the skeleton so we never hang like the pre-fix page did.
      setLoading(false);
      return;
    }
    load();
  }, [props.initialDetail, props.token, load]);

  const hasDownload =
    detail != null &&
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

          {loading ? (
            <Card>
              <div
                data-testid="dsr-detail-loading"
                aria-busy="true"
                aria-label="Loading"
              >
                <Skeleton rows={4} />
              </div>
            </Card>
          ) : error != null ? (
            <Card>
              <div data-testid="dsr-detail-error">
                <InlineError error={error} onRetry={load} />
              </div>
            </Card>
          ) : detail == null ? (
            <Card>
              <InlineError
                error="This request could not be found, or you don't have access to it."
              />
            </Card>
          ) : (
            <>
              <Card title={t(`dsr.rights.${detail.action}.label`)}>
                <dl className="grid gap-4 sm:grid-cols-2">
                  <div className="sm:col-span-2">
                    <dt className="lin-card__meta">
                      {t("dsr.status.table_request_id")}
                    </dt>
                    <dd className="m-0 mt-2" data-testid="dsr-detail-request-id">
                      <CopyField
                        value={detail.request_id}
                        label={t("dsr.status.table_request_id")}
                      />
                    </dd>
                  </div>
                  <div>
                    <dt className="lin-card__meta">
                      {t("dsr.status.table_action")}
                    </dt>
                    <dd className="m-0 mt-2">
                      {t(`dsr.rights.${detail.action}.label`)}
                    </dd>
                  </div>
                  <div>
                    <dt className="lin-card__meta">
                      {t("dsr.status.table_status")}
                    </dt>
                    <dd className="m-0 mt-2">
                      <Badge tone={STATUS_TONE[detail.status]} dot>
                        {t(`dsr.status_states.${detail.status}`)}
                      </Badge>
                    </dd>
                  </div>
                  <div>
                    <dt className="lin-card__meta">
                      {t("dsr.status.table_deadline")}
                    </dt>
                    <dd className="m-0 mt-2">
                      <SlaCountdown
                        locale={props.locale}
                        deadline={detail.sla_deadline}
                      />
                    </dd>
                  </div>
                </dl>
              </Card>

              <Card
                title={t("dsr.detail.timeline_title")}
                className="lin-mt-lg"
              >
                <ol
                  data-testid="dsr-detail-timeline"
                  className="m-0 flex list-none flex-col gap-3 p-0"
                >
                  {detail.timeline.map((evt, i) => (
                    <li
                      key={`${evt.at}-${i}`}
                      className="flex flex-wrap items-baseline gap-3"
                    >
                      <time dateTime={evt.at} className="lin-card__meta">
                        {evt.at}
                      </time>
                      <Badge tone={STATUS_TONE[evt.to]} dot>
                        {t(`dsr.status_states.${evt.to}`)}
                      </Badge>
                      {evt.note ? <span>{evt.note}</span> : null}
                    </li>
                  ))}
                </ol>
              </Card>

              <Card
                title={t("dsr.detail.data_download_title")}
                className="lin-mt-lg"
              >
                {hasDownload && detail.data_download_url ? (
                  <Button
                    href={detail.data_download_url}
                    data-testid="dsr-detail-download"
                  >
                    {t("dsr.detail.data_download_action")}
                  </Button>
                ) : (
                  <p className="lin-card__meta">
                    {t("dsr.detail.data_download_pending")}
                  </p>
                )}
              </Card>
            </>
          )}
        </main>
      </div>
    </div>
  );
}
