### Fixed

- Removed PAT-bearing `--remote_header` and `curl -H` recipes from the
  published Bazel, tutorial, migration, troubleshooting, and marketing
  surfaces in every translated locale. These examples now use the host-scoped
  Bazel credential helper or curl's stdin config, keeping the PAT out of
  process arguments and logs.
