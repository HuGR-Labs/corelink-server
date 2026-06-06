//! Slack Block Kit message builder.
//!
//! [`SlackMessage`] is the canonical rich-format wire payload sent to
//! Slack incoming webhooks. It models a subset of Block Kit
//! (<https://api.slack.com/block-kit>):
//!
//! - 1 `header` block (plain_text, ≤150 chars per Slack spec — we cap
//!   at 150 and truncate with an ellipsis).
//! - 1 `section` block carrying up to 10 key/value `fields`.
//! - 1 `context` block carrying the footer + ISO-8601 timestamp.
//! - 0..N `actions` block buttons (callers requiring interactivity).
//! - Optional `thread_ts` for follow-up posts.
//!
//! ## mrkdwn injection defence
//!
//! Field keys and values are rendered as Slack `mrkdwn` and therefore
//! pass through [`escape_mrkdwn`] which escapes the
//! Slack-special-character set (`<`, `>`, `&`, `*`, `_`, `~`,
//! backtick) so untrusted input cannot inject mrkdwn syntax. The
//! header uses `plain_text` blocks which Slack itself renders
//! literally — no escape is needed there but we cap length.

use crate::channel::SlackChannel;

/// Header maximum length (Slack hard limit per Block Kit spec).
pub const HEADER_MAX_CHARS: usize = 150;

/// Field key/value pair (rendered as `*key:*\nvalue` in a Slack
/// `section` block's `fields` array).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlackField {
    /// Field key (label).
    pub key: String,
    /// Field value.
    pub value: String,
}

impl SlackField {
    /// Construct a new field.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Optional action button — POSTs a structured payload back to a
/// configured action handler when clicked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlackActionButton {
    /// Internal action id (Slack `action_id`).
    pub action_id: String,
    /// Button label.
    pub label: String,
    /// Slack button "style". Production canonical values are
    /// `primary`, `danger`, or unset (default neutral).
    pub style: Option<String>,
}

impl SlackActionButton {
    /// Construct a new action button.
    pub fn new(action_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            action_id: action_id.into(),
            label: label.into(),
            style: None,
        }
    }

    /// Set the button style (`primary` / `danger`).
    #[must_use]
    pub fn with_style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }
}

/// Block Kit message + channel routing target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlackMessage {
    /// Routing target.
    pub channel: SlackChannel,
    /// Header text (plain_text; truncated to [`HEADER_MAX_CHARS`]).
    pub header: String,
    /// Body fields (≤ 10 per Slack section block limit).
    pub fields: Vec<SlackField>,
    /// Footer (context block text).
    pub footer: String,
    /// ISO-8601 timestamp appended to footer.
    pub timestamp_iso: String,
    /// Optional action buttons.
    pub actions: Vec<SlackActionButton>,
    /// Optional thread root timestamp (`ts` from earlier post) — when
    /// set, the message is sent as a reply in the same thread.
    pub thread_ts: Option<String>,
}

impl SlackMessage {
    /// Construct a new message builder.
    pub fn new(
        channel: SlackChannel,
        header: impl Into<String>,
        footer: impl Into<String>,
        timestamp_iso: impl Into<String>,
    ) -> Self {
        Self {
            channel,
            header: truncate(header.into(), HEADER_MAX_CHARS),
            fields: Vec::new(),
            footer: footer.into(),
            timestamp_iso: timestamp_iso.into(),
            actions: Vec::new(),
            thread_ts: None,
        }
    }

    /// Append a field.
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push(SlackField::new(key, value));
        self
    }

    /// Append an action button.
    #[must_use]
    pub fn with_action(mut self, action: SlackActionButton) -> Self {
        self.actions.push(action);
        self
    }

    /// Pin the message to an existing thread.
    #[must_use]
    pub fn in_thread(mut self, ts: impl Into<String>) -> Self {
        self.thread_ts = Some(ts.into());
        self
    }

    /// Serialise to the wire JSON Slack expects.
    #[must_use]
    pub fn to_block_kit_json(&self) -> serde_json::Value {
        // header
        let mut blocks: Vec<serde_json::Value> = Vec::new();
        blocks.push(serde_json::json!({
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": self.header,
                "emoji": true,
            }
        }));

        // section (key/value fields)
        if !self.fields.is_empty() {
            let fields: Vec<serde_json::Value> = self
                .fields
                .iter()
                .take(10) // Slack section.fields hard limit
                .map(|f| {
                    serde_json::json!({
                        "type": "mrkdwn",
                        "text": format!("*{}:*\n{}", escape_mrkdwn(&f.key), escape_mrkdwn(&f.value)),
                    })
                })
                .collect();
            blocks.push(serde_json::json!({
                "type": "section",
                "fields": fields,
            }));
        }

        // actions
        if !self.actions.is_empty() {
            let elements: Vec<serde_json::Value> = self
                .actions
                .iter()
                .map(|a| {
                    let mut btn = serde_json::json!({
                        "type": "button",
                        "action_id": a.action_id,
                        "text": {
                            "type": "plain_text",
                            "text": a.label,
                            "emoji": true,
                        },
                    });
                    if let Some(style) = &a.style {
                        if let Some(obj) = btn.as_object_mut() {
                            obj.insert(
                                "style".to_string(),
                                serde_json::Value::String(style.clone()),
                            );
                        }
                    }
                    btn
                })
                .collect();
            blocks.push(serde_json::json!({
                "type": "actions",
                "elements": elements,
            }));
        }

        // context (footer + timestamp)
        let context_text = if self.timestamp_iso.is_empty() {
            self.footer.clone()
        } else {
            format!("{} · {}", self.footer, self.timestamp_iso)
        };
        blocks.push(serde_json::json!({
            "type": "context",
            "elements": [{
                "type": "mrkdwn",
                "text": escape_mrkdwn(&context_text),
            }]
        }));

        // top-level payload
        let mut payload = serde_json::json!({
            "blocks": blocks,
            "text": self.header, // fallback for notifications
        });
        if let Some(ts) = &self.thread_ts {
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "thread_ts".to_string(),
                    serde_json::Value::String(ts.clone()),
                );
            }
        }
        payload
    }
}

