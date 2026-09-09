fn production_result(result: Result<u8, &'static str>) -> u8 {
    // ruleid: corelink.rust.no-unwrap-in-src
    result.unwrap()
}

fn propagated_result(result: Result<u8, &'static str>) -> Result<u8, &'static str> {
    // ok: corelink.rust.no-unwrap-in-src
    let value = result?;
    Ok(value)
}

async fn runtime_bound() {
    // ruleid: corelink.rust.no-tokio-in-lib-crates
    tokio::spawn(async {});
}

async fn runtime_agnostic() {
    // ok: corelink.rust.no-tokio-in-lib-crates
    futures::future::ready(()).await;
}

fn weak_property(result: Result<u8, ExampleError>) {
    // ruleid: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { .. }));

    // ruleid: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { .. },));

    // ruleid: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { .. } | ExampleError::Other));

    // ruleid: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { code, .. } if code > 0));

    // ruleid: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Nested { inner: Inner { .. }, .. }));

    // ruleid: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(select("punctuation;inside-string"), ExampleError::Invalid { .. }));
}

fn strong_property(result: Result<u8, ExampleError>) {
    // ok: corelink.rust.prop-assert-matches-struct-variant
    if let ExampleError::Invalid { code } = result.unwrap_err() {
        prop_assert_eq!(code, 7);
    }
}

fn explicit_fields_are_not_weak(result: Result<u8, ExampleError>) {
    // ok: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { code: 7 }));

    // ok: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { /* .. */ code: 7 }));

    // ok: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { message: " ..," }));

    // ok: corelink.rust.prop-assert-matches-struct-variant
    prop_assert!(matches!(result.unwrap_err(), ExampleError::Invalid { /* ignored: .., */ code: 7 }));
}

fn key_path(result: Result<u8, ExampleError>) -> u8 {
    // ruleid: corelink.rust.no-expect-in-byok-src
    result.expect("key operation")
}

fn fallible_key_path(result: Result<u8, ExampleError>) -> Result<u8, ExampleError> {
    // ok: corelink.rust.no-expect-in-byok-src
    let value = result?;
    Ok(value)
}
