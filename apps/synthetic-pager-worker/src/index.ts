import {
  buildPagerDutyEvent,
  PAGERDUTY_EVENTS_URL,
  parseSyntheticPageEnvelope,
  SYNTHETIC_PAGE_PATH,
  stableDrillId,
  validateReceiverEnvironment,
  type ReceiverEnv,
  type SyntheticPageEnvelope,
} from "./contract.js";

const json = (body: unknown, status = 200): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });

async function persistDelivery(env: ReceiverEnv, envelope: SyntheticPageEnvelope): Promise<void> {
  // Boundary-handoff is an accepted scheduling marker, not a page at this
  // invocation. The existing table deliberately permits only actual page
  // regions; no row is written until a future explicit activation emits it.
  if (envelope.synthetic_page.delivery_mode === "deferred") return;
  const drillId = await stableDrillId(envelope.synthetic_page.dedup_key);
  await env.CONFIG_DB!.prepare(
    `INSERT OR IGNORE INTO synthetic_page_drills
       (drill_id, region, severity, emit_ts_ms, outcome, correlation_id, schema_version)
     VALUES (?, ?, ?, ?, 'unacked', ?, 1)`,
  )
    .bind(
      drillId,
      envelope.synthetic_page.region,
      envelope.synthetic_page.synthetic_severity,
      envelope.synthetic_page.emit_at_ms,
      envelope.synthetic_page.correlation_id,
    )
    .run();
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

  // A deferred boundary marker is acknowledged without paging. It cannot be
  // mistaken for a production page and remains deterministic on scheduler retry.
  if (envelope.synthetic_page.delivery_mode === "deferred") {
    return json({ accepted: true, deferred: true, dedup_key: deliveryId }, 202);
  }

  try {
    await persistDelivery(env, envelope);
  } catch {
    // Do not contact PagerDuty if the audit/idempotency record cannot be written.
    return json({ error: "receiver_storage_unavailable" }, 503);
  }

  const pagerDutyResponse = await fetch(PAGERDUTY_EVENTS_URL, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(buildPagerDutyEvent(envelope.synthetic_page, env.PAGERDUTY_SYNTHETIC_ROUTING_KEY!)),
  }).catch(() => null);
  if (pagerDutyResponse === null || !pagerDutyResponse.ok) {
    // The scheduler retries this delivery. PagerDuty receives the same dedup
    // key and correlation id, so an accepted event remains idempotent.
    return json({ error: "pagerduty_delivery_failed" }, 502);
  }
  return json({ accepted: true, dedup_key: deliveryId }, 202);
}

const handler: ExportedHandler<ReceiverEnv> = {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname === "/healthz" && request.method === "GET") return json({ ok: true });
    if (url.pathname !== SYNTHETIC_PAGE_PATH) return json({ error: "not_found" }, 404);
    return handleSyntheticPage(request, env);
  },
};

export default handler;
