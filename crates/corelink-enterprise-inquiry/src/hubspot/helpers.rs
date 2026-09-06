use super::*;

// ----------------------------------------------------------------------
// JSON body builders (intentionally tiny — no serde dependency in this
// crate; the production binary can swap to a typed builder if it adds
// serde-json itself)
// ----------------------------------------------------------------------

pub(super) fn build_contact_body(
    email: &str,
    first_name: &str,
    last_name: &str,
    company: &str,
    inquiry_id: &InquiryId,
) -> String {
    format!(
        "{{\"properties\":{{\"email\":\"{}\",\"firstname\":\"{}\",\"lastname\":\"{}\",\"company\":\"{}\",\"hs_unique_creation_key\":\"contact:{}\"}}}}",
        escape_json(email),
        escape_json(first_name),
        escape_json(last_name),
        escape_json(company),
        escape_json(inquiry_id.as_str()),
    )
}
pub(super) fn build_company_search_body(legal_name: &str) -> String {
    format!(
        "{{\"filterGroups\":[{{\"filters\":[{{\"propertyName\":\"name\",\"operator\":\"EQ\",\"value\":\"{}\"}}]}}],\"limit\":1}}",
        escape_json(legal_name),
    )
}

pub(super) fn build_company_create_body(
    legal_name: &str,
    tax_id: Option<&str>,
    inquiry_id: &InquiryId,
) -> String {
    let tax_id_kv = match tax_id {
        Some(t) => format!(",\"tax_id\":\"{}\"", escape_json(t)),
        None => String::new(),
    };
    format!(
        "{{\"properties\":{{\"name\":\"{}\",\"hs_unique_creation_key\":\"company:{}\"{}}}}}",
        escape_json(legal_name),
        escape_json(inquiry_id.as_str()),
        tax_id_kv,
    )
}

pub(super) fn build_deal_body(
    contact_id: &str,
    company_id: &str,
    stage: &str,
    amount_usd: u64,
    inquiry_id: &InquiryId,
) -> String {
    format!(
        "{{\"properties\":{{\"dealname\":\"Enterprise inquiry {iid}\",\"dealstage\":\"{stage}\",\"amount\":\"{amt}\",\"hs_unique_creation_key\":\"deal:{iid}\"}},\
         \"associations\":[\
           {{\"to\":{{\"id\":\"{cid}\"}},\"types\":[{{\"associationCategory\":\"HUBSPOT_DEFINED\",\"associationTypeId\":3}}]}},\
           {{\"to\":{{\"id\":\"{coid}\"}},\"types\":[{{\"associationCategory\":\"HUBSPOT_DEFINED\",\"associationTypeId\":5}}]}}\
         ]}}",
        iid = escape_json(inquiry_id.as_str()),
        stage = escape_json(stage),
        amt = amount_usd,
        cid = escape_json(contact_id),
        coid = escape_json(company_id),
    )
}

pub(super) fn build_ticket_body(
    subject: &str,
    content: &str,
    owner_id: Option<&str>,
    inquiry_id: &InquiryId,
) -> String {
    let owner_kv = match owner_id {
        Some(o) => format!(",\"hubspot_owner_id\":\"{}\"", escape_json(o)),
        None => String::new(),
    };
    format!(
        "{{\"properties\":{{\"subject\":\"{}\",\"content\":\"{}\",\"hs_pipeline_stage\":\"1\",\"hs_unique_creation_key\":\"ticket:{}\"{}}}}}",
        escape_json(subject),
        escape_json(content),
        escape_json(inquiry_id.as_str()),
        owner_kv,
    )
}

/// JSON string escape — covers the four characters HubSpot will reject
/// (`"`, `\`, control chars).
pub(super) fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// URL path-segment escape — covers `/` and non-ASCII.
pub(super) fn escape_url(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Truncate a string to `max` chars, appending `…` if truncated. Used
/// when surfacing HubSpot error bodies into [`CrmError::Rejected`] so
/// the audit trail keeps a useful tail without leaking unbounded
/// HubSpot payload into our logs.
pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

/// Tiny "find `\"id\":\"...\"` field" parser — avoids a serde_json
/// dependency in this crate. Production wiring SHOULD replace this
/// with a real JSON parse once the crate adopts serde.
pub(super) fn parse_object_id(body: &str) -> Result<String, CrmError> {
    parse_string_field(body, "\"id\"").ok_or_else(|| {
        CrmError::Rejected(format!(
            "hubspot response missing `id`: {}",
            truncate(body, 256)
        ))
    })
}

/// Parse the first hit from a `/search` response (`{"results":[{"id":"123",...}],...}`).
/// Returns `None` when results is empty or absent.
#[must_use]
pub(super) fn parse_first_search_hit(body: &str) -> Option<String> {
    // Find `"results":[` then the first `"id":"..."` after it.
    let results_idx = body.find("\"results\":[")?;
    let tail = body.get(results_idx..)?;
    if tail.starts_with("\"results\":[]")
        || tail.starts_with("\"results\":[ ]")
        || tail.starts_with("\"results\": []")
    {
        return None;
    }
    parse_string_field(tail, "\"id\"")
}

pub(super) fn parse_string_field(body: &str, key: &str) -> Option<String> {
    let key_idx = body.find(key)?;
    let after = body.get(key_idx.checked_add(key.len())?..)?;
    let colon_idx = after.find(':')?;
    let after_colon = after.get(colon_idx.checked_add(1)?..)?;
    let trimmed = after_colon.trim_start();
    if !trimmed.starts_with('"') {
        return None;
    }
    let rest = trimmed.get(1..)?;
    let end = rest.find('"')?;
    rest.get(..end).map(str::to_string)
}

pub(super) fn split_name(full: &str) -> (String, String) {
    let trimmed = full.trim();
    if let Some((first, last)) = trimmed.split_once(' ') {
        (first.to_string(), last.to_string())
    } else {
        (trimmed.to_string(), "—".to_string())
    }
}
