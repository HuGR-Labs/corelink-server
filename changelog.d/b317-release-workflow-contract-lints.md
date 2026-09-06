# B-317: keep the CLI release contract strict-lint clean

The release workflow contract test now propagates fixture-read failures through
`Result`, checks the optional retry branch before destructuring it, and retains
all existing mutation assertions without `panic!`, `expect`, `unwrap`, or lint
suppression. A lightweight fail-closed verifier locks that source shape without
running Cargo.
