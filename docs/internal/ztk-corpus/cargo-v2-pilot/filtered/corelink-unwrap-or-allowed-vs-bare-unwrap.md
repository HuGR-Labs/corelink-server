# CoreLink unwrap_or vs bare unwrap in src/
ops: x ! ok <-

rule: CoreLink src/ => zero `unwrap()`/`expect()`/`panic!()`. !bans only bare assertions.

ok `unwrap_or(fallback)` in production code <- supplies fallback not assertion.

x bare `unwrap()`/`panic!()` in src/; ok confined to `#[cfg(test)]` blocks w/ `#[allow(clippy::unwrap_used, clippy::panic)]`.