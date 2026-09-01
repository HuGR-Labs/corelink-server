//! B-074 — prove that both money-path mount gates use the canonical internal
//! auth resolver, and that an auth denial stops before every external effect.
//!
//! The two routes share the resolver's security contract: an absent dedicated
//! key may use the shared key, but a present dedicated key is authoritative.
//! Therefore a short dedicated value MUST NOT silently fall back to a valid
//! shared value. The test exercises every resolver state for both routes.
//!
//! This is deliberately one sequential test. `build_state_from_env` reads the
//! process environment, so separate test functions would race over global
//! state. `EnvGuard` restores every touched variable even on an assertion
//! failure, keeping this integration-test binary hermetic.

use std::{
    env,
    ffi::OsString,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use corelink_server::routes::{dpa_accept, tier_select};
use rsa::pkcs8::{EncodePrivateKey, LineEnding};
use rsa::RsaPrivateKey;
use tower::ServiceExt;

/// Deliberately synthetic test material, not a Stripe-shaped credential.
const KEY_64: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OTHER_KEY_64: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
const WRONG_KEY_64: &str = "89abcdef0123456789abcdef0123456789abcdef0123456789abcdef01234567";
const KEY_20: &str = "01234567890123456789";

const ENV_VARS: &[&str] = &[
    "ALL_PROXY",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "all_proxy",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "CLOUDFLARE_ACCOUNT_ID",
    "CF_API_TOKEN",
    "CORELINK_DPA_VERSION",
    "CORELINK_DPA_ACCEPT_AUTH_KEY",
    "CORELINK_INTERNAL_AUTH_KEY",
    "CORELINK_TIER_SELECT_AUTH_KEY",
    "D1_DATABASE_ID",
    "DPA_RECEIPT_SIGNING_KEY",
    "R2_S3_ACCESS_KEY_ID",
    "R2_S3_ENDPOINT",
    "R2_S3_SECRET_ACCESS_KEY",
    "STRIPE_API_BASE",
    "STRIPE_AUTH_MODE",
    "STRIPE_SECRET_KEY",
];

/// Restores process-global configuration on every exit path.
struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn capture(names: &'static [&'static str]) -> Self {
        Self {
            saved: names
                .iter()
                .map(|name| (*name, env::var_os(name)))
                .collect(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            if let Some(value) = value {
                env::set_var(name, value);
            } else {
                env::remove_var(name);
            }
        }
    }
}

/// A local, refusing proxy/effect probe. It counts CONNECTs as D1 attempts and
/// ordinary HTTP requests as Stripe attempts. It never forwards traffic.
struct EffectProbe {
    d1_requests: Arc<AtomicUsize>,
    stripe_requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    url: String,
}

impl EffectProbe {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let d1_requests = Arc::new(AtomicUsize::new(0));
        let stripe_requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let d1_for_thread = Arc::clone(&d1_requests);
        let stripe_for_thread = Arc::clone(&stripe_requests);
        let stop_for_thread = Arc::clone(&stop);
        let worker = thread::Builder::new()
            .name("money-path-effect-probe".to_owned())
            .spawn(move || {
                while !stop_for_thread.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            classify_and_refuse(stream, &d1_for_thread, &stripe_for_thread);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            d1_requests,
            stripe_requests,
            stop,
            worker: Some(worker),
            url: format!("http://{address}"),
        })
    }

    fn reset(&self) {
        self.d1_requests.store(0, Ordering::Release);
        self.stripe_requests.store(0, Ordering::Release);
    }

    fn d1_requests(&self) -> usize {
        self.d1_requests.load(Ordering::Acquire)
    }

    fn stripe_requests(&self) -> usize {
        self.stripe_requests.load(Ordering::Acquire)
    }
}

