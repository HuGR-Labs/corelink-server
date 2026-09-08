# B-322: bind the durable D1 client in the mount gate

The Stripe webhook mount now carries the D1 client directly from its existing
fail-closed configuration match, preserving the durable DLQ contract without a
guarded `expect()` that violates the workspace's strict Clippy policy.
