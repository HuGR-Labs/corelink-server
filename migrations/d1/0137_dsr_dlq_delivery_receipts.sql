CREATE TABLE IF NOT EXISTS dsr_dlq_delivery_receipts (
  event_id TEXT PRIMARY KEY NOT NULL CHECK (
    length(event_id) = 80
    AND substr(event_id, 1, 16) = 'dsr-erasure-dlq:'
    AND substr(event_id, 17) NOT GLOB '*[^0-9a-f]*'
  ),
  status TEXT NOT NULL CHECK (status IN (
    'paging_pending',
    'paging_claimed',
    'paging_retry',
    'paging_ambiguous',
    'paging_recovered',
    'paged',
    'requeue_claimed',
    'requeued',
    'terminal',
    'delivery_exhausted',
    'requeue_ambiguous'
  )),
  paging_claimed INTEGER NOT NULL DEFAULT 0 CHECK (paging_claimed IN (0, 1)),
  requeue_claimed INTEGER NOT NULL DEFAULT 0 CHECK (requeue_claimed IN (0, 1)),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0)
);

CREATE INDEX IF NOT EXISTS idx_dsr_dlq_delivery_receipts_status_updated
  ON dsr_dlq_delivery_receipts (status, updated_at_ms, event_id);
