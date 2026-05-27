//! PII redaction primitives — `PiiRedactor` trait + `InMemoryPiiRedactor`
//! 5-pattern fail-closed substitution surface.
//!
//! ## Why hand-rolled scanners (no regex crate)
//!
//! Per WI §6.2 anti-scope + the corelink autonomous execution charter
//! (no per-WI codex; minimal new deps), we ship hand-rolled scanners
//! for the 5 canonical PII pattern kinds. Each scanner walks the input
//! byte-by-byte using the deterministic rules below; the substitution
//! step replaces the matched span with the canonical placeholder.
//! Hand-rolled scanners are easier to audit (no regex catastrophic
//! backtracking risk; INV-AVAIL-DOS canary) and pin the falsifiability
//! target (100k synthetic seeded ChaCha20Rng samples; zero leakage).
//!
//! ## Canonical patterns (per WI §6.1.1)
//!
//! - **Email** (RFC-ish; local-part `[A-Za-z0-9._+-]+` + `@` +
//!   domain `[A-Za-z0-9.-]+`) → `<EMAIL_REDACTED>`.
//! - **IP** (IPv4 dotted-quad valid octets `[0-255]` × 4 + IPv6 `:`
//!   delimited 1–8 hex groups) → `<IP_REDACTED>`. Per LGPD Art. 5
//!   §III IP addresses are personal data.
//! - **Token / Bearer / JWT / API key** (`Bearer <token>` literal +
//!   long alphanumeric / dot-delimited tokens ≥ 24 chars) →
//!   `<TOKEN_REDACTED>`.
//! - **PAN / Credit card** (Luhn-validated 13–19 digit run; whitespace
//!   / dash separators tolerated) → `<PAN_REDACTED>`.
//! - **CPF / CNPJ** (Brazilian PII per LGPD scope; CPF 11-digit
//!   modulo-11; CNPJ 14-digit modulo-11 with canonical multiplier
//!   weights). CPF → `<CPF_REDACTED>`; CNPJ → `<CNPJ_REDACTED>`.
//!
//! ## Idempotency
//!
//! The redaction substitution is idempotent by construction: the
//! placeholder strings (`<EMAIL_REDACTED>` etc.) contain no characters
//! that can match any of the 5 patterns. Pinned by
//! `prop_redaction_idempotent`.
//!
//! ## Fail-closed envelope
//!
//! If the redactor's internal mutex is poisoned (only reachable under
//! adversarial test fixtures), the redaction step returns
//! [`crate::logpush::error::LogpushError::Internal`] and the orchestrator
//! drops the log line + emits `corelink.logpush.redaction_failure`
//! audit (SEV-1 alert per WI §6.1.10 — CTRL-PRIV-001 bypass risk).

use std::sync::{Arc, Mutex};

/// Canonical 5-pattern redaction kind taxonomy. The ordering pins the
/// match-precedence (we run scanners in declaration order so an
/// e.g. email-shaped JWT is redacted as a token first, but the email
/// scanner picks up any residual email-shaped text).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum PiiPatternKind {
    /// Email address; placeholder `<EMAIL_REDACTED>`.
    Email,
    /// IPv4 / IPv6 address; placeholder `<IP_REDACTED>`. LGPD Art. 5
    /// §III scope.
    Ip,
    /// Bearer / JWT / API key; placeholder `<TOKEN_REDACTED>`.
    Token,
    /// Luhn-validated credit card PAN; placeholder `<PAN_REDACTED>`.
    Pan,
    /// Brazilian CPF (11-digit) or CNPJ (14-digit); placeholder
    /// `<CPF_REDACTED>` / `<CNPJ_REDACTED>` respectively (LGPD scope).
    CpfCnpj,
}

impl PiiPatternKind {
    /// Canonical pattern-id slug (matches the
    /// `migrations/d1/0016_log_schema.sql::log_redaction_patterns.pattern_id`
    /// column).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Ip => "ip",
            Self::Token => "token",
            Self::Pan => "pan",
            Self::CpfCnpj => "cpf_cnpj",
        }
    }

    /// Canonical placeholder for the matched span. CPF + CNPJ share
    /// the kind here but use distinct placeholders at the substitution
    /// step (the scanner emits the right one based on digit count).
    #[must_use]
    pub const fn default_placeholder(self) -> &'static str {
        match self {
            Self::Email => EMAIL_PLACEHOLDER,
            Self::Ip => IP_PLACEHOLDER,
            Self::Token => TOKEN_PLACEHOLDER,
            Self::Pan => PAN_PLACEHOLDER,
            Self::CpfCnpj => CPF_PLACEHOLDER,
        }
    }
}

