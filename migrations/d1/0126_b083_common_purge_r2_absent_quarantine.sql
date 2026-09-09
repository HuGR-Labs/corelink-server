-- 0126_b083_common_purge_r2_absent_quarantine.sql
--
-- 0121 correctly forbids an r2_absent row from returning to ordinary retry,
-- but its original forward-only trigger also rejected the terminal quarantine
-- state.  The common validator must be able to retain that checkpoint for the
-- first two invalid-identity observations and quarantine on the third.
--
-- Migration application is ledger-once.  The DROP is an audited replacement
-- of the pre-existing trigger; it does not delete data or weaken any identity
-- column.  The additive migration gate requires the explicit waiver below.

DROP TRIGGER IF EXISTS trg_byok_object_purge_forward_only; -- additive-allowed: ADR-0031 replace forward-only purge trigger to permit bounded r2_absent quarantine

CREATE TRIGGER IF NOT EXISTS trg_byok_object_purge_forward_only
BEFORE UPDATE ON byok_object_purge_item
WHEN OLD.purge_id IS NOT NEW.purge_id
  OR OLD.tenant_id IS NOT NEW.tenant_id
  OR OLD.intent_id IS NOT NEW.intent_id
  OR OLD.object_kind IS NOT NEW.object_kind
  OR OLD.logical_key IS NOT NEW.logical_key
  OR OLD.generation IS NOT NEW.generation
  OR OLD.allocation_id IS NOT NEW.allocation_id
  OR OLD.physical_key IS NOT NEW.physical_key
  OR OLD.object_size IS NOT NEW.object_size
  OR OLD.object_blake3 IS NOT NEW.object_blake3
  OR OLD.crypto_mode IS NOT NEW.crypto_mode
  OR OLD.reason IS NOT NEW.reason
  OR NEW.claim_epoch < OLD.claim_epoch
  OR NEW.attempts < OLD.attempts
  OR ((OLD.claim_owner IS NOT NEW.claim_owner OR OLD.claim_token IS NOT NEW.claim_token)
      AND NEW.state NOT IN ('verified', 'quarantined')
      AND NEW.claim_epoch <= OLD.claim_epoch)
  OR (OLD.claim_token IS NEW.claim_token AND OLD.claim_expires_at_ms IS NOT NULL
      AND NEW.claim_expires_at_ms < OLD.claim_expires_at_ms)
  OR (OLD.state = 'pending' AND NEW.state NOT IN ('pending', 'deleting'))
  OR (OLD.state = 'deleting' AND NEW.state NOT IN
      ('deleting', 'retry', 'r2_absent', 'quarantined'))
  OR (OLD.state = 'retry' AND NEW.state NOT IN ('retry', 'deleting', 'quarantined'))
  OR (OLD.state = 'r2_absent' AND NEW.state NOT IN ('r2_absent', 'verified', 'quarantined'))
  OR (OLD.state IN ('verified', 'quarantined') AND NEW.state IS NOT OLD.state)
BEGIN
    SELECT RAISE(ABORT, 'BYOK purge identity/state is not forward-only');
END;
