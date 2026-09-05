### Fixed

- **B-095:** team invites now use the persisted `owner`/`admin`/`member`/`viewer`
  role contract and reject unsupported or non-grantable roles; workspace pinning
  is an idempotent explicit state update; and installers no longer accept or
  advertise the unsupported client-side `--region` option.