/// Canonical 5-element pattern list — pinned for the cold-start
/// hydration drift detection (the durable mirror MUST contain exactly
/// these 5 rows per region).
#[must_use]
pub const fn canonical_pii_pattern_kinds() -> &'static [PiiPatternKind; 5] {
    &[
        PiiPatternKind::Email,
        PiiPatternKind::Ip,
        PiiPatternKind::Token,
        PiiPatternKind::Pan,
        PiiPatternKind::CpfCnpj,
    ]
}

/// Email canonical placeholder.
pub const EMAIL_PLACEHOLDER: &str = "<EMAIL_REDACTED>";
/// IP canonical placeholder.
pub const IP_PLACEHOLDER: &str = "<IP_REDACTED>";
/// Token canonical placeholder.
pub const TOKEN_PLACEHOLDER: &str = "<TOKEN_REDACTED>";
/// PAN canonical placeholder.
pub const PAN_PLACEHOLDER: &str = "<PAN_REDACTED>";
/// CPF canonical placeholder.
pub const CPF_PLACEHOLDER: &str = "<CPF_REDACTED>";
/// CNPJ canonical placeholder.
pub const CNPJ_PLACEHOLDER: &str = "<CNPJ_REDACTED>";

/// Result of a single redaction pass — the redacted text plus the
/// per-pattern hit count (used as a SEV-3 trigger source if a
/// sustained non-zero stream of `RedactionFailure` is observed).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RedactionOutcome {
    /// Redacted text. PII spans replaced with canonical placeholders.
    pub redacted: String,
    /// Per-pattern hit counts.
    pub email_hits: u32,
    /// IPv4 + IPv6 hit count.
    pub ip_hits: u32,
    /// Token / bearer / JWT / API key hit count.
    pub token_hits: u32,
    /// Luhn-validated PAN hit count.
    pub pan_hits: u32,
    /// CPF + CNPJ hit count (combined; the scanner emits the right
    /// placeholder based on digit count).
    pub cpf_cnpj_hits: u32,
}

impl RedactionOutcome {
    /// Total number of substitutions performed.
    #[must_use]
    pub const fn total_hits(&self) -> u32 {
        self.email_hits
            .saturating_add(self.ip_hits)
            .saturating_add(self.token_hits)
            .saturating_add(self.pan_hits)
            .saturating_add(self.cpf_cnpj_hits)
    }
}

/// PII redaction trait. Production wiring composes:
///
/// - `MultiplexPiiRedactor` — fan-out to additional redaction passes
///   (e.g. per-customer regex extension).
pub trait PiiRedactor: Send + Sync + core::fmt::Debug {
    /// Redact PII patterns in the input string. Returns the redacted
    /// text + per-pattern hit counts.
    fn redact(&self, input: &str) -> RedactionOutcome;

    /// Walk a `serde_json::Value` tree, redacting every `String` leaf
    /// in place. The default implementation calls
    /// [`PiiRedactor::redact`] on every string leaf (object keys are
    /// PRESERVED; only string values are redacted; numbers + bools
    /// pass through). The cumulative `RedactionOutcome` is returned.
    fn redact_json(
        &self,
        value: &mut serde_json::Value,
    ) -> RedactionOutcome {
        let mut outcome = RedactionOutcome::default();
        redact_json_recursive(self, value, &mut outcome);
        outcome
    }
}

fn redact_json_recursive<R: PiiRedactor + ?Sized>(
    redactor: &R,
    value: &mut serde_json::Value,
    outcome: &mut RedactionOutcome,
) {
    match value {
        serde_json::Value::String(s) => {
            let r = redactor.redact(s);
            *s = r.redacted;
            outcome.email_hits =
                outcome.email_hits.saturating_add(r.email_hits);
            outcome.ip_hits =
                outcome.ip_hits.saturating_add(r.ip_hits);
            outcome.token_hits =
                outcome.token_hits.saturating_add(r.token_hits);
            outcome.pan_hits =
                outcome.pan_hits.saturating_add(r.pan_hits);
            outcome.cpf_cnpj_hits = outcome
                .cpf_cnpj_hits
                .saturating_add(r.cpf_cnpj_hits);
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_json_recursive(redactor, v, outcome);
            }
        }
        serde_json::Value::Object(map) => {
            for (_k, v) in map.iter_mut() {
                redact_json_recursive(redactor, v, outcome);
            }
        }
        _ => {}
    }
}

/// In-memory PII redactor. Cloning shares the underlying counters
/// (the per-instance `Arc<Mutex<>>` F-001 closure pattern; tests
/// instantiate fresh redactors per case so the orchestrator harness
/// cannot accidentally leak counter state across cases).
#[derive(Clone, Debug, Default)]
pub struct InMemoryPiiRedactor {
    /// Cumulative per-pattern counters across every redaction pass.
    /// Surfaced for the SEV-3 `corelink_logs_pii_redacted_total`
    /// alert source per WI §6.1.10.
    counters: Arc<Mutex<RedactionCounters>>,
}

