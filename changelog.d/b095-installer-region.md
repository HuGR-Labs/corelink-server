### Fixed

- **B-095: the installer no longer accepts a client-side `--region` selector that the CLI ignored.** The served script now rejects unsupported options and its usage text advertises only the required token; tenant residency remains governed by the server-side tenant configuration. Focused parser tests cover token propagation, option rejection, truthful usage, and the Worker render call site.
