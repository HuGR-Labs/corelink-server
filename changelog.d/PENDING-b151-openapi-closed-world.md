### Fixed

- **B-151 closes the divergent second OpenAPI contract and stale RBAC translations.**
  The docs download is now generated from the canonical YAML, and the closed-world
  gate rejects missing or extra paths and methods, operation identity drift, and
  document-level mutations; ten focused mutation tests keep the gate non-vacuous.
