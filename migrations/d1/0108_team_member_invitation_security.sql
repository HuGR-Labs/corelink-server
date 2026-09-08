-- CoreLink D1 — migration 0108: cryptographically bound team invitations.
--
-- B-073: an email hash is an account hint, not an authorization artifact.
-- New invitations persist only a SHA-256 digest of a 256-bit random opaque
-- capability. Acceptance must present that capability, a verified Clerk
-- primary email, and an unexpired invited row; the conditional update is the
-- replay guard. Existing rows remain NULL and are intentionally not redeemable
-- (they must be re-issued), preserving fail-closed behavior.
--
-- INV-AUTH-MIGRATION-ADDITIVE: no existing rows or columns are rewritten.

ALTER TABLE team_member ADD COLUMN invitation_token_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_team_member_invitation_token_hash
    ON team_member (invitation_token_hash)
    WHERE invitation_token_hash IS NOT NULL;
