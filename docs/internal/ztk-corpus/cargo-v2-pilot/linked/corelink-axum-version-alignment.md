# CoreLink axum version alignment
ops: <- =>
vars: AX=`axum 0.7`

AX <- aligns with tonic 0.12 (uses hyper 1) => tower + http 1.x.
AX => default-features=false features=["http1", "tokio", "json"].

refs: [[corelink-axum-server-body-via-bodyext]] [[corelink-no-must-use-on-router]] [[corelink-reqwest-rustls-config]]