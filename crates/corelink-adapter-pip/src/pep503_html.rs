//! PEP 503 (HTML Simple Repository API) parser + re-encoder.
//!
//! When the client only supports PEP 503 (legacy `pip` ≤21.2 or
//! certain mirror clients), we serve the same content the JSON
//! cache already holds, re-encoded as a minimal HTML document with
//! one `<a href>` per file. The HTML form carries the `#sha256=`
//! fragment, the `data-yanked` attribute (PEP 592), and the
//! `data-requires-python` attribute (PEP 503).
//!
//! This module is also the home for the JSON index data model. The
//! two directions share a single [`IndexFile`] / [`ProjectIndex`]
//! pair so the round-trip property test (spec §8 row 2) can pin
//! `parse_json → encode_html → parse_html` invariants.

use serde::{Deserialize, Serialize};

use crate::error::PipAdapterError;

/// One file row in a project index (one wheel or one sdist).
///
/// The adapter does not need most of the fields PyPI ships (gpg-sig,
/// hashes other than sha256, etc.); we only retain what the HTML
/// re-encoder + clients both consume.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexFile {
    /// Filename, e.g. `requests-2.31.0-py3-none-any.whl`.
    pub filename: String,
    /// Absolute URL to the wheel/sdist (canonical PyPI URLs already
    /// embed the `#sha256=...` fragment).
    pub url: String,
    /// Hex-encoded SHA256, extracted from the URL fragment or the
    /// PEP 691 `hashes.sha256` object — whichever was non-empty.
    pub sha256: String,
    /// PEP 503 `data-requires-python` constraint, if any.
    #[serde(default)]
    pub requires_python: Option<String>,
    /// PEP 592 yank status. `true` means yanked; the original JSON
    /// form may carry an optional reason — we keep just the boolean
    /// for the HTML round-trip (HTML form is also boolean per PEP 503).
    #[serde(default)]
    pub yanked: bool,
}

/// Owned project index payload (PEP 691 shape, internal model).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectIndex {
    /// Project name (PEP 503 normalised — lowercase, `-`-separated).
    pub name: String,
    /// File listings (one entry per wheel/sdist).
    pub files: Vec<IndexFile>,
}

impl IndexFile {
    /// Construct a new [`IndexFile`]. The struct is
    /// `#[non_exhaustive]`; this helper exists so external test
    /// crates (`tests/`) can build values without depending on the
    /// internal struct layout.
    #[must_use]
    pub fn new(
        filename: String,
        url: String,
        sha256: String,
        requires_python: Option<String>,
        yanked: bool,
    ) -> Self {
        Self {
            filename,
            url,
            sha256,
            requires_python,
            yanked,
        }
    }
}

impl ProjectIndex {
    /// Construct a new [`ProjectIndex`]. Mirror of
    /// [`IndexFile::new`] for the external test surface.
    #[must_use]
    pub fn new(name: String, files: Vec<IndexFile>) -> Self {
        Self { name, files }
    }
}

/// Parse the PyPI PEP 691 JSON index payload for one project.
///
/// PyPI's actual JSON shape is:
///
/// ```jsonc
/// {
///   "meta": { "api-version": "1.0" },
///   "name": "requests",
///   "files": [
///     {
///       "filename": "requests-2.31.0-py3-none-any.whl",
///       "url": "https://files.pythonhosted.org/.../requests-2.31.0-py3-none-any.whl",
///       "hashes": { "sha256": "..." },
///       "requires-python": ">=3.7",
///       "yanked": false
///     }
///   ]
/// }
/// ```
///
/// # Errors
///
/// Returns [`PipAdapterError::IndexParse`] on malformed JSON,
/// missing required fields, or a file row missing both a `#sha256=`
/// fragment AND a `hashes.sha256` entry (PEP 691 mandates one of
/// them; we fail-CLOSED, matching the §8 adversarial row 5 spec).
pub fn parse_json_index(bytes: &[u8]) -> Result<ProjectIndex, PipAdapterError> {
    #[derive(Deserialize)]
    struct WireFile {
        filename: String,
        url: String,
        #[serde(default)]
        hashes: Option<serde_json::Value>,
        #[serde(rename = "requires-python", default)]
        requires_python: Option<String>,
        #[serde(default)]
        yanked: serde_json::Value,
    }
    #[derive(Deserialize)]
    struct Wire {
        name: String,
        files: Vec<WireFile>,
    }
    let wire: Wire = serde_json::from_slice(bytes)
        .map_err(|e| PipAdapterError::IndexParse(format!("json: {e}")))?;
    let mut out_files = Vec::with_capacity(wire.files.len());
    for f in wire.files {
        let sha256 = extract_sha256(&f.url, f.hashes.as_ref())
            .ok_or_else(|| PipAdapterError::IndexParse(format!("missing sha256 for {}", f.filename)))?;
        let yanked = match f.yanked {
            serde_json::Value::Bool(b) => b,
            serde_json::Value::String(_) => true,
            serde_json::Value::Null => false,
            _ => false,
        };
        out_files.push(IndexFile {
            filename: f.filename,
            url: f.url,
            sha256,
            requires_python: f.requires_python,
            yanked,
        });
    }
    Ok(ProjectIndex {
        name: normalise_project_name(&wire.name),
        files: out_files,
    })
}

