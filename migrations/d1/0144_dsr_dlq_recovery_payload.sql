-- B-216: retain a replayable DSR request before consuming an exhausted DLQ item.
-- Nullable keeps existing receipt rows readable; the consumer fills this
-- column before every terminal ACK and redrives legacy rows on replay.
ALTER TABLE dsr_dlq_delivery_receipts ADD COLUMN recovery_payload_json TEXT;

