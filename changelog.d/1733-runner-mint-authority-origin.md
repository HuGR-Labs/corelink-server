### Fixed

- **Runner mint now has its production Fabric authority origin.** All five production Worker environments explicitly configure the public HTTPS origin required for tenant credential-lifecycle checks, so the route no longer fails closed because its URL is missing. The issuer authentication key remains a separately provisioned Worker secret.