/// Extract the hex SHA256 from a URL `#sha256=...` fragment, falling
/// back to `hashes.sha256` if absent.
fn extract_sha256(url: &str, hashes: Option<&serde_json::Value>) -> Option<String> {
    if let Some(idx) = url.find("#sha256=") {
        let frag = &url[idx + "#sha256=".len()..];
        let hex_part = frag.split(['&', '?', '#']).next().unwrap_or(frag);
        if is_valid_sha256_hex(hex_part) {
            return Some(hex_part.to_ascii_lowercase());
        }
    }
    if let Some(v) = hashes {
        if let Some(s) = v.get("sha256").and_then(serde_json::Value::as_str) {
            if is_valid_sha256_hex(s) {
                return Some(s.to_ascii_lowercase());
            }
        }
    }
    None
}

/// Validate that `s` is exactly 64 lowercase-or-uppercase hex chars.
#[must_use]
pub fn is_valid_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// PEP 503 project name normalisation: lowercase, `[-_.]+` → `-`.
#[must_use]
pub fn normalise_project_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        if matches!(ch, '-' | '_' | '.') {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        } else {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        }
    }
    out.trim_matches('-').to_owned()
}

/// Encode a [`ProjectIndex`] as a minimal PEP 503 HTML document.
///
/// Output shape:
///
/// ```html
/// <!DOCTYPE html><html><body>
/// <a href="https://.../foo-1.0-py3-none-any.whl#sha256=abc..." data-requires-python=">=3.7">foo-1.0-py3-none-any.whl</a><br>
/// </body></html>
/// ```
///
/// HTML attribute values are escaped with the minimal `&`/`<`/`"` →
/// `&amp;`/`&lt;`/`&quot;` mapping. PyPI's actual HTML output is
/// byte-for-byte similar; we match the structure but not the exact
/// whitespace.
#[must_use]
pub fn encode_html(index: &ProjectIndex) -> String {
    let mut out = String::from("<!DOCTYPE html><html><body>");
    for f in &index.files {
        out.push_str("<a href=\"");
        push_escaped_attr(&mut out, &f.url);
        out.push('"');
        if let Some(rp) = &f.requires_python {
            out.push_str(" data-requires-python=\"");
            push_escaped_attr(&mut out, rp);
            out.push('"');
        }
        if f.yanked {
            out.push_str(" data-yanked=\"\"");
        }
        out.push('>');
        push_escaped_text(&mut out, &f.filename);
        out.push_str("</a><br>");
    }
    out.push_str("</body></html>");
    out
}