/// Escape Slack mrkdwn-special characters so untrusted input cannot
/// inject `<links>`, bold, italics or code blocks.
///
/// Slack mrkdwn requires escaping `&`, `<`, `>` per spec; we also
/// neutralise `*`, `_`, `~`, and backtick to prevent formatting
/// injection (defence-in-depth for keys/values rendered into the
/// `section` block).
#[must_use]
pub fn escape_mrkdwn(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '*' | '_' | '~' | '`' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn truncate(mut s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut end = 0usize;
    for (count, (idx, _)) in s.char_indices().enumerate() {
        if count >= max.saturating_sub(1) {
            end = idx;
            break;
        }
    }
    s.truncate(end);
    s.push('…');
    s
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

    #[test]
    fn escape_mrkdwn_neutralises_specials() {
        assert_eq!(escape_mrkdwn("a*b_c~d`e\\f"), "a\\*b\\_c\\~d\\`e\\\\f");
        assert_eq!(escape_mrkdwn("<a&b>"), "&lt;a&amp;b&gt;");
    }

    #[test]
    fn message_serialises_header() {
        let m = SlackMessage::new(
            SlackChannel::AlertsSev1,
            "Hello",
            "footer",
            "2026-05-14T00:00:00Z",
        );
        let j = m.to_block_kit_json();
        let blocks = j.get("blocks").unwrap().as_array().unwrap();
        let header = &blocks[0];
        assert_eq!(header.get("type").unwrap(), "header");
        assert_eq!(
            header
                .get("text")
                .unwrap()
                .get("text")
                .unwrap()
                .as_str()
                .unwrap(),
            "Hello"
        );
    }

    #[test]
    fn fields_are_escaped() {
        let m = SlackMessage::new(SlackChannel::EnterpriseInquiries, "Hi", "f", "t")
            .with_field("company", "*evil* <script>");
        let j = m.to_block_kit_json();
        let blocks = j.get("blocks").unwrap().as_array().unwrap();
        // header at [0], section at [1]
        let section = &blocks[1];
        let fields = section.get("fields").unwrap().as_array().unwrap();
        let text = fields[0].get("text").unwrap().as_str().unwrap();
        assert!(!text.contains("*evil*"));
        assert!(text.contains("\\*evil\\*"));
        assert!(text.contains("&lt;script&gt;"));
    }

    #[test]
    fn thread_ts_round_trips() {
        let m = SlackMessage::new(SlackChannel::AlertsSev1, "h", "f", "t")
            .in_thread("1715607600.000100");
        let j = m.to_block_kit_json();
        assert_eq!(
            j.get("thread_ts").unwrap().as_str().unwrap(),
            "1715607600.000100"
        );
    }

    #[test]
    fn header_truncation() {
        let long: String = "x".repeat(300);
        let m = SlackMessage::new(SlackChannel::AlertsSev1, long, "f", "t");
        // header must end with an ellipsis and respect the cap
        assert!(m.header.chars().count() <= HEADER_MAX_CHARS);
        assert!(m.header.ends_with('…'));
    }

    #[test]
    fn actions_render_buttons() {
        let m = SlackMessage::new(SlackChannel::AlertsSev1, "h", "f", "t")
            .with_action(SlackActionButton::new("ack", "Acknowledge").with_style("primary"));
        let j = m.to_block_kit_json();
        let blocks = j.get("blocks").unwrap().as_array().unwrap();
        // header [0], no section (no fields), actions [1], context [2]
        let actions = &blocks[1];
        assert_eq!(actions.get("type").unwrap(), "actions");
        let elements = actions.get("elements").unwrap().as_array().unwrap();
        assert_eq!(elements[0].get("action_id").unwrap(), "ack");
        assert_eq!(elements[0].get("style").unwrap(), "primary");
    }
}
