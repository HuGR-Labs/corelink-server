### Fixed

- Freeze a request-scoped staging ownership contract so synthetic D1 writes can register in the same transaction, R2 writes retain a durable reconcile intent, and signup-worker writes accept only a signed request-bound admission envelope.
