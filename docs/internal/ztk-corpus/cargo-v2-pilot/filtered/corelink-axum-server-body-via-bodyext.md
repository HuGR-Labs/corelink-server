# CoreLink axum body collection via BodyExt
ops: > x

incoming request body collect: use `http_body_util::BodyExt` w/ `axum::body::Body` streaming > `futures_util`. x `futures_util` for this.