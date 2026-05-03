//! W3C Trace Context Recommendation 2020 `traceparent` parser /
//! formatter + canonical 16-byte trace_id + 8-byte span_id + 1-byte
//! trace flags shape.
//!
//! ## W3C ABNF (`traceparent`)
//!
//! ```text
//! traceparent     = version "-" trace-id "-" parent-id "-" trace-flags
//! version         = 2HEXDIGLC          ; "00" canonical
//! trace-id        = 32HEXDIGLC          ; 16 bytes; MUST NOT be all-zero
//! parent-id       = 16HEXDIGLC          ; 8 bytes; MUST NOT be all-zero
//! trace-flags     = 2HEXDIGLC          ; bit 0 = sampled
//! ```
//!
//! Per W3C §3.2.2 a recipient MUST reject:
//!
//! - any non-`00` version (FROZEN at canonical for now; future
//!   `01..fe` reserved; `ff` invalid).
//! - any malformed hex.
//! - any wrong segment length.
//! - all-zero `trace-id` (16 bytes of `0x00`).
//! - all-zero `parent-id` / `span-id` (8 bytes of `0x00`).
//!
//! The parser here implements the strict canonical contract; the
//! permissive "version >= 00 try harder" path is deferred to
//! WI-S09-007 PRR ship gate (per `trait-abstraction-defer` charter
//! pattern; production parser will accept future versions while
//! preserving the strict zero-trace-id reject).

use crate::error::TracingError;

/// W3C Trace Context canonical version byte. Per W3C §3.2.2 the only
/// accepted version today is `00`. Future `01..fe` are reserved;
/// `ff` is INVALID per §3.2.2.1.
pub const W3C_TRACE_CONTEXT_VERSION: u8 = 0x00;

/// Canonical W3C `traceparent` header name (case-insensitive on the
/// HTTP wire per RFC 7230 §3.2; this constant is the canonical
/// lowercase form per W3C §3.2).
pub const TRACEPARENT_HEADER: &str = "traceparent";

/// Canonical W3C `tracestate` header name. Vendor-specific extensions
/// (per W3C §3.3) are FROZEN at empty in this WI; the trait surface
/// reserves additive growth for WI-S09-007 PRR ship gate.
pub const TRACESTATE_HEADER: &str = "tracestate";

/// 16-byte trace identifier. Per W3C §3.2.2.2 the value is a
/// 16-byte array hex-encoded as 32 lowercase hex digits.
pub type TraceId = [u8; 16];

/// 8-byte span identifier. Per W3C §3.2.2.3 the value is an 8-byte
/// array hex-encoded as 16 lowercase hex digits.
pub type SpanId = [u8; 8];

/// W3C `traceparent` decoded shape. Per W3C §3.2 the canonical wire
/// form is `<version>-<trace-id>-<span-id>-<flags>`.
///
/// `Copy` because the underlying types are fixed-size arrays + a u8
/// flag byte; cloning is free at the type-system level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TraceContext {
    /// W3C version byte. Always [`W3C_TRACE_CONTEXT_VERSION`] today.
    pub version: u8,
    /// 16-byte trace identifier.
    pub trace_id: TraceId,
    /// 8-byte span identifier (the originating span at this hop).
    pub span_id: SpanId,
    /// W3C trace-flags byte. Bit 0 (`0x01`) is `sampled`; the
    /// remaining bits are reserved and MUST be 0.
    pub flags: u8,
}

/// Canonical "all-zero" trace-id sentinel. Per W3C §3.2.2.2 a
/// trace-id of all zero MUST be rejected by the recipient.
pub const ALL_ZERO_TRACE_ID: TraceId = [0u8; 16];

/// Canonical "all-zero" span-id sentinel. Per W3C §3.2.2.3 a span-id
/// of all zero MUST be rejected by the recipient.
pub const ALL_ZERO_SPAN_ID: SpanId = [0u8; 8];

/// W3C `sampled` flag (bit 0). Per W3C §3.2.2.4 set when the upstream
/// has sampled this trace; recipients SHOULD honor the decision.
pub const TRACE_FLAGS_SAMPLED: u8 = 0x01;

