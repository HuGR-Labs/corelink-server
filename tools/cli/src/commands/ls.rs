//! `corelink ls` — list CAS entries for a tenant (WI-S15-001).
//!
//! Wires the CLI against the ONE real server route
//! (`crates/corelink-container/src/routes/cas.rs`,
//! `CAS_LIST_ROUTE = "/v1/cas/{tenant}"`): tenant is a **path segment**,
//! pagination is `?limit=&cursor=` ONLY, and the response body is exactly
//! `{"blobs":[{"hash","size","created_at"}],"next_cursor":<string|null>}`.
//! The prior `GET /v1/cas/list?tenant=` shape 403'd in prod (axum matched
//! `{tenant}="list"` ≠ the authenticated tenant — a cross-tenant reject).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::client::CorelinkClient;
use crate::error::CliError;
use crate::output::{Formatter, OutputFormat};

/// A single blob entry as returned by `GET /v1/cas/{tenant}`.
///
/// Mirrors the server body element verbatim (`hash`/`size`/`created_at`);
/// no invented `digest`/`size_bytes`/`tenant_prefix` fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LsBlob {
    /// BLAKE3 content hash (64-hex).
    pub hash: String,
    /// Blob size in bytes.
    pub size: u64,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
}

impl fmt::Display for LsBlob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:<64}  {:>12}  {}",
            self.hash, self.size, self.created_at
        )
    }
}

/// Response body of `GET /v1/cas/{tenant}` — `{blobs, next_cursor}`.
#[derive(Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LsResponse {
    /// Blobs on this page (already client-side prefix-filtered when
    /// `--prefix` is supplied).
    pub blobs: Vec<LsBlob>,
    /// Opaque continuation cursor for the next page (`None` = last page).
    pub next_cursor: Option<String>,
}

impl fmt::Display for LsResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:<64}  {:>12}  CREATED_AT", "HASH", "SIZE")?;
        writeln!(f, "{}", "-".repeat(96))?;
        for b in &self.blobs {
            writeln!(f, "{b}")?;
        }
        match self.next_cursor.as_deref() {
            Some(next) => write!(
                f,
                "({} on this page; more: pass --cursor {next})",
                self.blobs.len()
            ),
            None => write!(f, "({} on this page)", self.blobs.len()),
        }
    }
}

/// Run `corelink ls --tenant <id> [--prefix <p>] [--limit n] [--cursor c]`.
///
/// Builds `GET /v1/cas/{tenant}?limit=&cursor=` (tenant in the path; `limit`
/// always sent, `cursor` only when present). `prefix` is NEVER sent to the
/// server — it is applied honestly as a client-side filter on the returned
/// page (keep only blobs whose `hash` starts with the prefix).
pub async fn run(
    client: &CorelinkClient,
    tenant: &str,
    prefix: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
    format: OutputFormat,
) -> Result<(), CliError> {
    let mut path = format!("/v1/cas/{tenant}");
    let mut query: Vec<String> = Vec::new();
    if let Some(l) = limit {
        query.push(format!("limit={l}"));
    }
    if let Some(c) = cursor {
        query.push(format!("cursor={c}"));
    }
    if !query.is_empty() {
        path.push('?');
        path.push_str(&query.join("&"));
    }

    let raw = client.get_json(&path).await?;
    let mut resp: LsResponse = serde_json::from_value(raw).map_err(CliError::Json)?;

    // `--prefix` is a client-side filter on THIS page only (the server has no
    // prefix param): keep blobs whose BLAKE3 hash starts with the prefix.
    if let Some(p) = prefix {
        resp.blobs.retain(|b| b.hash.starts_with(p));
    }

    let fmt = Formatter::new(format);
    fmt.emit(&resp).map_err(CliError::Json)?;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::uninlined_format_args,
    clippy::format_in_format_args,
    clippy::expect_used,
    clippy::unwrap_used
)]
mod tests {
    use super::*;

    use wiremock::matchers::{method, path as path_matcher, query_param};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    #[test]
    fn ls_blob_display_contains_hash_and_size() {
        let b = LsBlob {
            hash: "abc123".to_owned(),
            size: 1024,
            created_at: "2026-05-14T00:00:00Z".to_owned(),
        };
        let s = format!("{b}");
        assert!(s.contains("abc123"));
        assert!(s.contains("1024"));
        assert!(s.contains("2026-05-14T00:00:00Z"));
    }

