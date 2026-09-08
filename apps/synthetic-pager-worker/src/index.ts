import {
  buildPagerDutyEvent,
  PAGERDUTY_EVENTS_URL,
  PAGERDUTY_WEBHOOK_PATH,
  parsePagerDutyWebhook,
  parseSyntheticPageEnvelope,
  SYNTHETIC_PAGE_PATH,
  validateReceiverEnvironment,
  validateWebhookEnvironment,
  verifyPagerDutySignature,
  type ReceiverEnv,
  type SyntheticPageEnvelope,
} from "./contract.js";

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });

type DrillRow = {
  drill_id: string;
  region: "americas" | "emea" | "apac" | "boundary_handoff";
  emit_ts_ms: number;
  scheduled_at_ms: number;
  correlation_id: string;
  delivery_mode: "immediate" | "deferred";
  delivered_at_ms: number | null;
};

function pageForRow(row: DrillRow): SyntheticPageEnvelope {
  return {
    drill: "synthetic_page",
    cron: "0 14 * * 1",
    scheduled_at_ms: row.scheduled_at_ms,
    synthetic_page: {
      service: "synthetic-drill",
      event_action: "trigger",
      severity: "info",
      synthetic_severity: "sev2_synthetic",
      region: row.region,
      rotation_week: row.region === "boundary_handoff" ? 3 : 0,
      emit_at_ms: row.emit_ts_ms,
      delivery_mode: "deferred",
      dedup_key: row.drill_id,
      correlation_id: row.correlation_id,
    },
  };
}

async function pagerDutyFetch(
  env: ReceiverEnv,
  page: SyntheticPageEnvelope["synthetic_page"],
  fetchImpl: typeof fetch,
): Promise<void> {
  const response = await fetchImpl(PAGERDUTY_EVENTS_URL, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(buildPagerDutyEvent(page, env.PAGERDUTY_SYNTHETIC_ROUTING_KEY!)),
  }).catch(() => null);
  if (response === null || !response.ok) throw new Error("PagerDuty delivery failed");
}

async function persistDelivery(env: ReceiverEnv, envelope: SyntheticPageEnvelope): Promise<void> {
  const page = envelope.synthetic_page;
  const deliveryMode = page.delivery_mode;
  const deliveredAt = deliveryMode === "immediate" ? page.emit_at_ms : null;
  await env.CONFIG_DB!.batch([
    env.CONFIG_DB!.prepare(
      `INSERT OR IGNORE INTO synthetic_page_drills
         (drill_id, region, severity, emit_ts_ms, outcome, correlation_id, schema_version,
          delivery_mode, scheduled_at_ms, delivered_at_ms)
       VALUES (?, ?, ?, ?, 'unacked', ?, 1, ?, ?, ?)`,
    ).bind(
      page.dedup_key,
      page.region,
      page.synthetic_severity,
      page.emit_at_ms,
      page.correlation_id,
      deliveryMode,
      envelope.scheduled_at_ms,
      deliveredAt,
    ),
    env.CONFIG_DB!.prepare(
      `INSERT OR IGNORE INTO synthetic_page_audit_events
         (event_id, drill_id, event_type, occurred_at_ms, correlation_id, source_event_id)
       VALUES (?, ?, 'triggered', ?, ?, ?)`,
    ).bind(
      `trigger:${page.dedup_key}`,
      page.dedup_key,
      page.emit_at_ms,
      page.correlation_id,
      page.dedup_key,
    ),
  ]);
}

async function handleSyntheticPage(request: Request, env: ReceiverEnv): Promise<Response> {
  const environmentError = validateReceiverEnvironment(env);
  if (environmentError !== null) return json({ error: "receiver_not_ready" }, 503);
  if (request.method !== "POST") return json({ error: "method_not_allowed" }, 405);
  if (request.headers.get("content-type")?.split(";", 1)[0]?.trim().toLowerCase() !== "application/json") {
    return json({ error: "content_type_required" }, 415);
  }

  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return json({ error: "invalid_json" }, 400);
  }
  const envelope = parseSyntheticPageEnvelope(body);
  const deliveryId = request.headers.get("x-corelink-scheduled-drill-id");
  if (envelope === null || deliveryId !== envelope.synthetic_page.dedup_key) {
    return json({ error: "invalid_synthetic_page_contract" }, 400);
  }

  try {
    // Durable row + audit event are committed before any external page.
    await persistDelivery(env, envelope);
  } catch {
    return json({ error: "receiver_storage_unavailable" }, 503);
  }

  if (envelope.synthetic_page.delivery_mode === "deferred") {
    // The Sunday scheduled handler will pick this durable row up. Returning
    // 202 is safe because the row, not this response, is the queue.
    return json({ accepted: true, deferred: true, dedup_key: deliveryId }, 202);
  }

  try {
    await pagerDutyFetch(env, envelope.synthetic_page, fetch);
  } catch {
    // Scheduler retry uses the same canonical SP-* dedup key and correlation.
    return json({ error: "pagerduty_delivery_failed" }, 502);
  }
  return json({ accepted: true, dedup_key: deliveryId }, 202);
}

