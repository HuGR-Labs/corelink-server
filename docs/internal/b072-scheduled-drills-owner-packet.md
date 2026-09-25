# B-072 owner packet — external delivery remains outstanding

Status: open. The repository now contains the fail-closed receiver Worker and
the dormant default/dev `SCHEDULED_DRILL_DELIVERY` service binding. The root
Worker has no active synthetic cron. The receiver stores
the trigger before any external delivery. Configured PagerDuty mode records
delivery only after PagerDuty accepts the canonical dedup key and accepts
signed acknowledgement/escalation webhooks only after that durable receipt.
Explicit `provider_deferred` mode requires no PagerDuty credentials and records
a terminal D1 receipt bound to the scheduled execution, scheduler revision,
serving SHA, receiver revision, and receiver result. It makes no claim about
alert delivery or human reachability. Repository contracts still do not prove
that the receiver was deployed or that a live D1 row exists.

Before changing B-072 to `done`, the owner must:

1. Deploy the committed receiver Worker in the same Cloudflare account and
   verify the committed `SCHEDULED_DRILL_DELIVERY` binding for the non-production
   environment that will own any explicitly reactivated Worker trigger. There
   is no active trigger at this commit; receiver deployment alone is not trigger
   evidence. The receiver validates the
   fixed synthetic scheduler contract and deduplicates the
   `x-corelink-scheduled-drill-id`. For synthetic
   week 3, it must honor `delivery_mode=deferred` and schedule `emit_at_ms` for the
   following Sunday at exactly 23:59:00 UTC; it must not page immediately at
   the Monday cron time.
2. Keep PagerDuty deferred under the current owner decision. If an owner later
   restores configured PagerDuty delivery, use the dedicated `synthetic-drill`
   service key and exact signed webhook callback; both secrets stay outside
   this Worker source and its payload/logs.
3. After an owner-approved non-production trigger is explicitly reactivated,
   prove one provider-deferred path end to end: trigger log → receiver request
   → correlated D1 `synthetic_page_provider_receipts` row and
   `provider_deferred` audit event. Bind the row to scheduled execution,
   scheduler revision, serving SHA, receiver revision, and receiver result.
   Do not claim alert delivery or human acknowledgement; do not commit secrets
   or customer data.
4. Re-run the B-072 focused Vitest, static verifier, and an adversarial retry
   test where the receiver returns 5xx after accepting the dedup key. Only then
   update the backlog status and `last-verified` evidence.

The current scheduler intentionally reports a non-2xx/exception as failure and
does not call `noRetry()` for those cases, preserving Cloudflare retry behavior.
Unknown cron values call `noRetry()` and fail, so a configuration drift cannot
create an unbounded external page loop.
