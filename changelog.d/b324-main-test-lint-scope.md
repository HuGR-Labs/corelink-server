# B-324: keep the server test lint scope unique

The server binary's extracted test module now declares its test-only unwrap
and expect allowance exactly once, allowing strict all-targets Clippy to
inspect the target without a duplicated-attribute failure.
