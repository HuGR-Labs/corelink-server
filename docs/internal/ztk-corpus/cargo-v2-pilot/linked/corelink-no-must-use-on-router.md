# CoreLink axum build_router no redundant must_use
ops: x !
vars: R=`Router`

x `#[must_use]` on fn returning axum R (e.g. `build_router`) <- R already `#[must_use]`. ! clippy flags redundant attribute under `-D warnings`.

refs: [[corelink-axum-version-alignment]] [[corelink-axum-server-body-via-bodyext]] [[corelink-unwrap-or-allowed-vs-bare-unwrap]]