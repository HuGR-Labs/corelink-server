### Added

- **B-071 GC artifact wiring.** The production container now carries the
  existing `gc_sweep` executable as a separately invoked utility. The shipped
  entrypoint remains the server, and every repository automation path is
  credentialless, bounded, audited, and forced to dry-run. No production
  customer data is deleted by this change; real R2/D1 adapters and the first
  owner-approved production execution remain explicitly gated.