impl Drop for EffectProbe {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn classify_and_refuse(
    mut stream: TcpStream,
    d1_requests: &AtomicUsize,
    stripe_requests: &AtomicUsize,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let mut request = [0_u8; 1024];
    let bytes_read = stream.read(&mut request).unwrap_or(0);
    if request
        .get(..bytes_read)
        .is_some_and(|request| request.starts_with(b"CONNECT "))
    {
        d1_requests.fetch_add(1, Ordering::AcqRel);
    } else {
        stripe_requests.fetch_add(1, Ordering::AcqRel);
    }
    let _ = stream.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n");
}

fn set_common_env(signing_key_pem: &str, probe_url: &str) {
    env::set_var("CORELINK_DPA_VERSION", "1.0.0");
    env::set_var("R2_S3_ENDPOINT", "https://example.invalid");
    env::set_var("R2_S3_ACCESS_KEY_ID", "test-r2-access-id");
    env::set_var("R2_S3_SECRET_ACCESS_KEY", "test-r2-placeholder");
    env::set_var("CLOUDFLARE_ACCOUNT_ID", "0123456789abcdef0123456789abcdef");
    env::set_var("CF_API_TOKEN", "test-cf-placeholder");
    env::set_var("D1_DATABASE_ID", "00000000-0000-0000-0000-000000000000");
    env::set_var("STRIPE_AUTH_MODE", "direct");
    env::set_var("STRIPE_API_BASE", probe_url);
    env::set_var("STRIPE_SECRET_KEY", "test-stripe-placeholder");
    env::set_var("DPA_RECEIPT_SIGNING_KEY", signing_key_pem);
    // The D1 client uses HTTPS and the local probe refuses CONNECT rather than
    // forwarding it. This makes any accidental effect observable and keeps the
    // test entirely off-network.
    for name in [
        "ALL_PROXY",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "all_proxy",
        "http_proxy",
        "https_proxy",
    ] {
        env::set_var(name, probe_url);
    }
    for name in ["NO_PROXY", "no_proxy"] {
        env::remove_var(name);
    }
}

fn clear_auth_env() {
    env::remove_var("CORELINK_INTERNAL_AUTH_KEY");
    env::remove_var("CORELINK_TIER_SELECT_AUTH_KEY");
    env::remove_var("CORELINK_DPA_ACCEPT_AUTH_KEY");
}

fn test_signing_key_pem() -> Result<String, Box<dyn std::error::Error>> {
    let mut rng = rand::thread_rng();
    let key = RsaPrivateKey::new(&mut rng, 2048)?;
    Ok(key.to_pkcs8_pem(LineEnding::LF)?.to_string())
}

fn assert_resolver_matrix<T>(route: &str, dedicated_env: &str, build: impl Fn() -> Option<T>) {
    clear_auth_env();
    env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_64);
    assert!(
        build().is_some(),
        "{route}: valid shared key must mount when dedicated is unset"
    );

    clear_auth_env();
    env::set_var(dedicated_env, KEY_64);
    assert!(
        build().is_some(),
        "{route}: valid dedicated key alone must mount"
    );

    clear_auth_env();
    assert!(
        build().is_none(),
        "{route}: all auth keys absent must leave route unmounted"
    );

    clear_auth_env();
    env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_20);
    assert!(
        build().is_none(),
        "{route}: a short shared key must leave route unmounted"
    );

    clear_auth_env();
    env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_64);
    env::set_var(dedicated_env, KEY_20);
    assert!(
        build().is_none(),
        "{route}: short dedicated key must fail closed rather than fall back to valid shared key"
    );

    clear_auth_env();
    env::set_var("CORELINK_INTERNAL_AUTH_KEY", KEY_20);
    env::set_var(dedicated_env, KEY_20);
    assert!(
        build().is_none(),
        "{route}: both short keys must leave route unmounted"
    );
}

fn tier_request(auth: &str) -> Result<Request<Body>, http::Error> {
    Request::builder()
        .method("POST")
        .uri("/v1/onboarding/tier-select")
        .header(tier_select::INTERNAL_AUTH_HEADER, auth)
        .header(tier_select::TENANT_HEADER, "tenant-money-proof")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"tier":"free","success_url":"https://humangr.com/success","cancel_url":"https://humangr.com/cancel"}"#,
        ))
}

fn dpa_request(auth: &str) -> Result<Request<Body>, http::Error> {
    Request::builder()
        .method("POST")
        .uri("/v1/onboarding/dpa-accept")
        .header(tier_select::INTERNAL_AUTH_HEADER, auth)
        .header(tier_select::TENANT_HEADER, "tenant-money-proof")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"dpa_version":"1.0.0","dpa_locale":"en","notice_text_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}"#,
        ))
}

