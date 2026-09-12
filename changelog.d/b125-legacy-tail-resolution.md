- Harden the audit drain against ambiguous legacy tails with a signed,
  append-only resolution ledger; preserve all fork evidence and fail closed
  unless one exact branch is authenticated by the existing signed checkpoint.
- Detect ambiguous maximum tails independently from head/hash mismatch in the
  read-only B-125 integrity verifier.