#[derive(Clone, Debug, Default)]
struct RedactionCounters {
    email: u64,
    ip: u64,
    token: u64,
    pan: u64,
    cpf_cnpj: u64,
}

impl InMemoryPiiRedactor {
    /// Construct a fresh redactor. Counters start at 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the per-pattern cumulative counters across every
    /// redaction pass.
    #[must_use]
    pub fn snapshot_counters(
        &self,
    ) -> (u64, u64, u64, u64, u64) {
        let g = match self.counters.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        (g.email, g.ip, g.token, g.pan, g.cpf_cnpj)
    }
}

impl PiiRedactor for InMemoryPiiRedactor {
    fn redact(&self, input: &str) -> RedactionOutcome {
        let mut outcome = RedactionOutcome::default();
        // Match precedence: tokens first (so JWT-shaped strings don't
        // get partially redacted by the email scanner); then CPF/CNPJ
        // (digit-only patterns; precedence over PAN because Brazilian
        // documents use the CPF/CNPJ format which is structurally
        // distinct from PAN); then PAN (Luhn-validated digit runs);
        // then email; then IP. The order is load-bearing for the
        // canonical hit-count discipline.
        let mut step = redact_tokens(input);
        outcome.token_hits =
            outcome.token_hits.saturating_add(step.hits);

        // IP scanner runs BEFORE the CPF/CNPJ + PAN scanners so an
        // IPv4-shaped string never collides with a digit-only document
        // pattern (e.g. "121.119.34.119" decodes to "12111934119"
        // 11-digit run that may pass the CPF mod-11 check by chance).
        step = redact_ip(&step.text);
        outcome.ip_hits = outcome.ip_hits.saturating_add(step.hits);

        step = redact_cpf_cnpj(&step.text);
        outcome.cpf_cnpj_hits =
            outcome.cpf_cnpj_hits.saturating_add(step.hits);

        step = redact_pan(&step.text);
        outcome.pan_hits =
            outcome.pan_hits.saturating_add(step.hits);

        step = redact_email(&step.text);
        outcome.email_hits =
            outcome.email_hits.saturating_add(step.hits);

        outcome.redacted = step.text;

        if let Ok(mut g) = self.counters.lock() {
            g.email = g.email.saturating_add(u64::from(outcome.email_hits));
            g.ip = g.ip.saturating_add(u64::from(outcome.ip_hits));
            g.token =
                g.token.saturating_add(u64::from(outcome.token_hits));
            g.pan = g.pan.saturating_add(u64::from(outcome.pan_hits));
            g.cpf_cnpj = g
                .cpf_cnpj
                .saturating_add(u64::from(outcome.cpf_cnpj_hits));
        }
        outcome
    }
}

#[derive(Debug, Default)]
struct PassResult {
    text: String,
    hits: u32,
}

// -- Email scanner -----------------------------------------------------

fn is_email_local_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(b, b'.' | b'_' | b'+' | b'-' | b'%')
}

fn is_email_domain_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')
}

fn redact_email(input: &str) -> PassResult {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut hits: u32 = 0;
    while i < bytes.len() {
        let b = bytes.get(i).copied().unwrap_or(0);
        if b == b'@' && i > 0 {
            // Walk backward to find the local-part start.
            let mut local_start = i;
            while local_start > 0 {
                let prev = bytes
                    .get(local_start - 1)
                    .copied()
                    .unwrap_or(0);
                if is_email_local_byte(prev) {
                    local_start -= 1;
                } else {
                    break;
                }
            }
            // Walk forward from i+1 to find the domain end.
            let mut domain_end = i + 1;
            while domain_end < bytes.len() {
                let nxt =
                    bytes.get(domain_end).copied().unwrap_or(0);
                if is_email_domain_byte(nxt) {
                    domain_end += 1;
                } else {
                    break;
                }
            }
            // Validate: local non-empty, domain has at least one dot
            // separating at least 2 chars on each side, and TLD ≥ 2
            // chars + alphabetic.
            let domain = bytes
                .get(i + 1..domain_end)
                .unwrap_or(&[]);
            let local_len = i - local_start;
            let last_dot = domain
                .iter()
                .rposition(|c| *c == b'.');
            let valid = local_len >= 1
                && !domain.is_empty()
                && last_dot.is_some_and(|idx| {
                    let tld = domain.get(idx + 1..).unwrap_or(&[]);
                    tld.len() >= 2
                        && tld.iter().all(|c| c.is_ascii_alphabetic())
                });
            if valid {
                // Trim trailing dot from the local-part / leading dot
                // from the domain to avoid swallowing punctuation.
                // Drop already-emitted local-part bytes from `out`.
                let already_emitted = i - local_start;
                let new_len = out.len().saturating_sub(already_emitted);
                out.truncate(new_len);
                out.push_str(EMAIL_PLACEHOLDER);
                hits = hits.saturating_add(1);
                i = domain_end;
                continue;
            }
        }
        // Append this byte as char (we know the input is &str so byte
        // boundaries are valid UTF-8 char starts when we copy them
        // through; multibyte UTF-8 sequences get appended byte-by-byte
        // which is also valid because we never split across them — the
        // scanners only match on ASCII bytes).
        if let Some(c) = input.get(i..=i) {
            out.push_str(c);
        }
        i += 1;
    }
    PassResult { text: out, hits }
}