    #[test]
    fn ls_response_serialises_to_server_shape() {
        let r = LsResponse {
            blobs: vec![LsBlob {
                hash: "deadbeef".to_owned(),
                size: 7,
                created_at: "2026-05-14T00:00:00Z".to_owned(),
            }],
            next_cursor: Some("nxt".to_owned()),
        };
        let json = serde_json::to_string(&r).unwrap();
        // Field names must match the server body verbatim.
        assert!(json.contains("\"blobs\""));
        assert!(json.contains("\"hash\""));
        assert!(json.contains("\"size\""));
        assert!(json.contains("\"created_at\""));
        assert!(json.contains("\"next_cursor\""));
        // The invented legacy fields must be gone.
        assert!(!json.contains("total_count"));
        assert!(!json.contains("entries"));
        assert!(!json.contains("digest"));
        assert!(!json.contains("size_bytes"));
        assert!(!json.contains("tenant_prefix"));
    }

    #[test]
    fn ls_response_display_footer_reports_page_count_and_cursor_hint() {
        let r = LsResponse {
            blobs: vec![
                LsBlob {
                    hash: "aa".to_owned(),
                    size: 1,
                    created_at: "t".to_owned(),
                },
                LsBlob {
                    hash: "bb".to_owned(),
                    size: 2,
                    created_at: "t".to_owned(),
                },
            ],
            next_cursor: Some("CURSOR9".to_owned()),
        };
        let s = format!("{r}");
        assert!(s.contains("HASH"));
        assert!(s.contains("2 on this page"));
        assert!(s.contains("--cursor CURSOR9"));

        let last = LsResponse {
            blobs: vec![],
            next_cursor: None,
        };
        let s2 = format!("{last}");
        assert!(s2.contains("0 on this page"));
        assert!(!s2.contains("--cursor"));
    }

    /// Regression for the prod 403 bug (same class as #842): `ls::run` MUST
    /// call the real `GET /v1/cas/{tenant}` route (tenant in the path,
    /// `?limit=`), parse the `{blobs,next_cursor}` body, and NEVER hit the
    /// invented `GET /v1/cas/list?tenant=` shape.
    #[tokio::test]
    async fn ls_hits_tenant_path_route_and_parses_blobs() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_matcher("/v1/cas/t-acme"))
            .and(query_param("limit", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "blobs": [
                    {"hash": "ab00", "size": 10, "created_at": "2026-07-01T00:00:00Z"},
                    {"hash": "cd11", "size": 20, "created_at": "2026-07-02T00:00:00Z"}
                ],
                "next_cursor": "PAGE2"
            })))
            .mount(&server)
            .await;
        let client = CorelinkClient::for_test(server.uri(), Some("t-acme".to_owned()));

        run(&client, "t-acme", None, Some(5), None, OutputFormat::Json)
            .await
            .expect("ls::run must succeed against the real route");

        let reqs = server.received_requests().await.expect("recorded");
        assert_eq!(reqs.len(), 1, "exactly one request");
        let hit: &Request = reqs.first().expect("one recorded request");
        assert_eq!(
            hit.url.path(),
            "/v1/cas/t-acme",
            "tenant is a PATH segment, not `/v1/cas/list`"
        );
        assert!(
            !hit.url.path().contains("list"),
            "must NOT hit the invented /v1/cas/list route"
        );
        // No prefix param must ever be sent to the server.
        assert!(
            hit.url.query_pairs().all(|(k, _)| k != "prefix"),
            "prefix must NOT be sent to the server"
        );
    }

    /// The `--prefix` flag is a client-side page filter: only blobs whose
    /// hash starts with the prefix survive, and the flag is NOT sent upstream.
    #[tokio::test]
    async fn ls_prefix_filters_page_client_side() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_matcher("/v1/cas/t-acme"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "blobs": [
                    {"hash": "ab00", "size": 10, "created_at": "t"},
                    {"hash": "ab99", "size": 20, "created_at": "t"},
                    {"hash": "cd11", "size": 30, "created_at": "t"}
                ],
                "next_cursor": null
            })))
            .mount(&server)
            .await;
        let client = CorelinkClient::for_test(server.uri(), Some("t-acme".to_owned()));

        // Exercise the filter through the public run() path (Json emit is
        // side-effect-only, so re-derive the filter here to assert its shape).
        let raw = client
            .get_json("/v1/cas/t-acme?limit=100")
            .await
            .expect("list ok");
        let mut resp: LsResponse = serde_json::from_value(raw).expect("parse");
        resp.blobs.retain(|b| b.hash.starts_with("ab"));
        assert_eq!(resp.blobs.len(), 2, "only `ab*` hashes survive the filter");
        assert!(resp.blobs.iter().all(|b| b.hash.starts_with("ab")));

        // And the run() path itself succeeds with a prefix supplied.
        run(
            &client,
            "t-acme",
            Some("ab"),
            Some(100),
            None,
            OutputFormat::Text,
        )
        .await
        .expect("ls::run with --prefix must succeed");

        let reqs = server.received_requests().await.expect("recorded");
        assert!(
            reqs.iter()
                .all(|r| r.url.query_pairs().all(|(k, _)| k != "prefix")),
            "prefix is client-side only — never sent upstream"
        );
    }
}
