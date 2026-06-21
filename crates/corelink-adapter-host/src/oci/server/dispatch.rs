//! Path-tail parser for the wildcard `/v2/*rest` route.
//!
//! Extracted from `server.rs` to keep the dispatcher under the L2.10
//! file-size cap. The shapes recognized are exactly those listed in
//! oci.md §2 and re-stated in [`crate::oci::server`]'s route table.

/// Parsed shape of a `/v2/<repo>/...` tail path.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum V2Path {
    /// `<repo>/blobs/<digest>`
    Blob {
        /// Repository name component(s).
        repo: String,
        /// `sha256:<hex>` digest.
        digest: String,
    },
    /// `<repo>/blobs/uploads/` (no trailing UUID) — open a session.
    BlobUploadsOpen {
        /// Repository name component(s).
        repo: String,
    },
    /// `<repo>/blobs/uploads/<uuid>` — PATCH or PUT against a session.
    BlobUploadsSession {
        /// Repository name component(s).
        repo: String,
        /// Server-allocated upload UUID.
        uuid: String,
    },
    /// `<repo>/manifests/<reference>` — tag OR digest.
    Manifest {
        /// Repository name component(s).
        repo: String,
        /// `<reference>` slot: tag or digest.
        reference: String,
    },
    /// `<repo>/tags/list`
    TagsList {
        /// Repository name component(s).
        repo: String,
    },
}

impl V2Path {
    /// The repository name this path targets — used to build the SPECIFIC
    /// `repository:<repo>:<action>` auth challenge (a wildcard `repository:*`
    /// challenge mints a bearer the exact-match `OciScope::allows` then rejects).
    #[must_use]
    pub fn repo(&self) -> &str {
        match self {
            V2Path::Blob { repo, .. }
            | V2Path::BlobUploadsOpen { repo }
            | V2Path::BlobUploadsSession { repo, .. }
            | V2Path::Manifest { repo, .. }
            | V2Path::TagsList { repo } => repo,
        }
    }
}

/// Parse the `/v2/*rest` tail into a [`V2Path`].
///
/// `tail` does NOT include the leading `/v2/`. Recognized shapes:
///
/// - `<repo>/blobs/<digest>`
/// - `<repo>/blobs/uploads/` → open session
/// - `<repo>/blobs/uploads/<uuid>`
/// - `<repo>/manifests/<reference>`
/// - `<repo>/tags/list`
#[must_use]
pub fn parse_v2_tail(tail: &str) -> Option<V2Path> {
    let trimmed = tail.trim_end_matches('/');
    // tags/list
    if let Some(repo) = trimmed.strip_suffix("/tags/list") {
        if !repo.is_empty() {
            return Some(V2Path::TagsList {
                repo: repo.to_string(),
            });
        }
    }
    // blobs/uploads/<uuid?>
    if let Some(idx) = trimmed.find("/blobs/uploads") {
        let repo = trimmed.get(..idx)?;
        let rest = trimmed.get(idx + "/blobs/uploads".len()..)?;
        if repo.is_empty() {
            return None;
        }
        let rest_trimmed = rest.trim_start_matches('/');
        if rest_trimmed.is_empty() {
            return Some(V2Path::BlobUploadsOpen {
                repo: repo.to_string(),
            });
        }
        return Some(V2Path::BlobUploadsSession {
            repo: repo.to_string(),
            uuid: rest_trimmed.to_string(),
        });
    }
    // blobs/<digest>
    if let Some(idx) = trimmed.find("/blobs/") {
        let repo = trimmed.get(..idx)?;
        let digest = trimmed.get(idx + "/blobs/".len()..)?;
        if !repo.is_empty() && !digest.is_empty() && !digest.contains('/') {
            return Some(V2Path::Blob {
                repo: repo.to_string(),
                digest: digest.to_string(),
            });
        }
    }
    // manifests/<reference>
    if let Some(idx) = trimmed.find("/manifests/") {
        let repo = trimmed.get(..idx)?;
        let reference = trimmed.get(idx + "/manifests/".len()..)?;
        if !repo.is_empty() && !reference.is_empty() {
            return Some(V2Path::Manifest {
                repo: repo.to_string(),
                reference: reference.to_string(),
            });
        }
    }
    None
}