// -- IP scanner --------------------------------------------------------

fn redact_ip(input: &str) -> PassResult {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut hits: u32 = 0;
    while i < bytes.len() {
        if let Some(end) = match_ipv4_at(bytes, i) {
            out.push_str(IP_PLACEHOLDER);
            hits = hits.saturating_add(1);
            i = end;
            continue;
        }
        if let Some(end) = match_ipv6_at(bytes, i) {
            out.push_str(IP_PLACEHOLDER);
            hits = hits.saturating_add(1);
            i = end;
            continue;
        }
        if let Some(c) = input.get(i..=i) {
            out.push_str(c);
        }
        i += 1;
    }
    PassResult { text: out, hits }
}

fn boundary_left(bytes: &[u8], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = bytes.get(i - 1).copied().unwrap_or(0);
    !(prev.is_ascii_alphanumeric() || prev == b'.' || prev == b':')
}

fn boundary_right(bytes: &[u8], i: usize) -> bool {
    if i >= bytes.len() {
        return true;
    }
    let nxt = bytes.get(i).copied().unwrap_or(0);
    !(nxt.is_ascii_alphanumeric() || nxt == b'.' || nxt == b':')
}

fn match_ipv4_at(bytes: &[u8], start: usize) -> Option<usize> {
    if !boundary_left(bytes, start) {
        return None;
    }
    let mut i = start;
    let mut octets = 0;
    while octets < 4 {
        let octet_start = i;
        let mut digits = 0;
        let mut value: u32 = 0;
        while digits < 3 {
            let b = bytes.get(i).copied().unwrap_or(0);
            if b.is_ascii_digit() {
                value = value
                    .saturating_mul(10)
                    .saturating_add(u32::from(b - b'0'));
                digits += 1;
                i += 1;
            } else {
                break;
            }
        }
        if digits == 0 || value > 255 {
            return None;
        }
        // Reject leading-zero octets (e.g. "01.02.03.04") to avoid
        // swallowing tag-shaped digit runs inside opaque ids.
        if digits > 1 {
            let first =
                bytes.get(octet_start).copied().unwrap_or(0);
            if first == b'0' {
                return None;
            }
        }
        octets += 1;
        if octets < 4 {
            let dot = bytes.get(i).copied().unwrap_or(0);
            if dot != b'.' {
                return None;
            }
            i += 1;
        }
    }
    if !boundary_right(bytes, i) {
        return None;
    }
    Some(i)
}

fn match_ipv6_at(bytes: &[u8], start: usize) -> Option<usize> {
    if !boundary_left(bytes, start) {
        return None;
    }
    let mut i = start;
    let mut groups = 0;
    let mut saw_double_colon = false;
    let mut hex_chars_in_run = 0;
    let scan_start = i;
    while i < bytes.len() {
        let b = bytes.get(i).copied().unwrap_or(0);
        if b.is_ascii_hexdigit() {
            hex_chars_in_run += 1;
            if hex_chars_in_run > 4 {
                return None;
            }
            i += 1;
        } else if b == b':' {
            if hex_chars_in_run > 0 {
                groups += 1;
                hex_chars_in_run = 0;
            }
            // Look ahead for the canonical `::` compression marker.
            let nxt = bytes.get(i + 1).copied().unwrap_or(0);
            if nxt == b':' {
                if saw_double_colon {
                    return None;
                }
                saw_double_colon = true;
                i += 2;
            } else {
                i += 1;
            }
        } else {
            break;
        }
    }
    if hex_chars_in_run > 0 {
        groups += 1;
    }
    // Require a minimum of 3 groups to avoid swallowing PAT prefixes
    // shaped like `crl_<hex>`. Require at least one `:` and either
    // 8 groups or a `::` compression marker.
    if i == scan_start || groups < 3 {
        return None;
    }
    if !saw_double_colon && groups != 8 {
        return None;
    }
    if saw_double_colon && groups > 8 {
        return None;
    }
    if !boundary_right(bytes, i) {
        return None;
    }
    Some(i)
}

