//! klistra over HTTPS, with this computer's token when it has one.

use serde_json::Value;
use std::io::Read;
use std::sync::OnceLock;
use std::time::Duration;

/// Headers sent with every request besides the token: the MCP server names its Claude Code session.
static EXTRA: OnceLock<Vec<(String, String)>> = OnceLock::new();

pub fn send_always(headers: Vec<(String, String)>) {
    let _ = EXTRA.set(headers);
}

pub struct Reply {
    pub status: u16,
    pub body: Value,
}

impl Reply {
    /// What klistra said went wrong, if it said.
    pub fn error(&self) -> Option<String> {
        match &self.body["error"] {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.get("message").and_then(Value::as_str).map(str::to_string),
            _ => None,
        }
    }
}

/// A request to klistra: `body` as JSON when given, and an answer read as JSON (or the last event
/// of an event stream). Answers of every status come back as a Reply; only an unreachable klistra
/// is an Err.
pub fn send(method: &str, url: &str, token: Option<&str>, body: Option<&Value>, timeout: Duration) -> Result<Reply, String> {
    let mut request = ureq::request(method, url)
        .timeout(timeout)
        .set("user-agent", concat!("klistra-claude-code/", env!("CARGO_PKG_VERSION")))
        .set("accept", "application/json, text/event-stream");
    if let Some(token) = token {
        request = request.set("authorization", &format!("Bearer {token}"));
    }
    for (name, value) in EXTRA.get().into_iter().flatten() {
        request = request.set(name, value);
    }
    let response = match body.map_or_else(|| request.clone().call(), |b| request.clone().send_json(b.clone())) {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(e)) => {
            let origin = url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
            return Err(format!("Could not reach klistra at {origin}: {e}"));
        }
    };
    let status = response.status();
    let stream = response.content_type().starts_with("text/event-stream");
    let mut raw = Vec::new();
    // a frame or a picture comes as base64 in the answer; Claude Code takes up to 16 MiB of it
    response.into_reader().take(32 << 20).read_to_end(&mut raw).map_err(|e| format!("Lost klistra's answer: {e}"))?;
    let text = String::from_utf8_lossy(&raw);
    let body = if stream {
        text.lines().filter_map(|l| l.strip_prefix("data:")).filter_map(|d| serde_json::from_str(d.trim()).ok()).last().unwrap_or(Value::Null)
    } else {
        serde_json::from_str(&text).unwrap_or(Value::Null)
    };
    Ok(Reply { status, body })
}
