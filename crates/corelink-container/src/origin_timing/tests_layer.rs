//! The axum middleware end to end: a real request through
//! `origin_timing_layer`, asserting the phase a handler was charged to reaches
//! the response header.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

use super::tests_support::parse;
use super::{origin_timing_layer, timed, Phase};

#[tokio::test]
async fn the_layer_attributes_a_delay_to_the_phase_it_was_charged_to() {
    let app = Router::new()
        .route(
            "/x",
            get(|| async {
                timed(
                    Phase::Quota,
                    tokio::time::sleep(std::time::Duration::from_millis(60)),
                )
                .await;
                StatusCode::OK
            }),
        )
        .layer(axum::middleware::from_fn(origin_timing_layer));

    let resp = app
        .oneshot(
            HttpRequest::builder()
                .uri("/x")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let header = resp
        .headers()
        .get(super::SERVER_TIMING)
        .expect("the layer must stamp Server-Timing on every response")
        .to_str()
        .unwrap()
        .to_owned();
    let parsed = parse(&header);
    assert!(
        parsed["oquota"] >= 50,
        "oquota did not absorb the 60ms charged to it — this clock is wired \
         to the wrong await. Header: {header}"
    );
    assert!(!parsed.contains_key("opat"));
    assert!(!parsed.contains_key("ostore"));
    // And it still reconciles: nothing escapes into an unnamed gap.
    assert!(parsed["ohandler"] >= 0);
}