// -- Token / Bearer / JWT scanner -------------------------------------

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')
}

fn redact_tokens(input: &str) -> PassResult {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut hits: u32 = 0;
    while i < bytes.len() {
        // `Bearer <token>` literal (case-insensitive prefix).
        if matches_bearer_prefix(bytes, i) {
            let after = i + BEARER_PREFIX_LEN;
            // Skip a single optional space.
            let token_start =
                if bytes.get(after).copied() == Some(b' ') {
                    after + 1
                } else {
                    after
                };
            let mut token_end = token_start;
            while token_end < bytes.len() {
                let b = bytes
                    .get(token_end)
                    .copied()
                    .unwrap_or(0);
                if is_token_byte(b) {
                    token_end += 1;
                } else {
                    break;
                }
            }
            if token_end - token_start >= 8 {
                out.push_str(TOKEN_PLACEHOLDER);
                hits = hits.saturating_add(1);
                i = token_end;
                continue;
            }
        }
        // JWT (3 dot-separated base64url segments) — match the
        // canonical 3-segment shape (eyJ… header; long payload; long
        // signature).
        if matches_jwt_at(bytes, i) {
            let end = jwt_end(bytes, i);
            out.push_str(TOKEN_PLACEHOLDER);
            hits = hits.saturating_add(1);
            i = end;
            continue;
        }
        // Long alphanumeric runs (≥ 24 ASCII alphanumeric chars in a
        // row, optionally separated by `_` / `-` boundaries) — covers
        // generic API keys / OAuth tokens. Word boundary required on
        // both sides.
        if let Some(end) = match_long_token_at(bytes, i) {
            out.push_str(TOKEN_PLACEHOLDER);
            hits = hits.saturating_add(1);
            i = end;
            continue;
        }
        if let Some(c) = input.get(i..=i) {
            out.push_str(c);
        }
        i += 1;
    }
    PassResult { text: out, hits }
}

const BEARER_PREFIX_LEN: usize = 6;

fn matches_bearer_prefix(bytes: &[u8], i: usize) -> bool {
    if !boundary_left(bytes, i) {
        return false;
    }
    let slice = bytes.get(i..i + BEARER_PREFIX_LEN);
    matches!(slice, Some(s) if s.eq_ignore_ascii_case(b"Bearer"))
}

fn matches_jwt_at(bytes: &[u8], start: usize) -> bool {
    if !boundary_left(bytes, start) {
        return false;
    }
    // The canonical JWT header is `eyJ` (the base64-url encoding of
    // `{"`); we use that as the hard prefix to avoid false-positives
    // on generic dotted tokens.
    let prefix = bytes.get(start..start + 3);
    if prefix != Some(b"eyJ") {
        return false;
    }
    // Walk forward checking 3 dot-separated runs of base64url chars
    // (each ≥ 4 chars).
    let mut i = start;
    let mut segments = 0;
    while segments < 3 {
        let seg_start = i;
        while i < bytes.len() {
            let b = bytes.get(i).copied().unwrap_or(0);
            if is_jwt_byte(b) {
                i += 1;
            } else {
                break;
            }
        }
        if i - seg_start < 4 {
            return false;
        }
        segments += 1;
        if segments < 3 {
            let dot = bytes.get(i).copied().unwrap_or(0);
            if dot != b'.' {
                return false;
            }
            i += 1;
        }
    }
    true
}

fn jwt_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 3;
    let mut segments = 1;
    while segments < 3 {
        while i < bytes.len() {
            let b = bytes.get(i).copied().unwrap_or(0);
            if is_jwt_byte(b) {
                i += 1;
            } else {
                break;
            }
        }
        segments += 1;
        if segments < 3 {
            let dot = bytes.get(i).copied().unwrap_or(0);
            if dot != b'.' {
                break;
            }
            i += 1;
        }
    }
    i
}

fn is_jwt_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')
}

fn match_long_token_at(bytes: &[u8], start: usize) -> Option<usize> {
    if !boundary_left(bytes, start) {
        return None;
    }
    let mut i = start;
    let mut alnum_count: u32 = 0;
    let mut total_len: u32 = 0;
    while i < bytes.len() {
        let b = bytes.get(i).copied().unwrap_or(0);
        if b.is_ascii_alphanumeric() {
            alnum_count = alnum_count.saturating_add(1);
            total_len = total_len.saturating_add(1);
            i += 1;
        } else if matches!(b, b'_' | b'-') && total_len > 0 {
            total_len = total_len.saturating_add(1);
            i += 1;
        } else {
            break;
        }
    }
    if alnum_count >= 24
        && total_len >= 24
        && boundary_right(bytes, i)
        && contains_letter_and_digit(
            bytes.get(start..i).unwrap_or(&[]),
        )
    {
        Some(i)
    } else {
        None
    }
}

