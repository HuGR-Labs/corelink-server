
fn parse_action(s: &str) -> Option<DsrRequestKind> {
    Some(match s {
        "access" => DsrRequestKind::Access,
        "portability" => DsrRequestKind::Portability,
        "rectification" => DsrRequestKind::Rectification,
        "erasure" => DsrRequestKind::Erasure,
        "restriction" => DsrRequestKind::Restriction,
        "objection" => DsrRequestKind::Objection,
        _ => return None,
    })
}

fn parse_timeline_event(v: &Value) -> Option<TimelineEvent> {
    let to = v.get("to").and_then(Value::as_str)?;
    Some(TimelineEvent {
        // The persisted `at` is ISO; the in-memory `at_ms` is only used for
        // re-serialization, so a parse miss falls back to 0 (display-only).
        at_ms: 0,
        from: v.get("from").and_then(Value::as_str).map(str_to_status),
        to: str_to_status(to),
        note: v.get("note").and_then(Value::as_str).map(str::to_owned),
    })
}

fn str_to_status(s: &str) -> TicketStatus {
    TicketStatus::parse(s)
}

/// Transition a ticket to `to`, appending a timeline row.
fn transition(ticket: &mut DsrTicket, now: u64, to: TicketStatus, note: &str) {
    let from = ticket.status;
    ticket.status = to;
    ticket.updated_at_ms = now;
    ticket.timeline.push(TimelineEvent {
        at_ms: now,
        from: Some(from),
        to,
        note: Some(note.to_owned()),
    });
}

/// Mark a ticket completed (+ optional export handle).
fn complete(ticket: &mut DsrTicket, now: u64, download: Option<String>, note: &str) {
    ticket.mfa_required = false;
    if let Some(url) = download {
        ticket.data_download_url = Some(url);
    }
    transition(ticket, now, TicketStatus::Completed, note);
}

/// Mark a ticket rejected with a (non-PII) reason note.
fn reject_ticket(ticket: &mut DsrTicket, now: u64, note: &str) {
    transition(ticket, now, TicketStatus::Rejected, note);
}

/// Map a pipeline engine fault to a fail-CLOSED 500 (no partial success).
fn pipeline_error(err: &str, surface: &str) -> Response {
    tracing::error!(error = %err, surface = %surface, "dsr: live pipeline error");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("{surface} failed"),
    )
        .into_response()
}

/// HS256 proof-of-submission receipt (compact JWT). Real + verifiable; the RS256
/// KMS binding is the deferred production hardening (`corelink-dsr`).
fn issue_receipt(
    key: &[u8],
    request_id: &str,
    action: DsrRequestKind,
    jurisdiction: DsrJurisdiction,
    sla_deadline_ms: u64,
    submitted_at_ms: u64,
    tenant_id: &str,
) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let header = json!({ "alg": "HS256", "typ": "JWT" });
    let iat = submitted_at_ms / 1000;
    let exp = iat.saturating_add(90 * 86_400);
    // Hashed tenant identifier — never the raw tenant_id (CTRL-PRIV-014).
    let tenant_id_hash = hex::encode(Sha256::digest(tenant_id.as_bytes()));
    let claims = json!({
        "iss": "corelink.dsr",
        "request_id": request_id,
        "action": action.as_str(),
        "jurisdiction": jurisdiction.as_str(),
        "sla_deadline": ms_to_iso8601(clamp_i64(sla_deadline_ms)),
        "jti": request_id,
        "iat": iat,
        "exp": exp,
        "tenant_id_hash": tenant_id_hash,
    });
    let h = b64.encode(serde_json::to_vec(&header).unwrap_or_default());
    let c = b64.encode(serde_json::to_vec(&claims).unwrap_or_default());
    let signing_input = format!("{h}.{c}");
    // HMAC accepts any key length, so `new_from_slice` never errs here.
    let mut mac = match <Hmac<Sha256> as KeyInit>::new_from_slice(key) {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    mac.update(signing_input.as_bytes());
    let sig = b64.encode(mac.finalize().into_bytes());
    format!("{signing_input}.{sig}")
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(0)
}