async function runDeferredDeliveries(
  controller: ScheduledController,
  env: ReceiverEnv,
  fetchImpl: typeof fetch,
): Promise<void> {
  const environmentError = validateReceiverEnvironment(env);
  if (environmentError !== null) throw new Error("receiver not ready");
  const result = await env.CONFIG_DB!.prepare(
    `SELECT drill_id, region, emit_ts_ms, scheduled_at_ms, correlation_id, delivery_mode, delivered_at_ms
       FROM synthetic_page_drills
      WHERE delivery_mode = 'deferred' AND delivered_at_ms IS NULL
        AND outcome = 'unacked' AND emit_ts_ms <= ?
      ORDER BY emit_ts_ms ASC
      LIMIT 20`,
  ).bind(controller.scheduledTime).all<DrillRow>();
  for (const row of result.results) {
    await pagerDutyFetch(env, pageForRow(row).synthetic_page, fetchImpl);
    await env.CONFIG_DB!.batch([
      env.CONFIG_DB!.prepare(
        `INSERT OR IGNORE INTO synthetic_page_audit_events
           (event_id, drill_id, event_type, occurred_at_ms, correlation_id, source_event_id)
         VALUES (?, ?, 'delivered', ?, ?, ?)`,
      ).bind(
        `delivered:${row.drill_id}`,
        row.drill_id,
        controller.scheduledTime,
        row.correlation_id,
        `delivered:${row.drill_id}`,
      ),
      env.CONFIG_DB!.prepare(
        `UPDATE synthetic_page_drills
            SET delivered_at_ms = ?, outcome = 'unacked'
          WHERE drill_id = ? AND delivery_mode = 'deferred' AND delivered_at_ms IS NULL`,
      ).bind(controller.scheduledTime, row.drill_id),
    ]);
  }
}

async function handlePagerDutyWebhook(request: Request, env: ReceiverEnv): Promise<Response> {
  const environmentError = validateWebhookEnvironment(env);
  if (environmentError !== null) return json({ error: "webhook_not_ready" }, 503);
  if (request.method !== "POST") return json({ error: "method_not_allowed" }, 405);
  const body = await request.text();
  if (!(await verifyPagerDutySignature(body, request.headers.get("x-pagerduty-signature"), env.PAGERDUTY_WEBHOOK_SECRET!))) {
    return json({ error: "invalid_webhook_signature" }, 401);
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(body);
  } catch {
    return json({ error: "invalid_json" }, 400);
  }
  const event = parsePagerDutyWebhook(parsed);
  if (event === null) return json({ error: "invalid_webhook_event" }, 400);
  const row = await env.CONFIG_DB!.prepare(
    `SELECT drill_id, emit_ts_ms, outcome, correlation_id
       FROM synthetic_page_drills WHERE drill_id = ?`,
  ).bind(event.drill_id).first<{ drill_id: string; emit_ts_ms: number; outcome: string; correlation_id: string }>();
  if (row === null) return json({ accepted: true, ignored: true }, 202);
  if (row.outcome !== "unacked") return json({ accepted: true, ignored: true, outcome: row.outcome }, 202);
  const outcome = event.kind === "acknowledged" ? "acked" : "escalated";
  const mtta = Math.max(0, event.occurred_at_ms - row.emit_ts_ms);
  await env.CONFIG_DB!.batch([
    env.CONFIG_DB!.prepare(
      `INSERT OR IGNORE INTO synthetic_page_audit_events
         (event_id, drill_id, event_type, occurred_at_ms, correlation_id, source_event_id, engineer_slug)
       VALUES (?, ?, ?, ?, ?, ?, ?)`,
    ).bind(
      event.event_id,
      event.drill_id,
      outcome,
      event.occurred_at_ms,
      row.correlation_id,
      event.event_id,
      event.engineer_slug,
    ),
    env.CONFIG_DB!.prepare(
      `UPDATE synthetic_page_drills
          SET outcome = ?, engineer_slug = ?, ack_ts_ms = ?, mtta_ms = ?, ack_vector = ?
        WHERE drill_id = ? AND outcome = 'unacked'`,
    ).bind(outcome, event.engineer_slug, event.occurred_at_ms, mtta, event.ack_vector, event.drill_id),
  ]);
  return json({ accepted: true, outcome, drill_id: event.drill_id }, 202);
}

const handler: ExportedHandler<ReceiverEnv> = {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/healthz" && request.method === "GET") return json({ ok: true });
    if (url.pathname === SYNTHETIC_PAGE_PATH) return handleSyntheticPage(request, env);
    if (url.pathname === PAGERDUTY_WEBHOOK_PATH) return handlePagerDutyWebhook(request, env);
    return json({ error: "not_found" }, 404);
  },
  async scheduled(controller, env) {
    await runDeferredDeliveries(controller, env, fetch);
  },
};

export { handlePagerDutyWebhook, handleSyntheticPage, runDeferredDeliveries };
export default handler;