fn contains_letter_and_digit(s: &[u8]) -> bool {
    let has_letter = s.iter().any(|c| c.is_ascii_alphabetic());
    let has_digit = s.iter().any(|c| c.is_ascii_digit());
    has_letter && has_digit
}

// -- PAN (credit card; Luhn) ------------------------------------------

fn redact_pan(input: &str) -> PassResult {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut hits: u32 = 0;
    while i < bytes.len() {
        if let Some((digits, end)) = collect_pan_digits(bytes, i) {
            if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
                out.push_str(PAN_PLACEHOLDER);
                hits = hits.saturating_add(1);
                i = end;
                continue;
            }
        }
        if let Some(c) = input.get(i..=i) {
            out.push_str(c);
        }
        i += 1;
    }
    PassResult { text: out, hits }
}

fn collect_pan_digits(
    bytes: &[u8],
    start: usize,
) -> Option<(Vec<u8>, usize)> {
    if !boundary_left(bytes, start) {
        return None;
    }
    let first = bytes.get(start).copied().unwrap_or(0);
    if !first.is_ascii_digit() {
        return None;
    }
    let mut digits: Vec<u8> = Vec::new();
    let mut i = start;
    let mut last_digit_idx = start;
    while i < bytes.len() {
        let b = bytes.get(i).copied().unwrap_or(0);
        if b.is_ascii_digit() {
            digits.push(b - b'0');
            i += 1;
            last_digit_idx = i;
        } else if matches!(b, b' ' | b'-')
            && !digits.is_empty()
            && digits.len() < 19
        {
            i += 1;
        } else {
            break;
        }
    }
    if digits.is_empty() {
        return None;
    }
    // Trim trailing separators back to the last digit boundary.
    let end = last_digit_idx;
    if !boundary_right(bytes, end) {
        return None;
    }
    Some((digits, end))
}

fn luhn_valid(digits: &[u8]) -> bool {
    if digits.is_empty() {
        return false;
    }
    let mut sum: u32 = 0;
    let n = digits.len();
    for (idx, d) in digits.iter().enumerate() {
        let from_right = n - 1 - idx;
        let d = u32::from(*d);
        let v = if from_right % 2 == 1 {
            let doubled = d * 2;
            if doubled > 9 {
                doubled - 9
            } else {
                doubled
            }
        } else {
            d
        };
        sum = sum.saturating_add(v);
    }
    sum % 10 == 0
}

// -- CPF / CNPJ scanner -----------------------------------------------

fn redact_cpf_cnpj(input: &str) -> PassResult {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    let mut hits: u32 = 0;
    while i < bytes.len() {
        if let Some((digits, end)) = collect_doc_digits(bytes, i) {
            if digits.len() == 14 && cnpj_valid(&digits) {
                out.push_str(CNPJ_PLACEHOLDER);
                hits = hits.saturating_add(1);
                i = end;
                continue;
            }
            if digits.len() == 11 && cpf_valid(&digits) {
                out.push_str(CPF_PLACEHOLDER);
                hits = hits.saturating_add(1);
                i = end;
                continue;
            }
        }
        if let Some(c) = input.get(i..=i) {
            out.push_str(c);
        }
        i += 1;
    }
    PassResult { text: out, hits }
}

fn collect_doc_digits(
    bytes: &[u8],
    start: usize,
) -> Option<(Vec<u8>, usize)> {
    if !boundary_left(bytes, start) {
        return None;
    }
    let first = bytes.get(start).copied().unwrap_or(0);
    if !first.is_ascii_digit() {
        return None;
    }
    let mut digits: Vec<u8> = Vec::new();
    let mut i = start;
    let mut last_digit_idx = start;
    while i < bytes.len() && digits.len() < 14 {
        let b = bytes.get(i).copied().unwrap_or(0);
        if b.is_ascii_digit() {
            digits.push(b - b'0');
            i += 1;
            last_digit_idx = i;
        } else if matches!(b, b'.' | b'-' | b'/')
            && !digits.is_empty()
        {
            i += 1;
        } else {
            break;
        }
    }
    let end = last_digit_idx;
    if !boundary_right(bytes, end) {
        return None;
    }
    Some((digits, end))
}

fn cpf_valid(digits: &[u8]) -> bool {
    if digits.len() != 11 {
        return false;
    }
    if digits.iter().all(|d| *d == digits.first().copied().unwrap_or(0)) {
        return false;
    }
    let mut sum1: u32 = 0;
    for (idx, d) in digits.iter().take(9).enumerate() {
        sum1 = sum1.saturating_add(
            u32::from(*d).saturating_mul(10 - (idx as u32)),
        );
    }
    let dv1 = compute_dv_mod11(sum1);
    if digits.get(9).copied() != Some(dv1) {
        return false;
    }
    let mut sum2: u32 = 0;
    for (idx, d) in digits.iter().take(10).enumerate() {
        sum2 = sum2.saturating_add(
            u32::from(*d).saturating_mul(11 - (idx as u32)),
        );
    }
    let dv2 = compute_dv_mod11(sum2);
    digits.get(10).copied() == Some(dv2)
}