impl TraceContext {
    /// Construct a canonical [`TraceContext`] from raw bytes. Returns
    /// [`TracingError::TraceContextParseError`] on the canonical
    /// reject paths (all-zero trace_id / span_id).
    ///
    /// # Errors
    ///
    /// - All-zero `trace_id` (W3C §3.2.2.2 violation).
    /// - All-zero `span_id` (W3C §3.2.2.3 violation).
    pub fn new(
        trace_id: TraceId,
        span_id: SpanId,
        flags: u8,
    ) -> Result<Self, TracingError> {
        if trace_id == ALL_ZERO_TRACE_ID {
            return Err(TracingError::TraceContextParseError(
                "all-zero trace_id rejected per W3C §3.2.2.2"
                    .to_string(),
            ));
        }
        if span_id == ALL_ZERO_SPAN_ID {
            return Err(TracingError::TraceContextParseError(
                "all-zero span_id rejected per W3C §3.2.2.3"
                    .to_string(),
            ));
        }
        Ok(Self {
            version: W3C_TRACE_CONTEXT_VERSION,
            trace_id,
            span_id,
            flags,
        })
    }

    /// Whether the upstream marked this trace as sampled (W3C bit 0).
    #[must_use]
    pub const fn is_sampled(&self) -> bool {
        self.flags & TRACE_FLAGS_SAMPLED != 0
    }
}

/// Format a [`TraceContext`] into the canonical W3C `traceparent`
/// wire format: `00-<32 lowercase hex>-<16 lowercase hex>-<2 hex>`.
#[must_use]
pub fn format_traceparent(ctx: &TraceContext) -> String {
    let mut out = String::with_capacity(55);
    push_hex_byte(&mut out, ctx.version);
    out.push('-');
    push_hex_bytes(&mut out, &ctx.trace_id);
    out.push('-');
    push_hex_bytes(&mut out, &ctx.span_id);
    out.push('-');
    push_hex_byte(&mut out, ctx.flags);
    out
}

/// Parse a canonical W3C `traceparent` header value into a
/// [`TraceContext`].
///
/// # Errors
///
/// - Wrong overall length (canonical is 55 ASCII chars).
/// - Wrong segment dash positions.
/// - Non-`00` version byte (we FREEZE at canonical 00 per the WI
///   spec; future versions deferred to WI-S09-007).
/// - Malformed hex.
/// - All-zero trace_id (W3C §3.2.2.2).
/// - All-zero span_id (W3C §3.2.2.3).
/// - Reserved bits in `flags` set (the parser tolerates the byte
///   itself; the canonical contract is enforced at construction).
pub fn parse_traceparent(s: &str) -> Result<TraceContext, TracingError> {
    let bytes = s.as_bytes();
    if bytes.len() != 55 {
        return Err(TracingError::TraceContextParseError(format!(
            "expected 55 ASCII chars; got {}",
            bytes.len()
        )));
    }
    if bytes.get(2) != Some(&b'-')
        || bytes.get(35) != Some(&b'-')
        || bytes.get(52) != Some(&b'-')
    {
        return Err(TracingError::TraceContextParseError(
            "canonical dash positions invalid".to_string(),
        ));
    }
    let version_slice = bytes.get(0..2).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "version slice out of range".to_string(),
        )
    })?;
    let trace_slice = bytes.get(3..35).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "trace_id slice out of range".to_string(),
        )
    })?;
    let span_slice = bytes.get(36..52).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "span_id slice out of range".to_string(),
        )
    })?;
    let flags_slice = bytes.get(53..55).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "flags slice out of range".to_string(),
        )
    })?;

    let version = parse_hex_byte(version_slice).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "version hex malformed".to_string(),
        )
    })?;
    if version != W3C_TRACE_CONTEXT_VERSION {
        return Err(TracingError::TraceContextParseError(format!(
            "unsupported version {version:#04x}; only 0x00 accepted"
        )));
    }

    let mut trace_id = [0u8; 16];
    parse_hex_into(trace_slice, &mut trace_id).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "trace_id hex malformed".to_string(),
        )
    })?;
    let mut span_id = [0u8; 8];
    parse_hex_into(span_slice, &mut span_id).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "span_id hex malformed".to_string(),
        )
    })?;
    let flags = parse_hex_byte(flags_slice).ok_or_else(|| {
        TracingError::TraceContextParseError(
            "flags hex malformed".to_string(),
        )
    })?;

    TraceContext::new(trace_id, span_id, flags)
}

fn push_hex_byte(out: &mut String, b: u8) {
    out.push(hex_digit(b >> 4));
    out.push(hex_digit(b & 0x0F));
}

fn push_hex_bytes(out: &mut String, bs: &[u8]) {
    for b in bs {
        push_hex_byte(out, *b);
    }
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        // Unreachable by construction: the caller masks to 4 bits.
        _ => '0',
    }
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        // Per W3C §3.2.2 lowercase is canonical; uppercase rejected.
        _ => None,
    }
}

