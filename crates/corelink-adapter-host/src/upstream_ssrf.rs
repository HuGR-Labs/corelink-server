//! Shared SSRF guard for the read-through upstream fetchers (brew / pip / npm,
//! and — F3.2 increment 4 — the public-base OCI pull-through mirror).
//!
//! Every adapter that fetches from a public upstream on a cache miss must refuse
//! to let a redirect `Location` bounce the request at an internal IP literal (the
//! classic SSRF escalation: loopback, RFC-1918 / RFC-4193 private space,
//! link-local — which includes the `169.254.169.254` cloud-metadata endpoint —
//! carrier-grade NAT, broadcast, documentation, and the unspecified address).
//!
//! This module is the SINGLE audited copy of that guard. brew/pip/npm previously
//! each carried their own copy; they had DRIFTED — brew blocked carrier-grade
//! NAT (`100.64.0.0/10`), IPv4 broadcast and documentation ranges while pip and
//! npm did NOT, so the pip/npm fetchers had a strictly weaker SSRF guard. This
//! consolidation adopts brew's (strongest) classification for all three, closing
//! that gap, and gives the increment-4 mirror one reviewed guard to build on.

use std::net::{IpAddr, Ipv4Addr};

fn ipv4_is_internal(v4: Ipv4Addr) -> bool {
    // Destructure (no indexing — `indexing_slicing` is denied).
    let [a, b, _, _] = v4.octets();
    v4.is_private()
        || v4.is_loopback()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_documentation()
        // Carrier-grade NAT 100.64.0.0/10 — internal-ish, deny.
        || (a == 100 && (b & 0xc0) == 0x40)
}

/// True when `host` is a literal IP address in a range that must never be
/// reachable from an outbound upstream fetch. DNS host names are NOT classified
/// here: they are resolved by the OS at connect time, and the danger surface this
/// guard closes is a redirect `Location` pointing straight at an internal IP
/// literal.
#[must_use]
pub fn host_is_internal_ip(host: &str) -> bool {
    // `url` hands IPv6 hosts back WITHOUT the surrounding brackets; accept both.
    let stripped = host.strip_prefix('[').and_then(|h| h.strip_suffix(']'));
    let candidate = stripped.unwrap_or(host);
    let Ok(ip) = candidate.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(v4) => ipv4_is_internal(v4),
        IpAddr::V6(v6) => {
            let [s0, ..] = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique-local fc00::/7.
                || (s0 & 0xfe00) == 0xfc00
                // Link-local fe80::/10.
                || (s0 & 0xffc0) == 0xfe80
                // IPv4-mapped / -compatible: re-classify the embedded v4.
                || v6.to_ipv4().is_some_and(ipv4_is_internal)
        }
    }
}

/// Build a redirect policy that follows up to `max` redirects but REFUSES any
/// hop whose `Location` resolves to an internal IP literal (see
/// [`host_is_internal_ip`]). reqwest's DEFAULT policy follows up to 10 hops to
/// ANY host, which would let a first-hop upstream bounce the request at an
/// RFC-1918 / metadata address; this policy closes that on EVERY hop while still
/// permitting the legitimate public-CDN 307.
#[must_use]
pub fn ssrf_safe_redirect_policy(max: usize) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= max {
            return attempt.stop();
        }
        match attempt.url().host_str() {
            Some(host) if host_is_internal_ip(host) => attempt.stop(),
            _ => attempt.follow(),
        }
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn classic_internal_ranges_are_flagged() {
        for h in [
            "127.0.0.1",            // loopback
            "10.0.0.5",             // RFC-1918
            "192.168.1.1",          // RFC-1918
            "172.16.0.1",           // RFC-1918
            "169.254.169.254",      // link-local / cloud metadata
            "0.0.0.0",              // unspecified
            "255.255.255.255",      // broadcast
            "192.0.2.1",            // documentation TEST-NET-1
            "[::1]",                // v6 loopback
            "[fc00::1]",            // v6 unique-local
            "[fe80::1]",            // v6 link-local
            "[::ffff:192.168.0.1]", // v4-mapped private
            "[::ffff:10.0.0.1]",    // v4-mapped private
            "[::ffff:127.0.0.1]",   // v4-mapped loopback
            "[::ffff:169.254.1.1]", // v4-mapped link-local
            "[::ffff:100.64.0.1]",  // v4-mapped CGNAT
            "[::ffff:192.0.2.1]",   // v4-mapped documentation
        ] {
            assert!(host_is_internal_ip(h), "{h} must be flagged internal");
        }
    }

    #[test]
    fn carrier_grade_nat_is_flagged_uplift() {
        // The pip/npm gap this consolidation closes: 100.64.0.0/10 (RFC-6598).
        for h in ["100.64.0.1", "100.96.0.1", "100.127.255.255"] {
            assert!(
                host_is_internal_ip(h),
                "{h} (CGNAT) must be flagged internal"
            );
        }
        // Boundaries just OUTSIDE 100.64.0.0/10 stay external.
        for h in ["100.63.255.255", "100.128.0.1"] {
            assert!(!host_is_internal_ip(h), "{h} is outside CGNAT — external");
        }
    }

    #[test]
    fn public_hosts_and_dns_names_are_not_flagged() {
        for h in [
            "1.1.1.1",
            "8.8.8.8",
            "pypi.org",
            "files.pythonhosted.org",
            "registry.npmjs.org",
            "ghcr.io",
            "registry-1.docker.io",
            "[2606:4700::1111]",
        ] {
            assert!(!host_is_internal_ip(h), "{h} must NOT be flagged internal");
        }
    }

    #[tokio::test]
    async fn guarded_client_follows_public_3xx_then_blocks_mapped_internal_hop() {
        let server = MockServer::start().await;
        let port = server.address().port();
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", format!("http://cdn.example.test:{port}/hop2")),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/hop2"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("location", "http://[::ffff:100.64.0.1]/blocked"),
            )
            .mount(&server)
            .await;

        let client = reqwest::Client::builder()
            // Resolve the public-CDN fixture to the local mock without making
            // the redirect host itself an internal IP literal.
            .resolve("cdn.example.test", server.address())
            .redirect(ssrf_safe_redirect_policy(3))
            .build()
            .expect("guarded test client builds");
        let response = client
            .get(format!("{}/start", server.uri()))
            .send()
            .await
            .expect("redirect policy returns the stopped response");

        // The first public-DNS redirect was followed; the second hop was
        // stopped before any request could reach the mapped CGNAT address.
        assert_eq!(response.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        assert!(response.url().path().ends_with("/hop2"));
    }
}
