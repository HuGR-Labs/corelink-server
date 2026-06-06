//! Production [`JwksFetcher`] implementation backed by the Cloudflare
//! Workers Fetch API (`worker::Fetch::Url`).
//!
//! # Trait surface analysis
//!
//! The [`JwksFetcher`] trait takes `&self` (not `&mut self`), which is
//! correct. CF Workers are single-threaded; no interior mutability is
//! needed. The `Send + Sync + 'static` bounds on the trait are
//! satisfied because `CfJwksFetcher` holds no state at all (it's a
//! zero-size struct). The CF Fetch API is accessed via a module-level
//! function `worker::Fetch::Url(...)` — no binding from Env required.
//!
//! One subtle issue validated here: the [`JwksFetchFuture`] type alias
//! is `Pin<Box<dyn Future<Output = ...> + Send + 'a>>`. The
//! `wasm_bindgen_futures::JsFuture` is NOT `Send` in Rust's type
//! system. Workers-rs works around this with `unsafe impl Send` on its
//! internal types. Our impl uses `worker::send::SendFuture` to wrap the
//! non-Send future and satisfy the `Send` bound — this is the canonical
//! workers-rs pattern documented in their README.

use corelink_clerk::jwks::{Jwks, JwksFetchError, JwksFetchFuture, JwksFetcher};
use worker::{Fetch, Method, Request, RequestInit};

/// Production [`JwksFetcher`] that calls the CF Workers Fetch API.
///
/// This is a zero-size struct — there is no state to carry. The
/// CF Fetch API is a runtime global, not a binding.
///
/// ```ignore
/// let fetcher = CfJwksFetcher::new();
/// ```
#[derive(Clone, Debug)]
pub struct CfJwksFetcher;

impl CfJwksFetcher {
    /// Construct a new fetcher. No-op (zero-size type).
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for CfJwksFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl JwksFetcher for CfJwksFetcher {
    /// Fetch a JWKS document from `url` via the CF Workers Fetch API.
    ///
    /// - Sends a `GET` request with no body.
    /// - Requires the URL to be HTTPS (enforced by the adapter's config
    ///   validation, which rejects any non-HTTPS `jwks_url`).
    /// - Maps CF `worker::Error` variants to the canonical
    ///   [`JwksFetchError`] taxonomy.
    fn fetch<'a>(&'a self, url: &'a str) -> JwksFetchFuture<'a> {
        Box::pin(worker::send::SendFuture::new(async move {
            let mut init = RequestInit::new();
            init.with_method(Method::Get);
            let req = Request::new_with_init(url, &init)
                .map_err(|e| JwksFetchError::Transport(format!("build request: {e}")))?;
            let mut response = Fetch::Request(req)
                .send()
                .await
                .map_err(|e| JwksFetchError::Transport(format!("fetch: {e}")))?;
            let status = response.status_code();
            if !(200..300).contains(&status) {
                return Err(JwksFetchError::HttpStatus { status });
            }
            let body = response
                .bytes()
                .await
                .map_err(|e| JwksFetchError::Transport(format!("read body: {e}")))?;
            Jwks::parse(&body)
        }))
    }
}