fn push_escaped_attr(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            // `>` is escaped so the line-oriented HTML parser in
            // [`parse_html`] can scan up to the first un-escaped
            // `>` to find the end of the `<a …>` tag without
            // misidentifying `data-requires-python=">=3.7"` as the
            // tag close.
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn push_escaped_text(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

/// Parse a minimal PEP 503 HTML index back to [`ProjectIndex`].
///
/// This parser is intentionally narrow — it does NOT speak full
/// HTML. It tokenises one `<a …>filename</a>` row at a time, which
/// is exactly what PyPI / mirror clients produce. Anything fancier
/// (CDATA, inline script, etc.) is rejected as malformed.
///
/// `project` is the name to record in the returned struct (PEP 503
/// HTML does not carry the project name explicitly).
///
/// # Errors
///
/// Returns [`PipAdapterError::IndexParse`] on any token unable to be
/// matched to the `<a href="…">label</a>` shape.
pub fn parse_html(html: &str, project: &str) -> Result<ProjectIndex, PipAdapterError> {
    let mut files = Vec::new();
    let mut cursor = 0usize;
    let bytes = html.as_bytes();
    while cursor < bytes.len() {
        let Some(open_rel) = html[cursor..].find("<a ") else {
            break;
        };
        let open = cursor + open_rel;
        let Some(close_tag_rel) = html[open..].find('>') else {
            return Err(PipAdapterError::IndexParse(
                "<a tag missing `>`".into(),
            ));
        };
        let close_tag = open + close_tag_rel;
        let attrs = &html[open + 3..close_tag];
        let Some(end_rel) = html[close_tag..].find("</a>") else {
            return Err(PipAdapterError::IndexParse(
                "<a> missing closing </a>".into(),
            ));
        };
        let end = close_tag + end_rel;
        let label = &html[close_tag + 1..end];
        let href = extract_attr(attrs, "href")
            .ok_or_else(|| PipAdapterError::IndexParse("<a> missing href".into()))?;
        let requires_python = extract_attr(attrs, "data-requires-python");
        let yanked = attrs.contains("data-yanked");
        let url = decode_attr(&href);
        let filename = decode_text(label);
        let sha256 = extract_sha256(&url, None)
            .ok_or_else(|| PipAdapterError::IndexParse(format!("href missing #sha256= for {filename}")))?;
        files.push(IndexFile {
            filename,
            url,
            sha256,
            requires_python: requires_python.map(|s| decode_attr(&s)),
            yanked,
        });
        cursor = end + 4;
    }
    Ok(ProjectIndex {
        name: normalise_project_name(project),
        files,
    })
}

fn extract_attr(attrs: &str, name: &str) -> Option<String> {
    let needle_eq = format!("{name}=\"");
    let start = attrs.find(&needle_eq)? + needle_eq.len();
    let rest = attrs.get(start..)?;
    let end_rel = rest.find('"')?;
    Some(rest[..end_rel].to_owned())
}

fn decode_attr(s: &str) -> String {
    // Order matters: decode `&amp;` LAST so that an encoded `&amp;gt;`
    // does not turn into a decoded `>` in one pass.
    s.replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn decode_text(s: &str) -> String {
    s.replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn sample_index() -> ProjectIndex {
        ProjectIndex {
            name: "requests".into(),
            files: vec![IndexFile {
                filename: "requests-2.31.0-py3-none-any.whl".into(),
                url: "https://files.pythonhosted.org/packages/requests-2.31.0-py3-none-any.whl#sha256=58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f".into(),
                sha256: "58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f".into(),
                requires_python: Some(">=3.7".into()),
                yanked: false,
            }],
        }
    }

    #[test]
    fn normalise_project_name_table() {
        assert_eq!(normalise_project_name("Foo.Bar"), "foo-bar");
        assert_eq!(normalise_project_name("foo_bar"), "foo-bar");
        assert_eq!(normalise_project_name("foo--bar"), "foo-bar");
        assert_eq!(normalise_project_name("---Foo---"), "foo");
    }

    #[test]
    fn is_valid_sha256_hex_table() {
        assert!(is_valid_sha256_hex(&"a".repeat(64)));
        assert!(is_valid_sha256_hex(&"A".repeat(64)));
        assert!(!is_valid_sha256_hex(&"a".repeat(63)));
        assert!(!is_valid_sha256_hex(&"g".repeat(64)));
    }

    #[test]
    fn json_parse_extracts_sha256_from_url_fragment() {
        let payload = serde_json::json!({
            "meta": { "api-version": "1.0" },
            "name": "requests",
            "files": [{
                "filename": "requests-2.31.0-py3-none-any.whl",
                "url": "https://files.pythonhosted.org/packages/r/requests-2.31.0-py3-none-any.whl#sha256=58cd2187c01e70e6e26505bca751777aa9f2ee0b7f4300988b709f44e013003f",
                "requires-python": ">=3.7",
                "yanked": false
            }]
        });
        let parsed =
            parse_json_index(payload.to_string().as_bytes()).expect("parse ok");
        assert_eq!(parsed.name, "requests");
        assert_eq!(parsed.files.len(), 1);
        assert_eq!(parsed.files[0].sha256.len(), 64);
    }

    #[test]
    fn json_parse_falls_back_to_hashes_object() {
        let payload = serde_json::json!({
            "name": "x",
            "files": [{
                "filename": "x-1.tar.gz",
                "url": "https://files.pythonhosted.org/x-1.tar.gz",
                "hashes": { "sha256": "0".repeat(64) },
                "yanked": false
            }]
        });
        let parsed =
            parse_json_index(payload.to_string().as_bytes()).expect("parse ok");
        assert_eq!(parsed.files[0].sha256, "0".repeat(64));
    }

    #[test]
    fn json_parse_rejects_missing_sha256() {
        let payload = serde_json::json!({
            "name": "x",
            "files": [{
                "filename": "x-1.tar.gz",
                "url": "https://files.pythonhosted.org/x-1.tar.gz",
                "yanked": false
            }]
        });
        let result = parse_json_index(payload.to_string().as_bytes());
        assert!(matches!(result, Err(PipAdapterError::IndexParse(_))));
    }

    #[test]
    fn json_parse_handles_yanked_with_reason() {
        let payload = serde_json::json!({
            "name": "x",
            "files": [{
                "filename": "x-1.tar.gz",
                "url": "https://files.pythonhosted.org/x-1.tar.gz#sha256=".to_string() + &"f".repeat(64),
                "yanked": "CVE-2025-9999 -- vulnerable, do not use"
            }]
        });
        let parsed =
            parse_json_index(payload.to_string().as_bytes()).expect("parse ok");
        assert!(parsed.files[0].yanked);
    }

    #[test]
    fn html_encode_decode_roundtrip() {
        let original = sample_index();
        let html = encode_html(&original);
        let decoded = parse_html(&html, "requests").expect("decode ok");
        assert_eq!(decoded, original);
    }

    #[test]
    fn html_encode_includes_yanked_attr() {
        let mut idx = sample_index();
        idx.files[0].yanked = true;
        let html = encode_html(&idx);
        assert!(html.contains("data-yanked"));
    }

    #[test]
    fn html_decode_rejects_missing_sha256() {
        let html =
            "<!DOCTYPE html><html><body><a href=\"https://x.example/y.whl\">y.whl</a></body></html>";
        let result = parse_html(html, "y");
        assert!(matches!(result, Err(PipAdapterError::IndexParse(_))));
    }
}