fn assert_no_effects(probe: &EffectProbe, route: &str, case: &str) {
    assert_eq!(
        probe.d1_requests(),
        0,
        "{route}: {case} must make exactly zero D1 requests"
    );
    assert_eq!(
        probe.stripe_requests(),
        0,
        "{route}: {case} must make exactly zero Stripe requests"
    );
}

#[test]
fn money_path_enforces_resolver_matrix_and_stops_unauthenticated_requests_before_effects(
) -> Result<(), Box<dyn std::error::Error>> {
    let _env = EnvGuard::capture(ENV_VARS);
    let probe = EffectProbe::start()?;
    let pem = test_signing_key_pem()?;
    set_common_env(&pem, &probe.url);

    assert_resolver_matrix(
        "tier-select",
        "CORELINK_TIER_SELECT_AUTH_KEY",
        tier_select::build_state_from_env,
    );
    assert_resolver_matrix(
        "dpa-accept",
        "CORELINK_DPA_ACCEPT_AUTH_KEY",
        dpa_accept::build_state_from_env,
    );

    // Use two different valid keys. A request carrying the shared key is a
    // real mismatch when the dedicated key is configured; it MUST NOT silently
    // authenticate through the fallback.
    clear_auth_env();
    env::set_var("CORELINK_INTERNAL_AUTH_KEY", OTHER_KEY_64);
    env::set_var("CORELINK_TIER_SELECT_AUTH_KEY", KEY_64);
    let tier_state = tier_select::build_state_from_env().ok_or("tier-select did not mount")?;
    env::set_var("CORELINK_DPA_ACCEPT_AUTH_KEY", KEY_64);
    let dpa_state = dpa_accept::build_state_from_env().ok_or("dpa-accept did not mount")?;

    // `D1Http*Store` owns a blocking reqwest client. It must be dropped after,
    // not inside, an async runtime; otherwise reqwest attempts its blocking
    // shutdown while Tokio forbids blocking and panics. Keep state outside this
    // deliberately scoped runtime, then run only the router futures inside it.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        // Positive controls: a correct dedicated key crosses the handler gate and
        // reaches the first durable D1 boundary exactly once. The probe refuses
        // that request, so neither test can accidentally call the internet.
        probe.reset();
        let response = tier_select::router(tier_state.clone())
            .oneshot(tier_request(KEY_64)?)
            .await?;
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "tier-select positive control must clear auth"
        );
        assert_eq!(
            probe.d1_requests(),
            1,
            "tier-select positive control must reach D1 exactly once"
        );
        assert_eq!(
            probe.stripe_requests(),
            0,
            "tier-select audit failure must stop before Stripe"
        );

        probe.reset();
        let response = dpa_accept::router(dpa_state.clone())
            .oneshot(dpa_request(KEY_64)?)
            .await?;
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "dpa-accept positive control must clear auth"
        );
        assert_eq!(
            probe.d1_requests(),
            1,
            "dpa-accept positive control must reach D1 exactly once"
        );
        assert_eq!(
            probe.stripe_requests(),
            0,
            "dpa-accept has no Stripe collaborator"
        );

        // Both a wrong same-length value and the valid-but-wrong shared key must
        // return 401 before D1, Stripe, or consent persistence. Exact zeroes make
        // the "before effects" assertion observable instead of documentary.
        for (case, presented) in [
            ("wrong", WRONG_KEY_64),
            ("shared-mismatch", OTHER_KEY_64),
            ("empty", ""),
        ] {
            probe.reset();
            let response = tier_select::router(tier_state.clone())
                .oneshot(tier_request(presented)?)
                .await?;
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "tier-select: {case} auth must return 401"
            );
            assert_no_effects(&probe, "tier-select", case);

            probe.reset();
            let response = dpa_accept::router(dpa_state.clone())
                .oneshot(dpa_request(presented)?)
                .await?;
            assert_eq!(
                response.status(),
                StatusCode::UNAUTHORIZED,
                "dpa-accept: {case} auth must return 401"
            );
            assert_no_effects(&probe, "dpa-accept", case);
        }

        Ok::<(), Box<dyn std::error::Error>>(())
    });
    drop(runtime);
    clear_auth_env();
    result
}
