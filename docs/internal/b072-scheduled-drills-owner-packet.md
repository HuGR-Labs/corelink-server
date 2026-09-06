# B-072 owner packet — external delivery remains outstanding

Status: open. The main Worker now has a fail-closed scheduled handoff, but this
repository does not contain the receiver Worker or a `SCHEDULED_DRILL_DELIVERY`
binding in `wrangler.toml`. The local tests therefore prove only that a real
binding would be called and that failures are retried; they do not prove that
PagerDuty accepted a page or that a human received it.

Before changing B-072 to `done`, the owner must:

1. Deploy the receiver Worker in the same Cloudflare account and bind it as
   `SCHEDULED_DRILL_DELIVERY` for the non-production environment that owns the
   default Worker cron. The receiver must validate the fixed synthetic scheduler
   contract and deduplicate the `x-corelink-scheduled-drill-id`. For synthetic
   week 3, it must honor `delivery_mode=deferred` and schedule `emit_at_ms` for the
   following Sunday at exactly 23:59:00 UTC; it must not page immediately at
   the Monday cron time.
2. Wire the receiver's real PagerDuty Events API v2 integration using the
   dedicated `synthetic-drill` service routing key. The key must remain outside
   this Worker and its payload/logs.
3. Prove one successful synthetic-page path end to end: Worker cron log →
   receiver request → PagerDuty incident on `synthetic-drill` → D1
   `synthetic_page_drills` row → acknowledgement/escalation outcome. Attach the
   PagerDuty incident identifier and timestamps (including the week-3 Sunday
   boundary when that rotation is exercised) as restricted operational
   evidence; do not commit credentials or customer data.
4. Re-run the B-072 focused Vitest, static verifier, and an adversarial retry
   test where the receiver returns 5xx after accepting the dedup key. Only then
   update the backlog status and `last-verified` evidence.

The current scheduler intentionally reports a non-2xx/exception as failure and
does not call `noRetry()` for those cases, preserving Cloudflare retry behavior.
Unknown cron values call `noRetry()` and fail, so a configuration drift cannot
create an unbounded external page loop.