/// Cheap URL-decode — only the `%XX` triplets matter for OCI scope
/// strings + `?digest=...` query parameters (the `:` separator is
/// sometimes percent-encoded by clients).
#[must_use]
pub fn urldecode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut iter = s.bytes().peekable();
    while let Some(b) = iter.next() {
        if b == b'%' {
            let h = iter.next();
            let l = iter.next();
            if let (Some(h), Some(l)) = (h, l) {
                let v = match (decode_hex(h), decode_hex(l)) {
                    (Some(hv), Some(lv)) => (hv << 4) | lv,
                    _ => b'?',
                };
                out.push(char::from(v));
            }
        } else if b == b'+' {
            out.push(' ');
        } else {
            out.push(char::from(b));
        }
    }
    out
}

const fn decode_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Validate a repository name per OCI Distribution Spec v1.1 §grammar.
///
/// Grammar (simplified — we accept a permissive superset):
///
/// ```text
/// name ::= component ( '/' component )*
/// component ::= [a-z0-9]+ ( [._-] [a-z0-9]+ )*
/// ```
///
/// # Errors
///
/// Returns [`crate::oci::error::OciAdapterError::InvalidRepoName`] if any
/// component is empty, starts with a separator, or contains
/// forbidden chars.
pub fn validate_repo_name(name: &str) -> Result<(), crate::oci::error::OciAdapterError> {
    use crate::oci::error::OciAdapterError;
    if name.is_empty() {
        return Err(OciAdapterError::InvalidRepoName(String::from("empty")));
    }
    for component in name.split('/') {
        if component.is_empty() {
            return Err(OciAdapterError::InvalidRepoName(format!(
                "empty component in {name}"
            )));
        }
        let first = component
            .chars()
            .next()
            .ok_or_else(|| OciAdapterError::InvalidRepoName(name.to_string()))?;
        if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
            return Err(OciAdapterError::InvalidRepoName(format!(
                "component {component} starts with non-alnum"
            )));
        }
        for c in component.chars() {
            let ok =
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-';
            if !ok {
                return Err(OciAdapterError::InvalidRepoName(format!(
                    "invalid char `{c}` in {name}"
                )));
            }
        }
    }
    Ok(())
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

    #[test]
    fn parse_blob_pull() {
        let p = parse_v2_tail("alpine/blobs/sha256:dead").expect("parse");
        match p {
            V2Path::Blob { repo, digest } => {
                assert_eq!(repo, "alpine");
                assert_eq!(digest, "sha256:dead");
            }
            other => panic!("expected Blob, got {other:?}"),
        }
    }

    #[test]
    fn parse_blob_uploads_open() {
        let p = parse_v2_tail("a/b/blobs/uploads/").expect("parse");
        assert!(matches!(p, V2Path::BlobUploadsOpen { ref repo } if repo == "a/b"));
    }

    #[test]
    fn parse_blob_uploads_session() {
        let p = parse_v2_tail("a/b/blobs/uploads/abc-uuid").expect("parse");
        match p {
            V2Path::BlobUploadsSession { repo, uuid } => {
                assert_eq!(repo, "a/b");
                assert_eq!(uuid, "abc-uuid");
            }
            other => panic!("expected BlobUploadsSession, got {other:?}"),
        }
    }

    #[test]
    fn parse_manifest_tag() {
        let p = parse_v2_tail("alpine/manifests/latest").expect("parse");
        assert!(matches!(p, V2Path::Manifest { ref repo, ref reference }
                          if repo == "alpine" && reference == "latest"));
    }

    #[test]
    fn parse_tags_list() {
        let p = parse_v2_tail("alpine/tags/list").expect("parse");
        assert!(matches!(p, V2Path::TagsList { ref repo } if repo == "alpine"));
    }

    #[test]
    fn validate_repo_name_accepts_simple() {
        validate_repo_name("alpine").expect("simple");
        validate_repo_name("corelink-test/hello").expect("ns/name");
        validate_repo_name("a/b/c").expect("nested");
        validate_repo_name("foo_bar").expect("underscore");
        validate_repo_name("v1.2.3").expect("dotted");
    }

    #[test]
    fn validate_repo_name_rejects_bad() {
        assert!(validate_repo_name("").is_err());
        assert!(validate_repo_name("/leading-slash").is_err());
        assert!(validate_repo_name("UPPER").is_err());
        assert!(validate_repo_name("has spaces").is_err());
        assert!(validate_repo_name("a//b").is_err());
    }

    #[test]
    fn urldecode_handles_percent_triplets() {
        assert_eq!(
            urldecode("repository%3Aa%2Fb%3Apull"),
            "repository:a/b:pull"
        );
    }
}
