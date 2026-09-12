# B-072 owner packet — external delivery remains outstanding

Status: open. The repository now contains the fail-closed receiver Worker and
the default/dev `SCHEDULED_DRILL_DELIVERY` service binding. The receiver stores
the trigger before calling PagerDuty, records delivery only after PagerDuty
accepts the canonical dedup key, and accepts signed acknowledgement/escalation
webhooks only after that durable receipt. These local contracts still do not
prove that the receiver was deployed, PagerDuty accepted a real page, or a
human received it.

Before changing B-072 to `done`, the owner must:

1. Deploy the committed receiver Worker in the same Cloudflare account and
   verify the committed `SCHEDULED_DRILL_DELIVERY` binding for the non-production
   environment that owns the default Worker cron. The receiver validates the
   fixed synthetic scheduler contract and deduplicates the
   `x-corelink-scheduled-drill-id`. For synthetic
   week 3, it must honor `delivery_mode=deferred` and schedule `emit_at_ms` for the
   following Sunday at exactly 23:59:00 UTC; it must not page immediately at
   the Monday cron time.
2. Wire the receiver's real PagerDuty Events API v2 integration using the
   dedicated `synthetic-drill` service routing key, and configure its signed
   webhook subscription to the exact committed staging callback path. Both
   secrets must remain outside this Worker source and its payload/logs.
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