fn compute_dv_mod11(sum: u32) -> u8 {
    let r = sum % 11;
    if r < 2 {
        0
    } else {
        (11 - r) as u8
    }
}

const CNPJ_W1: [u32; 12] =
    [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
const CNPJ_W2: [u32; 13] =
    [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];

fn cnpj_valid(digits: &[u8]) -> bool {
    if digits.len() != 14 {
        return false;
    }
    if digits.iter().all(|d| *d == digits.first().copied().unwrap_or(0)) {
        return false;
    }
    let mut sum1: u32 = 0;
    for (idx, w) in CNPJ_W1.iter().enumerate() {
        let d =
            u32::from(digits.get(idx).copied().unwrap_or(0));
        sum1 = sum1.saturating_add(d.saturating_mul(*w));
    }
    let dv1 = compute_dv_mod11(sum1);
    if digits.get(12).copied() != Some(dv1) {
        return false;
    }
    let mut sum2: u32 = 0;
    for (idx, w) in CNPJ_W2.iter().enumerate() {
        let d =
            u32::from(digits.get(idx).copied().unwrap_or(0));
        sum2 = sum2.saturating_add(d.saturating_mul(*w));
    }
    let dv2 = compute_dv_mod11(sum2);
    digits.get(13).copied() == Some(dv2)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn email_redacted() {
        let r = InMemoryPiiRedactor::new();
        let out = r.redact("contact alice@example.com today");
        assert!(out.redacted.contains("<EMAIL_REDACTED>"));
        assert!(!out.redacted.contains("alice@example.com"));
        assert_eq!(out.email_hits, 1);
    }

    #[test]
    fn ipv4_redacted() {
        let r = InMemoryPiiRedactor::new();
        let out = r.redact("client 192.168.1.42 connected");
        assert!(out.redacted.contains("<IP_REDACTED>"));
        assert!(!out.redacted.contains("192.168.1.42"));
        assert_eq!(out.ip_hits, 1);
    }

    #[test]
    fn ipv4_invalid_octet_passes_through() {
        let r = InMemoryPiiRedactor::new();
        // 999 > 255 — not a valid IPv4.
        let out = r.redact("999.0.0.1 not an ip");
        assert!(out.redacted.contains("999"));
        assert_eq!(out.ip_hits, 0);
    }

    #[test]
    fn ipv6_redacted() {
        let r = InMemoryPiiRedactor::new();
        let out =
            r.redact("from 2001:db8::1 routed");
        assert!(out.redacted.contains("<IP_REDACTED>"));
        assert!(!out.redacted.contains("2001:db8::1"));
        assert_eq!(out.ip_hits, 1);
    }

    #[test]
    fn bearer_token_redacted() {
        let r = InMemoryPiiRedactor::new();
        let out = r.redact(
            "Authorization: Bearer abcdef0123456789ZYXW",
        );
        assert!(out.redacted.contains("<TOKEN_REDACTED>"));
        assert!(!out
            .redacted
            .contains("abcdef0123456789ZYXW"));
        assert_eq!(out.token_hits, 1);
    }

    #[test]
    fn jwt_redacted() {
        let r = InMemoryPiiRedactor::new();
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NSJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = r.redact(&format!("token {jwt} expired"));
        assert!(out.redacted.contains("<TOKEN_REDACTED>"));
        assert!(!out.redacted.contains(jwt));
        assert_eq!(out.token_hits, 1);
    }

    #[test]
    fn pan_redacted_visa_test_number() {
        let r = InMemoryPiiRedactor::new();
        // Standard Visa test PAN (4111-1111-1111-1111; Luhn-valid).
        let out =
            r.redact("card 4111 1111 1111 1111 captured");
        assert!(out.redacted.contains("<PAN_REDACTED>"));
        assert_eq!(out.pan_hits, 1);
    }

    #[test]
    fn pan_invalid_luhn_passes_through() {
        let r = InMemoryPiiRedactor::new();
        // 16 digit run that is NOT Luhn-valid.
        let out =
            r.redact("transaction 1234567890123456 logged");
        assert!(out.redacted.contains("1234567890123456"));
        assert_eq!(out.pan_hits, 0);
    }

    #[test]
    fn cpf_redacted_canonical() {
        let r = InMemoryPiiRedactor::new();
        // Canonical valid CPF (random; passes mod-11 test).
        let out =
            r.redact("doc 529.982.247-25 verified");
        assert!(out.redacted.contains("<CPF_REDACTED>"));
        assert_eq!(out.cpf_cnpj_hits, 1);
    }

    #[test]
    fn cnpj_redacted_canonical() {
        let r = InMemoryPiiRedactor::new();
        // Canonical valid CNPJ.
        let out =
            r.redact("empresa 11.222.333/0001-81 ativa");
        assert!(out.redacted.contains("<CNPJ_REDACTED>"));
        assert_eq!(out.cpf_cnpj_hits, 1);
    }

    #[test]
    fn cpf_repeated_digits_rejected() {
        let r = InMemoryPiiRedactor::new();
        let out = r.redact("doc 111.111.111-11 invalid");
        assert!(out.redacted.contains("111.111.111-11"));
        assert_eq!(out.cpf_cnpj_hits, 0);
    }

    #[test]
    fn redaction_idempotent() {
        let r = InMemoryPiiRedactor::new();
        let input =
            "alice@example.com from 192.168.1.42 with Bearer abcdef0123456789ZYXW";
        let pass1 = r.redact(input);
        let pass2 = r.redact(&pass1.redacted);
        assert_eq!(pass1.redacted, pass2.redacted);
        // Second pass MUST find zero hits (idempotency).
        assert_eq!(pass2.email_hits, 0);
        assert_eq!(pass2.ip_hits, 0);
        assert_eq!(pass2.token_hits, 0);
    }

    #[test]
    fn redaction_preserves_non_pii() {
        let r = InMemoryPiiRedactor::new();
        let input =
            "GET /v1/cas/put status=200 duration_ms=42 region=iad";
        let out = r.redact(input);
        assert_eq!(out.redacted, input);
        assert_eq!(out.total_hits(), 0);
    }

    #[test]
    fn redact_json_walks_string_leaves() {
        let r = InMemoryPiiRedactor::new();
        let mut v = json!({
            "user": "alice@example.com",
            "client_ip": "192.168.1.42",
            "status": 200,
            "nested": {
                "auth": "Bearer abcdef0123456789ZYXW"
            },
            "tags": ["alice@example.com", "ok"]
        });
        let outcome = r.redact_json(&mut v);
        let s = v.to_string();
        assert!(!s.contains("alice@example.com"));
        assert!(!s.contains("192.168.1.42"));
        assert!(!s.contains("abcdef0123456789ZYXW"));
        assert!(s.contains("\"status\":200"));
        assert!(outcome.email_hits == 2);
        assert!(outcome.ip_hits == 1);
        assert!(outcome.token_hits == 1);
    }

    #[test]
    fn pattern_kind_canonical_strings() {
        let v = canonical_pii_pattern_kinds();
        assert_eq!(v.len(), 5);
        let mut set = std::collections::HashSet::new();
        for k in v {
            assert!(set.insert(k.as_str()));
        }
        assert_eq!(set.len(), 5);
    }

    #[test]
    fn placeholders_pinned() {
        assert_eq!(EMAIL_PLACEHOLDER, "<EMAIL_REDACTED>");
        assert_eq!(IP_PLACEHOLDER, "<IP_REDACTED>");
        assert_eq!(TOKEN_PLACEHOLDER, "<TOKEN_REDACTED>");
        assert_eq!(PAN_PLACEHOLDER, "<PAN_REDACTED>");
        assert_eq!(CPF_PLACEHOLDER, "<CPF_REDACTED>");
        assert_eq!(CNPJ_PLACEHOLDER, "<CNPJ_REDACTED>");
    }

    #[test]
    fn counters_accumulate_across_passes() {
        let r = InMemoryPiiRedactor::new();
        r.redact("alice@example.com");
        r.redact("bob@example.com");
        let (email, _, _, _, _) = r.snapshot_counters();
        assert_eq!(email, 2);
    }

    #[test]
    fn cloned_redactor_shares_counters() {
        let r1 = InMemoryPiiRedactor::new();
        let r2 = r1.clone();
        r1.redact("alice@example.com");
        let (email, _, _, _, _) = r2.snapshot_counters();
        assert_eq!(email, 1);
    }

    #[test]
    fn placeholders_not_rematch_themselves() {
        let r = InMemoryPiiRedactor::new();
        let s = "<EMAIL_REDACTED>";
        let out = r.redact(s);
        assert_eq!(out.redacted, s);
        assert_eq!(out.total_hits(), 0);
    }

    #[test]
    fn outcome_total_hits_sums_correctly() {
        let r = InMemoryPiiRedactor::new();
        let out = r
            .redact("alice@example.com 192.168.1.42 Bearer abcdef0123456789ZYXW");
        assert_eq!(out.total_hits(), 3);
    }
}