fn parse_hex_byte(slice: &[u8]) -> Option<u8> {
    let hi = hex_value(*slice.first()?)?;
    let lo = hex_value(*slice.get(1)?)?;
    Some((hi << 4) | lo)
}

fn parse_hex_into(slice: &[u8], dst: &mut [u8]) -> Option<()> {
    if slice.len() != dst.len() * 2 {
        return None;
    }
    for (i, byte) in dst.iter_mut().enumerate() {
        let pair = slice.get(i * 2..i * 2 + 2)?;
        *byte = parse_hex_byte(pair)?;
    }
    Some(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    fn sample_ctx() -> TraceContext {
        TraceContext::new(
            [
                0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6,
                0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e, 0x47, 0x36,
            ],
            [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7],
            TRACE_FLAGS_SAMPLED,
        )
        .unwrap()
    }

    #[test]
    fn canonical_constants_pinned() {
        assert_eq!(W3C_TRACE_CONTEXT_VERSION, 0x00);
        assert_eq!(TRACEPARENT_HEADER, "traceparent");
        assert_eq!(TRACESTATE_HEADER, "tracestate");
        assert_eq!(TRACE_FLAGS_SAMPLED, 0x01);
        assert_eq!(ALL_ZERO_TRACE_ID, [0u8; 16]);
        assert_eq!(ALL_ZERO_SPAN_ID, [0u8; 8]);
    }

    #[test]
    fn format_traceparent_matches_canonical_w3c_example() {
        // Canonical example from W3C §3.2.4.
        let ctx = sample_ctx();
        let s = format_traceparent(&ctx);
        assert_eq!(
            s,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
        assert_eq!(s.len(), 55);
    }

    #[test]
    fn parse_traceparent_round_trip() {
        let ctx = sample_ctx();
        let s = format_traceparent(&ctx);
        let parsed = parse_traceparent(&s).unwrap();
        assert_eq!(parsed, ctx);
    }

    #[test]
    fn parse_rejects_non_00_version() {
        let s =
            "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn parse_rejects_ff_version() {
        let s =
            "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn parse_rejects_uppercase_hex() {
        // W3C §3.2.2 mandates lowercase hex.
        let s =
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn parse_rejects_all_zero_trace_id() {
        let s =
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01";
        let err = parse_traceparent(s).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("all-zero trace_id"));
    }

    #[test]
    fn parse_rejects_all_zero_span_id() {
        let s =
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01";
        let err = parse_traceparent(s).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("all-zero span_id"));
    }

    #[test]
    fn parse_rejects_wrong_length() {
        let s = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn parse_rejects_missing_dash_separators() {
        let s =
            "00:4bf92f3577b34da6a3ce929d0e0e4736:00f067aa0ba902b7:01";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn parse_rejects_malformed_hex_in_trace_id() {
        let s =
            "00-zzzzzzzz3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn parse_rejects_malformed_hex_in_span_id() {
        let s =
            "00-4bf92f3577b34da6a3ce929d0e0e4736-zzf067aa0ba902b7-01";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn parse_rejects_malformed_hex_in_flags() {
        let s =
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-zz";
        let err = parse_traceparent(s).unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn new_rejects_all_zero_trace_id_constructor() {
        let err = TraceContext::new(
            [0u8; 16],
            [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7],
            0,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn new_rejects_all_zero_span_id_constructor() {
        let err = TraceContext::new(
            [
                0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6,
                0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e, 0x47, 0x36,
            ],
            [0u8; 8],
            0,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            TracingError::TraceContextParseError(_)
        ));
    }

    #[test]
    fn is_sampled_reflects_flag_bit() {
        let ctx = sample_ctx();
        assert!(ctx.is_sampled());
        let unsampled = TraceContext::new(
            ctx.trace_id, ctx.span_id, 0x00,
        )
        .unwrap();
        assert!(!unsampled.is_sampled());
    }

    #[test]
    fn ignores_reserved_bits_in_flags_at_construction() {
        // Per W3C §3.2.2.4 reserved bits MUST be 0 from sender; the
        // recipient SHOULD tolerate but ignore unknown bits. We honor
        // sampled bit only.
        let ctx = TraceContext::new(
            [
                0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6,
                0xa3, 0xce, 0x92, 0x9d, 0x0e, 0x0e, 0x47, 0x36,
            ],
            [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7],
            0xFF,
        )
        .unwrap();
        assert!(ctx.is_sampled());
    }
}
