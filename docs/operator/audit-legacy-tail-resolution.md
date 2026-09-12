# Legacy audit-tail resolution

This procedure is an exceptional continuation mechanism for a legacy audit
partition whose maximum `sequence_number` has multiple sealed rows. It never
deletes, rewrites, re-sequences, or unquarantines an audit row.

The normal drain remains fail-closed. It may resume an ambiguous maximum only
when all of these facts hold simultaneously:

1. the live checkpoint is strictly v1/legacy and its Ed25519 signature verifies
   under the current key while `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME` is off;
2. `checkpoint.next_sequence = max(sequence_number) + 1`;
3. exactly one maximum-sequence row has the checkpoint's `head_hash`; that row
   is archived, not quarantined, and has no quarantine reason, while every
   competing row is unarchived and quarantined with a non-empty reason;
4. every candidate link re-hashes from its stored `prev_hash` and exact
   `canonical_jcs`;
5. an append-only `audit_chain_legacy_tail_resolution` record binds the exact
   checkpoint, selected row, complete candidate-set commitment, and partition;
   its domain-separated resolution signature verifies under the same current
   operator-held key;
6. the head CAS still matches the verified signature, key id, and all NULL
   legacy epoch fields when the next head is advanced.

The resolution table introduced by migration 0123 is empty and inert. A record
must be produced and reviewed offline before a separate authorized production
operation inserts it. Missing or changed evidence always leaves the partition
stopped.

`dd35a645/enam` is structurally eligible only after the exact signed record is
produced. `bba0ff1d/enam` is not eligible: its checkpoint is ahead of the
maximum rows and matches neither branch. It requires an independently witnessed
new epoch. The current B-054 v2 writer explicitly refuses that transition until
external witness wiring exists, so no legacy fallback is permitted.

Run `scripts/verify_b125_chain_integrity.py` against a saved D1 response or via
the read-only D1 API after any rollout. Live mode reads `CLOUDFLARE_API_TOKEN`
only from the environment; never put credentials on the command line. An
ambiguous maximum is a failure even
when the checkpoint hash happens to match one of its rows; this catches the
historical `dd35a645` blind spot.
