# CoreLink reqwest TLS backend = rustls
ops: x <-
vars: DEP=`reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }`

tls: CoreLink workspace dep DEP => rustls backend <- `rustls-tls` feature + `default-features = false`.
     x native-tls.

refs: [[corelink-axum-version-alignment]]