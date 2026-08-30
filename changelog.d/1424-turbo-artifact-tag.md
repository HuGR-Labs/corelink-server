### Fixed

- **Turborepo `x-artifact-tag` was dropped on both verbs, so signature verification silently did nothing (F-009 / WP-9a).**
  A Turborepo client with `TURBO_REMOTE_CACHE_SIGNATURE_KEY` set sends an HMAC over the artifact hash
  and body as `x-artifact-tag` on PUT and expects the remote cache to echo it on GET, where the client
  verifies it locally. CoreLink had **zero occurrences of that header anywhere in the tree** — a customer
  who enabled signature verification against CoreLink got silence rather than verification, which is
  strictly worse than not supporting signatures at all: the client asks to verify and never receives a
  failure. (The separate claim that this surface was poisonable by overwrite did not hold — `PUT` has
  been create-only with a 409 refusal since B-024, verified against the code before this change.)
  The tag is now stored alongside the artifact and returned verbatim. The signing key is the customer's,
  so CoreLink neither computes nor validates the value; its protocol role is store-and-echo of an opaque
  string. Absence of a tag stays valid on both verbs — most clients never enable signatures, so an
  untagged PUT and its GET behave exactly as before and the header is omitted rather than invented;
  fail-closed (400) is reserved for a tag that is *present and malformed*, which is never silently
  dropped. The sidecar lives under a reserved `$tag/` key segment that no client-chosen hash can name,
  since `teamId` is charset-restricted to `[A-Za-z0-9_-]` — the two keyspaces are disjoint by
  construction, where a `<hash>.tag` suffix scheme would have turned the signature channel into a
  poisoning primitive. The 409 create-only semantics, cross-tenant denial, audit-before-mutation
  ordering and `MAX_HASH_LEN` bound are unchanged. Proven RED first: the round-trip test was written to
  compile against the pre-fix code and failed on the assertion (`left: None`), while the untagged
  non-regression control passed both before and after.
